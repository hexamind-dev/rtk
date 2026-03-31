pub mod batch;
pub mod compile;
pub mod deps;
pub mod detekt;
pub mod global;
pub mod health;
pub mod paths;
pub mod proto;
pub mod test_filter;

use crate::core::tee;
use crate::core::tracking;
use anyhow::{Context, Result};
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub enum TaskType {
    Compile,
    Test,
    Detekt,
    Health,
    Proto,
    Deps,
    Generic,
}

type TaskMatcher = fn(&str) -> bool;
type TaskRegistryEntry = (TaskMatcher, TaskType);

/// Registry of task type matchers, checked in priority order.
const TASK_TYPE_REGISTRY: &[TaskRegistryEntry] = &[
    (deps::matches_task, TaskType::Deps),
    (test_filter::matches_task, TaskType::Test),
    (detekt::matches_task, TaskType::Detekt),
    (health::matches_task, TaskType::Health),
    (compile::matches_task, TaskType::Compile),
    (proto::matches_task, TaskType::Proto),
];

/// Detect the task type from gradle arguments.
///
/// Scans all args for task name patterns using per-module matchers.
/// If multiple distinct task types are present (batch run), returns `Generic`
/// — the batch filter handles per-task routing.
pub fn detect_task_type(args: &[String]) -> TaskType {
    let mut detected: Vec<TaskType> = Vec::new();

    for arg in args {
        // Skip flags (start with -)
        if arg.starts_with('-') {
            continue;
        }

        // Extract the task name (last segment after :), lowercased for
        // case-insensitive CLI matching (Gradle accepts any casing on CLI).
        let task_name = match arg.rfind(':') {
            Some(pos) => arg[pos + 1..].to_ascii_lowercase(),
            None => arg.to_ascii_lowercase(),
        };

        // Walk registry in priority order, first match wins
        let task_type = TASK_TYPE_REGISTRY
            .iter()
            .find(|(matcher, _)| matcher(&task_name))
            .map(|(_, tt)| tt.clone());

        if let Some(tt) = task_type {
            if !detected.iter().any(|d| d == &tt) {
                detected.push(tt);
            }
        }
    }

    match detected.len() {
        0 => TaskType::Generic,
        1 => detected.into_iter().next().unwrap_or(TaskType::Generic),
        _ => TaskType::Generic, // Multiple distinct task types → batch handles routing
    }
}

/// Refine a Generic task type by scanning raw output for `> Task :...:taskName` lines.
///
/// Handles meta-tasks (like `check`, `build`, `lint`) that delegate to specific tasks.
/// If output reveals a single task type, returns that type; otherwise keeps Generic.
pub fn detect_task_type_from_output(raw: &str) -> TaskType {
    use lazy_static::lazy_static;
    use regex::Regex;

    lazy_static! {
        static ref TASK_LINE: Regex = Regex::new(r"^> Task :(?:[^:]+:)*([^\s]+)").unwrap();
    }

    let mut detected: Vec<TaskType> = Vec::new();

    for line in raw.lines() {
        if let Some(caps) = TASK_LINE.captures(line.trim()) {
            let task_name = caps.get(1).map_or("", |m| m.as_str());

            let task_type = TASK_TYPE_REGISTRY
                .iter()
                .find(|(matcher, _)| matcher(task_name))
                .map(|(_, tt)| tt.clone());

            if let Some(tt) = task_type {
                if !detected.iter().any(|d| d == &tt) {
                    detected.push(tt);
                }
            }
        }
    }

    match detected.len() {
        1 => detected.into_iter().next().unwrap_or(TaskType::Generic),
        _ => TaskType::Generic, // 0 or multiple types → keep Generic
    }
}

/// Find the gradle executable: prefer ./gradlew walking up parent dirs, fall back to gradle on PATH.
fn find_gradle_executable() -> String {
    let candidates = [
        "./gradlew",
        "../gradlew",
        "../../gradlew",
        "../../../gradlew",
    ];
    for candidate in &candidates {
        if std::path::Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }
    "gradle".to_string()
}

/// Normalize gradle args in one pass:
/// - Strip `--quiet`/`-q` (suppresses parseable output)
/// - Strip existing `--console <value>` (caller already rejected non-plain values)
/// - Append `--console plain`
fn normalize_args(args: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(args.len() + 2);
    let mut skip_next = false;

    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        match arg.as_str() {
            "--quiet" | "-q" => continue,
            "--console" => {
                skip_next = true; // skip the following value
                continue;
            }
            _ if arg.starts_with("--console=") => continue,
            _ => result.push(arg.clone()),
        }
    }

    result.push("--console".to_string());
    result.push("plain".to_string());
    result
}

/// Verbose logging flags that produce massive output (10-100x tokens).
/// Reject these and tell the user to run gradle directly.
const VERBOSE_FLAGS: &[&str] = &["--info", "--debug", "-d"];

/// Check if args contain a `--console` value that isn't `plain`.
/// Returns the non-plain value if found.
fn find_non_plain_console(args: &[String]) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--console" {
            if let Some(val) = iter.next() {
                if val != "plain" {
                    return Some(format!("--console {}", val));
                }
            }
        } else if let Some(val) = arg.strip_prefix("--console=") {
            if val != "plain" {
                return Some(arg.clone());
            }
        }
    }
    None
}

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    // Reject non-plain --console — rtk needs parseable output
    if let Some(console_arg) = find_non_plain_console(args) {
        let gradle = find_gradle_executable();
        eprintln!(
            "rtk: `{}` is incompatible with filtering — rtk requires `--console plain`. \
             Either remove the flag or run directly:\n\n  {} {}",
            console_arg,
            gradle,
            args.join(" ")
        );
        std::process::exit(1);
    }

    // Reject verbose flags — the output is enormous and not filterable
    if let Some(flag) = args.iter().find(|a| VERBOSE_FLAGS.contains(&a.as_str())) {
        let gradle = find_gradle_executable();
        eprintln!(
            "rtk: refusing to filter `{} {}` — {} produces 10-100x more output, \
             overwhelming token budgets. Run directly:\n\n  {} {}",
            flag,
            args.iter()
                .find(|a| !VERBOSE_FLAGS.contains(&a.as_str()))
                .map(|s| s.as_str())
                .unwrap_or("..."),
            flag,
            gradle,
            args.join(" ")
        );
        std::process::exit(1);
    }

    let timer = tracking::TimedExecution::start();

    let gradle = find_gradle_executable();
    let full_args = normalize_args(args);

    if verbose > 0 {
        eprintln!("Running: {} {}", gradle, full_args.join(" "));
    }

    let mut cmd = Command::new(&gradle);
    for arg in &full_args {
        cmd.arg(arg);
    }

    let output = cmd
        .output()
        .context("Failed to run gradle. Is gradle or ./gradlew available?")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let raw = format!("{}\n{}", stdout, stderr);

    let mut task_type = detect_task_type(args);
    // Fallback: if args didn't reveal a task type, scan output for executed tasks
    if task_type == TaskType::Generic {
        task_type = detect_task_type_from_output(&raw);
    }
    let filtered = filter_gradle_output(&raw, &task_type);

    let exit_code = output
        .status
        .code()
        .unwrap_or(if output.status.success() { 0 } else { 1 });

    if let Some(hint) = tee::tee_and_hint(&raw, "gradle", exit_code) {
        println!("{}\n{}", filtered, hint);
    } else {
        println!("{}", filtered);
    }

    // stderr is already included in `raw` (line 177) and filtered through the pipeline.
    // No separate stderr output needed — printing it again would duplicate the output.

    timer.track(
        &format!("{} {}", gradle, args.join(" ")),
        &format!("rtk gradle {}", args.join(" ")),
        &raw,
        &filtered,
    );

    if !output.status.success() {
        std::process::exit(exit_code);
    }

    Ok(())
}

/// Apply task-type-specific filtering to gradle output.
pub fn filter_gradle_output(raw: &str, task_type: &TaskType) -> String {
    // For batch runs (multiple executed tasks), use batch filter on raw input
    // regardless of detected task type — batch filter splits by task boundaries
    // and applies per-section filters, preserving per-task context.
    if has_multiple_tasks(raw) {
        let globally_filtered = global::apply_global_filters(raw);
        return batch::filter_batch_from_raw(raw, &globally_filtered);
    }

    let filtered = global::apply_global_filters(raw);

    match task_type {
        TaskType::Compile => compile::filter_compile(&filtered),
        TaskType::Test => test_filter::filter_test(&filtered),
        TaskType::Detekt => detekt::filter_detekt(&filtered),
        TaskType::Health => health::filter_health(&filtered),
        TaskType::Proto => proto::filter_proto(&filtered),
        TaskType::Deps => deps::filter_deps(&filtered),
        TaskType::Generic => filtered,
    }
}

/// Check if raw output contains multiple executed tasks (batch run).
fn has_multiple_tasks(raw: &str) -> bool {
    let task_count = raw
        .lines()
        .filter(|l| {
            let t = l.trim();
            t.starts_with("> Task ")
                && !t.ends_with("UP-TO-DATE")
                && !t.ends_with("SKIPPED")
                && !t.ends_with("NO-SOURCE")
                && !t.ends_with("FROM-CACHE")
        })
        .count();
    task_count > 1
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- detect_task_type tests ---

    #[test]
    fn test_detect_compile_kotlin() {
        let args = vec![":app:billing:compileKotlin".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Compile);
    }

    #[test]
    fn test_detect_compile_test_kotlin() {
        let args = vec![":app:billing:compileTestKotlin".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Compile);
    }

    #[test]
    fn test_detect_compile_integration_test_kotlin() {
        let args = vec![":app:billing:compileIntegrationTestKotlin".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Compile);
    }

    #[test]
    fn test_detect_compile_classes() {
        let args = vec![":app:billing:testClasses".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Compile);
    }

    #[test]
    fn test_detect_test() {
        let args = vec![":app:billing:test".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Test);
    }

    #[test]
    fn test_detect_integration_test() {
        let args = vec![":app:billing:integrationTest".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Test);
    }

    #[test]
    fn test_detect_component_test() {
        let args = vec![":app:billing:componentTest".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Test);
    }

    #[test]
    fn test_detect_detekt() {
        let args = vec![":app:billing:detekt".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Detekt);
    }

    #[test]
    fn test_detect_detekt_main() {
        let args = vec![":app:billing:detektMain".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Detekt);
    }

    #[test]
    fn test_detect_health() {
        let args = vec![":app:billing:projectHealth".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Health);
    }

    #[test]
    fn test_detect_proto_build() {
        let args = vec![":app:billing-api:buildProtos".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Proto);
    }

    #[test]
    fn test_detect_proto_generate() {
        let args = vec![":app:billing-api:generateProtos".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Proto);
    }

    #[test]
    fn test_detect_deps() {
        let args = vec![":app:billing:dependencies".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Deps);
    }

    #[test]
    fn test_detect_generic_unknown_task() {
        let args = vec![":app:billing:assemble".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Generic);
    }

    #[test]
    fn test_detect_generic_no_task() {
        let args: Vec<String> = vec!["--help".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Generic);
    }

    #[test]
    fn test_detect_skips_flags() {
        let args = vec![
            "--continue".to_string(),
            ":app:billing:test".to_string(),
            "--info".to_string(),
        ];
        assert_eq!(detect_task_type(&args), TaskType::Test);
    }

    #[test]
    fn test_detect_multiple_same_type_returns_single() {
        let args = vec![
            ":app:billing:test".to_string(),
            ":app:orders:test".to_string(),
        ];
        assert_eq!(detect_task_type(&args), TaskType::Test);
    }

    #[test]
    fn test_detect_multiple_different_types_returns_generic() {
        let args = vec![
            ":app:billing:test".to_string(),
            ":app:billing:detekt".to_string(),
        ];
        assert_eq!(detect_task_type(&args), TaskType::Generic);
    }

    // --- ensure_console_plain tests ---

    // --- normalize_args tests ---

    #[test]
    fn test_normalize_injects_console_plain() {
        let args = vec![":app:test".to_string()];
        let result = normalize_args(&args);
        assert_eq!(result, vec![":app:test", "--console", "plain"]);
    }

    #[test]
    fn test_normalize_appends_console_plain() {
        // --console plain is always appended (caller rejects non-plain before normalize)
        let args = vec![":app:test".to_string(), "--continue".to_string()];
        let result = normalize_args(&args);
        assert!(result.ends_with(&["--console".to_string(), "plain".to_string()]));
    }

    #[test]
    fn test_normalize_deduplicates_console_plain() {
        let args = vec![
            ":app:test".to_string(),
            "--console".to_string(),
            "plain".to_string(),
            "--quiet".to_string(),
        ];
        let result = normalize_args(&args);
        assert_eq!(result, vec![":app:test", "--console", "plain"]);
    }

    #[test]
    fn test_normalize_deduplicates_console_equals_plain() {
        let args = vec![":app:test".to_string(), "--console=plain".to_string()];
        let result = normalize_args(&args);
        assert_eq!(result, vec![":app:test", "--console", "plain"]);
    }

    // --- find_non_plain_console tests ---

    #[test]
    fn test_rejects_console_rich() {
        let args = vec![
            "--console".to_string(),
            "rich".to_string(),
            ":app:test".to_string(),
        ];
        assert_eq!(
            find_non_plain_console(&args),
            Some("--console rich".to_string())
        );
    }

    #[test]
    fn test_rejects_console_equals_auto() {
        let args = vec!["--console=auto".to_string(), ":app:test".to_string()];
        assert_eq!(
            find_non_plain_console(&args),
            Some("--console=auto".to_string())
        );
    }

    #[test]
    fn test_accepts_console_plain() {
        let args = vec![
            "--console".to_string(),
            "plain".to_string(),
            ":app:test".to_string(),
        ];
        assert_eq!(find_non_plain_console(&args), None);
    }

    #[test]
    fn test_accepts_console_equals_plain() {
        let args = vec!["--console=plain".to_string(), ":app:test".to_string()];
        assert_eq!(find_non_plain_console(&args), None);
    }

    #[test]
    fn test_accepts_no_console_flag() {
        let args = vec![":app:test".to_string()];
        assert_eq!(find_non_plain_console(&args), None);
    }

    #[test]
    fn test_normalize_strips_quiet_long() {
        let args = vec!["--quiet".to_string(), ":app:test".to_string()];
        let result = normalize_args(&args);
        assert_eq!(result, vec![":app:test", "--console", "plain"]);
    }

    #[test]
    fn test_normalize_strips_quiet_short() {
        let args = vec!["-q".to_string(), ":app:test".to_string()];
        let result = normalize_args(&args);
        assert_eq!(result, vec![":app:test", "--console", "plain"]);
    }

    #[test]
    fn test_normalize_preserves_other_flags() {
        let args = vec![
            "--continue".to_string(),
            ":app:test".to_string(),
            "--info".to_string(),
        ];
        let result = normalize_args(&args);
        assert_eq!(
            result,
            vec!["--continue", ":app:test", "--info", "--console", "plain"]
        );
    }

    // --- verbose flag rejection tests ---

    #[test]
    fn test_verbose_flags_detected() {
        for flag in VERBOSE_FLAGS {
            assert!(
                [":app:test", flag]
                    .iter()
                    .any(|a| VERBOSE_FLAGS.contains(a)),
                "{} should be detected as verbose",
                flag
            );
        }
    }

    #[test]
    fn test_normal_flags_not_rejected() {
        assert!(
            ["--continue", ":app:test", "--no-daemon"]
                .iter()
                .all(|a| !VERBOSE_FLAGS.contains(a)),
            "normal flags should not be rejected"
        );
    }

    // --- detect_task_type_from_output tests ---

    #[test]
    fn test_output_detection_finds_test() {
        let output = "> Task :app:billing:processResources UP-TO-DATE\n> Task :app:billing:test\n> Task :app:billing:test FAILED";
        assert_eq!(detect_task_type_from_output(output), TaskType::Test);
    }

    #[test]
    fn test_output_detection_finds_detekt() {
        let output = "> Task :app:billing:detektMain\n> Task :app:billing:detektTest";
        assert_eq!(detect_task_type_from_output(output), TaskType::Detekt);
    }

    #[test]
    fn test_output_detection_multiple_types_returns_generic() {
        let output = "> Task :app:billing:test\n> Task :app:billing:detektMain";
        assert_eq!(detect_task_type_from_output(output), TaskType::Generic);
    }

    #[test]
    fn test_output_detection_no_tasks_returns_generic() {
        let output = "BUILD SUCCESSFUL in 5s";
        assert_eq!(detect_task_type_from_output(output), TaskType::Generic);
    }

    #[test]
    fn test_output_detection_ignores_compile_when_test_present() {
        // Compile tasks are common prerequisites — if test tasks also appear,
        // both types are detected → Generic (batch handles routing)
        let output = "> Task :app:compileKotlin\n> Task :app:test";
        // Two distinct types → Generic
        assert_eq!(detect_task_type_from_output(output), TaskType::Generic);
    }

    // --- case-insensitive matching tests ---

    #[test]
    fn test_detect_case_insensitive_test() {
        let args = vec![":app:billing:Test".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Test);
    }

    #[test]
    fn test_detect_case_insensitive_compile_kotlin() {
        let args = vec![":app:billing:CompileKotlin".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Compile);
    }

    #[test]
    fn test_detect_case_insensitive_detekt() {
        let args = vec![":app:billing:Detekt".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Detekt);
    }

    #[test]
    fn test_detect_case_insensitive_project_health() {
        let args = vec![":app:billing:ProjectHealth".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Health);
    }

    #[test]
    fn test_detect_case_insensitive_dependencies() {
        let args = vec![":app:billing:Dependencies".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Deps);
    }

    #[test]
    fn test_detect_case_insensitive_build_protos() {
        let args = vec![":app:billing:BuildProtos".to_string()];
        assert_eq!(detect_task_type(&args), TaskType::Proto);
    }

    // --- stderr noise filtering tests ---

    #[test]
    fn test_global_filters_strip_jvm_warning_from_stderr() {
        let stderr = "OpenJDK 64-Bit Server VM warning: Sharing is only supported for boot loader classes because bootstrap classpath has been appended";
        let filtered = global::apply_global_filters(stderr);
        assert!(
            filtered.trim().is_empty(),
            "JVM warning should be stripped from stderr: got '{}'",
            filtered
        );
    }

    #[test]
    fn test_global_filters_keep_real_stderr_errors() {
        let stderr = "FAILURE: Build failed with an exception.\n\n* What went wrong:\nExecution failed for task ':app:test'.";
        let filtered = global::apply_global_filters(stderr);
        assert!(
            filtered.contains("FAILURE: Build failed"),
            "Real errors should be preserved in stderr"
        );
    }
}
