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
            parts.push(format!("MI {avg:.1}"));
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
    }
}

/// Build human-readable output lines for health (complexity) findings.
pub(in crate::report) fn build_health_human_lines(
    report: &crate::health_types::HealthReport,
    root: &Path,
) -> Vec<String> {
    let mut lines = Vec::new();

    // Health score (shown first when available)
    if let Some(ref hs) = report.health_score {
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
            parts.push(format!("MI -{mi:.1}"));
        }
        if let Some(hp) = p.hotspots
            && hp > 0.0
        {
            parts.push(format!("hotspots -{hp:.1}"));
        }
        if let Some(ud) = p.unused_deps
            && ud > 0.0
        {
            parts.push(format!("unused deps -{ud:.1}"));
        }
        if let Some(cd) = p.circular_deps
            && cd > 0.0
        {
            parts.push(format!("circular deps -{cd:.1}"));
        }
        if !parts.is_empty() {
            lines.push(format!("  {}", parts.join(" \u{00b7} ").dimmed()));
        }
        // Check for N/A components
        let mut na_parts = Vec::new();
        if p.dead_files.is_none() {
            na_parts.push("dead code");
        }
        if p.maintainability.is_none() {
            na_parts.push("MI");
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

    // Vital signs summary line (always shown when available)
    if let Some(ref vs) = report.vital_signs {
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
            parts.push(format!("MI {mi:.1}"));
        }
        if let Some(hc) = vs.hotspot_count {
            parts.push(format!("{hc} hotspot{}", plural(hc as usize)));
        }
        if let Some(cd) = vs.circular_dep_count
            && cd > 0
        {
            parts.push(format!("{cd} circular dep{}", plural(cd as usize)));
        }
        if let Some(ud) = vs.unused_dep_count
            && ud > 0
        {
            parts.push(format!("{ud} unused dep{}", plural(ud as usize)));
        }
        lines.push(format!(
            "{} {}",
            "\u{25a0}".dimmed(),
            parts.join(" \u{00b7} ").dimmed()
        ));
        lines.push(String::new());
    }

    if !report.findings.is_empty() {
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
    }

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

        // Line 1: function name
        lines.push(format!(
            "    {} {}",
            format!(":{}", finding.line).dimmed(),
            finding.name.bold(),
        ));
        // Line 2: metrics (indented, aligned like hotspots)
        lines.push(format!(
            "         {} cyclomatic  {} cognitive  {} lines",
            cyc_colored,
            cog_colored,
            format!("{:>3}", finding.line_count).dimmed(),
        ));
    }
    if !report.findings.is_empty() {
        lines.push(format!(
            "  {}",
            format!(
                "Functions exceeding cyclomatic or cognitive complexity thresholds \u{2014} {DOCS_HEALTH}#complexity-metrics"
            )
            .dimmed()
        ));
        lines.push(String::new());
    }

    // File health scores (truncated)
    if !report.file_scores.is_empty() {
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
                    "... and {} more files",
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

    // Hotspots
    if !report.hotspots.is_empty() {
        let header = if let Some(ref summary) = report.hotspot_summary {
            format!(
                "Hotspots ({} files, since {})",
                report.hotspots.len(),
                summary.since,
            )
        } else {
            format!("Hotspots ({} files)", report.hotspots.len())
        };
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
            format!(
                "Files with high churn and high complexity \u{2014} {DOCS_HEALTH}#hotspot-metrics"
            )
            .dimmed()
        ));
        lines.push(String::new());
    }

    // Refactoring targets (last section — synthesis of data above)
    if !report.targets.is_empty() {
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
            lines.push(format!(
                "         {} \u{00b7} effort:{} \u{00b7} confidence:{}  {}",
                label.yellow(),
                effort_colored,
                confidence_colored,
                target.recommendation.dimmed(),
            ));

            // Blank line between entries
            lines.push(String::new());
        }
        if report.targets.len() > MAX_FLAT_ITEMS {
            lines.push(format!(
                "  {}",
                format!(
                    "... and {} more targets",
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

    lines
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
            hotspots: vec![],
            hotspot_summary: None,
            targets: vec![],
            target_thresholds: None,
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
            hotspots: vec![],
            hotspot_summary: None,
            targets: vec![],
            target_thresholds: None,
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
            hotspots: vec![],
            hotspot_summary: None,
            targets: vec![],
            target_thresholds: None,
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
            hotspots: vec![],
            hotspot_summary: None,
            targets: vec![],
            target_thresholds: None,
        };
        let lines = build_health_human_lines(&report, &root);
        let text = plain(&lines);
        // File path should appear once (grouping)
        let count = text.matches("src/parser.ts").count();
        assert_eq!(count, 1, "File header should appear once for grouped items");
    }
}
