use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Matches dependency tree lines and captures depth from leading pipe/space chars.
    /// Depth 0: starts with +--- or \---
    /// Depth 1: starts with |    +--- or |    \---
    /// Depth 2+: starts with more pipes
    static ref DEP_LINE: Regex = Regex::new(r"^([| ]*)[+\\]---").unwrap();
    /// Configuration header line
    static ref CONFIG_HEADER: Regex = Regex::new(r"^\S+.*- .*classpath|^\S+.*- .*dependencies").unwrap();
    /// Project header separator
    static ref PROJECT_HEADER: Regex = Regex::new(r"^-{4,}$").unwrap();
}

/// Returns true if the task name is a dependency listing task.
/// Case-insensitive via internal lowercasing.
pub fn matches_task(task_name: &str) -> bool {
    task_name.eq_ignore_ascii_case("dependencies")
}

/// Keep top-level (depth 0) and first child (depth 1) dependencies.
/// Drop deeper transitive dependencies.
const MAX_DEPTH: usize = 1;

/// Apply DEPS-specific filtering.
///
/// Truncates dependency tree to depth 0-1 only.
pub fn filter_deps(input: &str) -> String {
    let mut result = Vec::new();
    let mut truncated_count = 0;
    let mut in_tree = false;

    for line in input.lines() {
        let trimmed = line.trim();

        // Detect start of dependency tree
        if CONFIG_HEADER.is_match(trimmed) || PROJECT_HEADER.is_match(trimmed) {
            in_tree = true;
            if truncated_count > 0 {
                result.push(format!(
                    "    ... {} transitive dependencies truncated",
                    truncated_count
                ));
                truncated_count = 0;
            }
            result.push(line.to_string());
            continue;
        }

        // Detect project header
        if trimmed.starts_with("Project '") {
            result.push(line.to_string());
            continue;
        }

        if in_tree {
            if let Some(caps) = DEP_LINE.captures(line) {
                let prefix = &caps[1];
                // Count depth by number of pipe-space groups (each level is 5 chars: "|    ")
                let depth = prefix.len() / 5;
                if depth <= MAX_DEPTH {
                    if truncated_count > 0 {
                        result.push(format!(
                            "    ... {} transitive dependencies truncated",
                            truncated_count
                        ));
                        truncated_count = 0;
                    }
                    result.push(line.to_string());
                } else {
                    truncated_count += 1;
                }
                continue;
            }

            // Lines like "(*) - Dependency has been listed previously" or empty
            if trimmed.starts_with("(*)") || trimmed.is_empty() {
                if truncated_count > 0 {
                    result.push(format!(
                        "    ... {} transitive dependencies truncated",
                        truncated_count
                    ));
                    truncated_count = 0;
                }
                if trimmed.is_empty() {
                    in_tree = false;
                }
                continue;
            }
        }

        // Non-tree content (BUILD result, etc.)
        if truncated_count > 0 {
            result.push(format!(
                "    ... {} transitive dependencies truncated",
                truncated_count
            ));
            truncated_count = 0;
        }
        result.push(line.to_string());
    }

    if truncated_count > 0 {
        result.push(format!(
            "    ... {} transitive dependencies truncated",
            truncated_count
        ));
    }

    // Trim leading/trailing blank lines
    let start = result
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(0);
    let end = result
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map(|i| i + 1)
        .unwrap_or(result.len());
    result[start..end].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmds::gradle::global::apply_global_filters;
    use insta::assert_snapshot;

    fn count_tokens(text: &str) -> usize {
        text.split_whitespace().count()
    }

    #[test]
    fn test_matches_dependencies() {
        assert!(matches_task("dependencies"));
    }

    #[test]
    fn test_matches_dependencies_case_insensitive() {
        assert!(matches_task("Dependencies"));
        assert!(matches_task("DEPENDENCIES"));
    }

    #[test]
    fn test_no_match_partial() {
        assert!(!matches_task("dep"));
        assert!(!matches_task("dependency"));
    }

    #[test]
    fn test_no_match_test() {
        assert!(!matches_task("test"));
    }

    #[test]
    fn test_deps_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/deps_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_deps(&globally_filtered);
        assert_snapshot!(output);
    }

    #[test]
    fn test_deps_token_savings() {
        let input = include_str!("../../../tests/fixtures/gradle/deps_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_deps(&globally_filtered);
        let input_tokens = count_tokens(input);
        let output_tokens = count_tokens(&output);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 60.0,
            "Expected >=60% savings on deps, got {:.1}% (input={}, output={})",
            savings,
            input_tokens,
            output_tokens
        );
    }

    #[test]
    fn test_deps_keeps_depth_0_and_1() {
        let input = include_str!("../../../tests/fixtures/gradle/deps_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_deps(&globally_filtered);
        // Depth 0 dependencies should be kept
        assert!(output.contains("guava:31.1-jre"));
        assert!(output.contains("commons-lang3:3.12.0"));
        // Depth 1 dependencies should be kept
        assert!(output.contains("failureaccess:1.0.1"));
        // Deep transitive should be truncated
        assert!(output.contains("transitive dependencies truncated"));
    }
}
