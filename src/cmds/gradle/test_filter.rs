use lazy_static::lazy_static;
use regex::Regex;

use super::paths::normalize_paths;
use crate::core::config;

/// Returns true if the task name is any kind of test task (unit, integration, component, Android).
/// Case-insensitive: callers may pass lowercase (CLI args) or original casing (output detection).
pub fn matches_task(task_name: &str) -> bool {
    let t = task_name.to_ascii_lowercase();
    // Unit test tasks
    t == "test"
        || (t.starts_with("test") && t.ends_with("unittest"))
        // Integration/component test tasks
        || t == "integrationtest"
        || t == "componenttest"
        // Android instrumented tests
        || t.contains("androidtest")
        || t.starts_with("connected")
}

/// Built-in framework prefixes that are always dropped from stack traces.
/// These are JDK/Kotlin stdlib and internal packages — universally noise.
const BUILTIN_FRAMEWORK_PREFIXES: &[&str] = &[
    "java.",
    "kotlin.",
    "kotlinx.coroutines.",
    "sun.",
    "javax.",
    "jdk.",
    "jakarta.",
    "android.",
    "androidx.",
    "dalvik.",
    "com.android.internal.",
];

lazy_static! {
    /// Patterns for passing test lines
    static ref PASSING_TEST: Regex = Regex::new(r"^\S+.*\bPASSED\s*$").unwrap();
    /// JUnit discovery/execution noise
    static ref JUNIT_NOISE: Regex = Regex::new(
        r"^(Discovering tests|Starting test execution|Gradle Test Executor)"
    ).unwrap();
    /// Test worker STANDARD_OUT/STANDARD_ERR headers + content
    static ref STANDARD_STREAM: Regex = Regex::new(
        r"^\S+.*\bSTANDARD_(OUT|ERR)\s*$"
    ).unwrap();

    // Stack trace frame classification patterns
    static ref USER_CODE: Regex = Regex::new(r"^\s+at com\.example\.").unwrap();
    static ref ASSERTION_FRAME: Regex = Regex::new(
        r"^\s+at (org\.junit\.Assert|org\.assertj\.core\.api\.|kotlin\.test|org\.junit\.jupiter\.api\.Assertion|org\.opentest4j\.)"
    ).unwrap();
    static ref CAUSED_BY: Regex = Regex::new(r"^\s+(Caused by:|Suppressed:)").unwrap();
    static ref FRAME_LINE: Regex = Regex::new(r"^\s+at ").unwrap();
    static ref MORE_LINE: Regex = Regex::new(r"^\s+\.\.\. \d+ more").unwrap();
}

/// Build a framework frame regex from built-in prefixes + configurable drop_frame_packages.
fn build_framework_regex(drop_frame_packages: &[String]) -> Regex {
    let mut prefixes: Vec<String> = BUILTIN_FRAMEWORK_PREFIXES
        .iter()
        .map(|p| regex::escape(p))
        .collect();

    for pkg in drop_frame_packages {
        prefixes.push(regex::escape(pkg));
    }

    // Also match any package containing ".internal." (catches org.gradle.internal, etc.)
    let pattern = format!(r"^\s+at ({}|\S+\.internal\.)", prefixes.join("|"));
    Regex::new(&pattern).unwrap_or_else(|_| {
        // Fallback: just built-in prefixes if user config is invalid
        let builtin: Vec<String> = BUILTIN_FRAMEWORK_PREFIXES
            .iter()
            .map(|p| regex::escape(p))
            .collect();
        Regex::new(&format!(r"^\s+at ({}|\S+\.internal\.)", builtin.join("|"))).unwrap()
    })
}

/// Apply TEST-specific filtering on top of globally-filtered output.
pub fn filter_test(input: &str) -> String {
    let (user_packages, drop_frame_packages) = load_config();
    filter_test_with_config(input, &user_packages, &drop_frame_packages)
}

/// Load user_packages and drop_frame_packages from config.
fn load_config() -> (Vec<String>, Vec<String>) {
    match config::Config::load() {
        Ok(config) => (
            config.gradle.user_packages,
            config.gradle.drop_frame_packages,
        ),
        Err(_) => (Vec::new(), config::default_drop_frame_packages()),
    }
}

/// Core test filter logic, testable with explicit config.
#[cfg(test)]
pub fn filter_test_with_packages(input: &str, user_packages: &[String]) -> String {
    let drop_frame_packages = config::default_drop_frame_packages();
    filter_test_with_config(input, user_packages, &drop_frame_packages)
}

/// Core test filter logic with full config.
fn filter_test_with_config(
    input: &str,
    user_packages: &[String],
    drop_frame_packages: &[String],
) -> String {
    let framework_re = build_framework_regex(drop_frame_packages);
    let mut result = Vec::new();
    let mut in_standard_stream = false;
    let lines: Vec<&str> = input.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Drop passing test lines
        if PASSING_TEST.is_match(trimmed) {
            i += 1;
            continue;
        }

        // Drop JUnit discovery noise
        if JUNIT_NOISE.is_match(trimmed) {
            i += 1;
            continue;
        }

        // Drop STANDARD_OUT/STANDARD_ERR blocks
        if STANDARD_STREAM.is_match(trimmed) {
            in_standard_stream = true;
            i += 1;
            continue;
        }

        // If in a STANDARD_OUT/ERR block, drop indented content
        if in_standard_stream {
            if trimmed.is_empty()
                || trimmed.starts_with("> Task")
                || trimmed.contains("FAILED")
                || trimmed.contains("tests completed")
                || trimmed.starts_with("FAILURE:")
                || trimmed.starts_with("BUILD ")
                || trimmed.starts_with("* ")
            {
                in_standard_stream = false;
                // Fall through to process this line
            } else {
                // Non-failure content in standard stream — drop
                i += 1;
                continue;
            }
        }

        // Stack trace handling: detect FAILED line followed by exception
        if trimmed.contains("FAILED") && !trimmed.starts_with("> Task") {
            result.push(line.to_string());
            i += 1;

            // Collect and filter the stack trace
            let (trace_lines, consumed) =
                collect_stack_trace(&lines[i..], user_packages, &framework_re);
            result.extend(trace_lines);
            i += consumed;
            continue;
        }

        result.push(line.to_string());
        i += 1;
    }

    let joined = result.join("\n");
    let normalized = normalize_paths(&joined);

    // Trim leading/trailing blank lines
    let trimmed_lines: Vec<&str> = normalized.lines().collect();
    let start = trimmed_lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(0);
    let end = trimmed_lines
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map(|i| i + 1)
        .unwrap_or(trimmed_lines.len());
    trimmed_lines[start..end].join("\n")
}

/// Collect and filter a stack trace starting from the exception line.
/// Returns (filtered_lines, number_of_input_lines_consumed).
fn collect_stack_trace(
    lines: &[&str],
    user_packages: &[String],
    framework_re: &Regex,
) -> (Vec<String>, usize) {
    let mut result = Vec::new();
    let mut consumed = 0;
    let mut user_frames_kept = 0;
    let mut assertion_kept = false;
    let mut dropped_count = 0;
    let max_user_frames = 3;
    let mut in_caused_by = false;
    let mut caused_by_user_frames = 0;

    for line in lines {
        let trimmed = line.trim();

        // End of stack trace: empty line, non-indented, next test result, etc.
        if trimmed.is_empty() {
            // Flush dropped count
            if dropped_count > 0 {
                result.push(format!("    ... {} more", dropped_count));
                dropped_count = 0;
            }
            result.push(String::new());
            consumed += 1;
            break;
        }

        // Non-trace content — end of trace
        if !FRAME_LINE.is_match(line)
            && !CAUSED_BY.is_match(line)
            && !MORE_LINE.is_match(line)
            && !trimmed.starts_with("at ")
        {
            // Could be exception message or next test — check if it looks like an exception
            if consumed == 0 || is_exception_line(trimmed) {
                // Exception message line — keep it
                result.push(line.to_string());
                consumed += 1;
                continue;
            }
            // End of trace
            if dropped_count > 0 {
                result.push(format!("    ... {} more", dropped_count));
            }
            break;
        }

        consumed += 1;

        // Handle "Caused by:" chains
        if CAUSED_BY.is_match(line) {
            if dropped_count > 0 {
                result.push(format!("    ... {} more", dropped_count));
                dropped_count = 0;
            }
            in_caused_by = true;
            caused_by_user_frames = 0;
            result.push(line.to_string());
            continue;
        }

        // Handle "... N more" lines
        if MORE_LINE.is_match(line) {
            // Add to dropped count
            if let Some(n) = extract_more_count(trimmed) {
                dropped_count += n;
            }
            continue;
        }

        // Frame classification — classify first, then enforce limits.
        // User frames are always kept (up to max_user_frames) regardless of position
        // to avoid being shadowed by framework frames in deep traces.
        let is_user = is_user_code_frame(line, user_packages);
        let is_assertion = ASSERTION_FRAME.is_match(line);
        let is_framework = framework_re.is_match(line);

        // User frames are ALWAYS kept (up to max_user_frames)
        if is_user {
            let uf = if in_caused_by {
                &mut caused_by_user_frames
            } else {
                &mut user_frames_kept
            };
            if *uf < max_user_frames {
                result.push(line.to_string());
                *uf += 1;
            } else {
                dropped_count += 1;
            }
        } else if is_assertion && !assertion_kept {
            // Keep first assertion frame
            result.push(line.to_string());
            assertion_kept = true;
        } else if is_framework {
            // Framework frame — drop
            dropped_count += 1;
        } else {
            // Unknown frame — keep (could be third-party library user cares about)
            result.push(line.to_string());
        }
    }

    // Final flush
    if dropped_count > 0 {
        result.push(format!("    ... {} more", dropped_count));
    }

    (result, consumed)
}

/// Check if a frame is user code (matches configured packages or com.example.* for tests).
fn is_user_code_frame(frame: &str, user_packages: &[String]) -> bool {
    if user_packages.is_empty() {
        // Default: check against built-in USER_CODE pattern (com.example.*)
        USER_CODE.is_match(frame)
    } else {
        user_packages
            .iter()
            .any(|pkg| frame.contains(&format!("at {}.", pkg)))
    }
}

/// Check if a line looks like an exception message (not a frame).
fn is_exception_line(line: &str) -> bool {
    // Exception messages typically contain a colon and don't start with "at "
    !line.trim_start().starts_with("at ")
        && (line.contains("Exception")
            || line.contains("Error")
            || line.contains("Throwable")
            || line.contains("expected:")
            || line.contains("Expected"))
}

/// Extract count from "... N more" lines.
fn extract_more_count(line: &str) -> Option<usize> {
    lazy_static! {
        static ref MORE_COUNT: Regex = Regex::new(r"\.\.\. (\d+) more").unwrap();
    }
    MORE_COUNT
        .captures(line)
        .and_then(|caps| caps[1].parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmds::gradle::global::apply_global_filters;
    use insta::assert_snapshot;

    fn count_tokens(text: &str) -> usize {
        text.split_whitespace().count()
    }

    // --- matches_task tests (unified matcher) ---

    #[test]
    fn test_matches_unit_test() {
        assert!(matches_task("test"));
    }

    #[test]
    fn test_matches_integration_test() {
        assert!(matches_task("integrationTest"));
    }

    #[test]
    fn test_matches_component_test() {
        assert!(matches_task("componentTest"));
    }

    #[test]
    fn test_no_match_compile() {
        assert!(!matches_task("compileTestKotlin"));
    }

    // Android variant tests

    #[test]
    fn test_matches_android_unit_test() {
        assert!(matches_task("testDebugUnitTest"));
        assert!(matches_task("testReleaseUnitTest"));
    }

    #[test]
    fn test_matches_connected_android_test() {
        assert!(matches_task("connectedDebugAndroidTest"));
        assert!(matches_task("connectedAndroidTest"));
    }

    // --- build_framework_regex tests ---

    #[test]
    fn test_framework_regex_matches_builtin() {
        let re = build_framework_regex(&[]);
        assert!(re.is_match("    at java.lang.Thread.run(Thread.java:750)"));
        assert!(re.is_match("    at kotlin.coroutines.jvm.internal.BaseContinuationImpl.resumeWith(ContinuationImpl.kt:33)"));
        assert!(re.is_match("    at sun.reflect.NativeMethodAccessorImpl.invoke0(Native Method)"));
        assert!(re.is_match(
            "    at jdk.internal.reflect.NativeMethodAccessorImpl.invoke0(Native Method)"
        ));
    }

    #[test]
    fn test_framework_regex_matches_internal() {
        let re = build_framework_regex(&[]);
        // Any package with .internal. should match
        assert!(re.is_match(
            "    at org.gradle.api.internal.tasks.testing.SomeClass.run(SomeClass.java:42)"
        ));
        assert!(re.is_match("    at com.example.internal.SomeUtil.run(SomeUtil.java:1)"));
    }

    #[test]
    fn test_framework_regex_matches_configured() {
        let extras = vec![
            "org.springframework".to_string(),
            "com.google.inject".to_string(),
        ];
        let re = build_framework_regex(&extras);
        assert!(re.is_match("    at org.springframework.test.context.TestContextManager.prepareTestInstance(TestContextManager.java:244)"));
        assert!(re.is_match(
            "    at com.google.inject.internal.InjectorImpl.inject(InjectorImpl.java:123)"
        ));
    }

    #[test]
    fn test_framework_regex_does_not_match_user_code() {
        let re = build_framework_regex(&[]);
        assert!(
            !re.is_match("    at com.example.billing.PaymentTest.testCharge(PaymentTest.kt:42)")
        );
    }

    // Filter tests
    #[test]
    fn test_test_success_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/test_success_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_test(&globally_filtered);
        assert_snapshot!(output);
    }

    #[test]
    fn test_test_failure_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/test_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_test(&globally_filtered);
        assert_snapshot!(output);
    }

    #[test]
    fn test_test_failure_with_user_packages_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/test_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_test_with_packages(&globally_filtered, &["com.example".to_string()]);
        assert_snapshot!(output);
    }

    #[test]
    fn test_integration_test_failure_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/integration_test_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_test(&globally_filtered);
        assert_snapshot!(output);
    }

    #[test]
    fn test_test_failure_token_savings() {
        let input = include_str!("../../../tests/fixtures/gradle/test_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_test(&globally_filtered);
        let input_tokens = count_tokens(input);
        let output_tokens = count_tokens(&output);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 60.0,
            "Expected >=60% savings on test failure, got {:.1}% (input={}, output={})",
            savings,
            input_tokens,
            output_tokens
        );
    }

    #[test]
    fn test_passing_tests_dropped() {
        let input = include_str!("../../../tests/fixtures/gradle/test_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_test(&globally_filtered);
        assert!(
            !output.contains("PASSED"),
            "Passing test lines should be dropped"
        );
    }

    #[test]
    fn test_failures_preserved() {
        let input = include_str!("../../../tests/fixtures/gradle/test_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_test(&globally_filtered);
        assert!(output.contains("testChargeAmount FAILED"));
        assert!(output.contains("testRefundProcess FAILED"));
        assert!(output.contains("testCreateOrder FAILED"));
    }

    #[test]
    fn test_stack_trace_truncation_basic() {
        let re = build_framework_regex(&config::default_drop_frame_packages());
        let trace = vec![
            "    java.lang.AssertionError: expected:<1> but was:<2>",
            "        at org.junit.Assert.failNotEquals(Assert.java:834)",
            "        at com.example.foo.FooTest.test(FooTest.kt:10)",
            "        at sun.reflect.NativeMethodAccessorImpl.invoke0(Native Method)",
            "        at java.lang.reflect.Method.invoke(Method.java:498)",
            "        at org.junit.platform.commons.util.ReflectionUtils.invokeMethod(ReflectionUtils.java:688)",
            "        at org.gradle.api.internal.tasks.testing.junitplatform.JUnitPlatformTestClassProcessor$CollectAllTestClassesExecutor.execute(JUnitPlatformTestClassProcessor.java:110)",
            "        at java.lang.Thread.run(Thread.java:750)",
            "",
        ];
        let (result, consumed) = collect_stack_trace(&trace, &["com.example".to_string()], &re);
        assert_eq!(consumed, trace.len());

        let output = result.join("\n");
        // Should keep: exception, assertion frame, user frame
        assert!(output.contains("AssertionError"));
        assert!(output.contains("org.junit.Assert.failNotEquals"));
        assert!(output.contains("com.example.foo.FooTest.test"));
        // Should drop framework frames
        assert!(!output.contains("NativeMethodAccessorImpl"));
        assert!(!output.contains("ReflectionUtils"));
        // Should have "... N more" summary
        assert!(output.contains("... "));
    }

    #[test]
    fn test_caused_by_chain() {
        let re = build_framework_regex(&config::default_drop_frame_packages());
        let trace = vec![
            "    org.opentest4j.AssertionFailedError: Expected failure",
            "        at org.junit.jupiter.api.Assertions.fail(Assertions.java:55)",
            "        at com.example.foo.FooTest.test(FooTest.kt:10)",
            "        at sun.reflect.NativeMethodAccessorImpl.invoke0(Native Method)",
            "        Caused by: java.lang.RuntimeException: root cause",
            "            at com.example.foo.FooService.doThing(FooService.kt:42)",
            "            at sun.reflect.NativeMethodAccessorImpl.invoke0(Native Method)",
            "            ... 5 more",
            "",
        ];
        let (result, _) = collect_stack_trace(&trace, &["com.example".to_string()], &re);
        let output = result.join("\n");
        assert!(output.contains("Caused by: java.lang.RuntimeException"));
        assert!(output.contains("com.example.foo.FooService.doThing"));
    }

    #[test]
    fn test_empty_user_packages_max_truncation() {
        let re = build_framework_regex(&config::default_drop_frame_packages());
        let trace = vec![
            "    java.lang.AssertionError: test failed",
            "        at org.junit.Assert.fail(Assert.java:100)",
            "        at com.example.foo.FooTest.test(FooTest.kt:10)",
            "        at sun.reflect.NativeMethodAccessorImpl.invoke0(Native Method)",
            "",
        ];
        let (result, _) = collect_stack_trace(&trace, &[], &re);
        let output = result.join("\n");
        // com.example should still be kept via the built-in USER_CODE regex
        assert!(output.contains("com.example.foo.FooTest"));
    }

    #[test]
    fn test_non_matching_package_keeps_unknown_frames() {
        // With user_packages=["com.acme"], com.example is NOT a user frame
        // but also NOT a framework frame — so it's kept as an unknown (potentially useful) frame
        let re = build_framework_regex(&config::default_drop_frame_packages());
        let trace = vec![
            "    java.lang.AssertionError: expected:<1> but was:<2>",
            "        at org.junit.Assert.failNotEquals(Assert.java:834)",
            "        at com.example.foo.FooTest.test(FooTest.kt:10)",
            "        at sun.reflect.NativeMethodAccessorImpl.invoke0(Native Method)",
            "",
        ];
        let (result, _) = collect_stack_trace(&trace, &["com.acme".to_string()], &re);
        let output = result.join("\n");
        // com.example IS kept — it's not in the framework list, could be useful
        assert!(
            output.contains("com.example.foo.FooTest"),
            "Unknown frames (not framework, not user) should be kept"
        );
        // Framework frames (sun.reflect) should be dropped
        assert!(!output.contains("NativeMethodAccessorImpl"));
    }

    #[test]
    fn test_user_frames_kept_after_many_framework_frames() {
        let re = build_framework_regex(&config::default_drop_frame_packages());
        let mut trace = vec!["    java.lang.AssertionError: test failed".to_string()];
        for i in 0..10 {
            trace.push(format!(
                "        at org.junit.platform.internal.Frame{}.run(Frame.java:{})",
                i, i
            ));
        }
        trace.push("        at com.example.foo.FooTest.test(FooTest.kt:10)".to_string());
        trace.push(String::new());
        let trace_refs: Vec<&str> = trace.iter().map(|s| s.as_str()).collect();
        let (result, _) = collect_stack_trace(&trace_refs, &["com.example".to_string()], &re);
        let output = result.join("\n");
        assert!(
            output.contains("com.example.foo.FooTest.test"),
            "User frame should be kept even after 10 framework frames"
        );
    }

    #[test]
    fn test_standard_out_dropped() {
        let input = include_str!("../../../tests/fixtures/gradle/test_failure_raw.txt");
        let globally_filtered = apply_global_filters(input);
        let output = filter_test(&globally_filtered);
        assert!(
            !output.contains("STANDARD_OUT"),
            "STANDARD_OUT header should be dropped"
        );
        assert!(
            !output.contains("SLF4J"),
            "STANDARD_OUT content should be dropped"
        );
    }
}
