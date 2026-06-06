// Multi-task batched run handling.
use lazy_static::lazy_static;
use regex::Regex;

use super::compile;
use super::detekt;
use super::global;
use super::test_filter;
use super::TaskType;

lazy_static! {
    /// Matches "> Task :module:taskName" lines (with optional status suffix)
    static ref TASK_LINE: Regex = Regex::new(
        r"^> Task (\S+?)(?:\s+(FAILED|UP-TO-DATE|SKIPPED|NO-SOURCE|FROM-CACHE))?\s*$"
    ).unwrap();
}

/// A section of output belonging to a single gradle task.
struct TaskSection {
    task_name: String,
    task_type: TaskType,
    failed: bool,
    lines: Vec<String>,
}

/// Filter batch output. Uses raw input for task boundary detection
/// (needs "> Task" lines before global filter strips them).
/// The globally_filtered version is used for trailing content only.
pub fn filter_batch_from_raw(raw: &str, globally_filtered: &str) -> String {
    let (sections, _) = split_into_sections(raw);
    let trailing = extract_trailing(globally_filtered);
    filter_batch_impl(&sections, &trailing)
}

fn filter_batch_impl(sections: &[TaskSection], trailing: &str) -> String {
    if sections.is_empty() {
        return trailing.to_string();
    }

    let mut result = Vec::new();
    let mut success_tasks: Vec<String> = Vec::new();
    let mut failed_tasks: std::collections::HashSet<String> = std::collections::HashSet::new();

    // First pass: identify all failed tasks
    for section in sections {
        if section.failed {
            failed_tasks.insert(section.task_name.clone());
        }
    }

    // Second pass: process sections
    for section in sections {
        if section.failed {
            let content = section.lines.join("\n");
            // Apply global filters to section content, then task-specific filter
            let globally_filtered_content = global::apply_global_filters(&content);
            let filtered = filter_section_content(
                &globally_filtered_content,
                &section.task_type,
                &section.task_name,
            );
            result.push(format!("--- {} FAILED ---", section.task_name));
            if !filtered.trim().is_empty() {
                result.push(filtered.trim().to_string());
            }
            result.push(String::new());
        } else if !failed_tasks.contains(&section.task_name) {
            // Only add to success if the task didn't also fail
            success_tasks.push(format!("{} ✓", section.task_name));
        }
    }

    // Add success tasks
    for task in &success_tasks {
        result.push(task.clone());
    }

    // Add trailing content
    if !trailing.trim().is_empty() {
        result.push(String::new());
        result.push(trailing.trim().to_string());
    }

    // Trim
    let joined = result.join("\n");
    let lines: Vec<&str> = joined.lines().collect();
    let start = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
    let end = lines
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map(|i| i + 1)
        .unwrap_or(lines.len());
    lines[start..end].join("\n")
}

/// Extract trailing content from globally-filtered output
/// (FAILURE:, * What went wrong:, BUILD FAILED, actionable tasks).
fn extract_trailing(input: &str) -> String {
    let mut trailing = Vec::new();
    let mut in_trailing = false;

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("FAILURE:")
            || trimmed.starts_with("* What went wrong:")
            || trimmed.starts_with("BUILD ")
        {
            in_trailing = true;
        }
        if in_trailing {
            trailing.push(line.to_string());
        }
    }
    trailing.join("\n")
}

/// Split raw output into per-task sections based on "> Task" markers.
fn split_into_sections(input: &str) -> (Vec<TaskSection>, String) {
    let mut sections: Vec<TaskSection> = Vec::new();
    let mut current: Option<TaskSection> = None;
    let mut trailing = Vec::new();
    let mut past_all_tasks = false;

    for line in input.lines() {
        let trimmed = line.trim();

        if let Some(caps) = TASK_LINE.captures(trimmed) {
            let task_name = caps[1].to_string();
            let status = caps.get(2).map(|m| m.as_str());
            let failed = status == Some("FAILED");

            // Skip UP-TO-DATE, SKIPPED, etc.
            if matches!(
                status,
                Some("UP-TO-DATE") | Some("SKIPPED") | Some("NO-SOURCE") | Some("FROM-CACHE")
            ) {
                continue;
            }

            past_all_tasks = false;

            // Save previous section
            if let Some(sec) = current.take() {
                sections.push(sec);
            }

            let task_type = detect_task_type_from_name(&task_name);
            current = Some(TaskSection {
                task_name,
                task_type,
                failed,
                lines: Vec::new(),
            });
            continue;
        }

        // Detect end of task sections
        if trimmed.starts_with("FAILURE:") || trimmed.starts_with("* What went wrong:") {
            if let Some(sec) = current.take() {
                sections.push(sec);
            }
            past_all_tasks = true;
        }

        if past_all_tasks {
            trailing.push(line.to_string());
        } else if let Some(ref mut sec) = current {
            sec.lines.push(line.to_string());
        }
    }

    if let Some(sec) = current {
        sections.push(sec);
    }

    (sections, trailing.join("\n"))
}

/// Detect task type from a fully-qualified task name (e.g. `:app:billing:test`).
/// Reuses per-module matchers via TASK_TYPE_REGISTRY.
fn detect_task_type_from_name(task_name: &str) -> TaskType {
    let name = task_name.rsplit(':').next().unwrap_or(task_name);
    super::TASK_TYPE_REGISTRY
        .iter()
        .find(|(matcher, _)| matcher(name))
        .map(|(_, tt)| tt.clone())
        .unwrap_or(TaskType::Generic)
}

/// Filter a single task section content based on its type.
fn filter_section_content(content: &str, task_type: &TaskType, _task_name: &str) -> String {
    match task_type {
        TaskType::Compile => compile::filter_compile(content),
        TaskType::Test => test_filter::filter_test(content),
        TaskType::Detekt => detekt::filter_detekt(content),
        _ => content.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;

    fn count_tokens(text: &str) -> usize {
        text.split_whitespace().count()
    }

    #[test]
    fn test_batch_mixed_snapshot() {
        let input = include_str!("../../../tests/fixtures/gradle/batch_mixed_raw.txt");
        let globally_filtered = global::apply_global_filters(input);
        let output = filter_batch_from_raw(input, &globally_filtered);
        assert_snapshot!(output);
    }

    #[test]
    fn test_batch_mixed_token_savings() {
        let input = include_str!("../../../tests/fixtures/gradle/batch_mixed_raw.txt");
        let globally_filtered = global::apply_global_filters(input);
        let output = filter_batch_from_raw(input, &globally_filtered);
        let input_tokens = count_tokens(input);
        let output_tokens = count_tokens(&output);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 60.0,
            "Expected >=60% savings on batch mixed, got {:.1}% (input={}, output={})",
            savings,
            input_tokens,
            output_tokens
        );
    }

    #[test]
    fn test_batch_shows_failed_sections() {
        let input = include_str!("../../../tests/fixtures/gradle/batch_mixed_raw.txt");
        let globally_filtered = global::apply_global_filters(input);
        let output = filter_batch_from_raw(input, &globally_filtered);
        assert!(
            output.contains("FAILED ---"),
            "Should show FAILED section headers"
        );
    }

    #[test]
    fn test_batch_shows_success_tasks() {
        let input = include_str!("../../../tests/fixtures/gradle/batch_mixed_raw.txt");
        let globally_filtered = global::apply_global_filters(input);
        let output = filter_batch_from_raw(input, &globally_filtered);
        assert!(
            output.contains(":app-orders:test ✓"),
            "Should show success tasks with ✓"
        );
    }
}
