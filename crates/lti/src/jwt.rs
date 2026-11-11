//! Checked base64url, JWT and RSA helpers for the LTI protocol.
//!
//! Everything here is a pure function over bytes and strings: no clock, no
//! network and no I/O. Callers pass the current time and the platform's JWKS.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::rand::SystemRandom;
use ring::signature::{
    RSA_PKCS1_2048_8192_SHA256, RSA_PKCS1_SHA256, RsaKeyPair, UnparsedPublicKey,
};

use crate::LtiError;

/// Tokens larger than this are rejected before any decoding.
pub const MAX_TOKEN_LEN: usize = 64 * 1024;

/// Encode bytes as unpadded base64url.
pub fn base64url_encode(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decode unpadded base64url.
pub fn base64url_decode(text: &str) -> Result<Vec<u8>, LtiError> {
    URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|e| LtiError::InvalidToken(format!("base64url: {e}")))
}

/// Split `header.payload.signature`, rejecting oversized or malformed tokens.
pub fn split_jwt(token: &str) -> Result<(&str, &str, &str), LtiError> {
    if token.len() > MAX_TOKEN_LEN {
        return Err(LtiError::BadRequest(format!(
            "token is larger than {MAX_TOKEN_LEN} bytes"
        )));
    }
    let mut parts = token.split('.');
    let header = parts.next().unwrap_or_default();
    let payload = parts.next().unwrap_or_default();
    let signature = parts.next().unwrap_or_default();
    if header.is_empty() || payload.is_empty() || parts.next().is_some() {
        return Err(LtiError::BadRequest(
            "token must have header.payload.signature".into(),
        ));
    }
    Ok((header, payload, signature))
}

/// Sign a JWT header and payload with RS256 (SHA-256, PKCS#1 v1.5).
pub fn sign_rs256(
    private_key_pkcs8: &[u8],
    header: &serde_json::Value,
    payload: &serde_json::Value,
) -> Result<String, LtiError> {
    let key =
        RsaKeyPair::from_pkcs8(private_key_pkcs8).map_err(|e| LtiError::Key(e.to_string()))?;
    let header = serde_json::to_vec(header).map_err(|e| LtiError::Json(e.to_string()))?;
    let payload = serde_json::to_vec(payload).map_err(|e| LtiError::Json(e.to_string()))?;
    let input = format!(
        "{}.{}",
        base64url_encode(&header),
        base64url_encode(&payload)
    );
    let mut signature = vec![0u8; key.public().modulus_len()];
    key.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        input.as_bytes(),
        &mut signature,
    )
    .map_err(|_| LtiError::Key("RSA signing failed".into()))?;
    Ok(format!("{input}.{}", base64url_encode(&signature)))
}

/// Verify an RS256 signature from a JWK's raw modulus and exponent bytes.
pub fn verify_rs256(n: &[u8], e: &[u8], message: &[u8], signature: &[u8]) -> Result<(), LtiError> {
    let der = rsa_public_key_der(n, e)?;
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &der)
        .verify(message, signature)
        .map_err(|_| LtiError::InvalidToken("signature verification failed".into()))
}

fn rsa_public_key_der(n: &[u8], e: &[u8]) -> Result<Vec<u8>, LtiError> {
    let n = der_integer(n)?;
    let e = der_integer(e)?;
    let content_len = n
        .len()
        .checked_add(e.len())
        .ok_or_else(|| LtiError::Key("RSA public key is too large".into()))?;
    let mut out = Vec::with_capacity(content_len + 4);
    out.push(0x30);
    push_length(&mut out, content_len)?;
    out.extend_from_slice(&n);
    out.extend_from_slice(&e);
    Ok(out)
}

fn der_integer(bytes: &[u8]) -> Result<Vec<u8>, LtiError> {
    if bytes.is_empty() {
        return Err(LtiError::Key("empty RSA component".into()));
    }
    let mut start = 0;
    while start + 1 < bytes.len() && bytes.get(start) == Some(&0) {
        start += 1;
    }
    let value = bytes
        .get(start..)
        .ok_or_else(|| LtiError::Key("invalid RSA component".into()))?;
    let needs_zero = value.first().is_some_and(|byte| byte & 0x80 != 0);
    let content_len = value
        .len()
        .checked_add(usize::from(needs_zero))
        .ok_or_else(|| LtiError::Key("RSA component is too large".into()))?;
    let mut out = Vec::with_capacity(content_len + 4);
    out.push(0x02);
    push_length(&mut out, content_len)?;
    if needs_zero {
        out.push(0);
    }
    out.extend_from_slice(value);
    Ok(out)
}

fn push_length(out: &mut Vec<u8>, len: usize) -> Result<(), LtiError> {
    if len < 0x80 {
        out.push(len as u8);
    } else if len <= 0xFF {
        out.push(0x81);
        out.push(len as u8);
    } else if len <= 0xFFFF {
        out.push(0x82);
        out.push((len >> 8) as u8);
        out.push(len as u8);
    } else {
        return Err(LtiError::Key("RSA public key is too large".into()));
    }
    Ok(())
}
