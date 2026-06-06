/// Returns true if the task name is a project health task.
/// Case-insensitive via internal lowercasing.
pub fn matches_task(task_name: &str) -> bool {
    task_name.to_ascii_lowercase().starts_with("projecthealth")
}

/// HEALTH passthrough — health report content is already concise.
/// Just applies global filters (already done by caller) and passes through.
pub fn filter_health(input: &str) -> String {
    // Health output is already useful — just trim blank lines
    let lines: Vec<&str> = input.lines().collect();
    let start = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
    let end = lines
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map(|i| i + 1)
        .unwrap_or(lines.len());
    lines[start..end].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmds::gradle::global::apply_global_filters;
    use insta::assert_snapshot;

    #[test]
    fn test_matches_project_health() {
        assert!(matches_task("projectHealth"));
    }

    #[test]
    fn test_matches_project_health_case_insensitive() {
        assert!(matches_task("ProjectHealth"));
        assert!(matches_task("PROJECTHEALTH"));
    }

    #[test]
    fn test_no_match_health_alone() {
        assert!(!matches_task("health"));
    }

    #[test]
    fn test_no_match_test() {
        assert!(!matches_task("test"));
    }

    #[test]
    fn test_health_failure_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/health_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_health(&globally_filtered);
        assert_snapshot!(output);
    }

    #[test]
    fn test_health_preserves_advice() {
        let input = include_str!("../../../tests/fixtures/gradle/health_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_health(&globally_filtered);
        assert!(output.contains("Unused dependencies"));
        assert!(output.contains("Used transitive dependencies"));
        assert!(output.contains("com.google.guava:guava"));
    }
}
