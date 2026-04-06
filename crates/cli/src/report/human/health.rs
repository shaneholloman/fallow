use std::path::Path;
use std::time::Duration;

use colored::Colorize;

use super::{MAX_FLAT_ITEMS, format_path, plural, relative_path, split_dir_filename};

/// Docs base URL for health explanations.
const DOCS_HEALTH: &str = "https://docs.fallow.tools/explanations/health";

pub(in crate::report) fn print_health_human(
    report: &crate::health_types::HealthReport,
    root: &Path,
    elapsed: Duration,
    quiet: bool,
) {
    if !quiet {
        eprintln!();
    }

    let has_score = report.health_score.is_some();
    if report.findings.is_empty()
        && report.file_scores.is_empty()
        && report.coverage_gaps.is_none()
        && report.hotspots.is_empty()
        && report.targets.is_empty()
        && !has_score
    {
        if !quiet {
            eprintln!(
                "{}",
                format!(
                    "\u{2713} No functions exceed complexity thresholds ({:.2}s)",
                    elapsed.as_secs_f64()
                )
                .green()
                .bold()
            );
            eprintln!(
                "{}",
                format!(
                    "  {} functions analyzed (max cyclomatic: {}, max cognitive: {})",
                    report.summary.functions_analyzed,
                    report.summary.max_cyclomatic_threshold,
                    report.summary.max_cognitive_threshold,
                )
                .dimmed()
            );
        }
        return;
    }

    for line in build_health_human_lines(report, root) {
        println!("{line}");
    }

    if !quiet {
        let s = &report.summary;
        let mut parts = Vec::new();
        parts.push(format!("{} above threshold", s.functions_above_threshold));
        parts.push(format!("{} analyzed", s.functions_analyzed));
        if let Some(avg) = s.average_maintainability {
            let label = if avg >= 85.0 {
                "good"
            } else if avg >= 65.0 {
                "moderate"
            } else {
                "low"
            };
            parts.push(format!("maintainability {avg:.1} ({label})"));
        }
        eprintln!(
            "{}",
            format!(
                "\u{2717} {} ({:.2}s)",
                parts.join(" \u{00b7} "),
                elapsed.as_secs_f64()
            )
            .red()
            .bold()
        );
        if s.average_maintainability.is_some_and(|mi| mi < 85.0) {
            eprintln!(
                "{}",
                "  Maintainability scale: good \u{2265}85, moderate \u{2265}65, low <65 (0\u{2013}100)"
                    .dimmed()
            );
        }
    }
}

/// Build human-readable output lines for health (complexity) findings.
pub(in crate::report) fn build_health_human_lines(
    report: &crate::health_types::HealthReport,
    root: &Path,
) -> Vec<String> {
    let mut lines = Vec::new();
    render_health_score(&mut lines, report);
    render_health_trend(&mut lines, report);
    render_vital_signs(&mut lines, report);
    render_findings(&mut lines, report, root);
    render_coverage_gaps(&mut lines, report, root);
    render_file_scores(&mut lines, report, root);
    render_hotspots(&mut lines, report, root);
    render_refactoring_targets(&mut lines, report, root);
    lines
}

// ── Section renderers ────

fn render_health_score(lines: &mut Vec<String>, report: &crate::health_types::HealthReport) {
    let Some(ref hs) = report.health_score else {
        return;
    };

    let score_str = format!("{:.0}", hs.score);
    let grade_str = hs.grade;
    let score_colored = if hs.score >= 85.0 {
        format!("{score_str} {grade_str}")
            .green()
            .bold()
            .to_string()
    } else if hs.score >= 70.0 {
        format!("{score_str} {grade_str}")
            .yellow()
            .bold()
            .to_string()
    } else if hs.score >= 55.0 {
        format!("{score_str} {grade_str}").yellow().to_string()
    } else {
        format!("{score_str} {grade_str}").red().bold().to_string()
    };
    lines.push(format!(
        "{} {} {}",
        "\u{25cf}".cyan(),
        "Health score:".cyan().bold(),
        score_colored,
    ));

    // Penalty breakdown (dimmed, one line)
    let p = &hs.penalties;
    let mut parts = Vec::new();
    if let Some(df) = p.dead_files
        && df > 0.0
    {
        parts.push(format!("dead files -{df:.1}"));
    }
    if let Some(de) = p.dead_exports
        && de > 0.0
    {
        parts.push(format!("dead exports -{de:.1}"));
    }
    if p.complexity > 0.0 {
        parts.push(format!("complexity -{:.1}", p.complexity));
    }
    if p.p90_complexity > 0.0 {
        parts.push(format!("p90 -{:.1}", p.p90_complexity));
    }
    if let Some(mi) = p.maintainability
        && mi > 0.0
    {
        parts.push(format!("maintainability -{mi:.1}"));
    }
    if let Some(hp) = p.hotspots
        && hp > 0.0
    {
        parts.push(format!("hotspots -{hp:.1}"));
    }
    if let Some(ud) = p.unused_deps
        && ud > 0.0
    {
        parts.push(format!("unused dependencies -{ud:.1}"));
    }
    if let Some(cd) = p.circular_deps
        && cd > 0.0
    {
        parts.push(format!("circular dependencies -{cd:.1}"));
    }
    if !parts.is_empty() {
        lines.push(format!(
            "  {} {}",
            "Deductions:".dimmed(),
            parts.join(" \u{00b7} ").dimmed()
        ));
    }
    // Check for N/A components
    let mut na_parts = Vec::new();
    if p.dead_files.is_none() {
        na_parts.push("dead code");
    }
    if p.maintainability.is_none() {
        na_parts.push("maintainability");
    }
    if p.hotspots.is_none() {
        na_parts.push("hotspots");
    }
    if !na_parts.is_empty() {
        lines.push(format!(
            "  {}",
            format!(
                "N/A: {} (run --score for full pipeline)",
                na_parts.join(", ")
            )
            .dimmed()
        ));
    }
    lines.push(String::new());
}

/// Format a float for trend display: show as integer if it is one, otherwise 1dp.
fn fmt_trend_val(v: f64, unit: &str) -> String {
    if unit == "%" {
        format!("{v:.1}%")
    } else if (v - v.round()).abs() < 0.05 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// Format a delta for trend display: show with sign prefix.
fn fmt_trend_delta(v: f64, unit: &str) -> String {
    if unit == "%" {
        format!("{v:+.1}%")
    } else if (v - v.round()).abs() < 0.05 {
        format!("{v:+.0}")
    } else {
        format!("{v:+.1}")
    }
}

fn render_health_trend(lines: &mut Vec<String>, report: &crate::health_types::HealthReport) {
    let Some(ref trend) = report.health_trend else {
        return;
    };

    use crate::health_types::TrendDirection;

    // Section header with overall direction — the headline
    let date = trend
        .compared_to
        .timestamp
        .get(..10)
        .unwrap_or(&trend.compared_to.timestamp);
    let sha_str = trend
        .compared_to
        .git_sha
        .as_deref()
        .map_or(String::new(), |sha| format!(" \u{00b7} {sha}"));
    let direction_label = format!(
        "{} {}",
        trend.overall_direction.arrow(),
        trend.overall_direction.label()
    );
    let direction_colored = match trend.overall_direction {
        TrendDirection::Improving => direction_label.green().bold().to_string(),
        TrendDirection::Declining => direction_label.red().bold().to_string(),
        TrendDirection::Stable => direction_label.dimmed().to_string(),
    };
    lines.push(format!(
        "{} {} {} {}",
        "\u{25cf}".cyan(),
        "Trend:".cyan().bold(),
        direction_colored,
        format!("(vs {date}{sha_str})").dimmed(),
    ));

    // All-stable collapse: single dimmed line instead of N identical rows
    let all_stable = trend
        .metrics
        .iter()
        .all(|m| m.direction == TrendDirection::Stable);
    if all_stable {
        lines.push(format!(
            "  {}",
            format!("All {} metrics unchanged", trend.metrics.len()).dimmed()
        ));
        lines.push(String::new());
        return;
    }

    // Metric rows — aligned columns, no arrow separator (avoids collision with direction arrow)
    for m in &trend.metrics {
        let label = format!("{:<18}", m.label);
        let prev_str = fmt_trend_val(m.previous, m.unit);
        let cur_str = fmt_trend_val(m.current, m.unit);
        let delta_str = fmt_trend_delta(m.delta, m.unit);

        let direction_str = match m.direction {
            TrendDirection::Improving => format!("{} {}", m.direction.arrow(), m.direction.label())
                .green()
                .to_string(),
            TrendDirection::Declining => format!("{} {}", m.direction.arrow(), m.direction.label())
                .red()
                .to_string(),
            TrendDirection::Stable => format!("{} {}", m.direction.arrow(), m.direction.label())
                .dimmed()
                .to_string(),
        };

        let values = format!("{prev_str:>8}  {cur_str:<8}");
        lines.push(format!(
            "  {label} {values}  {delta_str:<10} {direction_str}"
        ));
    }

    lines.push(String::new());
}

fn render_vital_signs(lines: &mut Vec<String>, report: &crate::health_types::HealthReport) {
    // Suppress when trend is active — the trend table already shows all metrics
    if report.health_trend.is_some() {
        return;
    }
    let Some(ref vs) = report.vital_signs else {
        return;
    };

    let mut parts = Vec::new();
    if let Some(dfp) = vs.dead_file_pct {
        parts.push(format!("dead files {dfp:.1}%"));
    }
    if let Some(dep) = vs.dead_export_pct {
        parts.push(format!("dead exports {dep:.1}%"));
    }
    parts.push(format!("avg cyclomatic {:.1}", vs.avg_cyclomatic));
    parts.push(format!("p90 cyclomatic {}", vs.p90_cyclomatic));
    if let Some(mi) = vs.maintainability_avg {
        let label = if mi >= 85.0 {
            "good"
        } else if mi >= 65.0 {
            "moderate"
        } else {
            "low"
        };
        parts.push(format!("maintainability {mi:.1} ({label})"));
    }
    if let Some(hc) = vs.hotspot_count {
        parts.push(format!("{hc} churn hotspot{}", plural(hc as usize)));
    }
    if let Some(cd) = vs.circular_dep_count
        && cd > 0
    {
        parts.push(format!(
            "{cd} circular {}",
            if cd == 1 {
                "dependency"
            } else {
                "dependencies"
            }
        ));
    }
    if let Some(ud) = vs.unused_dep_count
        && ud > 0
    {
        parts.push(format!(
            "{ud} unused {}",
            if ud == 1 {
                "dependency"
            } else {
                "dependencies"
            }
        ));
    }
    lines.push(format!(
        "{} {} {}",
        "\u{25a0}".dimmed(),
        "Metrics:".dimmed(),
        parts.join(" \u{00b7} ").dimmed()
    ));
    lines.push(String::new());
}

fn render_findings(
    lines: &mut Vec<String>,
    report: &crate::health_types::HealthReport,
    root: &Path,
) {
    if report.findings.is_empty() {
        return;
    }

    lines.push(format!(
        "{} {}",
        "\u{25cf}".red(),
        if report.findings.len() < report.summary.functions_above_threshold {
            format!(
                "High complexity functions ({} shown, {} total)",
                report.findings.len(),
                report.summary.functions_above_threshold
            )
        } else {
            format!(
                "High complexity functions ({})",
                report.summary.functions_above_threshold
            )
        }
        .red()
        .bold()
    ));

    let mut last_file = String::new();
    for finding in &report.findings {
        let file_str = relative_path(&finding.path, root).display().to_string();
        if file_str != last_file {
            lines.push(format!("  {}", format_path(&file_str)));
            last_file = file_str;
        }

        let cyc_val = format!("{:>3}", finding.cyclomatic);
        let cog_val = format!("{:>3}", finding.cognitive);

        let cyc_colored = if finding.cyclomatic > report.summary.max_cyclomatic_threshold {
            cyc_val.red().bold().to_string()
        } else {
            cyc_val.dimmed().to_string()
        };
        let cog_colored = if finding.cognitive > report.summary.max_cognitive_threshold {
            cog_val.red().bold().to_string()
        } else {
            cog_val.dimmed().to_string()
        };

        // Line 1: function name (tag likely generated code)
        let generated_tag = if is_likely_generated(&finding.name, finding.cyclomatic) {
            format!(" {}", "(generated)".dimmed())
        } else {
            String::new()
        };
        lines.push(format!(
            "    {} {}{}",
            format!(":{}", finding.line).dimmed(),
            finding.name.bold(),
            generated_tag,
        ));
        // Line 2: metrics (indented, aligned like hotspots)
        lines.push(format!(
            "         {} cyclomatic  {} cognitive  {} lines",
            cyc_colored,
            cog_colored,
            format!("{:>3}", finding.line_count).dimmed(),
        ));
    }
    lines.push(format!(
        "  {}",
        format!(
            "Functions exceeding cyclomatic or cognitive complexity thresholds \u{2014} {DOCS_HEALTH}#complexity-metrics"
        )
        .dimmed()
    ));
    lines.push(String::new());
}

/// Detect likely generated code based on function name patterns.
fn is_likely_generated(name: &str, cyclomatic: u16) -> bool {
    // AJV-style validators: validate0, validate10, validate123
    if name.starts_with("validate")
        && name.len() > 8
        && name[8..].chars().all(|c| c.is_ascii_digit())
    {
        return true;
    }
    // Extremely high complexity with generic names suggests generated/bundled code
    if cyclomatic > 200 && (name == "module.exports" || name == "default" || name == "<anonymous>")
    {
        return true;
    }
    false
}

/// Check if a refactoring recommendation references a likely-generated function name.
///
/// Recommendations from Rule 5 embed function names like `"Extract validate10 (cognitive: 350)"`.
/// This detects those patterns so the display can tag them.
fn recommendation_mentions_generated(recommendation: &str) -> bool {
    // Look for AJV-style validator names: "validate" followed immediately by digits
    let mut rest = recommendation;
    while let Some(pos) = rest.find("validate") {
        let after_validate = &rest[pos + 8..];
        if !after_validate.is_empty() {
            let digits: String = after_validate
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                // Ensure next char after digits is not alphanumeric (word boundary)
                let next = after_validate.chars().nth(digits.len());
                if !next.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    return true;
                }
            }
        }
        rest = &rest[pos + 8..];
    }
    false
}

fn render_file_scores(
    lines: &mut Vec<String>,
    report: &crate::health_types::HealthReport,
    root: &Path,
) {
    if report.file_scores.is_empty() {
        return;
    }

    lines.push(format!(
        "{} {}",
        "\u{25cf}".cyan(),
        format!("File health scores ({} files)", report.file_scores.len())
            .cyan()
            .bold()
    ));
    lines.push(String::new());

    let shown_scores = report.file_scores.len().min(MAX_FLAT_ITEMS);
    for score in &report.file_scores[..shown_scores] {
        let file_str = relative_path(&score.path, root).display().to_string();
        let mi = score.maintainability_index;

        // MI score: color-coded by quality
        let mi_str = format!("{mi:>5.1}");
        let mi_colored = if mi >= 80.0 {
            mi_str.green().to_string()
        } else if mi >= 50.0 {
            mi_str.yellow().to_string()
        } else {
            mi_str.red().bold().to_string()
        };

        // Path: dim directory, normal filename
        let (dir, filename) = split_dir_filename(&file_str);

        // Line 1: MI score + path
        lines.push(format!("  {}    {}{}", mi_colored, dir.dimmed(), filename,));

        // Line 2: metrics (indented, dimmed)
        lines.push(format!(
            "         {} fan-in  {} fan-out  {} dead  {} density",
            format!("{:>3}", score.fan_in).dimmed(),
            format!("{:>3}", score.fan_out).dimmed(),
            format!("{:>3.0}%", score.dead_code_ratio * 100.0).dimmed(),
            format!("{:.2}", score.complexity_density).dimmed(),
        ));

        // Blank line between entries
        lines.push(String::new());
    }
    if report.file_scores.len() > MAX_FLAT_ITEMS {
        lines.push(format!(
            "  {}",
            format!(
                "... and {} more files (--format json for full list)",
                report.file_scores.len() - MAX_FLAT_ITEMS
            )
            .dimmed()
        ));
        lines.push(String::new());
    }
    lines.push(format!(
        "  {}",
        format!("Composite file quality scores based on complexity, coupling, and dead code \u{2014} {DOCS_HEALTH}#file-health-scores").dimmed()
    ));
    lines.push(String::new());
}

fn render_coverage_gaps(
    lines: &mut Vec<String>,
    report: &crate::health_types::HealthReport,
    root: &Path,
) {
    let Some(ref gaps) = report.coverage_gaps else {
        return;
    };

    lines.push(format!(
        "{} {}",
        "\u{25cf}".yellow(),
        format!(
            "Coverage gaps ({} untested {}, {} untested {}, {:.1}% file coverage)",
            gaps.summary.untested_files,
            if gaps.summary.untested_files == 1 {
                "file"
            } else {
                "files"
            },
            gaps.summary.untested_exports,
            if gaps.summary.untested_exports == 1 {
                "export"
            } else {
                "exports"
            },
            gaps.summary.file_coverage_pct,
        )
        .yellow()
        .bold()
    ));
    lines.push(String::new());

    if !gaps.files.is_empty() {
        let shown_files = gaps.files.len().min(MAX_FLAT_ITEMS);
        lines.push(format!("  {}", "Files".dimmed()));
        for item in &gaps.files[..shown_files] {
            let file_str = relative_path(&item.path, root).display().to_string();
            let (dir, filename) = split_dir_filename(&file_str);
            lines.push(format!("  {}{}", dir.dimmed(), filename,));
        }
        if gaps.files.len() > MAX_FLAT_ITEMS {
            lines.push(format!(
                "  {}",
                format!(
                    "... and {} more files (--format json for full list)",
                    gaps.files.len() - MAX_FLAT_ITEMS
                )
                .dimmed()
            ));
        }
        lines.push(String::new());
    }

    if !gaps.exports.is_empty() {
        lines.push(format!("  {}", "Exports".dimmed()));

        // Group exports by file for barrel file collapsing
        let mut by_file: Vec<(&std::path::Path, Vec<&crate::health_types::UntestedExport>)> =
            Vec::new();
        for item in &gaps.exports {
            if let Some(entry) = by_file
                .last_mut()
                .filter(|(p, _)| *p == item.path.as_path())
            {
                entry.1.push(item);
            } else {
                by_file.push((item.path.as_path(), vec![item]));
            }
        }

        let mut shown = 0;
        for (file_path, exports) in &by_file {
            if shown >= MAX_FLAT_ITEMS {
                break;
            }
            let file_str = relative_path(file_path, root).display().to_string();
            if exports.len() > 10 {
                // Barrel file: collapse into a single summary line
                lines.push(format!(
                    "  {} ({} untested re-exports)",
                    file_str.dimmed(),
                    exports.len(),
                ));
                shown += 1;
            } else {
                for item in exports {
                    if shown >= MAX_FLAT_ITEMS {
                        break;
                    }
                    lines.push(format!(
                        "  {}:{} `{}`",
                        file_str.dimmed(),
                        item.line,
                        item.export_name,
                    ));
                    shown += 1;
                }
            }
        }
        let total_exports = gaps.exports.len();
        if total_exports > shown {
            lines.push(format!(
                "  {}",
                format!(
                    "... and {} more exports (--format json for full list)",
                    total_exports - shown
                )
                .dimmed()
            ));
        }
        lines.push(String::new());
    }

    lines.push(format!(
        "  {}",
        format!(
            "Static test dependency gaps (not line-level coverage): {DOCS_HEALTH}#coverage-gaps"
        )
        .dimmed()
    ));
    lines.push(String::new());
}

fn render_hotspots(
    lines: &mut Vec<String>,
    report: &crate::health_types::HealthReport,
    root: &Path,
) {
    if report.hotspots.is_empty() {
        return;
    }

    let header = report.hotspot_summary.as_ref().map_or_else(
        || format!("Hotspots ({} files)", report.hotspots.len()),
        |summary| {
            format!(
                "Hotspots ({} files, since {})",
                report.hotspots.len(),
                summary.since,
            )
        },
    );
    lines.push(format!("{} {}", "\u{25cf}".red(), header.red().bold()));
    lines.push(String::new());

    for entry in &report.hotspots {
        let file_str = relative_path(&entry.path, root).display().to_string();

        // Score: color-coded by severity
        let score_str = format!("{:>5.1}", entry.score);
        let score_colored = if entry.score >= 70.0 {
            score_str.red().bold().to_string()
        } else if entry.score >= 30.0 {
            score_str.yellow().to_string()
        } else {
            score_str.green().to_string()
        };

        // Trend: symbol + color
        let (trend_symbol, trend_colored) = match entry.trend {
            fallow_core::churn::ChurnTrend::Accelerating => {
                ("\u{25b2}", "\u{25b2} accelerating".red().to_string())
            }
            fallow_core::churn::ChurnTrend::Cooling => {
                ("\u{25bc}", "\u{25bc} cooling".green().to_string())
            }
            fallow_core::churn::ChurnTrend::Stable => {
                ("\u{2500}", "\u{2500} stable".dimmed().to_string())
            }
        };

        // Path: dim directory, normal filename
        let (dir, filename) = split_dir_filename(&file_str);

        // Line 1: score + trend symbol + path
        lines.push(format!(
            "  {} {}  {}{}",
            score_colored,
            match entry.trend {
                fallow_core::churn::ChurnTrend::Accelerating => trend_symbol.red().to_string(),
                fallow_core::churn::ChurnTrend::Cooling => trend_symbol.green().to_string(),
                fallow_core::churn::ChurnTrend::Stable => trend_symbol.dimmed().to_string(),
            },
            dir.dimmed(),
            filename,
        ));

        // Line 2: metrics (indented, dimmed) + trend label
        lines.push(format!(
            "         {} commits  {} churn  {} density  {} fan-in  {}",
            format!("{:>3}", entry.commits).dimmed(),
            format!("{:>5}", entry.lines_added + entry.lines_deleted).dimmed(),
            format!("{:.2}", entry.complexity_density).dimmed(),
            format!("{:>2}", entry.fan_in).dimmed(),
            trend_colored,
        ));

        // Blank line between entries
        lines.push(String::new());
    }

    if let Some(ref summary) = report.hotspot_summary
        && summary.files_excluded > 0
    {
        lines.push(format!(
            "  {}",
            format!(
                "{} file{} excluded (< {} commits)",
                summary.files_excluded,
                plural(summary.files_excluded),
                summary.min_commits,
            )
            .dimmed()
        ));
        lines.push(String::new());
    }
    lines.push(format!(
        "  {}",
        format!("Files with high churn and high complexity \u{2014} {DOCS_HEALTH}#hotspot-metrics")
            .dimmed()
    ));
    lines.push(String::new());
}

fn render_refactoring_targets(
    lines: &mut Vec<String>,
    report: &crate::health_types::HealthReport,
    root: &Path,
) {
    if report.targets.is_empty() {
        return;
    }

    lines.push(format!(
        "{} {}",
        "\u{25cf}".cyan(),
        format!("Refactoring targets ({})", report.targets.len())
            .cyan()
            .bold()
    ));

    // Effort summary: "3 low effort · 5 medium effort · 2 high effort"
    let low = report
        .targets
        .iter()
        .filter(|t| matches!(t.effort, crate::health_types::EffortEstimate::Low))
        .count();
    let medium = report
        .targets
        .iter()
        .filter(|t| matches!(t.effort, crate::health_types::EffortEstimate::Medium))
        .count();
    let high = report
        .targets
        .iter()
        .filter(|t| matches!(t.effort, crate::health_types::EffortEstimate::High))
        .count();
    let mut effort_parts = Vec::new();
    if low > 0 {
        effort_parts.push(format!("{low} low effort"));
    }
    if medium > 0 {
        effort_parts.push(format!("{medium} medium"));
    }
    if high > 0 {
        effort_parts.push(format!("{high} high"));
    }
    lines.push(format!("  {}", effort_parts.join(" \u{00b7} ").dimmed()));
    lines.push(format!(
        "  {}",
        "  score = quick-win ROI (higher = better) \u{00b7} pri = absolute priority".dimmed()
    ));
    lines.push(String::new());

    let shown_targets = report.targets.len().min(MAX_FLAT_ITEMS);
    for target in &report.targets[..shown_targets] {
        let file_str = relative_path(&target.path, root).display().to_string();

        // Efficiency score (sort key): color-coded by quick-win value
        let eff_str = format!("{:>5.1}", target.efficiency);
        let eff_colored = if target.efficiency >= 40.0 {
            eff_str.green().to_string()
        } else if target.efficiency >= 20.0 {
            eff_str.yellow().to_string()
        } else {
            eff_str.dimmed().to_string()
        };

        // Path: dim directory, normal filename
        let (dir, filename) = split_dir_filename(&file_str);

        // Line 1: efficiency (sort key) + priority (secondary) + path
        lines.push(format!(
            "  {}  {}    {}{}",
            eff_colored,
            format!("pri:{:.1}", target.priority).dimmed(),
            dir.dimmed(),
            filename,
        ));

        // Line 2: category (yellow) + effort:label (colored) + confidence:label + recommendation (dimmed)
        let label = target.category.label();
        let effort = target.effort.label();
        let effort_colored = match target.effort {
            crate::health_types::EffortEstimate::Low => effort.green().to_string(),
            crate::health_types::EffortEstimate::Medium => effort.yellow().to_string(),
            crate::health_types::EffortEstimate::High => effort.red().to_string(),
        };
        let confidence = target.confidence.label();
        let confidence_colored = match target.confidence {
            crate::health_types::Confidence::High => confidence.green().to_string(),
            crate::health_types::Confidence::Medium => confidence.yellow().to_string(),
            crate::health_types::Confidence::Low => confidence.dimmed().to_string(),
        };
        let generated_tag = if recommendation_mentions_generated(&target.recommendation) {
            format!(" {}", "(generated)".dimmed())
        } else {
            String::new()
        };
        lines.push(format!(
            "         {} \u{00b7} effort:{} \u{00b7} confidence:{}  {}{}",
            label.yellow(),
            effort_colored,
            confidence_colored,
            target.recommendation.dimmed(),
            generated_tag,
        ));

        // Blank line between entries
        lines.push(String::new());
    }
    if report.targets.len() > MAX_FLAT_ITEMS {
        lines.push(format!(
            "  {}",
            format!(
                "... and {} more targets (--format json for full list)",
                report.targets.len() - MAX_FLAT_ITEMS
            )
            .dimmed()
        ));
        lines.push(String::new());
    }
    lines.push(format!(
        "  {}",
        format!(
            "Prioritized refactoring recommendations based on complexity, churn, and coupling signals \u{2014} {DOCS_HEALTH}#refactoring-targets"
        )
        .dimmed()
    ));
    lines.push(String::new());
}

/// Print a concise health summary showing only aggregate statistics.
pub(in crate::report) fn print_health_summary(
    report: &crate::health_types::HealthReport,
    elapsed: Duration,
    quiet: bool,
) {
    let s = &report.summary;

    println!("{}", "Health Summary".bold());
    println!();
    println!("  {:>6}  Functions analyzed", s.functions_analyzed);
    println!("  {:>6}  Above threshold", s.functions_above_threshold);
    if let Some(mi) = s.average_maintainability {
        let label = if mi >= 85.0 {
            "good"
        } else if mi >= 65.0 {
            "moderate"
        } else {
            "low"
        };
        println!("  {mi:>5.1}   Average maintainability ({label})");
    }
    if let Some(ref score) = report.health_score {
        println!("  {:>5.0} {}  Health score", score.score, score.grade);
    }
    if let Some(ref gaps) = report.coverage_gaps {
        println!(
            "  {:>6}  Untested {} ({:.1}% file coverage)",
            gaps.summary.untested_files,
            if gaps.summary.untested_files == 1 {
                "file"
            } else {
                "files"
            },
            gaps.summary.file_coverage_pct,
        );
        println!(
            "  {:>6}  Untested {}",
            gaps.summary.untested_exports,
            if gaps.summary.untested_exports == 1 {
                "export"
            } else {
                "exports"
            },
        );
    }

    if !quiet {
        eprintln!(
            "{}",
            format!(
                "\u{2713} {} functions analyzed ({:.2}s)",
                s.functions_analyzed,
                elapsed.as_secs_f64()
            )
            .green()
            .bold()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Strip ANSI escape sequences from a string, leaving only the printable text.
    fn strip_ansi(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for inner in chars.by_ref() {
                    if inner == 'm' {
                        break;
                    }
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    fn plain(lines: &[String]) -> String {
        lines
            .iter()
            .map(|l| strip_ansi(l))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn health_empty_findings_produces_no_header() {
        let root = PathBuf::from("/project");
        let report = crate::health_types::HealthReport {
            findings: vec![],
            summary: crate::health_types::HealthSummary {
                files_analyzed: 10,
                functions_analyzed: 50,
                functions_above_threshold: 0,
                max_cyclomatic_threshold: 20,
                max_cognitive_threshold: 15,
                files_scored: None,
                average_maintainability: None,
            },
            vital_signs: None,
            health_score: None,
            file_scores: vec![],
            coverage_gaps: None,
            hotspots: vec![],
            hotspot_summary: None,
            targets: vec![],
            target_thresholds: None,
            health_trend: None,
        };
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        // With no findings and no file scores, no complexity header is produced
        assert!(!text.contains("High complexity functions"));
    }

    #[test]
    fn health_findings_show_function_details() {
        let root = PathBuf::from("/project");
        let report = crate::health_types::HealthReport {
            findings: vec![crate::health_types::HealthFinding {
                path: root.join("src/parser.ts"),
                name: "parseExpression".to_string(),
                line: 42,
                col: 0,
                cyclomatic: 25,
                cognitive: 30,
                line_count: 80,
                exceeded: crate::health_types::ExceededThreshold::Both,
            }],
            summary: crate::health_types::HealthSummary {
                files_analyzed: 10,
                functions_analyzed: 50,
                functions_above_threshold: 1,
                max_cyclomatic_threshold: 20,
                max_cognitive_threshold: 15,
                files_scored: None,
                average_maintainability: None,
            },
            vital_signs: None,
            health_score: None,
            file_scores: vec![],
            coverage_gaps: None,
            hotspots: vec![],
            hotspot_summary: None,
            targets: vec![],
            target_thresholds: None,
            health_trend: None,
        };
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("High complexity functions (1)"));
        assert!(text.contains("src/parser.ts"));
        assert!(text.contains(":42"));
        assert!(text.contains("parseExpression"));
        assert!(text.contains("25 cyclomatic"));
        assert!(text.contains("30 cognitive"));
        assert!(text.contains("80 lines"));
    }

    #[test]
    fn health_shown_vs_total_when_truncated() {
        let root = PathBuf::from("/project");
        let report = crate::health_types::HealthReport {
            findings: vec![crate::health_types::HealthFinding {
                path: root.join("src/a.ts"),
                name: "fn1".to_string(),
                line: 1,
                col: 0,
                cyclomatic: 25,
                cognitive: 20,
                line_count: 50,
                exceeded: crate::health_types::ExceededThreshold::Both,
            }],
            summary: crate::health_types::HealthSummary {
                files_analyzed: 100,
                functions_analyzed: 500,
                functions_above_threshold: 10,
                max_cyclomatic_threshold: 20,
                max_cognitive_threshold: 15,
                files_scored: None,
                average_maintainability: None,
            },
            vital_signs: None,
            health_score: None,
            file_scores: vec![],
            coverage_gaps: None,
            hotspots: vec![],
            hotspot_summary: None,
            targets: vec![],
            target_thresholds: None,
            health_trend: None,
        };
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        // When shown < total, header says "N shown, M total"
        assert!(text.contains("1 shown, 10 total"));
    }

    #[test]
    fn health_findings_grouped_by_file() {
        let root = PathBuf::from("/project");
        let report = crate::health_types::HealthReport {
            findings: vec![
                crate::health_types::HealthFinding {
                    path: root.join("src/parser.ts"),
                    name: "fn1".to_string(),
                    line: 10,
                    col: 0,
                    cyclomatic: 25,
                    cognitive: 20,
                    line_count: 40,
                    exceeded: crate::health_types::ExceededThreshold::Both,
                },
                crate::health_types::HealthFinding {
                    path: root.join("src/parser.ts"),
                    name: "fn2".to_string(),
                    line: 60,
                    col: 0,
                    cyclomatic: 22,
                    cognitive: 18,
                    line_count: 30,
                    exceeded: crate::health_types::ExceededThreshold::Both,
                },
            ],
            summary: crate::health_types::HealthSummary {
                files_analyzed: 10,
                functions_analyzed: 50,
                functions_above_threshold: 2,
                max_cyclomatic_threshold: 20,
                max_cognitive_threshold: 15,
                files_scored: None,
                average_maintainability: None,
            },
            vital_signs: None,
            health_score: None,
            file_scores: vec![],
            coverage_gaps: None,
            hotspots: vec![],
            hotspot_summary: None,
            targets: vec![],
            target_thresholds: None,
            health_trend: None,
        };
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        // File path should appear once (grouping)
        let count = text.matches("src/parser.ts").count();
        assert_eq!(count, 1, "File header should appear once for grouped items");
    }

    // ── Helper: build an empty base report ───────────────────────

    fn empty_report() -> crate::health_types::HealthReport {
        crate::health_types::HealthReport {
            findings: vec![],
            summary: crate::health_types::HealthSummary {
                files_analyzed: 10,
                functions_analyzed: 50,
                functions_above_threshold: 0,
                max_cyclomatic_threshold: 20,
                max_cognitive_threshold: 15,
                files_scored: None,
                average_maintainability: None,
            },
            vital_signs: None,
            health_score: None,
            file_scores: vec![],
            coverage_gaps: None,
            hotspots: vec![],
            hotspot_summary: None,
            targets: vec![],
            target_thresholds: None,
            health_trend: None,
        }
    }

    #[test]
    fn health_coverage_gaps_render_section() {
        use crate::health_types::*;

        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.coverage_gaps = Some(CoverageGaps {
            summary: CoverageGapSummary {
                runtime_files: 1,
                covered_files: 0,
                file_coverage_pct: 0.0,
                untested_files: 1,
                untested_exports: 1,
            },
            files: vec![UntestedFile {
                path: root.join("src/app.ts"),
                value_export_count: 2,
            }],
            exports: vec![UntestedExport {
                path: root.join("src/app.ts"),
                export_name: "loader".into(),
                line: 12,
                col: 4,
            }],
        });

        let text = plain(&build_health_human_lines(&report, &root));
        assert!(
            text.contains("Coverage gaps (1 untested file, 1 untested export, 0.0% file coverage)")
        );
        assert!(text.contains("src/app.ts"));
        assert!(text.contains("loader"));
    }

    // ── fmt_trend_val / fmt_trend_delta ───────────────────────────

    #[test]
    fn fmt_trend_val_percentage() {
        assert_eq!(fmt_trend_val(15.5, "%"), "15.5%");
        assert_eq!(fmt_trend_val(0.0, "%"), "0.0%");
    }

    #[test]
    fn fmt_trend_val_integer_when_round() {
        assert_eq!(fmt_trend_val(72.0, ""), "72");
        assert_eq!(fmt_trend_val(5.0, "pts"), "5");
    }

    #[test]
    fn fmt_trend_val_decimal_when_fractional() {
        assert_eq!(fmt_trend_val(4.7, ""), "4.7");
        assert_eq!(fmt_trend_val(1.3, "pts"), "1.3");
    }

    #[test]
    fn fmt_trend_delta_percentage() {
        assert_eq!(fmt_trend_delta(2.5, "%"), "+2.5%");
        assert_eq!(fmt_trend_delta(-1.3, "%"), "-1.3%");
    }

    #[test]
    fn fmt_trend_delta_integer_when_round() {
        assert_eq!(fmt_trend_delta(5.0, ""), "+5");
        assert_eq!(fmt_trend_delta(-3.0, "pts"), "-3");
    }

    #[test]
    fn fmt_trend_delta_decimal_when_fractional() {
        assert_eq!(fmt_trend_delta(4.9, ""), "+4.9");
        assert_eq!(fmt_trend_delta(-0.7, "pts"), "-0.7");
    }

    // ── render_health_score ──────────────────────────────────────

    #[test]
    fn health_score_grade_a_display() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.health_score = Some(crate::health_types::HealthScore {
            score: 92.0,
            grade: "A",
            penalties: crate::health_types::HealthScorePenalties {
                dead_files: Some(3.0),
                dead_exports: Some(2.0),
                complexity: 1.5,
                p90_complexity: 1.5,
                maintainability: Some(0.0),
                hotspots: Some(0.0),
                unused_deps: Some(0.0),
                circular_deps: Some(0.0),
            },
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("Health score:"));
        assert!(text.contains("92 A"));
        assert!(text.contains("dead files -3.0"));
        assert!(text.contains("dead exports -2.0"));
        assert!(text.contains("complexity -1.5"));
        assert!(text.contains("p90 -1.5"));
    }

    #[test]
    fn health_score_grade_b_display() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.health_score = Some(crate::health_types::HealthScore {
            score: 76.0,
            grade: "B",
            penalties: crate::health_types::HealthScorePenalties {
                dead_files: Some(5.0),
                dead_exports: Some(6.0),
                complexity: 3.0,
                p90_complexity: 2.0,
                maintainability: Some(4.0),
                hotspots: Some(2.0),
                unused_deps: Some(1.0),
                circular_deps: Some(1.0),
            },
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("76 B"));
        assert!(text.contains("maintainability -4.0"));
        assert!(text.contains("hotspots -2.0"));
        assert!(text.contains("unused dependencies -1.0"));
        assert!(text.contains("circular dependencies -1.0"));
    }

    #[test]
    fn health_score_grade_c_display() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.health_score = Some(crate::health_types::HealthScore {
            score: 60.0,
            grade: "C",
            penalties: crate::health_types::HealthScorePenalties {
                dead_files: Some(10.0),
                dead_exports: Some(10.0),
                complexity: 10.0,
                p90_complexity: 5.0,
                maintainability: Some(5.0),
                hotspots: None,
                unused_deps: None,
                circular_deps: None,
            },
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("60 C"));
    }

    #[test]
    fn health_score_grade_f_display() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.health_score = Some(crate::health_types::HealthScore {
            score: 30.0,
            grade: "F",
            penalties: crate::health_types::HealthScorePenalties {
                dead_files: Some(15.0),
                dead_exports: Some(15.0),
                complexity: 20.0,
                p90_complexity: 10.0,
                maintainability: Some(10.0),
                hotspots: None,
                unused_deps: None,
                circular_deps: None,
            },
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("30 F"));
    }

    #[test]
    fn health_score_na_components_shown() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.health_score = Some(crate::health_types::HealthScore {
            score: 90.0,
            grade: "A",
            penalties: crate::health_types::HealthScorePenalties {
                dead_files: None,
                dead_exports: None,
                complexity: 0.0,
                p90_complexity: 0.0,
                maintainability: None,
                hotspots: None,
                unused_deps: None,
                circular_deps: None,
            },
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("N/A: dead code, maintainability, hotspots"));
        assert!(text.contains("run --score for full pipeline"));
    }

    #[test]
    fn health_score_no_na_when_all_present() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.health_score = Some(crate::health_types::HealthScore {
            score: 85.0,
            grade: "A",
            penalties: crate::health_types::HealthScorePenalties {
                dead_files: Some(0.0),
                dead_exports: Some(0.0),
                complexity: 0.0,
                p90_complexity: 0.0,
                maintainability: Some(0.0),
                hotspots: Some(0.0),
                unused_deps: Some(0.0),
                circular_deps: Some(0.0),
            },
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(!text.contains("N/A:"));
    }

    #[test]
    fn health_score_zero_penalties_suppressed() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.health_score = Some(crate::health_types::HealthScore {
            score: 100.0,
            grade: "A",
            penalties: crate::health_types::HealthScorePenalties {
                dead_files: Some(0.0),
                dead_exports: Some(0.0),
                complexity: 0.0,
                p90_complexity: 0.0,
                maintainability: Some(0.0),
                hotspots: Some(0.0),
                unused_deps: Some(0.0),
                circular_deps: Some(0.0),
            },
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        // No penalty line when all are zero
        assert!(!text.contains("dead files"));
        assert!(!text.contains("complexity -"));
    }

    // ── render_health_trend ──────────────────────────────────────

    #[test]
    fn health_trend_improving_display() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.health_trend = Some(crate::health_types::HealthTrend {
            compared_to: crate::health_types::TrendPoint {
                timestamp: "2026-03-25T14:30:00Z".into(),
                git_sha: Some("abc1234".into()),
                score: Some(72.0),
                grade: Some("B".into()),
            },
            metrics: vec![
                crate::health_types::TrendMetric {
                    name: "score",
                    label: "Health Score",
                    previous: 72.0,
                    current: 85.0,
                    delta: 13.0,
                    direction: crate::health_types::TrendDirection::Improving,
                    unit: "",
                    previous_count: None,
                    current_count: None,
                },
                crate::health_types::TrendMetric {
                    name: "dead_file_pct",
                    label: "Dead Files",
                    previous: 10.0,
                    current: 5.0,
                    delta: -5.0,
                    direction: crate::health_types::TrendDirection::Improving,
                    unit: "%",
                    previous_count: None,
                    current_count: None,
                },
            ],
            snapshots_loaded: 2,
            overall_direction: crate::health_types::TrendDirection::Improving,
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("Trend:"));
        assert!(text.contains("improving"));
        assert!(text.contains("vs 2026-03-25"));
        assert!(text.contains("abc1234"));
        assert!(text.contains("Health Score"));
        assert!(text.contains("+13"));
        assert!(text.contains("Dead Files"));
        assert!(text.contains("-5.0%"));
    }

    #[test]
    fn health_trend_declining_display() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.health_trend = Some(crate::health_types::HealthTrend {
            compared_to: crate::health_types::TrendPoint {
                timestamp: "2026-03-20T10:00:00Z".into(),
                git_sha: None,
                score: None,
                grade: None,
            },
            metrics: vec![crate::health_types::TrendMetric {
                name: "unused_deps",
                label: "Unused Deps",
                previous: 5.0,
                current: 10.0,
                delta: 5.0,
                direction: crate::health_types::TrendDirection::Declining,
                unit: "",
                previous_count: None,
                current_count: None,
            }],
            snapshots_loaded: 1,
            overall_direction: crate::health_types::TrendDirection::Declining,
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("declining"));
        assert!(text.contains("Unused Deps"));
    }

    #[test]
    fn health_trend_all_stable_collapsed() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.health_trend = Some(crate::health_types::HealthTrend {
            compared_to: crate::health_types::TrendPoint {
                timestamp: "2026-03-25T14:30:00Z".into(),
                git_sha: Some("def5678".into()),
                score: Some(80.0),
                grade: Some("B".into()),
            },
            metrics: vec![
                crate::health_types::TrendMetric {
                    name: "score",
                    label: "Health Score",
                    previous: 80.0,
                    current: 80.0,
                    delta: 0.0,
                    direction: crate::health_types::TrendDirection::Stable,
                    unit: "",
                    previous_count: None,
                    current_count: None,
                },
                crate::health_types::TrendMetric {
                    name: "avg_cyclomatic",
                    label: "Avg Cyclomatic",
                    previous: 2.0,
                    current: 2.0,
                    delta: 0.0,
                    direction: crate::health_types::TrendDirection::Stable,
                    unit: "",
                    previous_count: None,
                    current_count: None,
                },
            ],
            snapshots_loaded: 3,
            overall_direction: crate::health_types::TrendDirection::Stable,
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("stable"));
        assert!(text.contains("All 2 metrics unchanged"));
        // Individual metric rows should NOT appear
        assert!(!text.contains("Health Score"));
    }

    #[test]
    fn health_trend_without_sha() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.health_trend = Some(crate::health_types::HealthTrend {
            compared_to: crate::health_types::TrendPoint {
                timestamp: "2026-03-20T10:00:00Z".into(),
                git_sha: None,
                score: None,
                grade: None,
            },
            metrics: vec![crate::health_types::TrendMetric {
                name: "score",
                label: "Health Score",
                previous: 80.0,
                current: 82.0,
                delta: 2.0,
                direction: crate::health_types::TrendDirection::Improving,
                unit: "",
                previous_count: None,
                current_count: None,
            }],
            snapshots_loaded: 1,
            overall_direction: crate::health_types::TrendDirection::Improving,
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        // No SHA in output
        assert!(text.contains("vs 2026-03-20"));
        assert!(!text.contains("\u{00b7}"));
    }

    // ── render_vital_signs ───────────────────────────────────────

    #[test]
    fn vital_signs_shown_without_trend() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.vital_signs = Some(crate::health_types::VitalSigns {
            dead_file_pct: Some(3.2),
            dead_export_pct: Some(8.1),
            avg_cyclomatic: 4.7,
            p90_cyclomatic: 12,
            duplication_pct: None,
            hotspot_count: Some(2),
            maintainability_avg: Some(72.4),
            unused_dep_count: Some(3),
            circular_dep_count: Some(1),
            counts: None,
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("dead files 3.2%"));
        assert!(text.contains("dead exports 8.1%"));
        assert!(text.contains("avg cyclomatic 4.7"));
        assert!(text.contains("p90 cyclomatic 12"));
        assert!(text.contains("maintainability 72.4"));
        assert!(text.contains("2 churn hotspots"));
        assert!(text.contains("3 unused dependencies"));
        assert!(text.contains("1 circular dependency"));
    }

    #[test]
    fn vital_signs_suppressed_when_trend_active() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.vital_signs = Some(crate::health_types::VitalSigns {
            dead_file_pct: Some(3.2),
            dead_export_pct: Some(8.1),
            avg_cyclomatic: 4.7,
            p90_cyclomatic: 12,
            duplication_pct: None,
            hotspot_count: Some(2),
            maintainability_avg: Some(72.4),
            unused_dep_count: None,
            circular_dep_count: None,
            counts: None,
        });
        report.health_trend = Some(crate::health_types::HealthTrend {
            compared_to: crate::health_types::TrendPoint {
                timestamp: "2026-03-25T14:30:00Z".into(),
                git_sha: None,
                score: None,
                grade: None,
            },
            metrics: vec![],
            snapshots_loaded: 1,
            overall_direction: crate::health_types::TrendDirection::Stable,
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        // vital signs should be suppressed when trend is active
        assert!(!text.contains("dead files"));
        assert!(!text.contains("avg cyclomatic"));
    }

    #[test]
    fn vital_signs_optional_fields_omitted_when_none() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.vital_signs = Some(crate::health_types::VitalSigns {
            dead_file_pct: None,
            dead_export_pct: None,
            avg_cyclomatic: 2.0,
            p90_cyclomatic: 5,
            duplication_pct: None,
            hotspot_count: None,
            maintainability_avg: None,
            unused_dep_count: None,
            circular_dep_count: None,
            counts: None,
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(!text.contains("dead files"));
        assert!(!text.contains("dead exports"));
        assert!(!text.contains("maintainability "));
        assert!(!text.contains("hotspot"));
        assert!(text.contains("avg cyclomatic 2.0"));
        assert!(text.contains("p90 cyclomatic 5"));
    }

    #[test]
    fn vital_signs_zero_counts_suppressed() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.vital_signs = Some(crate::health_types::VitalSigns {
            dead_file_pct: None,
            dead_export_pct: None,
            avg_cyclomatic: 1.0,
            p90_cyclomatic: 2,
            duplication_pct: None,
            hotspot_count: None,
            maintainability_avg: None,
            unused_dep_count: Some(0),
            circular_dep_count: Some(0),
            counts: None,
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        // Zero counts should not appear
        assert!(!text.contains("unused dependenc"));
        assert!(!text.contains("circular dependenc"));
    }

    #[test]
    fn vital_signs_plural_vs_singular() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.vital_signs = Some(crate::health_types::VitalSigns {
            dead_file_pct: None,
            dead_export_pct: None,
            avg_cyclomatic: 1.0,
            p90_cyclomatic: 2,
            duplication_pct: None,
            hotspot_count: Some(1),
            maintainability_avg: None,
            unused_dep_count: Some(1),
            circular_dep_count: Some(2),
            counts: None,
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("1 churn hotspot"));
        assert!(!text.contains("1 churn hotspots"));
        assert!(text.contains("1 unused dependency"));
        assert!(!text.contains("1 unused dependencies"));
        assert!(text.contains("2 circular dependencies"));
    }

    // ── render_file_scores ───────────────────────────────────────

    #[test]
    fn file_scores_single_entry() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.file_scores = vec![crate::health_types::FileHealthScore {
            path: root.join("src/utils.ts"),
            fan_in: 5,
            fan_out: 3,
            dead_code_ratio: 0.15,
            complexity_density: 0.42,
            maintainability_index: 85.3,
            total_cyclomatic: 12,
            total_cognitive: 8,
            function_count: 4,
            lines: 200,
        }];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("File health scores (1 files)"));
        assert!(text.contains("85.3"));
        assert!(text.contains("src/utils.ts"));
        assert!(text.contains("5 fan-in"));
        assert!(text.contains("3 fan-out"));
        assert!(text.contains("15% dead"));
        assert!(text.contains("0.42 density"));
    }

    #[test]
    fn file_scores_mi_color_thresholds() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.file_scores = vec![
            crate::health_types::FileHealthScore {
                path: root.join("src/good.ts"),
                fan_in: 1,
                fan_out: 1,
                dead_code_ratio: 0.0,
                complexity_density: 0.1,
                maintainability_index: 90.0, // green: >= 80
                total_cyclomatic: 2,
                total_cognitive: 1,
                function_count: 1,
                lines: 50,
            },
            crate::health_types::FileHealthScore {
                path: root.join("src/okay.ts"),
                fan_in: 2,
                fan_out: 3,
                dead_code_ratio: 0.1,
                complexity_density: 0.3,
                maintainability_index: 65.0, // yellow: >= 50
                total_cyclomatic: 8,
                total_cognitive: 5,
                function_count: 3,
                lines: 100,
            },
            crate::health_types::FileHealthScore {
                path: root.join("src/bad.ts"),
                fan_in: 8,
                fan_out: 12,
                dead_code_ratio: 0.5,
                complexity_density: 0.9,
                maintainability_index: 30.0, // red: < 50
                total_cyclomatic: 40,
                total_cognitive: 30,
                function_count: 10,
                lines: 500,
            },
        ];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("File health scores (3 files)"));
        assert!(text.contains("90.0"));
        assert!(text.contains("65.0"));
        assert!(text.contains("30.0"));
    }

    #[test]
    fn file_scores_truncation_above_max_flat_items() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        // Create 12 file scores (MAX_FLAT_ITEMS = 10)
        for i in 0..12 {
            report
                .file_scores
                .push(crate::health_types::FileHealthScore {
                    path: root.join(format!("src/file{i}.ts")),
                    fan_in: 1,
                    fan_out: 1,
                    dead_code_ratio: 0.0,
                    complexity_density: 0.1,
                    maintainability_index: 80.0,
                    total_cyclomatic: 2,
                    total_cognitive: 1,
                    function_count: 1,
                    lines: 50,
                });
        }
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("File health scores (12 files)"));
        assert!(text.contains("... and 2 more files"));
        // First 10 should be shown
        assert!(text.contains("file0.ts"));
        assert!(text.contains("file9.ts"));
        // 11th and 12th should not
        assert!(!text.contains("file10.ts"));
        assert!(!text.contains("file11.ts"));
    }

    #[test]
    fn file_scores_docs_link() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.file_scores = vec![crate::health_types::FileHealthScore {
            path: root.join("src/a.ts"),
            fan_in: 1,
            fan_out: 1,
            dead_code_ratio: 0.0,
            complexity_density: 0.1,
            maintainability_index: 80.0,
            total_cyclomatic: 2,
            total_cognitive: 1,
            function_count: 1,
            lines: 50,
        }];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("docs.fallow.tools/explanations/health#file-health-scores"));
    }

    // ── render_hotspots ──────────────────────────────────────────

    #[test]
    fn hotspots_accelerating_trend() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.hotspots = vec![crate::health_types::HotspotEntry {
            path: root.join("src/core.ts"),
            score: 75.0,
            commits: 42,
            weighted_commits: 30.0,
            lines_added: 500,
            lines_deleted: 200,
            complexity_density: 0.85,
            fan_in: 10,
            trend: fallow_core::churn::ChurnTrend::Accelerating,
        }];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("Hotspots (1 files)"));
        assert!(text.contains("75.0"));
        assert!(text.contains("src/core.ts"));
        assert!(text.contains("42 commits"));
        assert!(text.contains("700 churn"));
        assert!(text.contains("0.85 density"));
        assert!(text.contains("10 fan-in"));
        assert!(text.contains("accelerating"));
    }

    #[test]
    fn hotspots_cooling_trend() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.hotspots = vec![crate::health_types::HotspotEntry {
            path: root.join("src/old.ts"),
            score: 20.0,
            commits: 5,
            weighted_commits: 2.0,
            lines_added: 50,
            lines_deleted: 30,
            complexity_density: 0.3,
            fan_in: 2,
            trend: fallow_core::churn::ChurnTrend::Cooling,
        }];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("20.0"));
        assert!(text.contains("cooling"));
    }

    #[test]
    fn hotspots_stable_trend() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.hotspots = vec![crate::health_types::HotspotEntry {
            path: root.join("src/mid.ts"),
            score: 45.0,
            commits: 15,
            weighted_commits: 10.0,
            lines_added: 200,
            lines_deleted: 100,
            complexity_density: 0.5,
            fan_in: 5,
            trend: fallow_core::churn::ChurnTrend::Stable,
        }];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("45.0"));
        assert!(text.contains("stable"));
    }

    #[test]
    fn hotspots_with_summary_and_since() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.hotspots = vec![crate::health_types::HotspotEntry {
            path: root.join("src/a.ts"),
            score: 50.0,
            commits: 10,
            weighted_commits: 8.0,
            lines_added: 100,
            lines_deleted: 50,
            complexity_density: 0.4,
            fan_in: 3,
            trend: fallow_core::churn::ChurnTrend::Stable,
        }];
        report.hotspot_summary = Some(crate::health_types::HotspotSummary {
            since: "6 months".to_string(),
            min_commits: 3,
            files_analyzed: 50,
            files_excluded: 20,
            shallow_clone: false,
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("Hotspots (1 files, since 6 months)"));
        assert!(text.contains("20 files excluded (< 3 commits)"));
    }

    #[test]
    fn hotspots_summary_no_exclusions() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.hotspots = vec![crate::health_types::HotspotEntry {
            path: root.join("src/a.ts"),
            score: 50.0,
            commits: 10,
            weighted_commits: 8.0,
            lines_added: 100,
            lines_deleted: 50,
            complexity_density: 0.4,
            fan_in: 3,
            trend: fallow_core::churn::ChurnTrend::Stable,
        }];
        report.hotspot_summary = Some(crate::health_types::HotspotSummary {
            since: "3 months".to_string(),
            min_commits: 2,
            files_analyzed: 50,
            files_excluded: 0,
            shallow_clone: false,
        });
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        // No exclusion line when files_excluded == 0
        assert!(!text.contains("files excluded"));
    }

    #[test]
    fn hotspots_docs_link() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.hotspots = vec![crate::health_types::HotspotEntry {
            path: root.join("src/a.ts"),
            score: 50.0,
            commits: 10,
            weighted_commits: 8.0,
            lines_added: 100,
            lines_deleted: 50,
            complexity_density: 0.4,
            fan_in: 3,
            trend: fallow_core::churn::ChurnTrend::Stable,
        }];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("docs.fallow.tools/explanations/health#hotspot-metrics"));
    }

    // ── render_refactoring_targets ───────────────────────────────

    #[test]
    fn refactoring_targets_single_low_effort() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.targets = vec![crate::health_types::RefactoringTarget {
            path: root.join("src/legacy.ts"),
            priority: 65.0,
            efficiency: 65.0,
            recommendation: "Extract complex logic into helper functions".to_string(),
            category: crate::health_types::RecommendationCategory::ExtractComplexFunctions,
            effort: crate::health_types::EffortEstimate::Low,
            confidence: crate::health_types::Confidence::High,
            factors: vec![],
            evidence: None,
        }];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("Refactoring targets (1)"));
        assert!(text.contains("1 low effort"));
        assert!(text.contains("65.0"));
        assert!(text.contains("pri:65.0"));
        assert!(text.contains("src/legacy.ts"));
        assert!(text.contains("complexity"));
        assert!(text.contains("effort:low"));
        assert!(text.contains("confidence:high"));
        assert!(text.contains("Extract complex logic into helper functions"));
    }

    #[test]
    fn refactoring_targets_mixed_effort() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.targets = vec![
            crate::health_types::RefactoringTarget {
                path: root.join("src/a.ts"),
                priority: 80.0,
                efficiency: 80.0,
                recommendation: "Remove dead exports".to_string(),
                category: crate::health_types::RecommendationCategory::RemoveDeadCode,
                effort: crate::health_types::EffortEstimate::Low,
                confidence: crate::health_types::Confidence::High,
                factors: vec![],
                evidence: None,
            },
            crate::health_types::RefactoringTarget {
                path: root.join("src/b.ts"),
                priority: 60.0,
                efficiency: 30.0,
                recommendation: "Split into smaller modules".to_string(),
                category: crate::health_types::RecommendationCategory::SplitHighImpact,
                effort: crate::health_types::EffortEstimate::Medium,
                confidence: crate::health_types::Confidence::Medium,
                factors: vec![],
                evidence: None,
            },
            crate::health_types::RefactoringTarget {
                path: root.join("src/c.ts"),
                priority: 50.0,
                efficiency: 16.7,
                recommendation: "Break circular dependency".to_string(),
                category: crate::health_types::RecommendationCategory::BreakCircularDependency,
                effort: crate::health_types::EffortEstimate::High,
                confidence: crate::health_types::Confidence::Low,
                factors: vec![],
                evidence: None,
            },
        ];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("Refactoring targets (3)"));
        assert!(text.contains("1 low effort"));
        assert!(text.contains("1 medium"));
        assert!(text.contains("1 high"));
        assert!(text.contains("effort:low"));
        assert!(text.contains("effort:medium"));
        assert!(text.contains("effort:high"));
        assert!(text.contains("confidence:high"));
        assert!(text.contains("confidence:medium"));
        assert!(text.contains("confidence:low"));
    }

    #[test]
    fn refactoring_targets_truncation_above_max_flat_items() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        for i in 0..12 {
            report.targets.push(crate::health_types::RefactoringTarget {
                path: root.join(format!("src/target{i}.ts")),
                priority: 50.0,
                efficiency: 25.0,
                recommendation: format!("Fix target {i}"),
                category: crate::health_types::RecommendationCategory::ExtractComplexFunctions,
                effort: crate::health_types::EffortEstimate::Medium,
                confidence: crate::health_types::Confidence::Medium,
                factors: vec![],
                evidence: None,
            });
        }
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("Refactoring targets (12)"));
        assert!(text.contains("... and 2 more targets"));
        assert!(text.contains("target0.ts"));
        assert!(text.contains("target9.ts"));
        assert!(!text.contains("target10.ts"));
    }

    #[test]
    fn refactoring_targets_docs_link() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.targets = vec![crate::health_types::RefactoringTarget {
            path: root.join("src/a.ts"),
            priority: 50.0,
            efficiency: 50.0,
            recommendation: "Fix it".to_string(),
            category: crate::health_types::RecommendationCategory::ExtractDependencies,
            effort: crate::health_types::EffortEstimate::Low,
            confidence: crate::health_types::Confidence::High,
            factors: vec![],
            evidence: None,
        }];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("docs.fallow.tools/explanations/health#refactoring-targets"));
    }

    #[test]
    fn refactoring_targets_all_categories() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        let categories = [
            (
                crate::health_types::RecommendationCategory::UrgentChurnComplexity,
                "churn+complexity",
            ),
            (
                crate::health_types::RecommendationCategory::BreakCircularDependency,
                "circular dependency",
            ),
            (
                crate::health_types::RecommendationCategory::SplitHighImpact,
                "high impact",
            ),
            (
                crate::health_types::RecommendationCategory::RemoveDeadCode,
                "dead code",
            ),
            (
                crate::health_types::RecommendationCategory::ExtractComplexFunctions,
                "complexity",
            ),
            (
                crate::health_types::RecommendationCategory::ExtractDependencies,
                "coupling",
            ),
        ];
        for (i, (cat, _label)) in categories.iter().enumerate() {
            report.targets.push(crate::health_types::RefactoringTarget {
                path: root.join(format!("src/cat{i}.ts")),
                priority: 50.0,
                efficiency: 50.0,
                recommendation: format!("Fix cat{i}"),
                category: cat.clone(),
                effort: crate::health_types::EffortEstimate::Low,
                confidence: crate::health_types::Confidence::High,
                factors: vec![],
                evidence: None,
            });
        }
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        for (_cat, label) in &categories {
            assert!(
                text.contains(label),
                "Expected category label '{label}' in output"
            );
        }
    }

    #[test]
    fn refactoring_targets_efficiency_color_thresholds() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.targets = vec![
            crate::health_types::RefactoringTarget {
                path: root.join("src/high.ts"),
                priority: 50.0,
                efficiency: 50.0, // green: >= 40
                recommendation: "High eff".to_string(),
                category: crate::health_types::RecommendationCategory::RemoveDeadCode,
                effort: crate::health_types::EffortEstimate::Low,
                confidence: crate::health_types::Confidence::High,
                factors: vec![],
                evidence: None,
            },
            crate::health_types::RefactoringTarget {
                path: root.join("src/mid.ts"),
                priority: 50.0,
                efficiency: 25.0, // yellow: >= 20
                recommendation: "Mid eff".to_string(),
                category: crate::health_types::RecommendationCategory::RemoveDeadCode,
                effort: crate::health_types::EffortEstimate::Medium,
                confidence: crate::health_types::Confidence::Medium,
                factors: vec![],
                evidence: None,
            },
            crate::health_types::RefactoringTarget {
                path: root.join("src/low.ts"),
                priority: 50.0,
                efficiency: 10.0, // dimmed: < 20
                recommendation: "Low eff".to_string(),
                category: crate::health_types::RecommendationCategory::RemoveDeadCode,
                effort: crate::health_types::EffortEstimate::High,
                confidence: crate::health_types::Confidence::Low,
                factors: vec![],
                evidence: None,
            },
        ];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("50.0"));
        assert!(text.contains("25.0"));
        assert!(text.contains("10.0"));
    }

    // ── Combined sections ────────────────────────────────────────

    #[test]
    fn all_sections_combined() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.summary.functions_above_threshold = 1;
        report.findings = vec![crate::health_types::HealthFinding {
            path: root.join("src/complex.ts"),
            name: "bigFn".to_string(),
            line: 10,
            col: 0,
            cyclomatic: 25,
            cognitive: 20,
            line_count: 80,
            exceeded: crate::health_types::ExceededThreshold::Both,
        }];
        report.health_score = Some(crate::health_types::HealthScore {
            score: 75.0,
            grade: "B",
            penalties: crate::health_types::HealthScorePenalties {
                dead_files: Some(5.0),
                dead_exports: Some(5.0),
                complexity: 5.0,
                p90_complexity: 2.0,
                maintainability: Some(3.0),
                hotspots: Some(2.0),
                unused_deps: Some(2.0),
                circular_deps: Some(1.0),
            },
        });
        report.file_scores = vec![crate::health_types::FileHealthScore {
            path: root.join("src/complex.ts"),
            fan_in: 5,
            fan_out: 3,
            dead_code_ratio: 0.1,
            complexity_density: 0.5,
            maintainability_index: 60.0,
            total_cyclomatic: 15,
            total_cognitive: 10,
            function_count: 3,
            lines: 200,
        }];
        report.hotspots = vec![crate::health_types::HotspotEntry {
            path: root.join("src/complex.ts"),
            score: 65.0,
            commits: 20,
            weighted_commits: 15.0,
            lines_added: 300,
            lines_deleted: 100,
            complexity_density: 0.5,
            fan_in: 5,
            trend: fallow_core::churn::ChurnTrend::Accelerating,
        }];
        report.targets = vec![crate::health_types::RefactoringTarget {
            path: root.join("src/complex.ts"),
            priority: 70.0,
            efficiency: 70.0,
            recommendation: "Extract complex functions".to_string(),
            category: crate::health_types::RecommendationCategory::ExtractComplexFunctions,
            effort: crate::health_types::EffortEstimate::Low,
            confidence: crate::health_types::Confidence::High,
            factors: vec![],
            evidence: None,
        }];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        // All sections present
        assert!(text.contains("Health score:"));
        assert!(text.contains("High complexity functions"));
        assert!(text.contains("File health scores"));
        assert!(text.contains("Hotspots"));
        assert!(text.contains("Refactoring targets"));
    }

    #[test]
    fn completely_empty_report_produces_no_lines() {
        let root = PathBuf::from("/project");
        let report = empty_report();
        let lines = build_health_human_lines(&report, &root);
        assert!(lines.is_empty());
    }

    // ── Finding threshold coloring ───────────────────────────────

    #[test]
    fn finding_only_cyclomatic_exceeds() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.summary.functions_above_threshold = 1;
        report.findings = vec![crate::health_types::HealthFinding {
            path: root.join("src/a.ts"),
            name: "fn1".to_string(),
            line: 1,
            col: 0,
            cyclomatic: 25, // exceeds 20
            cognitive: 10,  // does not exceed 15
            line_count: 50,
            exceeded: crate::health_types::ExceededThreshold::Cyclomatic,
        }];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("25 cyclomatic"));
        assert!(text.contains("10 cognitive"));
    }

    #[test]
    fn finding_only_cognitive_exceeds() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.summary.functions_above_threshold = 1;
        report.findings = vec![crate::health_types::HealthFinding {
            path: root.join("src/a.ts"),
            name: "fn1".to_string(),
            line: 1,
            col: 0,
            cyclomatic: 10, // does not exceed 20
            cognitive: 25,  // exceeds 15
            line_count: 50,
            exceeded: crate::health_types::ExceededThreshold::Cognitive,
        }];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("10 cyclomatic"));
        assert!(text.contains("25 cognitive"));
    }

    #[test]
    fn findings_across_multiple_files() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.summary.functions_above_threshold = 2;
        report.findings = vec![
            crate::health_types::HealthFinding {
                path: root.join("src/a.ts"),
                name: "fn1".to_string(),
                line: 1,
                col: 0,
                cyclomatic: 25,
                cognitive: 20,
                line_count: 50,
                exceeded: crate::health_types::ExceededThreshold::Both,
            },
            crate::health_types::HealthFinding {
                path: root.join("src/b.ts"),
                name: "fn2".to_string(),
                line: 5,
                col: 0,
                cyclomatic: 22,
                cognitive: 18,
                line_count: 40,
                exceeded: crate::health_types::ExceededThreshold::Both,
            },
        ];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        // Both file paths should appear
        assert!(text.contains("src/a.ts"));
        assert!(text.contains("src/b.ts"));
    }

    #[test]
    fn findings_docs_link() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.summary.functions_above_threshold = 1;
        report.findings = vec![crate::health_types::HealthFinding {
            path: root.join("src/a.ts"),
            name: "fn1".to_string(),
            line: 1,
            col: 0,
            cyclomatic: 25,
            cognitive: 20,
            line_count: 50,
            exceeded: crate::health_types::ExceededThreshold::Both,
        }];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("docs.fallow.tools/explanations/health#complexity-metrics"));
    }

    // ── Hotspot score color thresholds ────────────────────────────

    #[test]
    fn hotspot_score_high_medium_low() {
        let root = PathBuf::from("/project");
        let mut report = empty_report();
        report.hotspots = vec![
            crate::health_types::HotspotEntry {
                path: root.join("src/high.ts"),
                score: 80.0, // red: >= 70
                commits: 30,
                weighted_commits: 25.0,
                lines_added: 400,
                lines_deleted: 200,
                complexity_density: 0.9,
                fan_in: 8,
                trend: fallow_core::churn::ChurnTrend::Accelerating,
            },
            crate::health_types::HotspotEntry {
                path: root.join("src/medium.ts"),
                score: 45.0, // yellow: >= 30
                commits: 15,
                weighted_commits: 10.0,
                lines_added: 200,
                lines_deleted: 100,
                complexity_density: 0.5,
                fan_in: 4,
                trend: fallow_core::churn::ChurnTrend::Stable,
            },
            crate::health_types::HotspotEntry {
                path: root.join("src/low.ts"),
                score: 15.0, // green: < 30
                commits: 5,
                weighted_commits: 3.0,
                lines_added: 50,
                lines_deleted: 20,
                complexity_density: 0.2,
                fan_in: 1,
                trend: fallow_core::churn::ChurnTrend::Cooling,
            },
        ];
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        assert!(text.contains("80.0"));
        assert!(text.contains("45.0"));
        assert!(text.contains("15.0"));
        assert!(text.contains("Hotspots (3 files)"));
    }
}
