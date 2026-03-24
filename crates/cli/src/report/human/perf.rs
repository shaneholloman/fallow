use colored::Colorize;
use fallow_core::trace::PipelineTimings;

pub(in crate::report) fn print_performance_human(t: &PipelineTimings) {
    for line in build_performance_human_lines(t) {
        eprintln!("{line}");
    }
}

/// Build human-readable output lines for pipeline performance timings.
pub(in crate::report) fn build_performance_human_lines(t: &PipelineTimings) -> Vec<String> {
    let mut lines = Vec::new();

    lines.push(String::new());
    lines.push(
        "┌─ Pipeline Performance ─────────────────────────────"
            .dimmed()
            .to_string(),
    );
    lines.push(
        format!(
            "│  discover files:   {:>8.1}ms  ({} files)",
            t.discover_files_ms, t.file_count
        )
        .dimmed()
        .to_string(),
    );
    lines.push(
        format!(
            "│  workspaces:       {:>8.1}ms  ({} workspaces)",
            t.workspaces_ms, t.workspace_count
        )
        .dimmed()
        .to_string(),
    );
    lines.push(
        format!("│  plugins:          {:>8.1}ms", t.plugins_ms)
            .dimmed()
            .to_string(),
    );
    lines.push(
        format!("│  script analysis:  {:>8.1}ms", t.script_analysis_ms)
            .dimmed()
            .to_string(),
    );
    let cache_detail = if t.cache_hits > 0 {
        format!(", {} cached, {} parsed", t.cache_hits, t.cache_misses)
    } else {
        String::new()
    };
    lines.push(
        format!(
            "│  parse/extract:    {:>8.1}ms  ({} modules{})",
            t.parse_extract_ms, t.module_count, cache_detail
        )
        .dimmed()
        .to_string(),
    );
    lines.push(
        format!("│  cache update:     {:>8.1}ms", t.cache_update_ms)
            .dimmed()
            .to_string(),
    );
    lines.push(
        format!(
            "│  entry points:     {:>8.1}ms  ({} entries)",
            t.entry_points_ms, t.entry_point_count
        )
        .dimmed()
        .to_string(),
    );
    lines.push(
        format!("│  resolve imports:  {:>8.1}ms", t.resolve_imports_ms)
            .dimmed()
            .to_string(),
    );
    lines.push(
        format!("│  build graph:      {:>8.1}ms", t.build_graph_ms)
            .dimmed()
            .to_string(),
    );
    lines.push(
        format!("│  analyze:          {:>8.1}ms", t.analyze_ms)
            .dimmed()
            .to_string(),
    );
    lines.push(
        "│  ────────────────────────────────────────────────"
            .dimmed()
            .to_string(),
    );
    lines.push(
        format!("│  TOTAL:            {:>8.1}ms", t.total_ms)
            .bold()
            .dimmed()
            .to_string(),
    );
    lines.push(
        "└───────────────────────────────────────────────────"
            .dimmed()
            .to_string(),
    );
    lines.push(String::new());

    lines
}
