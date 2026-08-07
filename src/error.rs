//! Upstream error classification.
//!
//! Host-compilable: this returns a `(kind, message)` pair rather than an
//! `ActError` precisely so the mapping table — the fiddliest part of the
//! component — is unit-testable without wasm. `lib.rs` turns the pair into an
//! `ActError` on the wasm side.

use act_types::constants::{ERR_CAPABILITY_DENIED, ERR_INTERNAL, ERR_INVALID_ARGS, ERR_NOT_FOUND};
use anydoc::ConvertError;

/// A fixed safety limit fired: a decompression bomb, runaway nesting, or a
/// pathological expansion was refused.
///
/// This gets its own namespaced kind rather than `std:invalid-args` because
/// the input is not malformed arguments — it is a document that crossed a
/// ceiling, which is the sandbox working as designed. Callers should be able
/// to branch on it (quarantine the file, alert, retry elsewhere) without
/// string-matching a message. ACT-SPEC permits custom namespaced kinds and
/// requires hosts not to reject unrecognised ones.
pub const ERR_RESOURCE_LIMIT: &str = "anydoc:resource-limit";

/// Map an upstream conversion failure onto an ACT error kind and message.
///
/// Branches on the variant rather than on `code()` so the compiler flags any
/// variant we stop handling — but note `ConvertError` is `#[non_exhaustive]`,
/// so the wildcard arm is mandatory and cannot be removed.
pub fn classify(e: &ConvertError) -> (&'static str, String) {
    match e {
        ConvertError::Unsupported(what) => (ERR_INVALID_ARGS, format!("Unsupported input: {what}")),
        ConvertError::Malformed {
            part: Some(part),
            detail,
        } => (
            ERR_INVALID_ARGS,
            format!("Malformed document ({part}): {detail}"),
        ),
        ConvertError::Malformed { part: None, detail } => {
            (ERR_INVALID_ARGS, format!("Malformed document: {detail}"))
        }
        ConvertError::Encrypted => (
            ERR_INVALID_ARGS,
            "Document is encrypted or password-protected — anydoc cannot decrypt it".to_string(),
        ),
        ConvertError::ResourceLimit { limit, detail } => (
            ERR_RESOURCE_LIMIT,
            format!("Safety limit `{limit}` exceeded: {detail}"),
        ),
        ConvertError::MissingPart { part } => {
            (ERR_INVALID_ARGS, format!("Missing required part: {part}"))
        }
        ConvertError::Io(e) => (ERR_INTERNAL, format!("IO error: {e}")),
        // ConvertError is #[non_exhaustive]: a future variant must still map
        // to something sane rather than break the build.
        other => (ERR_INVALID_ARGS, other.to_string()),
    }
}

/// Triage a filesystem read failure for the `path` source.
///
/// A denied read is a capability problem, not an internal fault, and the
/// message names the grant that would fix it — an agent reading the error
/// should not have to guess.
pub fn classify_io(e: &std::io::Error, path: &str) -> (&'static str, String) {
    match e.kind() {
        std::io::ErrorKind::NotFound => (ERR_NOT_FOUND, format!("File not found: {path}")),
        std::io::ErrorKind::PermissionDenied => (
            ERR_CAPABILITY_DENIED,
            format!(
                "Permission denied: {path} — grant wasi:filesystem read access covering this path"
            ),
        ),
        _ => (ERR_INTERNAL, format!("Cannot read {path}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anydoc::ConvertError;

    #[test]
    fn malformed_input_is_the_callers_fault() {
        let (kind, msg) = classify(&ConvertError::Malformed {
            part: Some("word/document.xml".into()),
            detail: "unexpected end of stream".into(),
        });
        assert_eq!(kind, "std:invalid-args");
        assert!(
            msg.contains("word/document.xml"),
            "part must be named: {msg}"
        );
        assert!(msg.contains("unexpected end of stream"));
    }

    #[test]
    fn malformed_without_a_part_still_reads_cleanly() {
        let (kind, msg) = classify(&ConvertError::Malformed {
            part: None,
            detail: "bad record".into(),
        });
        assert_eq!(kind, "std:invalid-args");
        assert!(msg.contains("bad record"));
        assert!(
            !msg.contains("None"),
            "must not leak a debug-formatted Option: {msg}"
        );
    }

    #[test]
    fn encrypted_says_plainly_that_we_cannot_decrypt() {
        let (kind, msg) = classify(&ConvertError::Encrypted);
        assert_eq!(kind, "std:invalid-args");
        assert!(msg.to_lowercase().contains("encrypted"));
        assert!(msg.to_lowercase().contains("cannot decrypt"));
    }

    /// The whole point of the component. A safety limit firing is the ceiling
    /// doing its job, not malformed arguments, and a caller must be able to
    /// branch on it without string-matching a message.
    #[test]
    fn a_safety_limit_gets_its_own_kind_and_names_the_limit() {
        let (kind, msg) = classify(&ConvertError::ResourceLimit {
            limit: "max_entry_bytes",
            detail: "entry expands to 700 MiB".into(),
        });
        assert_eq!(kind, ERR_RESOURCE_LIMIT);
        assert_eq!(kind, "anydoc:resource-limit");
        assert!(
            msg.contains("max_entry_bytes"),
            "must name the limit: {msg}"
        );
        assert!(msg.contains("700 MiB"));
    }

    #[test]
    fn unsupported_and_missing_part_are_invalid_args() {
        let (kind, _) = classify(&ConvertError::Unsupported("no idea".into()));
        assert_eq!(kind, "std:invalid-args");
        let (kind, msg) = classify(&ConvertError::MissingPart {
            part: "mimetype".into(),
        });
        assert_eq!(kind, "std:invalid-args");
        assert!(msg.contains("mimetype"));
    }

    #[test]
    fn a_missing_file_is_not_found_not_internal() {
        let e = std::io::Error::from(std::io::ErrorKind::NotFound);
        let (kind, msg) = classify_io(&e, "/data/report.docx");
        assert_eq!(kind, "std:not-found");
        assert!(msg.contains("/data/report.docx"));
    }

    /// A denied read is a capability problem, and the message must tell the
    /// caller which grant would fix it.
    #[test]
    fn a_denied_read_names_the_grant_that_would_fix_it() {
        let e = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let (kind, msg) = classify_io(&e, "/data/report.docx");
        assert_eq!(kind, "std:capability-denied");
        assert!(
            msg.contains("wasi:filesystem"),
            "must name the capability: {msg}"
        );
    }

    #[test]
    fn any_other_io_failure_is_internal() {
        let e = std::io::Error::from(std::io::ErrorKind::UnexpectedEof);
        let (kind, _) = classify_io(&e, "/data/x");
        assert_eq!(kind, "std:internal");
    }
}
