//! The import failure type (contract §4): one variant per failure class,
//! each carrying the exact HTTP status, code, and verbatim human message.
//!
//! The wording is spec, not copy: tests assert these strings byte-for-byte
//! (FR-4, FR-5, SC-2). A wording change is a spec change — it goes back
//! through the spec, never straight into code.
//!
//! Pure module: no framework imports. The HTTP layer maps [`ImportError::status`]
//! onto `axum::http::StatusCode` and the fields onto E3's standard error
//! envelope (`src/auth/error.rs` shape).

/// How an import attempt failed. Carries no dynamic data: every class has a
/// fixed status, code, and message per `contracts/pb-export.md` §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    /// Body over 1 MiB (class: size).
    PayloadTooLarge,
    /// JSON nesting deeper than 64 (class: depth).
    PayloadTooDeep,
    /// Body is not valid JSON (class a).
    InvalidJson,
    /// Valid JSON, but not a Pathbuilder export — the required shape
    /// (`contracts/pb-export.md` §2) is violated (class b).
    NotPathbuilder,
}

impl ImportError {
    /// The HTTP status this failure answers with (contract §4).
    #[must_use]
    pub fn status(self) -> u16 {
        match self {
            ImportError::PayloadTooLarge => 413,
            ImportError::PayloadTooDeep
            | ImportError::InvalidJson
            | ImportError::NotPathbuilder => 400,
        }
    }

    /// The machine-readable failure code (contract §4).
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            ImportError::PayloadTooLarge => "payload-too-large",
            ImportError::PayloadTooDeep => "payload-too-deep",
            ImportError::InvalidJson => "invalid-json",
            ImportError::NotPathbuilder => "not-pathbuilder",
        }
    }

    /// The verbatim human message (contract §4 — spec wording, asserted in
    /// tests byte-for-byte).
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            ImportError::PayloadTooLarge => {
                "That's too large to be a character export (limit 1 MB). \
                 Make sure you exported a single character."
            }
            ImportError::PayloadTooDeep => {
                "That JSON is nested too deeply to be a character export."
            }
            ImportError::InvalidJson => {
                "That isn't valid JSON. Copy the whole export from Pathbuilder \
                 (Share → Export JSON) and paste it again."
            }
            ImportError::NotPathbuilder => {
                "That's valid JSON, but it doesn't look like a Pathbuilder export \
                 — the character sheet fields (name, level, abilities, \
                 proficiencies) are missing."
            }
        }
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.code(), self.message())
    }
}

impl std::error::Error for ImportError {}
