//! Vital signs computation and snapshot persistence.
//!
//! Vital signs are a fixed set of project-wide metrics computed from available
//! health data. They are always shown as a summary in the health report and can
//! be persisted to `.fallow/snapshots/` for Phase 2b trend tracking.

use std::path::{Path, PathBuf};

use crate::health_types::{
    FileHealthScore, HOTSPOT_SCORE_THRESHOLD, HealthScore, HealthScorePenalties, HotspotEntry,
    SNAPSHOT_SCHEMA_VERSION, VitalSigns, VitalSignsCounts, VitalSignsSnapshot, letter_grade,
};

/// Data sources for computing vital signs.
///
/// Fields are `Option` because not all pipelines run in every health invocation.
pub struct VitalSignsInput<'a> {
    /// All parsed modules (always available).
    pub modules: &'a [fallow_core::extract::ModuleInfo],
    /// File health scores (available when file_scores/hotspots/targets are computed).
    pub file_scores: Option<&'a [FileHealthScore]>,
    /// Hotspot entries (available when hotspots are computed).
    pub hotspots: Option<&'a [HotspotEntry]>,
    /// Total discovered files.
    pub total_files: usize,
    /// Analysis results (available when file_scores pipeline ran).
    pub analysis_counts: Option<AnalysisCounts>,
}

/// Aggregate counts from the analysis pipeline.
pub struct AnalysisCounts {
    pub total_exports: usize,
    pub dead_files: usize,
    pub dead_exports: usize,
    pub unused_deps: usize,
    pub circular_deps: usize,
    pub total_deps: usize,
}

/// Compute vital signs from available health data.
pub fn compute_vital_signs(input: &VitalSignsInput<'_>) -> VitalSigns {
    // Cyclomatic complexity: always available from parsed modules
    let mut all_cyclomatic: Vec<u16> = input
        .modules
        .iter()
        .flat_map(|m| m.complexity.iter().map(|c| c.cyclomatic))
        .collect();
    all_cyclomatic.sort_unstable();

    let avg_cyclomatic = if all_cyclomatic.is_empty() {
        0.0
    } else {
        let sum: u64 = all_cyclomatic.iter().map(|&c| u64::from(c)).sum();
        (sum as f64 / all_cyclomatic.len() as f64 * 10.0).round() / 10.0
    };

    let p90_cyclomatic = if all_cyclomatic.is_empty() {
        0
    } else {
        let idx = (all_cyclomatic.len() as f64 * 0.9).ceil() as usize;
        let idx = idx.min(all_cyclomatic.len()) - 1;
        u32::from(all_cyclomatic[idx])
    };

    // Dead code percentages: only available when analysis pipeline ran
    let (dead_file_pct, dead_export_pct, unused_dep_count, circular_dep_count) =
        if let Some(ref counts) = input.analysis_counts {
            let dfp = if input.total_files > 0 {
                Some((counts.dead_files as f64 / input.total_files as f64 * 1000.0).round() / 10.0)
            } else {
                Some(0.0)
            };
            let dep = if counts.total_exports > 0 {
                Some(
                    (counts.dead_exports as f64 / counts.total_exports as f64 * 1000.0).round()
                        / 10.0,
                )
            } else {
                Some(0.0)
            };
            (
                dfp,
                dep,
                Some(counts.unused_deps as u32),
                Some(counts.circular_deps as u32),
            )
        } else {
            (None, None, None, None)
        };

    // Maintainability average: from file scores
    let maintainability_avg = input.file_scores.and_then(|scores| {
        if scores.is_empty() {
            return None;
        }
        let sum: f64 = scores.iter().map(|s| s.maintainability_index).sum();
        Some((sum / scores.len() as f64 * 10.0).round() / 10.0)
    });

    // Hotspot count: files with score >= threshold
    let hotspot_count = input.hotspots.map(|entries| {
        entries
            .iter()
            .filter(|e| e.score >= HOTSPOT_SCORE_THRESHOLD)
            .count() as u32
    });

    VitalSigns {
        dead_file_pct,
        dead_export_pct,
        avg_cyclomatic,
        p90_cyclomatic,
        duplication_pct: None, // Lazy: only set if duplication pipeline was run
        hotspot_count,
        maintainability_avg,
        unused_dep_count,
        circular_dep_count,
    }
}

/// Compute a project-level health score from vital signs.
///
/// The score starts at 100 and subtracts penalties for each metric.
/// Missing metrics (from pipelines that didn't run) don't penalize.
/// `total_files` is used to normalize the hotspot count penalty.
pub fn compute_health_score(vs: &VitalSigns, total_files: usize) -> HealthScore {
    // Round each penalty to 1dp BEFORE subtracting so that JSON consumers
    // can reproduce the score as `100 - sum(penalties)`.
    let round1 = |v: f64| -> f64 { (v * 10.0).round() / 10.0 };

    let mut score = 100.0_f64;

    // Dead file penalty: 0.2 points per percent, max 15
    let dead_files_penalty = vs.dead_file_pct.map(|dfp| round1((dfp * 0.2).min(15.0)));
    if let Some(p) = dead_files_penalty {
        score -= p;
    }

    // Dead export penalty: 0.2 points per percent, max 15
    let dead_exports_penalty = vs.dead_export_pct.map(|dep| round1((dep * 0.2).min(15.0)));
    if let Some(p) = dead_exports_penalty {
        score -= p;
    }

    // Complexity penalty: 5 points per unit above 1.5, max 20
    let complexity_penalty = round1(((vs.avg_cyclomatic - 1.5).max(0.0) * 5.0).min(20.0));
    score -= complexity_penalty;

    // P90 penalty: 1 point per unit above 10, max 10
    let p90_penalty = round1((f64::from(vs.p90_cyclomatic) - 10.0).clamp(0.0, 10.0));
    score -= p90_penalty;

    // Maintainability penalty: 0.5 points per unit below 70, max 15
    let maintainability_penalty = vs
        .maintainability_avg
        .map(|mi| round1(((70.0 - mi).max(0.0) * 0.5).min(15.0)));
    if let Some(p) = maintainability_penalty {
        score -= p;
    }

    // Hotspot penalty: normalized by total files, max 10
    let hotspot_penalty = vs.hotspot_count.map(|hc| {
        if total_files > 0 {
            round1((f64::from(hc) / total_files as f64 * 200.0).min(10.0))
        } else {
            0.0
        }
    });
    if let Some(p) = hotspot_penalty {
        score -= p;
    }

    // Unused dep penalty: 1 point per dep, max 10
    let unused_deps_penalty = vs
        .unused_dep_count
        .map(|ud| round1(f64::from(ud).min(10.0)));
    if let Some(p) = unused_deps_penalty {
        score -= p;
    }

    // Circular dep penalty: 1 point per chain, max 10
    let circular_deps_penalty = vs
        .circular_dep_count
        .map(|cd| round1(f64::from(cd).min(10.0)));
    if let Some(p) = circular_deps_penalty {
        score -= p;
    }

    let score = (score * 10.0).round() / 10.0;
    let score = score.clamp(0.0, 100.0);
    let grade = letter_grade(score);

    HealthScore {
        score,
        grade,
        penalties: HealthScorePenalties {
            dead_files: dead_files_penalty,
            dead_exports: dead_exports_penalty,
            complexity: complexity_penalty,
            p90_complexity: p90_penalty,
            maintainability: maintainability_penalty,
            hotspots: hotspot_penalty,
            unused_deps: unused_deps_penalty,
            circular_deps: circular_deps_penalty,
        },
    }
}

/// Build the raw counts for a snapshot.
pub fn build_counts(input: &VitalSignsInput<'_>) -> VitalSignsCounts {
    let (total_exports, dead_files, dead_exports, total_deps) =
        if let Some(ref counts) = input.analysis_counts {
            (
                counts.total_exports,
                counts.dead_files,
                counts.dead_exports,
                counts.total_deps,
            )
        } else {
            (0, 0, 0, 0)
        };

    VitalSignsCounts {
        total_files: input.total_files,
        total_exports,
        dead_files,
        dead_exports,
        duplicated_lines: None,
        total_lines: None,
        files_scored: input.file_scores.map(|s| s.len()),
        total_deps,
    }
}

/// Get the current git SHA (short form).
fn git_sha(root: &Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Get the current git branch name.
fn git_branch(root: &Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
            // Detached HEAD returns "HEAD" — treat as None
            if name == "HEAD" { None } else { Some(name) }
        })
}

/// Build a snapshot from vital signs and input data.
pub fn build_snapshot(
    vital_signs: VitalSigns,
    counts: VitalSignsCounts,
    root: &Path,
    shallow_clone: bool,
    health_score: Option<&HealthScore>,
) -> VitalSignsSnapshot {
    let now = chrono_timestamp();

    VitalSignsSnapshot {
        snapshot_schema_version: SNAPSHOT_SCHEMA_VERSION,
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: now,
        git_sha: git_sha(root),
        git_branch: git_branch(root),
        shallow_clone,
        vital_signs,
        counts,
        score: health_score.map(|s| s.score),
        grade: health_score.map(|s| s.grade.to_string()),
    }
}

/// ISO 8601 UTC timestamp without external chrono dependency.
fn chrono_timestamp() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();

    // Simple UTC conversion (no leap seconds, good enough for timestamps)
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Convert days since epoch to y/m/d
    let (year, month, day) = days_to_ymd(days);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since Unix epoch to (year, month, day).
const fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from Howard Hinnant's date library (public domain)
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Save a snapshot to disk.
///
/// If `path` is `None`, writes to `.fallow/snapshots/{timestamp}.json`.
/// Creates parent directories as needed.
pub fn save_snapshot(
    snapshot: &VitalSignsSnapshot,
    root: &Path,
    explicit_path: Option<&Path>,
) -> Result<PathBuf, String> {
    let path = if let Some(p) = explicit_path {
        p.to_path_buf()
    } else {
        let dir = root.join(".fallow").join("snapshots");
        // Use the snapshot timestamp for the filename (replace colons for Windows compat)
        let filename = snapshot.timestamp.replace(':', "-");
        dir.join(format!("{filename}.json"))
    };

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create snapshot directory: {e}"))?;
    }

    let json =
        serde_json::to_string_pretty(snapshot).map_err(|e| format!("failed to serialize: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("failed to write snapshot: {e}"))?;

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_module(id: u32, cyclomatic: u16) -> fallow_core::extract::ModuleInfo {
        fallow_core::extract::ModuleInfo {
            file_id: fallow_core::discover::FileId(id),
            exports: Vec::new(),
            imports: Vec::new(),
            re_exports: Vec::new(),
            dynamic_imports: Vec::new(),
            dynamic_import_patterns: Vec::new(),
            require_calls: Vec::new(),
            member_accesses: Vec::new(),
            whole_object_uses: Vec::new(),
            has_cjs_exports: false,
            content_hash: 0,
            suppressions: Vec::new(),
            unused_import_bindings: Vec::new(),
            line_offsets: Vec::new(),
            complexity: vec![fallow_types::extract::FunctionComplexity {
                name: format!("fn_{id}"),
                line: id + 1,
                col: 0,
                cyclomatic,
                cognitive: 0,
                line_count: 10,
            }],
        }
    }

    fn make_modules() -> Vec<fallow_core::extract::ModuleInfo> {
        // Cyclomatic values: 2, 4, 6, 8, 10, 12, 14, 16, 18, 20
        (0..10)
            .map(|i| make_module(i, (i as u16 + 1) * 2))
            .collect()
    }

    #[test]
    fn compute_cyclomatic_stats() {
        let modules = make_modules();
        let input = VitalSignsInput {
            modules: &modules,
            file_scores: None,
            hotspots: None,
            total_files: 10,
            analysis_counts: None,
        };
        let vs = compute_vital_signs(&input);
        // avg of 2,4,6,8,10,12,14,16,18,20 = 11.0
        assert!((vs.avg_cyclomatic - 11.0).abs() < f64::EPSILON);
        // p90 of sorted [2,4,6,8,10,12,14,16,18,20] at index ceil(10*0.9)-1 = 8 → value 18
        assert_eq!(vs.p90_cyclomatic, 18);
    }

    #[test]
    fn compute_with_analysis_counts() {
        let modules = make_modules();
        let input = VitalSignsInput {
            modules: &modules,
            file_scores: None,
            hotspots: None,
            total_files: 100,
            analysis_counts: Some(AnalysisCounts {
                total_exports: 500,
                dead_files: 5,
                dead_exports: 50,
                unused_deps: 3,
                circular_deps: 2,
                total_deps: 40,
            }),
        };
        let vs = compute_vital_signs(&input);
        assert_eq!(vs.dead_file_pct, Some(5.0)); // 5/100 * 100
        assert_eq!(vs.dead_export_pct, Some(10.0)); // 50/500 * 100
        assert_eq!(vs.unused_dep_count, Some(3));
        assert_eq!(vs.circular_dep_count, Some(2));
    }

    #[test]
    fn compute_hotspot_count_with_threshold() {
        let hotspots = vec![
            HotspotEntry {
                path: PathBuf::from("a.ts"),
                score: 80.0,
                commits: 10,
                weighted_commits: 8.0,
                lines_added: 100,
                lines_deleted: 50,
                complexity_density: 0.5,
                fan_in: 5,
                trend: fallow_core::churn::ChurnTrend::Stable,
            },
            HotspotEntry {
                path: PathBuf::from("b.ts"),
                score: 30.0, // Below threshold
                commits: 5,
                weighted_commits: 3.0,
                lines_added: 40,
                lines_deleted: 20,
                complexity_density: 0.2,
                fan_in: 2,
                trend: fallow_core::churn::ChurnTrend::Cooling,
            },
            HotspotEntry {
                path: PathBuf::from("c.ts"),
                score: 50.0, // At threshold
                commits: 8,
                weighted_commits: 6.0,
                lines_added: 80,
                lines_deleted: 30,
                complexity_density: 0.4,
                fan_in: 3,
                trend: fallow_core::churn::ChurnTrend::Accelerating,
            },
        ];
        let modules = Vec::new();
        let input = VitalSignsInput {
            modules: &modules,
            file_scores: None,
            hotspots: Some(&hotspots),
            total_files: 10,
            analysis_counts: None,
        };
        let vs = compute_vital_signs(&input);
        assert_eq!(vs.hotspot_count, Some(2)); // 80.0 and 50.0 meet threshold
    }

    #[test]
    fn compute_without_hotspots_gives_none() {
        let modules = Vec::new();
        let input = VitalSignsInput {
            modules: &modules,
            file_scores: None,
            hotspots: None,
            total_files: 0,
            analysis_counts: None,
        };
        let vs = compute_vital_signs(&input);
        assert!(vs.hotspot_count.is_none());
    }

    #[test]
    fn snapshot_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let vs = VitalSigns {
            dead_file_pct: Some(3.2),
            dead_export_pct: Some(8.1),
            avg_cyclomatic: 4.7,
            p90_cyclomatic: 12,
            duplication_pct: None,
            hotspot_count: Some(5),
            maintainability_avg: Some(72.4),
            unused_dep_count: Some(4),
            circular_dep_count: Some(2),
        };
        let counts = VitalSignsCounts {
            total_files: 1200,
            total_exports: 5400,
            dead_files: 38,
            dead_exports: 437,
            duplicated_lines: None,
            total_lines: None,
            files_scored: Some(1150),
            total_deps: 42,
        };
        let health_score = compute_health_score(&vs, 1200);
        let snapshot = build_snapshot(vs, counts, root, false, Some(&health_score));
        let saved_path = save_snapshot(&snapshot, root, None).unwrap();

        assert!(saved_path.exists());
        assert!(saved_path.starts_with(root.join(".fallow/snapshots")));

        // Load and verify
        let content = std::fs::read_to_string(&saved_path).unwrap();
        let loaded: VitalSignsSnapshot = serde_json::from_str(&content).unwrap();
        assert_eq!(loaded.snapshot_schema_version, SNAPSHOT_SCHEMA_VERSION);
        assert!((loaded.vital_signs.avg_cyclomatic - 4.7).abs() < f64::EPSILON);
        assert_eq!(loaded.counts.total_files, 1200);
        assert!(loaded.score.is_some());
        assert!(loaded.grade.is_some());
    }

    #[test]
    fn snapshot_save_explicit_path() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let explicit = root.join("my-snapshot.json");
        let vs = VitalSigns {
            dead_file_pct: None,
            dead_export_pct: None,
            avg_cyclomatic: 1.0,
            p90_cyclomatic: 2,
            duplication_pct: None,
            hotspot_count: None,
            maintainability_avg: None,
            unused_dep_count: None,
            circular_dep_count: None,
        };
        let counts = VitalSignsCounts {
            total_files: 0,
            total_exports: 0,
            dead_files: 0,
            dead_exports: 0,
            duplicated_lines: None,
            total_lines: None,
            files_scored: None,
            total_deps: 0,
        };
        let snapshot = build_snapshot(vs, counts, root, false, None);
        let saved = save_snapshot(&snapshot, root, Some(&explicit)).unwrap();
        assert_eq!(saved, explicit);
        assert!(explicit.exists());
    }

    #[test]
    fn snapshot_save_creates_nested_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let nested = root.join("a/b/c/snapshot.json");
        let vs = VitalSigns {
            dead_file_pct: None,
            dead_export_pct: None,
            avg_cyclomatic: 1.0,
            p90_cyclomatic: 2,
            duplication_pct: None,
            hotspot_count: None,
            maintainability_avg: None,
            unused_dep_count: None,
            circular_dep_count: None,
        };
        let counts = VitalSignsCounts {
            total_files: 0,
            total_exports: 0,
            dead_files: 0,
            dead_exports: 0,
            duplicated_lines: None,
            total_lines: None,
            files_scored: None,
            total_deps: 0,
        };
        let snapshot = build_snapshot(vs, counts, root, false, None);
        let saved = save_snapshot(&snapshot, root, Some(&nested)).unwrap();
        assert_eq!(saved, nested);
        assert!(nested.exists());
    }

    #[test]
    fn days_to_ymd_epoch() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date() {
        // 2026-03-25 is 20,537 days since epoch
        assert_eq!(days_to_ymd(20_537), (2026, 3, 25));
    }

    // --- compute_health_score ---

    #[test]
    fn health_score_perfect() {
        let vs = VitalSigns {
            dead_file_pct: Some(0.0),
            dead_export_pct: Some(0.0),
            avg_cyclomatic: 1.0,
            p90_cyclomatic: 2,
            duplication_pct: None,
            hotspot_count: Some(0),
            maintainability_avg: Some(90.0),
            unused_dep_count: Some(0),
            circular_dep_count: Some(0),
        };
        let score = compute_health_score(&vs, 100);
        assert!((score.score - 100.0).abs() < f64::EPSILON);
        assert_eq!(score.grade, "A");
    }

    #[test]
    fn health_score_no_optional_metrics() {
        // Only avg_cyclomatic and p90_cyclomatic are always present
        let vs = VitalSigns {
            dead_file_pct: None,
            dead_export_pct: None,
            avg_cyclomatic: 1.0,
            p90_cyclomatic: 2,
            duplication_pct: None,
            hotspot_count: None,
            maintainability_avg: None,
            unused_dep_count: None,
            circular_dep_count: None,
        };
        let score = compute_health_score(&vs, 0);
        // Only complexity penalties apply (both 0 since below thresholds)
        assert!((score.score - 100.0).abs() < f64::EPSILON);
        assert_eq!(score.grade, "A");
        assert!(score.penalties.dead_files.is_none());
        assert!(score.penalties.unused_deps.is_none());
    }

    #[test]
    fn health_score_dead_code_penalty() {
        let vs = VitalSigns {
            dead_file_pct: Some(50.0),
            dead_export_pct: Some(30.0),
            avg_cyclomatic: 1.0,
            p90_cyclomatic: 2,
            duplication_pct: None,
            hotspot_count: None,
            maintainability_avg: None,
            unused_dep_count: None,
            circular_dep_count: None,
        };
        let score = compute_health_score(&vs, 100);
        // dead_file: min(50*0.2, 15) = 10
        // dead_export: min(30*0.2, 15) = 6
        // total penalty: 16
        assert!((score.score - 84.0).abs() < 0.1);
        assert_eq!(score.grade, "B");
    }

    #[test]
    fn health_score_complexity_penalty() {
        let vs = VitalSigns {
            dead_file_pct: None,
            dead_export_pct: None,
            avg_cyclomatic: 5.5,
            p90_cyclomatic: 15,
            duplication_pct: None,
            hotspot_count: None,
            maintainability_avg: None,
            unused_dep_count: None,
            circular_dep_count: None,
        };
        let score = compute_health_score(&vs, 100);
        // complexity: min((5.5-1.5)*5, 20) = 20
        // p90: min(15-10, 10) = 5
        // total penalty: 25
        assert!((score.score - 75.0).abs() < 0.1);
        assert_eq!(score.grade, "B");
    }

    #[test]
    fn health_score_clamped_at_zero() {
        let vs = VitalSigns {
            dead_file_pct: Some(100.0),
            dead_export_pct: Some(100.0),
            avg_cyclomatic: 10.0,
            p90_cyclomatic: 30,
            duplication_pct: None,
            hotspot_count: Some(50),
            maintainability_avg: Some(20.0),
            unused_dep_count: Some(100),
            circular_dep_count: Some(50),
        };
        let score = compute_health_score(&vs, 100);
        assert!((score.score).abs() < f64::EPSILON);
        assert_eq!(score.grade, "F");
    }

    #[test]
    fn health_score_hotspot_normalized_by_files() {
        let vs = VitalSigns {
            dead_file_pct: None,
            dead_export_pct: None,
            avg_cyclomatic: 1.0,
            p90_cyclomatic: 2,
            duplication_pct: None,
            hotspot_count: Some(5),
            maintainability_avg: None,
            unused_dep_count: None,
            circular_dep_count: None,
        };
        // 5 hotspots in 100 files = 5% = 10 points
        let score_100 = compute_health_score(&vs, 100);
        // 5 hotspots in 1000 files = 0.5% = 1 point
        let score_1000 = compute_health_score(&vs, 1000);
        assert!(score_1000.score > score_100.score);
    }
}
