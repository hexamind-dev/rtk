use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    /// Proto-specific noise patterns
    static ref PROTO_NOISE: Vec<Regex> = vec![
        Regex::new(r"^> Task.*extract.*Proto").unwrap(),
    ];
    /// Proto error lines (keep these)
    static ref PROTO_ERROR: Regex = Regex::new(r"^e: ").unwrap();
}

/// Returns true if the task name is a proto generation task.
/// Case-insensitive via internal lowercasing.
pub fn matches_task(task_name: &str) -> bool {
    let t = task_name.to_ascii_lowercase();
    t == "buildprotos" || t == "generateprotos" || t.contains("proto")
}

/// Apply PROTO-specific filtering.
///
/// Keeps error lines, BUILD result, What went wrong.
/// Drops proto extraction noise.
pub fn filter_proto(input: &str) -> String {
    let mut result = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();

        // Drop proto-specific noise
        if PROTO_NOISE.iter().any(|re| re.is_match(trimmed)) {
            continue;
        }

        result.push(line.to_string());
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

    #[test]
    fn test_matches_build_protos() {
        assert!(matches_task("buildProtos"));
    }

    #[test]
    fn test_matches_generate_protos() {
        assert!(matches_task("generateProtos"));
    }

    #[test]
    fn test_matches_contains_proto() {
        assert!(matches_task("extractProto"));
        assert!(matches_task("generateTestProto"));
    }

    #[test]
    fn test_matches_proto_case_insensitive() {
        assert!(matches_task("BuildProtos"));
        assert!(matches_task("GenerateProtos"));
    }

    #[test]
    fn test_no_match_test() {
        assert!(!matches_task("test"));
    }

    #[test]
    fn test_no_match_compile() {
        assert!(!matches_task("compileKotlin"));
    }

    #[test]
    fn test_proto_failure_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/proto_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_proto(&globally_filtered);
        assert_snapshot!(output);
    }

    #[test]
    fn test_proto_preserves_errors() {
        let input = include_str!("../../../tests/fixtures/gradle/proto_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_proto(&globally_filtered);
        assert!(output.contains("Field number 5 has already been used"));
        assert!(output.contains("is already defined"));
    }
}
