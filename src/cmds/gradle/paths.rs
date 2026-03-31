use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Matches absolute paths that look like a repo checkout path.
    /// Captures: /Users/*/backend/, /home/*/backend/, /opt/*/backend/, etc.
    /// Also handles generic project roots: /Users/*/project-name/
    static ref ABSOLUTE_PATH_PREFIX: Regex =
        Regex::new(r"(?:file://)?/(?:Users|home|opt|var|tmp)/[^/]+/(?:[^/]+/)*").unwrap();
}

/// Strip absolute path prefixes to make paths repo-relative.
///
/// Converts paths like `/Users/developer/backend/app-payments/src/...`
/// to `app-payments/src/...`. Works for paths in error messages, file
/// references, and gradle task output.
pub fn normalize_paths(input: &str) -> String {
    // Find the repo root from the first absolute path in the output.
    // Heuristic: look for a path containing /src/ and extract everything before the module dir.
    let repo_root = detect_repo_root(input);

    match repo_root {
        Some(root) => input.replace(&root, ""),
        None => {
            // Fallback: just strip common prefixes with regex
            ABSOLUTE_PATH_PREFIX.replace_all(input, "").to_string()
        }
    }
}

/// Detect the repo root from paths in the output.
/// Looks for patterns like `/Users/dev/backend/app-foo/src/` and extracts
/// `/Users/dev/backend/` as the repo root.
fn detect_repo_root(input: &str) -> Option<String> {
    // Look for error lines with paths like:
    // e: /Users/developer/backend/app-payments/src/main/kotlin/...
    // w: /Users/developer/backend/app-payments/src/main/kotlin/...
    lazy_static! {
        // Non-greedy: capture up to but not including the module directory
        // e.g., /Users/dev/backend/ from /Users/dev/backend/app-payments/src/...
        static ref PATH_IN_ERROR: Regex =
            Regex::new(r"(?:e|w): (/(?:Users|home|opt|var|tmp)/[^/]+/(?:[^/]+/)*?)[^/]+/(?:src/|build/)")
                .unwrap();
        static ref PATH_ANYWHERE: Regex =
            Regex::new(r"(/(?:Users|home|opt|var|tmp)/[^/]+/(?:[^/]+/)*?)[^/]+/(?:src/|build/)").unwrap();
    }

    // Try error lines first (most reliable)
    if let Some(caps) = PATH_IN_ERROR.captures(input) {
        return Some(caps[1].to_string());
    }

    // Fallback to any path
    if let Some(caps) = PATH_ANYWHERE.captures(input) {
        return Some(caps[1].to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_error_path() {
        let input =
            "e: /Users/developer/backend/app-payments/src/main/kotlin/com/example/Foo.kt:42:5 Unresolved reference";
        let output = normalize_paths(input);
        assert_eq!(
            output,
            "e: app-payments/src/main/kotlin/com/example/Foo.kt:42:5 Unresolved reference"
        );
    }

    #[test]
    fn test_normalize_warning_path() {
        let input =
            "w: /Users/developer/backend/app-payments/src/main/kotlin/com/example/Foo.kt:8:1 Unused param";
        let output = normalize_paths(input);
        assert_eq!(
            output,
            "w: app-payments/src/main/kotlin/com/example/Foo.kt:8:1 Unused param"
        );
    }

    #[test]
    fn test_normalize_multiple_paths_same_root() {
        let input = "e: /Users/dev/backend/app-payments/src/Foo.kt:1 Error\ne: /Users/dev/backend/app-orders/src/Bar.kt:2 Error";
        let output = normalize_paths(input);
        assert!(output.contains("app-payments/src/Foo.kt:1"));
        assert!(output.contains("app-orders/src/Bar.kt:2"));
        assert!(!output.contains("/Users/dev/backend/"));
    }

    #[test]
    fn test_normalize_no_paths() {
        let input = "BUILD SUCCESSFUL in 12s";
        let output = normalize_paths(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_normalize_file_protocol() {
        let input = "file:///Users/dev/backend/app-payments/src/Foo.kt:1 Error";
        // The regex-based fallback should handle this
        let output = normalize_paths(input);
        assert!(!output.contains("/Users/dev/"));
    }
}
