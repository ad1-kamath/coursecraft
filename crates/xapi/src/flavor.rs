//! The two xAPI launch flavors CourseCraft understands.

/// An xAPI package flavor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// The cmi5 profile, launched with a `fetch` URL.
    Cmi5,
    /// Plain xAPI / Tin Can launched with an `auth` header value.
    Xapi,
}

impl Flavor {
    /// The stable identifier written into `coursecraft-package.json`.
    #[must_use]
    pub fn as_id(self) -> &'static str {
        match self {
            Self::Cmi5 => "cmi5",
            Self::Xapi => "xapi",
        }
    }

    /// Parse the stable identifier from `coursecraft-package.json`.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "cmi5" => Some(Self::Cmi5),
            "xapi" => Some(Self::Xapi),
            _ => None,
        }
    }
}
