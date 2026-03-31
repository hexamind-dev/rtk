use lazy_static::lazy_static;
use regex::Regex;
use std::collections::BTreeMap;

lazy_static! {
    /// Detekt violation line: file:line:col: RuleName - Message [detekt.RuleName]
    static ref VIOLATION: Regex = Regex::new(
        r"^(\S+:\d+:\d+): (\w+) - (.+?)(?:\s+\[detekt\.\w+\])?\s*$"
    ).unwrap();
    /// Detekt-specific noise
    static ref DETEKT_NOISE: Vec<Regex> = vec![
        Regex::new(r"^Loading baseline").unwrap(),
        Regex::new(r"^Comparing against baseline").unwrap(),
        Regex::new(r"^Progress: analyzing").unwrap(),
    ];
}

/// Returns true if the task name is a detekt task.
/// Case-insensitive via internal lowercasing.
pub fn matches_task(task_name: &str) -> bool {
    task_name.to_ascii_lowercase().starts_with("detekt")
}

/// Grouping threshold: >3 violations of the same rule get grouped.
const GROUPING_THRESHOLD: usize = 3;

/// A parsed detekt violation.
#[derive(Debug)]
struct Violation {
    location: String, // file:line:col
    rule: String,
    message: String,
}

/// Apply DETEKT-specific filtering.
///
/// Groups violations by rule when >3 of the same rule exist.
/// Inline format for <=3 violations of the same rule.
pub fn filter_detekt(input: &str) -> String {
    let mut violations: Vec<Violation> = Vec::new();
    let mut other_lines: Vec<String> = Vec::new();
    let mut violations_section_started = false;

    for line in input.lines() {
        let trimmed = line.trim();

        // Drop detekt-specific noise
        if DETEKT_NOISE.iter().any(|re| re.is_match(trimmed)) {
            continue;
        }

        // Try to parse as violation
        if let Some(caps) = VIOLATION.captures(trimmed) {
            violations_section_started = true;
            violations.push(Violation {
                location: caps[1].to_string(),
                rule: caps[2].to_string(),
                message: caps[3].to_string(),
            });
            continue;
        }

        // Detect the FAILED task line to mark where violations start
        if trimmed.contains("detekt FAILED") {
            violations_section_started = true;
            // Don't add this line — it's noise
            continue;
        }

        // Keep non-violation lines (BUILD FAILED, What went wrong, etc.)
        if violations_section_started || !trimmed.is_empty() {
            other_lines.push(line.to_string());
        }
    }

    // Format violations
    let formatted_violations = format_violations(&violations);

    // Combine: violations first, then other lines
    let mut result = Vec::new();
    if !formatted_violations.is_empty() {
        result.push(formatted_violations);
    }
    for line in &other_lines {
        result.push(line.clone());
    }

    // Trim leading/trailing blank lines
    let joined = result.join("\n");
    let trimmed: Vec<&str> = joined.lines().collect();
    let start = trimmed
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(0);
    let end = trimmed
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map(|i| i + 1)
        .unwrap_or(trimmed.len());
    trimmed[start..end].join("\n")
}

/// Format violations with grouping.
fn format_violations(violations: &[Violation]) -> String {
    if violations.is_empty() {
        return String::new();
    }

    // Group by rule
    let mut by_rule: BTreeMap<&str, Vec<&Violation>> = BTreeMap::new();
    for v in violations {
        by_rule.entry(&v.rule).or_default().push(v);
    }

    let mut result = Vec::new();

    for (rule, vols) in &by_rule {
        if vols.len() > GROUPING_THRESHOLD {
            // Grouped format
            let msg = &vols[0].message;
            result.push(format!("[{}] {} violations: {}", rule, vols.len(), msg));
            for v in vols {
                result.push(format!("  {}", v.location));
            }
        } else {
            // Inline format
            for v in vols {
                result.push(format!("{}: [{}] {}", v.location, v.rule, v.message));
            }
        }
    }

    result.join("\n")
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
    fn test_detekt_success_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/detekt_success_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_detekt(&globally_filtered);
        assert_snapshot!(output);
    }

    #[test]
    fn test_detekt_failure_few_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/detekt_failure_few_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_detekt(&globally_filtered);
        assert_snapshot!(output);
    }

    #[test]
    fn test_detekt_failure_many_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/detekt_failure_many_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_detekt(&globally_filtered);
        assert_snapshot!(output);
    }

    #[test]
    fn test_detekt_many_token_savings() {
        let input = include_str!("../../../tests/fixtures/gradle/detekt_failure_many_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_detekt(&globally_filtered);
        let input_tokens = count_tokens(input);
        let output_tokens = count_tokens(&output);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 60.0,
            "Expected >=60% savings on detekt many, got {:.1}% (input={}, output={})",
            savings,
            input_tokens,
            output_tokens
        );
    }

    #[test]
    fn test_grouping_threshold() {
        // 4 MagicNumber violations should be grouped
        let violations = vec![
            Violation {
                location: "a.kt:1:1".into(),
                rule: "MagicNumber".into(),
                message: "Magic number.".into(),
            },
            Violation {
                location: "b.kt:2:1".into(),
                rule: "MagicNumber".into(),
                message: "Magic number.".into(),
            },
            Violation {
                location: "c.kt:3:1".into(),
                rule: "MagicNumber".into(),
                message: "Magic number.".into(),
            },
            Violation {
                location: "d.kt:4:1".into(),
                rule: "MagicNumber".into(),
                message: "Magic number.".into(),
            },
        ];
        let output = format_violations(&violations);
        assert!(output.contains("[MagicNumber] 4 violations"));
        assert!(output.contains("  a.kt:1:1"));
    }

    #[test]
    fn test_below_threshold_inline() {
        // 2 violations should be inline
        let violations = vec![
            Violation {
                location: "a.kt:1:1".into(),
                rule: "MagicNumber".into(),
                message: "Magic number.".into(),
            },
            Violation {
                location: "b.kt:2:1".into(),
                rule: "MagicNumber".into(),
                message: "Magic number.".into(),
            },
        ];
        let output = format_violations(&violations);
        assert!(output.contains("a.kt:1:1: [MagicNumber] Magic number."));
        assert!(!output.contains("2 violations"));
    }

    #[test]
    fn test_detekt_noise_dropped() {
        let input = "Loading baseline from /path/to/baseline.xml\nComparing against baseline...\nProgress: analyzing module\na.kt:1:1: MagicNumber - Magic. [detekt.MagicNumber]\nBUILD FAILED in 4s";
        let output = filter_detekt(input);
        assert!(!output.contains("Loading baseline"));
        assert!(!output.contains("Progress:"));
        assert!(output.contains("MagicNumber"));
    }

    #[test]
    fn test_matches_detekt() {
        assert!(matches_task("detekt"));
    }

    #[test]
    fn test_matches_detekt_main() {
        assert!(matches_task("detektMain"));
    }

    #[test]
    fn test_matches_detekt_test() {
        assert!(matches_task("detektTest"));
    }

    #[test]
    fn test_matches_detekt_case_insensitive() {
        assert!(matches_task("Detekt"));
        assert!(matches_task("DetektMain"));
    }

    #[test]
    fn test_no_match_test() {
        assert!(!matches_task("test"));
    }

    #[test]
    fn test_no_match_compile() {
        assert!(!matches_task("compileKotlin"));
    }
}
