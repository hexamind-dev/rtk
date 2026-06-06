use crate::core::config;
use crate::core::utils::strip_ansi;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref NOISE_PATTERNS: Vec<Regex> = vec![
        // Task status lines (UP-TO-DATE, SKIPPED, NO-SOURCE, FROM-CACHE)
        Regex::new(r"^> Task \S+ (UP-TO-DATE|SKIPPED|NO-SOURCE|FROM-CACHE)$").unwrap(),
        // Bare executed task lines (no suffix) — replaced by ✓ summary
        Regex::new(r"^> Task \S+\s*$").unwrap(),
        // Configure lines
        Regex::new(r"^> Configure project ").unwrap(),
        // Daemon startup
        Regex::new(r"^(Starting a? ?Gradle Daemon|Gradle Daemon started|Daemon initialized|Worker lease)").unwrap(),
        // JVM warnings
        Regex::new(r"^(OpenJDK 64-Bit Server VM warning:|Initialized native services|Initialized jansi)").unwrap(),
        // Incubating (including Problems report)
        Regex::new(r"\[Incubating\]|Configuration on demand is an incubating feature|Parallel Configuration Cache is an incubating feature").unwrap(),
        // Config cache
        Regex::new(r"^(Reusing configuration cache|Calculating task graph|Configuration cache entry|Storing configuration cache|Loading configuration cache)").unwrap(),
        // Deprecation
        Regex::new(r"^(Deprecated Gradle features were used|For more on this, please refer to|You can use '--warning-mode all')").unwrap(),
        // Downloads + progress bars
        Regex::new(r"^(Download |Downloading )").unwrap(),
        Regex::new(r"^\s*\[[\s<=\->]+\]\s+\d+%").unwrap(),
        // Build scan + develocity URLs (both private develocity.* and public scans.gradle.com)
        Regex::new(r"^(Publishing build scan|https://(develocity\.|scans\.gradle\.com)|Upload .* build scan|Waiting for build scan)").unwrap(),
        // VFS (all VFS> lines and Virtual file system lines)
        Regex::new(r"^(VFS>|Virtual file system )").unwrap(),
        // Evaluation
        Regex::new(r"^(Evaluating root project|All projects evaluated|Settings evaluated)").unwrap(),
        // Classpath
        Regex::new(r"^(Classpath snapshot |Snapshotting classpath)").unwrap(),
        // Kotlin daemon
        Regex::new(r"^(Kotlin compile daemon|Connected to the daemon)").unwrap(),
        // Reflection warnings
        Regex::new(r"(?i)^WARNING:.*illegal reflective|(?i)^WARNING:.*reflect").unwrap(),
        // File system events
        Regex::new(r"^Received \d+ file system events").unwrap(),
        // Javac/kapt notes (not actionable)
        Regex::new(r"^Note: ").unwrap(),
    ];
}

/// Apply global noise filters to gradle output.
///
/// Drops noise lines, removes `* Try:` blocks, and trims blank lines.
pub fn apply_global_filters(input: &str) -> String {
    let config = load_extra_patterns();
    apply_global_filters_with_extras(input, &config)
}

/// Load extra drop patterns from config.toml [gradle] section.
fn load_extra_patterns() -> Vec<Regex> {
    match config::Config::load() {
        Ok(config) => compile_extra_patterns(&config.gradle.extra_drop_patterns),
        Err(_) => Vec::new(),
    }
}

/// Compile user-supplied regex patterns, skipping invalid ones with stderr warning.
pub fn compile_extra_patterns(patterns: &[String]) -> Vec<Regex> {
    let mut compiled = Vec::new();
    for p in patterns {
        match Regex::new(p) {
            Ok(re) => compiled.push(re),
            Err(e) => {
                eprintln!("rtk: invalid extra_drop_pattern '{}': {}", p, e);
            }
        }
    }
    compiled
}

/// Core filter logic, testable with explicit extra patterns.
pub fn apply_global_filters_with_extras(input: &str, extra_patterns: &[Regex]) -> String {
    let mut result: Vec<String> = Vec::new();
    let mut in_try_block = false;

    for line in input.lines() {
        let trimmed = line.trim();
        // Strip ANSI escape codes for pattern matching (but keep original line for output)
        let clean = strip_ansi(trimmed);
        let clean_trimmed = clean.trim();

        // Try block removal: "* Try:" through next "* " header or end of block
        // Must be checked before blank line handling so blank lines inside Try blocks are consumed
        if clean_trimmed.starts_with("* Try:") {
            in_try_block = true;
            continue;
        }
        if in_try_block {
            if clean_trimmed.is_empty() {
                // Blank lines inside Try block — consume
                continue;
            } else if clean_trimmed.starts_with("* ") {
                // Next * header ends the Try block
                in_try_block = false;
                // Fall through to process this line normally
            } else if clean_trimmed.starts_with("> ")
                || clean_trimmed.starts_with("Get more help at")
            {
                // Indented content within Try block
                continue;
            } else {
                // Non-Try-block content — end the block
                in_try_block = false;
                // Fall through to process this line normally
            }
        }

        // Skip empty lines (blank line collapsing)
        if clean_trimmed.is_empty() {
            // Only add blank if last line wasn't blank
            if !matches!(result.last(), Some(l) if l.trim().is_empty()) {
                result.push(String::new());
            }
            continue;
        }

        // Check against built-in noise patterns (use ANSI-stripped text)
        if NOISE_PATTERNS.iter().any(|re| re.is_match(clean_trimmed)) {
            continue;
        }

        // Drop lines that are only ANSI escape codes (no visible content after stripping)
        if clean_trimmed.is_empty() && !trimmed.is_empty() {
            continue;
        }

        // Check against extra user-supplied patterns
        if extra_patterns.iter().any(|re| re.is_match(clean_trimmed)) {
            continue;
        }

        // Lines always kept (BUILD SUCCESSFUL/FAILED, FAILURE header, What went wrong)
        // These pass through naturally since they don't match noise patterns

        result.push(line.to_string());
    }

    // Trim leading/trailing blank lines
    while matches!(result.first(), Some(l) if l.trim().is_empty()) {
        result.remove(0);
    }
    while matches!(result.last(), Some(l) if l.trim().is_empty()) {
        result.pop();
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    fn count_tokens(text: &str) -> usize {
        text.split_whitespace().count()
    }

    #[test]
    fn test_compile_success_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/compile_success_raw.txt");
        let output = apply_global_filters(input);
        assert_snapshot!(output);
    }

    #[test]
    fn test_compile_success_token_savings() {
        let input = include_str!("../../../tests/fixtures/gradle/compile_success_raw.txt");
        let output = apply_global_filters(input);
        let input_tokens = count_tokens(input);
        let output_tokens = count_tokens(&output);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 90.0,
            "Expected >=90% savings on compile success, got {:.1}% (input={}, output={})",
            savings,
            input_tokens,
            output_tokens
        );
    }

    #[test]
    fn test_generic_noise_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/generic_noise_raw.txt");
        let output = apply_global_filters(input);
        assert_snapshot!(output);
    }

    #[test]
    fn test_generic_noise_token_savings() {
        let input = include_str!("../../../tests/fixtures/gradle/generic_noise_raw.txt");
        let output = apply_global_filters(input);
        let input_tokens = count_tokens(input);
        let output_tokens = count_tokens(&output);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 90.0,
            "Expected >=90% savings on generic noise, got {:.1}% (input={}, output={})",
            savings,
            input_tokens,
            output_tokens
        );
    }

    #[test]
    fn test_try_block_removal() {
        let input = "Some content\n\n* Try:\n> Run with --stacktrace option.\n> Run with --info option.\n> Run with --scan.\n> Get more help at https://help.gradle.org.\n\n* What went wrong:\nSomething failed";
        let output = apply_global_filters_with_extras(input, &[]);
        assert!(!output.contains("* Try:"), "Try block should be removed");
        assert!(
            !output.contains("--stacktrace"),
            "Try block content should be removed"
        );
        assert!(
            output.contains("* What went wrong:"),
            "What went wrong should be kept"
        );
    }

    #[test]
    fn test_note_lines_dropped() {
        let input = "Note: Some input files use unchecked or unsafe operations.\nNote: Recompile with -Xlint:unchecked for details.\nBUILD SUCCESSFUL in 1s";
        let output = apply_global_filters_with_extras(input, &[]);
        assert!(!output.contains("Note:"), "Note: lines should be dropped");
        assert!(output.contains("BUILD SUCCESSFUL"));
    }

    #[test]
    fn test_build_result_always_kept() {
        let input = "Starting Gradle Daemon...\nBUILD SUCCESSFUL in 12s\n8 actionable tasks: 1 executed, 7 up-to-date";
        let output = apply_global_filters_with_extras(input, &[]);
        assert!(output.contains("BUILD SUCCESSFUL"));
    }

    #[test]
    fn test_failure_header_kept() {
        let input = "FAILURE: Build failed with an exception\n\n* What went wrong:\nCompilation failed\n\nBUILD FAILED in 5s";
        let output = apply_global_filters_with_extras(input, &[]);
        assert!(output.contains("FAILURE: Build failed with an exception"));
        assert!(output.contains("* What went wrong:"));
        assert!(output.contains("BUILD FAILED"));
    }

    #[test]
    fn test_extra_drop_patterns() {
        let input = "Normal line\nCustomOrgBuildPlugin: initializing\nAnother normal line";
        let extras = compile_extra_patterns(&["^CustomOrgBuildPlugin:".to_string()]);
        let output = apply_global_filters_with_extras(input, &extras);
        assert!(!output.contains("CustomOrgBuildPlugin"));
        assert!(output.contains("Normal line"));
        assert!(output.contains("Another normal line"));
    }

    #[test]
    fn test_invalid_extra_pattern_skipped() {
        let patterns = vec!["[invalid".to_string(), "^valid$".to_string()];
        let compiled = compile_extra_patterns(&patterns);
        assert_eq!(compiled.len(), 1, "Invalid pattern should be skipped");
    }

    #[test]
    fn test_blank_line_trimming() {
        let input = "\n\n\nBUILD SUCCESSFUL in 1s\n\n\n";
        let output = apply_global_filters_with_extras(input, &[]);
        assert!(!output.starts_with('\n'));
        assert!(!output.ends_with('\n'));
        assert!(output.contains("BUILD SUCCESSFUL"));
    }
}
