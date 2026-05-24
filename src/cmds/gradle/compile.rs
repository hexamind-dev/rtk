use lazy_static::lazy_static;
use regex::Regex;

use super::paths::normalize_paths;

lazy_static! {
    /// COMPILE-specific noise patterns (dropped in addition to global filters).
    static ref COMPILE_NOISE: Vec<Regex> = vec![
        // kapt/KSP annotation processing noise
        Regex::new(r"^(Annotation processing|kapt|ksp|KSP)").unwrap(),
        Regex::new(r"kaptGenerateStubs").unwrap(),
        // Incremental compilation messages
        Regex::new(r"^(Incremental compilation|Performing full compilation|Full recompilation)").unwrap(),
        // Resource processing
        Regex::new(r"^Resource processing completed").unwrap(),
    ];
}

/// Returns true if the task name is a compile task.
/// Matches any source set: compileKotlin, compileTestKotlin, compileIntegrationTestJava, etc.
/// Expects lowercase input from detect_task_type; output-based detection passes properly-cased names.
pub fn matches_task(task_name: &str) -> bool {
    let t = task_name.to_ascii_lowercase();
    (t.starts_with("compile") && (t.ends_with("kotlin") || t.ends_with("java")))
        || t.ends_with("classes")
}

/// Apply COMPILE-specific filtering on top of globally-filtered output.
///
/// Drops kapt/KSP noise, incremental compilation info, resource processing.
/// Normalizes absolute paths to repo-relative.
pub fn filter_compile(input: &str) -> String {
    let mut result = Vec::new();
    let mut task_names: Vec<String> = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();

        // Track executed tasks for ✓ summary
        if trimmed.starts_with("> Task ") && trimmed.ends_with("compileKotlin") {
            if let Some(task) = trimmed.strip_prefix("> Task ") {
                task_names.push(task.to_string());
            }
            continue;
        }

        // Drop COMPILE-specific noise
        if COMPILE_NOISE.iter().any(|re| re.is_match(trimmed)) {
            continue;
        }

        result.push(line.to_string());
    }

    let mut output = normalize_paths(&result.join("\n"));

    // Add task ✓ summary after BUILD line
    if !task_names.is_empty() {
        let summary: Vec<String> = task_names.iter().map(|t| format!("{} ✓", t)).collect();
        // Insert after BUILD SUCCESSFUL/FAILED line
        if let Some(pos) = output.find("BUILD SUCCESSFUL") {
            if let Some(end) = output[pos..].find('\n') {
                let insert_pos = pos + end;
                let tasks_str = summary.join("\n");
                output.insert_str(insert_pos, &format!("\n{}", tasks_str));
            }
        }
    }

    // Trim leading/trailing blank lines
    let trimmed: Vec<&str> = output.lines().collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmds::gradle::global::apply_global_filters;
    use insta::assert_snapshot;

    fn count_tokens(text: &str) -> usize {
        text.split_whitespace().count()
    }

    // --- matches_task tests ---

    #[test]
    fn test_matches_compile_kotlin() {
        assert!(matches_task("compileKotlin"));
    }

    #[test]
    fn test_matches_compile_test_kotlin() {
        assert!(matches_task("compileTestKotlin"));
    }

    #[test]
    fn test_matches_compile_integration_test_kotlin() {
        assert!(matches_task("compileIntegrationTestKotlin"));
    }

    #[test]
    fn test_matches_compile_java() {
        assert!(matches_task("compileJava"));
    }

    #[test]
    fn test_matches_classes() {
        assert!(matches_task("testClasses"));
        assert!(matches_task("integrationTestClasses"));
    }

    #[test]
    fn test_matches_android_variant_compile() {
        assert!(matches_task("compileDebugKotlin"));
        assert!(matches_task("compileReleaseJava"));
    }

    #[test]
    fn test_no_match_test() {
        assert!(!matches_task("test"));
    }

    #[test]
    fn test_no_match_detekt() {
        assert!(!matches_task("detekt"));
    }

    // --- filter tests ---

    #[test]
    fn test_compile_success_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/compile_success_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_compile(&globally_filtered);
        assert_snapshot!(output);
    }

    #[test]
    fn test_compile_failure_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/compile_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_compile(&globally_filtered);
        assert_snapshot!(output);
    }

    #[test]
    fn test_compile_failure_token_savings() {
        let input = include_str!("../../../tests/fixtures/gradle/compile_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_compile(&globally_filtered);
        let input_tokens = count_tokens(input);
        let output_tokens = count_tokens(&output);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 60.0,
            "Expected >=60% savings on compile failure, got {:.1}% (input={}, output={})",
            savings,
            input_tokens,
            output_tokens
        );
    }

    #[test]
    fn test_compile_preserves_error_lines() {
        let input = include_str!("../../../tests/fixtures/gradle/compile_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_compile(&globally_filtered);
        assert!(output.contains("Unresolved reference: Bar"));
        assert!(output.contains("Type mismatch: expected String, got Int"));
    }

    #[test]
    fn test_compile_normalizes_paths() {
        let input = include_str!("../../../tests/fixtures/gradle/compile_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_compile(&globally_filtered);
        assert!(
            !output.contains("/Users/developer/"),
            "Absolute paths should be normalized"
        );
        assert!(
            output.contains("app-payments/src/main/kotlin/com/example/payments/Foo.kt"),
            "Path should be relative"
        );
    }

    #[test]
    fn test_compile_drops_kapt_noise() {
        let input = "Annotation processing was successful. Generated 42 files.\nkaptGenerateStubsKotlin completed\ne: Foo.kt:1 Error\nBUILD FAILED in 5s";
        let output = filter_compile(input);
        assert!(!output.contains("Annotation processing"));
        assert!(!output.contains("kaptGenerateStubs"));
        assert!(output.contains("Error"));
    }

    #[test]
    fn test_compile_drops_incremental() {
        let input = "Incremental compilation was started but abandoned\nPerforming full compilation\ne: Foo.kt:1 Error\nBUILD FAILED in 5s";
        let output = filter_compile(input);
        assert!(!output.contains("Incremental compilation"));
        assert!(!output.contains("Performing full"));
        assert!(output.contains("Error"));
    }
}
