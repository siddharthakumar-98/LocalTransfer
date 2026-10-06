//! File-name helpers: Finder-style clash names and path-component checks.
//!
//! Used by the mock peer now; the real filesystem layer (`lt-fs`, M2) will
//! enforce the same rules (ROADMAP.md §3).

use crate::backend::BackendError;

/// Longest path accepted from a peer, in components.
pub const MAX_DEPTH: usize = 64;

/// Longest single component, in bytes (the APFS limit).
pub const MAX_COMPONENT_LEN: usize = 255;

/// The Finder-style name for the `n`-th clash: `x.zip` → `x (1).zip`,
/// `.env` → `.env (1)`, `notes` → `notes (1)`. `n == 0` returns `name` unchanged.
#[must_use]
pub fn numbered_name(name: &str, n: u32) -> String {
    if n == 0 {
        return name.to_owned();
    }
    match name.rfind('.') {
        // A leading dot (".env") or a trailing dot ("x.") is not an extension.
        Some(i) if i > 0 && i + 1 < name.len() => format!("{} ({n}){}", &name[..i], &name[i..]),
        _ => format!("{name} ({n})"),
    }
}

/// Checks one path component received from a peer.
///
/// # Errors
/// [`BackendError::InvalidPath`] for empty, `.`, `..`, over-long components,
/// or components containing `/` or NUL.
pub fn validate_component(component: &str) -> Result<(), BackendError> {
    let bad = component.is_empty()
        || component == "."
        || component == ".."
        || component.len() > MAX_COMPONENT_LEN
        || component.contains(['/', '\0']);
    if bad {
        Err(BackendError::InvalidPath)
    } else {
        Ok(())
    }
}

/// Checks a whole component list (see [`validate_component`]).
///
/// # Errors
/// [`BackendError::InvalidPath`] if any component is invalid or the path is
/// deeper than [`MAX_DEPTH`].
pub fn validate_components(components: &[String]) -> Result<(), BackendError> {
    if components.len() > MAX_DEPTH {
        return Err(BackendError::InvalidPath);
    }
    components.iter().try_for_each(|c| validate_component(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_names_follow_finder() {
        assert_eq!(numbered_name("x.zip", 0), "x.zip");
        assert_eq!(numbered_name("x.zip", 1), "x (1).zip");
        assert_eq!(numbered_name("x.zip", 2), "x (2).zip");
        assert_eq!(numbered_name("archive.tar.gz", 1), "archive.tar (1).gz");
        assert_eq!(numbered_name(".env", 1), ".env (1)");
        assert_eq!(numbered_name("notes", 3), "notes (3)");
        assert_eq!(numbered_name("odd.", 1), "odd. (1)");
    }

    #[test]
    fn hostile_components_are_rejected() {
        for bad in ["", ".", "..", "a/b", "/", "nul\0byte", &"x".repeat(256)] {
            assert_eq!(
                validate_component(bad),
                Err(BackendError::InvalidPath),
                "{bad:?}"
            );
        }
        for good in ["a", "x.zip", "..hidden", "with space", "Ünïcödé", "..."] {
            assert_eq!(validate_component(good), Ok(()), "{good:?}");
        }
        let deep = vec!["d".to_owned(); MAX_DEPTH + 1];
        assert_eq!(validate_components(&deep), Err(BackendError::InvalidPath));
    }
}
