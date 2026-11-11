use super::{
    CaptionError, Cue, MAX_CUE_CHARS, MAX_CUES, MAX_INPUT_BYTES, cue_text_at, parse_webvtt,
};

fn parse(input: &str) -> Vec<Cue> {
    parse_webvtt(input.as_bytes()).expect("valid captions")
}

fn text(input: &str) -> String {
    parse(input)
        .first()
        .map(|cue| cue.text.clone())
        .unwrap_or_default()
}

const HEADER: &str = "WEBVTT\n\n";

#[test]
fn header_is_required() {
    assert_eq!(parse_webvtt(b""), Err(CaptionError::MissingHeader));
    assert_eq!(
        parse_webvtt(b"00:00:01.000 --> 00:00:02.000\nHi\n"),
        Err(CaptionError::MissingHeader)
    );
}

#[test]
fn only_a_header_has_no_cues() {
    assert_eq!(parse_webvtt(b"WEBVTT\n"), Ok(Vec::new()));
}

#[test]
fn optional_bom_and_crlf_are_accepted() {
    let input = "\u{feff}WEBVTT\r\n\r\n00:00:01.000 --> 00:00:02.000\r\nHi\r\n";
    let cues = parse(input);
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].text, "Hi");
}

#[test]
fn both_timestamp_formats_parse() {
    let cues = parse(&format!(
        "{HEADER}00:01.500 --> 00:02.250\nshort\n\n01:00:03.000 --> 01:00:04.000\nlong\n"
    ));
    assert_eq!(cues[0].start_ms, 1_500);
    assert_eq!(cues[0].end_ms, 2_250);
    assert_eq!(cues[1].start_ms, 3_603_000);
}

#[test]
fn cue_identifiers_and_settings_are_handled() {
    let cues = parse(&format!(
        "{HEADER}intro\n00:00:01.000 --> 00:00:02.000 align:center line:0\nHi\n"
    ));
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].start_ms, 1_000);
    assert_eq!(cues[0].text, "Hi");
}

#[test]
fn note_style_and_region_blocks_are_skipped() {
    let cues = parse(&format!(
        "{HEADER}NOTE this is a comment\nmore note\n\nSTYLE\n::cue {{ color: red }}\n\nREGION\nid:r1\n\n00:00:01.000 --> 00:00:02.000\nHi\n"
    ));
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].text, "Hi");
}

#[test]
fn malformed_cues_are_skipped_not_fatal() {
    let cues = parse(&format!(
        "{HEADER}not a cue at all\n\n00:00:01.000 --> nope\nbad\n\n00:00:02.000 --> 00:00:03.000\nGood\n"
    ));
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].text, "Good");
}

#[test]
fn cue_whose_end_is_not_after_start_is_skipped() {
    let cues = parse(&format!(
        "{HEADER}00:00:05.000 --> 00:00:05.000\nempty\n\n00:00:06.000 --> 00:00:04.000\nbackwards\n\n00:00:07.000 --> 00:00:08.000\nGood\n"
    ));
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].text, "Good");
}

#[test]
fn tags_are_stripped_and_a_speaker_is_kept() {
    assert_eq!(
        text(&format!(
            "{HEADER}00:00:01.000 --> 00:00:02.000\n<v Narrator><i>Hello</i> <c.x>there</c>\n"
        )),
        "Narrator: Hello there"
    );
}

#[test]
fn timestamp_tags_are_stripped() {
    assert_eq!(
        text(&format!(
            "{HEADER}00:00:01.000 --> 00:00:02.000\nbefore <00:00:01.500>after\n"
        )),
        "before after"
    );
}

#[test]
fn entities_are_decoded() {
    assert_eq!(
        text(&format!(
            "{HEADER}00:00:01.000 --> 00:00:02.000\nA &amp; B &lt; C &gt; D&nbsp;E&lrm;F&rlm;G\n"
        )),
        "A & B < C > D\u{a0}E\u{200e}F\u{200f}G"
    );
}

#[test]
fn multi_line_text_keeps_its_line_breaks() {
    assert_eq!(
        text(&format!(
            "{HEADER}00:00:01.000 --> 00:00:02.000\none\ntwo\nthree\n"
        )),
        "one\ntwo\nthree"
    );
}

#[test]
fn cues_are_sorted_by_start() {
    let cues = parse(&format!(
        "{HEADER}00:00:05.000 --> 00:00:06.000\nlater\n\n00:00:01.000 --> 00:00:02.000\nfirst\n"
    ));
    assert_eq!(cues[0].text, "first");
    assert_eq!(cues[1].text, "later");
}

#[test]
fn cue_text_at_joins_active_cues_and_respects_half_open_ranges() {
    let cues = parse(&format!(
        "{HEADER}00:00:01.000 --> 00:00:02.000\none\n\n00:00:01.500 --> 00:00:03.000\ntwo\n"
    ));
    assert_eq!(cue_text_at(&cues, 500), None);
    assert_eq!(cue_text_at(&cues, 1_000), Some("one".to_owned()));
    assert_eq!(cue_text_at(&cues, 1_500), Some("one\ntwo".to_owned()));
    assert_eq!(cue_text_at(&cues, 2_000), Some("two".to_owned()));
    assert_eq!(cue_text_at(&cues, 3_000), None);
}

#[test]
fn oversized_input_is_an_error() {
    let bytes = vec![b'a'; MAX_INPUT_BYTES + 1];
    assert_eq!(
        parse_webvtt(&bytes),
        Err(CaptionError::TooLarge {
            size: MAX_INPUT_BYTES + 1
        })
    );
}

#[test]
fn cue_count_is_capped() {
    let mut input = String::from(HEADER);
    for i in 0..(MAX_CUES + 50) {
        input.push_str(&format!(
            "00:00:{:02}.000 --> 00:00:{:02}.500\ncue {i}\n\n",
            i % 60,
            i % 60
        ));
    }
    let cues = parse(&input);
    assert_eq!(cues.len(), MAX_CUES);
}

#[test]
fn cue_text_is_cut_on_a_character_boundary() {
    let long = "é".repeat(MAX_CUE_CHARS + 100);
    let input = format!("{HEADER}00:00:01.000 --> 00:00:02.000\n{long}\n");
    let cue = parse(&input).into_iter().next().expect("cue");
    assert_eq!(cue.text.chars().count(), MAX_CUE_CHARS);
    assert!(cue.text.chars().all(|c| c == 'é'));
}

#[test]
fn hostile_bytes_do_not_panic() {
    let random: Vec<u8> = (0..4096u32)
        .map(|i| (i.wrapping_mul(37) % 251) as u8)
        .collect();
    let _ = parse_webvtt(&random);
    let _ = parse_webvtt(b"WEBVTT\n\n<<<<>>>>\n\n00:00:01.000 --> 00:00:02.000\n<v open\n");
    let _ = parse_webvtt(b"WEBVTT\n\n99999999:99:99.999 --> 99999999:99:99.999\nhuge\n");
}

#[test]
fn huge_but_valid_timestamps_do_not_overflow() {
    let cues = parse(&format!(
        "{HEADER}99999999:00:00.000 --> 99999999:00:01.000\nfar\n"
    ));
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].start_ms, 99_999_999 * 3_600_000);
}

#[test]
fn unterminated_tags_stay_as_text() {
    assert_eq!(
        text(&format!(
            "{HEADER}00:00:01.000 --> 00:00:02.000\nbefore <i open\n"
        )),
        "before <i open"
    );
}

#[test]
fn invalid_utf8_is_lossy() {
    let mut input = b"WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHi ".to_vec();
    input.push(0xFF);
    input.extend_from_slice(b"\n");
    let cues = parse_webvtt(&input).expect("lossy parse");
    assert_eq!(cues.len(), 1);
    assert!(cues[0].text.starts_with("Hi "));
    assert!(cues[0].text.contains('\u{fffd}'));
}
