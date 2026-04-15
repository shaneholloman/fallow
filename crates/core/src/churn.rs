//! Git churn analysis for hotspot detection.
//!
//! Shells out to `git log` to collect per-file change history, then computes
//! recency-weighted churn scores and trend indicators.

use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

/// Number of seconds in one day.
const SECS_PER_DAY: f64 = 86_400.0;

/// Recency weight half-life in days. A commit from 90 days ago counts half
/// as much as today's commit; 180 days ago counts 25%.
const HALF_LIFE_DAYS: f64 = 90.0;

/// Parsed duration for the `--since` flag.
#[derive(Debug, Clone)]
pub struct SinceDuration {
    /// Value to pass to `git log --after` (e.g., `"6 months ago"` or `"2025-06-01"`).
    pub git_after: String,
    /// Human-readable display string (e.g., `"6 months"`).
    pub display: String,
}

/// Churn trend indicator based on comparing recent vs older halves of the analysis period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, bitcode::Encode, bitcode::Decode)]
#[serde(rename_all = "snake_case")]
pub enum ChurnTrend {
    /// Recent half has >1.5× the commits of the older half.
    Accelerating,
    /// Churn is roughly stable between halves.
    Stable,
    /// Recent half has <0.67× the commits of the older half.
    Cooling,
}

impl std::fmt::Display for ChurnTrend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accelerating => write!(f, "accelerating"),
            Self::Stable => write!(f, "stable"),
            Self::Cooling => write!(f, "cooling"),
        }
    }
}

/// Per-author commit aggregation for a single file.
///
/// Authors are interned via [`ChurnResult::author_pool`] indices to keep
/// per-file maps small and the bitcode cache compact.
#[derive(Debug, Clone, Copy)]
pub struct AuthorContribution {
    /// Total commits by this author touching this file in the analysis window.
    pub commits: u32,
    /// Recency-weighted commit sum (exponential decay, half-life 90 days).
    pub weighted_commits: f64,
    /// Earliest commit timestamp by this author (epoch seconds).
    pub first_commit_ts: u64,
    /// Latest commit timestamp by this author (epoch seconds).
    pub last_commit_ts: u64,
}

/// Per-file churn data collected from git history.
#[derive(Debug, Clone)]
pub struct FileChurn {
    /// Absolute file path.
    pub path: PathBuf,
    /// Total number of commits touching this file in the analysis window.
    pub commits: u32,
    /// Recency-weighted commit count (exponential decay, half-life 90 days).
    pub weighted_commits: f64,
    /// Total lines added across all commits.
    pub lines_added: u32,
    /// Total lines deleted across all commits.
    pub lines_deleted: u32,
    /// Churn trend: accelerating, stable, or cooling.
    pub trend: ChurnTrend,
    /// Per-author contributions keyed by interned author index.
    /// Indices reference [`ChurnResult::author_pool`].
    pub authors: FxHashMap<u32, AuthorContribution>,
}

/// Result of churn analysis.
pub struct ChurnResult {
    /// Per-file churn data, keyed by absolute path.
    pub files: FxHashMap<PathBuf, FileChurn>,
    /// Whether the repository is a shallow clone.
    pub shallow_clone: bool,
    /// Author email pool. Per-file [`AuthorContribution`] entries reference
    /// authors by their index into this vector.
    pub author_pool: Vec<String>,
}

/// Parse a `--since` value into a git-compatible duration.
///
/// Accepts:
/// - Durations: `6m`, `6months`, `90d`, `90days`, `1y`, `1year`, `2w`, `2weeks`
/// - ISO dates: `2025-06-01`
///
/// # Errors
///
/// Returns an error if the input is not a recognized duration format or ISO date,
/// the numeric part is invalid, or the duration is zero.
pub fn parse_since(input: &str) -> Result<SinceDuration, String> {
    // Try ISO date first (YYYY-MM-DD)
    if is_iso_date(input) {
        return Ok(SinceDuration {
            git_after: input.to_string(),
            display: input.to_string(),
        });
    }

    // Parse duration: number + unit
    let (num_str, unit) = split_number_unit(input)?;
    let num: u64 = num_str
        .parse()
        .map_err(|_| format!("invalid number in --since: {input}"))?;

    if num == 0 {
        return Err("--since duration must be greater than 0".to_string());
    }

    match unit {
        "d" | "day" | "days" => {
            let s = if num == 1 { "" } else { "s" };
            Ok(SinceDuration {
                git_after: format!("{num} day{s} ago"),
                display: format!("{num} day{s}"),
            })
        }
        "w" | "week" | "weeks" => {
            let s = if num == 1 { "" } else { "s" };
            Ok(SinceDuration {
                git_after: format!("{num} week{s} ago"),
                display: format!("{num} week{s}"),
            })
        }
        "m" | "month" | "months" => {
            let s = if num == 1 { "" } else { "s" };
            Ok(SinceDuration {
                git_after: format!("{num} month{s} ago"),
                display: format!("{num} month{s}"),
            })
        }
        "y" | "year" | "years" => {
            let s = if num == 1 { "" } else { "s" };
            Ok(SinceDuration {
                git_after: format!("{num} year{s} ago"),
                display: format!("{num} year{s}"),
            })
        }
        _ => Err(format!(
            "unknown duration unit '{unit}' in --since. Use d/w/m/y (e.g., 6m, 90d, 1y)"
        )),
    }
}

/// Analyze git churn for files in the given root directory.
///
/// Returns `None` if git is not available or the directory is not a git repository.
pub fn analyze_churn(root: &Path, since: &SinceDuration) -> Option<ChurnResult> {
    let shallow = is_shallow_clone(root);

    let output = Command::new("git")
        .args([
            "log",
            "--numstat",
            "--no-merges",
            "--no-renames",
            "--use-mailmap",
            "--format=format:%at|%ae",
            &format!("--after={}", since.git_after),
        ])
        .current_dir(root)
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("hotspot analysis skipped: failed to run git: {e}");
            return None;
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("hotspot analysis skipped: git log failed: {stderr}");
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let (files, author_pool) = parse_git_log(&stdout, root);

    Some(ChurnResult {
        files,
        shallow_clone: shallow,
        author_pool,
    })
}

/// Check if the repository is a shallow clone.
#[must_use]
pub fn is_shallow_clone(root: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-shallow-repository"])
        .current_dir(root)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .eq_ignore_ascii_case("true")
        })
        .unwrap_or(false)
}

/// Check if the directory is inside a git repository.
#[must_use]
pub fn is_git_repo(root: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ── Churn cache ──────────────────────────────────────────────────

/// Maximum size of a churn cache file (16 MB).
const MAX_CHURN_CACHE_SIZE: usize = 16 * 1024 * 1024;

/// Cache schema version. Bump when the on-disk shape of [`ChurnCache`]
/// changes so older payloads are rejected on load. Bumped to 2 in v2.37.0
/// when per-author contributions and the author pool were added.
const CHURN_CACHE_VERSION: u8 = 2;

/// Serializable per-author contribution entry for the disk cache.
#[derive(bitcode::Encode, bitcode::Decode)]
struct CachedAuthorContribution {
    author_idx: u32,
    commits: u32,
    weighted_commits: f64,
    first_commit_ts: u64,
    last_commit_ts: u64,
}

/// Serializable per-file churn entry for the disk cache.
#[derive(bitcode::Encode, bitcode::Decode)]
struct CachedFileChurn {
    path: String,
    commits: u32,
    weighted_commits: f64,
    lines_added: u32,
    lines_deleted: u32,
    trend: ChurnTrend,
    authors: Vec<CachedAuthorContribution>,
}

/// Cached churn data keyed by HEAD SHA and since string.
#[derive(bitcode::Encode, bitcode::Decode)]
struct ChurnCache {
    /// Schema version; must equal [`CHURN_CACHE_VERSION`] to be accepted.
    version: u8,
    head_sha: String,
    git_after: String,
    files: Vec<CachedFileChurn>,
    shallow_clone: bool,
    /// Author email pool referenced by [`CachedAuthorContribution::author_idx`].
    author_pool: Vec<String>,
}

/// Get the full HEAD SHA for cache keying.
fn get_head_sha(root: &Path) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Try to load churn data from disk cache. Returns `None` on cache miss
/// or version mismatch.
fn load_churn_cache(cache_dir: &Path, head_sha: &str, git_after: &str) -> Option<ChurnResult> {
    let cache_file = cache_dir.join("churn.bin");
    let data = std::fs::read(&cache_file).ok()?;
    if data.len() > MAX_CHURN_CACHE_SIZE {
        return None;
    }
    let cache: ChurnCache = bitcode::decode(&data).ok()?;
    if cache.version != CHURN_CACHE_VERSION
        || cache.head_sha != head_sha
        || cache.git_after != git_after
    {
        return None;
    }
    let mut files = FxHashMap::default();
    for entry in cache.files {
        let path = PathBuf::from(&entry.path);
        let authors = entry
            .authors
            .into_iter()
            .map(|a| {
                (
                    a.author_idx,
                    AuthorContribution {
                        commits: a.commits,
                        weighted_commits: a.weighted_commits,
                        first_commit_ts: a.first_commit_ts,
                        last_commit_ts: a.last_commit_ts,
                    },
                )
            })
            .collect();
        files.insert(
            path.clone(),
            FileChurn {
                path,
                commits: entry.commits,
                weighted_commits: entry.weighted_commits,
                lines_added: entry.lines_added,
                lines_deleted: entry.lines_deleted,
                trend: entry.trend,
                authors,
            },
        );
    }
    Some(ChurnResult {
        files,
        shallow_clone: cache.shallow_clone,
        author_pool: cache.author_pool,
    })
}

/// Save churn data to disk cache.
fn save_churn_cache(cache_dir: &Path, head_sha: &str, git_after: &str, result: &ChurnResult) {
    let files: Vec<CachedFileChurn> = result
        .files
        .values()
        .map(|f| CachedFileChurn {
            path: f.path.to_string_lossy().to_string(),
            commits: f.commits,
            weighted_commits: f.weighted_commits,
            lines_added: f.lines_added,
            lines_deleted: f.lines_deleted,
            trend: f.trend,
            authors: f
                .authors
                .iter()
                .map(|(&idx, c)| CachedAuthorContribution {
                    author_idx: idx,
                    commits: c.commits,
                    weighted_commits: c.weighted_commits,
                    first_commit_ts: c.first_commit_ts,
                    last_commit_ts: c.last_commit_ts,
                })
                .collect(),
        })
        .collect();
    let cache = ChurnCache {
        version: CHURN_CACHE_VERSION,
        head_sha: head_sha.to_string(),
        git_after: git_after.to_string(),
        files,
        shallow_clone: result.shallow_clone,
        author_pool: result.author_pool.clone(),
    };
    let _ = std::fs::create_dir_all(cache_dir);
    let data = bitcode::encode(&cache);
    // Write to temp file then rename for atomic update (avoids partial reads by concurrent processes)
    let tmp = cache_dir.join("churn.bin.tmp");
    if std::fs::write(&tmp, data).is_ok() {
        let _ = std::fs::rename(&tmp, cache_dir.join("churn.bin"));
    }
}

/// Analyze churn with disk caching. Uses cached result when HEAD SHA and
/// since duration match. On cache miss, runs `git log` and saves the result.
///
/// Returns `(ChurnResult, bool)` where the bool indicates whether the cache was hit.
/// Returns `None` if git analysis fails.
pub fn analyze_churn_cached(
    root: &Path,
    since: &SinceDuration,
    cache_dir: &Path,
    no_cache: bool,
) -> Option<(ChurnResult, bool)> {
    let head_sha = get_head_sha(root)?;

    if !no_cache && let Some(cached) = load_churn_cache(cache_dir, &head_sha, &since.git_after) {
        return Some((cached, true));
    }

    let result = analyze_churn(root, since)?;

    if !no_cache {
        save_churn_cache(cache_dir, &head_sha, &since.git_after, &result);
    }

    Some((result, false))
}

// ── Internal ──────────────────────────────────────────────────────

/// Intermediate per-file accumulator during git log parsing.
struct FileAccum {
    /// Commit timestamps (epoch seconds) for trend computation.
    commit_timestamps: Vec<u64>,
    /// Recency-weighted commit sum.
    weighted_commits: f64,
    lines_added: u32,
    lines_deleted: u32,
    /// Per-author contributions keyed by interned author index.
    authors: FxHashMap<u32, AuthorContribution>,
}

/// Parse `git log --numstat --format=format:%at|%ae` output.
///
/// Returns a per-file churn map plus the author email pool referenced by
/// interned indices in [`FileChurn::authors`].
#[expect(
    clippy::cast_possible_truncation,
    reason = "commit count per file is bounded by git history depth"
)]
fn parse_git_log(stdout: &str, root: &Path) -> (FxHashMap<PathBuf, FileChurn>, Vec<String>) {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut accum: FxHashMap<PathBuf, FileAccum> = FxHashMap::default();
    let mut author_pool: Vec<String> = Vec::new();
    let mut author_index: FxHashMap<String, u32> = FxHashMap::default();
    let mut current_timestamp: Option<u64> = None;
    let mut current_author_idx: Option<u32> = None;

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Header lines have shape: "<ts>|<email>"
        if let Some((ts_str, email)) = line.split_once('|')
            && let Ok(ts) = ts_str.parse::<u64>()
        {
            current_timestamp = Some(ts);
            current_author_idx = Some(intern_author(email, &mut author_pool, &mut author_index));
            continue;
        }

        // Backwards-compat: bare timestamp (legacy format or test fixtures).
        if let Ok(ts) = line.parse::<u64>() {
            current_timestamp = Some(ts);
            current_author_idx = None;
            continue;
        }

        // Numstat line: "10\t5\tpath/to/file"
        if let Some((added, deleted, path)) = parse_numstat_line(line) {
            let abs_path = root.join(path);
            let ts = current_timestamp.unwrap_or(now_secs);
            let age_days = (now_secs.saturating_sub(ts)) as f64 / SECS_PER_DAY;
            let weight = 0.5_f64.powf(age_days / HALF_LIFE_DAYS);

            let entry = accum.entry(abs_path).or_insert_with(|| FileAccum {
                commit_timestamps: Vec::new(),
                weighted_commits: 0.0,
                lines_added: 0,
                lines_deleted: 0,
                authors: FxHashMap::default(),
            });
            entry.commit_timestamps.push(ts);
            entry.weighted_commits += weight;
            entry.lines_added += added;
            entry.lines_deleted += deleted;

            if let Some(idx) = current_author_idx {
                entry
                    .authors
                    .entry(idx)
                    .and_modify(|c| {
                        c.commits += 1;
                        c.weighted_commits += weight;
                        c.first_commit_ts = c.first_commit_ts.min(ts);
                        c.last_commit_ts = c.last_commit_ts.max(ts);
                    })
                    .or_insert(AuthorContribution {
                        commits: 1,
                        weighted_commits: weight,
                        first_commit_ts: ts,
                        last_commit_ts: ts,
                    });
            }
        }
    }

    let files = accum
        .into_iter()
        .map(|(path, acc)| {
            let commits = acc.commit_timestamps.len() as u32;
            let trend = compute_trend(&acc.commit_timestamps);
            let mut authors = acc.authors;
            // Round per-author weighted sums for cache stability.
            for c in authors.values_mut() {
                c.weighted_commits = (c.weighted_commits * 100.0).round() / 100.0;
            }
            let churn = FileChurn {
                path: path.clone(),
                commits,
                weighted_commits: (acc.weighted_commits * 100.0).round() / 100.0,
                lines_added: acc.lines_added,
                lines_deleted: acc.lines_deleted,
                trend,
                authors,
            };
            (path, churn)
        })
        .collect();

    (files, author_pool)
}

/// Intern an author email into the pool, returning its stable index.
fn intern_author(email: &str, pool: &mut Vec<String>, index: &mut FxHashMap<String, u32>) -> u32 {
    if let Some(&idx) = index.get(email) {
        return idx;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "author count is bounded by git history; u32 is far above any realistic ceiling"
    )]
    let idx = pool.len() as u32;
    let owned = email.to_string();
    index.insert(owned.clone(), idx);
    pool.push(owned);
    idx
}

/// Parse a single numstat line: `"10\t5\tpath/to/file.ts"`.
/// Binary files show as `"-\t-\tpath"` — skip those.
fn parse_numstat_line(line: &str) -> Option<(u32, u32, &str)> {
    let mut parts = line.splitn(3, '\t');
    let added_str = parts.next()?;
    let deleted_str = parts.next()?;
    let path = parts.next()?;

    // Binary files show "-" for added/deleted — skip them
    let added: u32 = added_str.parse().ok()?;
    let deleted: u32 = deleted_str.parse().ok()?;

    Some((added, deleted, path))
}

/// Compute churn trend by splitting commits into two temporal halves.
///
/// Finds the midpoint between the oldest and newest commit timestamps,
/// then compares commit counts in each half:
/// - Recent > 1.5× older → Accelerating
/// - Recent < 0.67× older → Cooling
/// - Otherwise → Stable
fn compute_trend(timestamps: &[u64]) -> ChurnTrend {
    if timestamps.len() < 2 {
        return ChurnTrend::Stable;
    }

    let min_ts = timestamps.iter().copied().min().unwrap_or(0);
    let max_ts = timestamps.iter().copied().max().unwrap_or(0);

    if max_ts == min_ts {
        return ChurnTrend::Stable;
    }

    let midpoint = min_ts + (max_ts - min_ts) / 2;
    let recent = timestamps.iter().filter(|&&ts| ts > midpoint).count() as f64;
    let older = timestamps.iter().filter(|&&ts| ts <= midpoint).count() as f64;

    if older < 1.0 {
        return ChurnTrend::Stable;
    }

    let ratio = recent / older;
    if ratio > 1.5 {
        ChurnTrend::Accelerating
    } else if ratio < 0.67 {
        ChurnTrend::Cooling
    } else {
        ChurnTrend::Stable
    }
}

fn is_iso_date(input: &str) -> bool {
    input.len() == 10
        && input.as_bytes().get(4) == Some(&b'-')
        && input.as_bytes().get(7) == Some(&b'-')
        && input[..4].bytes().all(|b| b.is_ascii_digit())
        && input[5..7].bytes().all(|b| b.is_ascii_digit())
        && input[8..10].bytes().all(|b| b.is_ascii_digit())
}

fn split_number_unit(input: &str) -> Result<(&str, &str), String> {
    let pos = input.find(|c: char| !c.is_ascii_digit()).ok_or_else(|| {
        format!("--since requires a unit suffix (e.g., 6m, 90d, 1y), got: {input}")
    })?;
    if pos == 0 {
        return Err(format!(
            "--since must start with a number (e.g., 6m, 90d, 1y), got: {input}"
        ));
    }
    Ok((&input[..pos], &input[pos..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_since ──────────────────────────────────────────────

    #[test]
    fn parse_since_months_short() {
        let d = parse_since("6m").unwrap();
        assert_eq!(d.git_after, "6 months ago");
        assert_eq!(d.display, "6 months");
    }

    #[test]
    fn parse_since_months_long() {
        let d = parse_since("6months").unwrap();
        assert_eq!(d.git_after, "6 months ago");
        assert_eq!(d.display, "6 months");
    }

    #[test]
    fn parse_since_days() {
        let d = parse_since("90d").unwrap();
        assert_eq!(d.git_after, "90 days ago");
        assert_eq!(d.display, "90 days");
    }

    #[test]
    fn parse_since_year_singular() {
        let d = parse_since("1y").unwrap();
        assert_eq!(d.git_after, "1 year ago");
        assert_eq!(d.display, "1 year");
    }

    #[test]
    fn parse_since_years_plural() {
        let d = parse_since("2years").unwrap();
        assert_eq!(d.git_after, "2 years ago");
        assert_eq!(d.display, "2 years");
    }

    #[test]
    fn parse_since_weeks() {
        let d = parse_since("2w").unwrap();
        assert_eq!(d.git_after, "2 weeks ago");
        assert_eq!(d.display, "2 weeks");
    }

    #[test]
    fn parse_since_iso_date() {
        let d = parse_since("2025-06-01").unwrap();
        assert_eq!(d.git_after, "2025-06-01");
        assert_eq!(d.display, "2025-06-01");
    }

    #[test]
    fn parse_since_month_singular() {
        let d = parse_since("1month").unwrap();
        assert_eq!(d.display, "1 month");
    }

    #[test]
    fn parse_since_day_singular() {
        let d = parse_since("1day").unwrap();
        assert_eq!(d.display, "1 day");
    }

    #[test]
    fn parse_since_zero_rejected() {
        assert!(parse_since("0m").is_err());
    }

    #[test]
    fn parse_since_no_unit_rejected() {
        assert!(parse_since("90").is_err());
    }

    #[test]
    fn parse_since_unknown_unit_rejected() {
        assert!(parse_since("6x").is_err());
    }

    #[test]
    fn parse_since_no_number_rejected() {
        assert!(parse_since("months").is_err());
    }

    // ── parse_numstat_line ───────────────────────────────────────

    #[test]
    fn numstat_normal() {
        let (a, d, p) = parse_numstat_line("10\t5\tsrc/file.ts").unwrap();
        assert_eq!(a, 10);
        assert_eq!(d, 5);
        assert_eq!(p, "src/file.ts");
    }

    #[test]
    fn numstat_binary_skipped() {
        assert!(parse_numstat_line("-\t-\tsrc/image.png").is_none());
    }

    #[test]
    fn numstat_zero_lines() {
        let (a, d, p) = parse_numstat_line("0\t0\tsrc/empty.ts").unwrap();
        assert_eq!(a, 0);
        assert_eq!(d, 0);
        assert_eq!(p, "src/empty.ts");
    }

    // ── compute_trend ────────────────────────────────────────────

    #[test]
    fn trend_empty_is_stable() {
        assert_eq!(compute_trend(&[]), ChurnTrend::Stable);
    }

    #[test]
    fn trend_single_commit_is_stable() {
        assert_eq!(compute_trend(&[100]), ChurnTrend::Stable);
    }

    #[test]
    fn trend_accelerating() {
        // 2 old commits, 5 recent commits
        let timestamps = vec![100, 200, 800, 850, 900, 950, 1000];
        assert_eq!(compute_trend(&timestamps), ChurnTrend::Accelerating);
    }

    #[test]
    fn trend_cooling() {
        // 5 old commits, 2 recent commits
        let timestamps = vec![100, 150, 200, 250, 300, 900, 1000];
        assert_eq!(compute_trend(&timestamps), ChurnTrend::Cooling);
    }

    #[test]
    fn trend_stable_even_distribution() {
        // 3 old commits, 3 recent commits → ratio = 1.0 → stable
        let timestamps = vec![100, 200, 300, 700, 800, 900];
        assert_eq!(compute_trend(&timestamps), ChurnTrend::Stable);
    }

    #[test]
    fn trend_same_timestamp_is_stable() {
        let timestamps = vec![500, 500, 500];
        assert_eq!(compute_trend(&timestamps), ChurnTrend::Stable);
    }

    // ── is_iso_date ──────────────────────────────────────────────

    #[test]
    fn iso_date_valid() {
        assert!(is_iso_date("2025-06-01"));
        assert!(is_iso_date("2025-12-31"));
    }

    #[test]
    fn iso_date_with_time_rejected() {
        // Only exact YYYY-MM-DD (10 chars) is accepted
        assert!(!is_iso_date("2025-06-01T00:00:00"));
    }

    #[test]
    fn iso_date_invalid() {
        assert!(!is_iso_date("6months"));
        assert!(!is_iso_date("2025"));
        assert!(!is_iso_date("not-a-date"));
        assert!(!is_iso_date("abcd-ef-gh"));
    }

    // ── Display ──────────────────────────────────────────────────

    #[test]
    fn trend_display() {
        assert_eq!(ChurnTrend::Accelerating.to_string(), "accelerating");
        assert_eq!(ChurnTrend::Stable.to_string(), "stable");
        assert_eq!(ChurnTrend::Cooling.to_string(), "cooling");
    }

    // ── parse_git_log ───────────────────────────────────────────

    #[test]
    fn parse_git_log_single_commit() {
        let root = Path::new("/project");
        let output = "1700000000\n10\t5\tsrc/index.ts\n";
        let (result, _) = parse_git_log(output, root);
        assert_eq!(result.len(), 1);
        let churn = &result[&PathBuf::from("/project/src/index.ts")];
        assert_eq!(churn.commits, 1);
        assert_eq!(churn.lines_added, 10);
        assert_eq!(churn.lines_deleted, 5);
    }

    #[test]
    fn parse_git_log_multiple_commits_same_file() {
        let root = Path::new("/project");
        let output = "1700000000\n10\t5\tsrc/index.ts\n\n1700100000\n3\t2\tsrc/index.ts\n";
        let (result, _) = parse_git_log(output, root);
        assert_eq!(result.len(), 1);
        let churn = &result[&PathBuf::from("/project/src/index.ts")];
        assert_eq!(churn.commits, 2);
        assert_eq!(churn.lines_added, 13);
        assert_eq!(churn.lines_deleted, 7);
    }

    #[test]
    fn parse_git_log_multiple_files() {
        let root = Path::new("/project");
        let output = "1700000000\n10\t5\tsrc/a.ts\n3\t1\tsrc/b.ts\n";
        let (result, _) = parse_git_log(output, root);
        assert_eq!(result.len(), 2);
        assert!(result.contains_key(&PathBuf::from("/project/src/a.ts")));
        assert!(result.contains_key(&PathBuf::from("/project/src/b.ts")));
    }

    #[test]
    fn parse_git_log_empty_output() {
        let root = Path::new("/project");
        let (result, _) = parse_git_log("", root);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_git_log_skips_binary_files() {
        let root = Path::new("/project");
        let output = "1700000000\n-\t-\timage.png\n10\t5\tsrc/a.ts\n";
        let (result, _) = parse_git_log(output, root);
        assert_eq!(result.len(), 1);
        assert!(!result.contains_key(&PathBuf::from("/project/image.png")));
    }

    #[test]
    fn parse_git_log_weighted_commits_are_positive() {
        let root = Path::new("/project");
        // Use a timestamp near "now" to ensure weight doesn't decay to zero
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let output = format!("{now_secs}\n10\t5\tsrc/a.ts\n");
        let (result, _) = parse_git_log(&output, root);
        let churn = &result[&PathBuf::from("/project/src/a.ts")];
        assert!(
            churn.weighted_commits > 0.0,
            "weighted_commits should be positive for recent commits"
        );
    }

    // ── compute_trend edge cases ─────────────────────────────────

    #[test]
    fn trend_boundary_1_5x_ratio() {
        // Exactly 1.5x ratio (3 recent : 2 old) → boundary between stable and accelerating
        // midpoint = 100 + (1000-100)/2 = 550
        // old: 100, 200 (2 timestamps <= 550)
        // recent: 600, 800, 1000 (3 timestamps > 550)
        // ratio = 3/2 = 1.5 — NOT > 1.5, so stable
        let timestamps = vec![100, 200, 600, 800, 1000];
        assert_eq!(compute_trend(&timestamps), ChurnTrend::Stable);
    }

    #[test]
    fn trend_just_above_1_5x() {
        // midpoint = 100 + (1000-100)/2 = 550
        // old: 100 (1 timestamp <= 550)
        // recent: 600, 800, 1000 (3 timestamps > 550)
        // ratio = 3/1 = 3.0 → accelerating
        let timestamps = vec![100, 600, 800, 1000];
        assert_eq!(compute_trend(&timestamps), ChurnTrend::Accelerating);
    }

    #[test]
    fn trend_boundary_0_67x_ratio() {
        // Exactly 0.67x ratio → boundary between cooling and stable
        // midpoint = 100 + (1000-100)/2 = 550
        // old: 100, 200, 300 (3 timestamps <= 550)
        // recent: 600, 1000 (2 timestamps > 550)
        // ratio = 2/3 = 0.666... < 0.67 → cooling
        let timestamps = vec![100, 200, 300, 600, 1000];
        assert_eq!(compute_trend(&timestamps), ChurnTrend::Cooling);
    }

    #[test]
    fn trend_two_timestamps_different() {
        // Only 2 timestamps: midpoint = 100 + (200-100)/2 = 150
        // old: 100 (1 timestamp <= 150)
        // recent: 200 (1 timestamp > 150)
        // ratio = 1/1 = 1.0 → stable
        let timestamps = vec![100, 200];
        assert_eq!(compute_trend(&timestamps), ChurnTrend::Stable);
    }

    // ── parse_since additional coverage ─────────────────────────

    #[test]
    fn parse_since_week_singular() {
        let d = parse_since("1week").unwrap();
        assert_eq!(d.git_after, "1 week ago");
        assert_eq!(d.display, "1 week");
    }

    #[test]
    fn parse_since_weeks_long() {
        let d = parse_since("3weeks").unwrap();
        assert_eq!(d.git_after, "3 weeks ago");
        assert_eq!(d.display, "3 weeks");
    }

    #[test]
    fn parse_since_days_long() {
        let d = parse_since("30days").unwrap();
        assert_eq!(d.git_after, "30 days ago");
        assert_eq!(d.display, "30 days");
    }

    #[test]
    fn parse_since_year_long() {
        let d = parse_since("1year").unwrap();
        assert_eq!(d.git_after, "1 year ago");
        assert_eq!(d.display, "1 year");
    }

    #[test]
    fn parse_since_overflow_number_rejected() {
        // Number too large for u64
        let result = parse_since("99999999999999999999d");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("invalid number"));
    }

    #[test]
    fn parse_since_zero_days_rejected() {
        assert!(parse_since("0d").is_err());
    }

    #[test]
    fn parse_since_zero_weeks_rejected() {
        assert!(parse_since("0w").is_err());
    }

    #[test]
    fn parse_since_zero_years_rejected() {
        assert!(parse_since("0y").is_err());
    }

    // ── parse_numstat_line additional coverage ──────────────────

    #[test]
    fn numstat_missing_path() {
        // Only two tab-separated fields, no path
        assert!(parse_numstat_line("10\t5").is_none());
    }

    #[test]
    fn numstat_single_field() {
        assert!(parse_numstat_line("10").is_none());
    }

    #[test]
    fn numstat_empty_string() {
        assert!(parse_numstat_line("").is_none());
    }

    #[test]
    fn numstat_only_added_is_binary() {
        // Added is "-" but deleted is numeric
        assert!(parse_numstat_line("-\t5\tsrc/file.ts").is_none());
    }

    #[test]
    fn numstat_only_deleted_is_binary() {
        // Added is numeric but deleted is "-"
        assert!(parse_numstat_line("10\t-\tsrc/file.ts").is_none());
    }

    #[test]
    fn numstat_path_with_spaces() {
        let (a, d, p) = parse_numstat_line("3\t1\tpath with spaces/file.ts").unwrap();
        assert_eq!(a, 3);
        assert_eq!(d, 1);
        assert_eq!(p, "path with spaces/file.ts");
    }

    #[test]
    fn numstat_large_numbers() {
        let (a, d, p) = parse_numstat_line("9999\t8888\tsrc/big.ts").unwrap();
        assert_eq!(a, 9999);
        assert_eq!(d, 8888);
        assert_eq!(p, "src/big.ts");
    }

    // ── is_iso_date additional coverage ─────────────────────────

    #[test]
    fn iso_date_wrong_separator_positions() {
        // Dashes in wrong positions
        assert!(!is_iso_date("20-25-0601"));
        assert!(!is_iso_date("202506-01-"));
    }

    #[test]
    fn iso_date_too_short() {
        assert!(!is_iso_date("2025-06-0"));
    }

    #[test]
    fn iso_date_letters_in_day() {
        assert!(!is_iso_date("2025-06-ab"));
    }

    #[test]
    fn iso_date_letters_in_month() {
        assert!(!is_iso_date("2025-ab-01"));
    }

    // ── split_number_unit additional coverage ───────────────────

    #[test]
    fn split_number_unit_valid() {
        let (num, unit) = split_number_unit("42days").unwrap();
        assert_eq!(num, "42");
        assert_eq!(unit, "days");
    }

    #[test]
    fn split_number_unit_single_digit() {
        let (num, unit) = split_number_unit("1m").unwrap();
        assert_eq!(num, "1");
        assert_eq!(unit, "m");
    }

    #[test]
    fn split_number_unit_no_digits() {
        let err = split_number_unit("abc").unwrap_err();
        assert!(err.contains("must start with a number"));
    }

    #[test]
    fn split_number_unit_no_unit() {
        let err = split_number_unit("123").unwrap_err();
        assert!(err.contains("requires a unit suffix"));
    }

    // ── parse_git_log additional coverage ───────────────────────

    #[test]
    fn parse_git_log_numstat_before_timestamp_uses_now() {
        let root = Path::new("/project");
        // No timestamp line before the numstat line
        let output = "10\t5\tsrc/no_ts.ts\n";
        let (result, _) = parse_git_log(output, root);
        assert_eq!(result.len(), 1);
        let churn = &result[&PathBuf::from("/project/src/no_ts.ts")];
        assert_eq!(churn.commits, 1);
        assert_eq!(churn.lines_added, 10);
        assert_eq!(churn.lines_deleted, 5);
        // Without a timestamp, it falls back to now_secs, so weight should be ~1.0
        assert!(
            churn.weighted_commits > 0.9,
            "weight should be near 1.0 when timestamp defaults to now"
        );
    }

    #[test]
    fn parse_git_log_whitespace_lines_ignored() {
        let root = Path::new("/project");
        let output = "  \n1700000000\n  \n10\t5\tsrc/a.ts\n  \n";
        let (result, _) = parse_git_log(output, root);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn parse_git_log_trend_is_computed_per_file() {
        let root = Path::new("/project");
        // Two commits far apart for one file, recent-heavy for another
        let output = "\
1000\n5\t1\tsrc/old.ts\n\
2000\n3\t1\tsrc/old.ts\n\
1000\n1\t0\tsrc/hot.ts\n\
1800\n1\t0\tsrc/hot.ts\n\
1900\n1\t0\tsrc/hot.ts\n\
1950\n1\t0\tsrc/hot.ts\n\
2000\n1\t0\tsrc/hot.ts\n";
        let (result, _) = parse_git_log(output, root);
        let old = &result[&PathBuf::from("/project/src/old.ts")];
        let hot = &result[&PathBuf::from("/project/src/hot.ts")];
        assert_eq!(old.commits, 2);
        assert_eq!(hot.commits, 5);
        // hot.ts has 4 recent vs 1 old => accelerating
        assert_eq!(hot.trend, ChurnTrend::Accelerating);
    }

    #[test]
    fn parse_git_log_weighted_decay_for_old_commits() {
        let root = Path::new("/project");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // One commit from 180 days ago (two half-lives) should weigh ~0.25
        let old_ts = now - (180 * 86_400);
        let output = format!("{old_ts}\n10\t5\tsrc/old.ts\n");
        let (result, _) = parse_git_log(&output, root);
        let churn = &result[&PathBuf::from("/project/src/old.ts")];
        assert!(
            churn.weighted_commits < 0.5,
            "180-day-old commit should weigh ~0.25, got {}",
            churn.weighted_commits
        );
        assert!(
            churn.weighted_commits > 0.1,
            "180-day-old commit should weigh ~0.25, got {}",
            churn.weighted_commits
        );
    }

    #[test]
    fn parse_git_log_path_stored_as_absolute() {
        let root = Path::new("/my/project");
        let output = "1700000000\n1\t0\tlib/utils.ts\n";
        let (result, _) = parse_git_log(output, root);
        let key = PathBuf::from("/my/project/lib/utils.ts");
        assert!(result.contains_key(&key));
        assert_eq!(result[&key].path, key);
    }

    #[test]
    fn parse_git_log_weighted_commits_rounded() {
        let root = Path::new("/project");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // A commit right now should weigh exactly 1.00
        let output = format!("{now}\n1\t0\tsrc/a.ts\n");
        let (result, _) = parse_git_log(&output, root);
        let churn = &result[&PathBuf::from("/project/src/a.ts")];
        // Weighted commits are rounded to 2 decimal places
        let decimals = format!("{:.2}", churn.weighted_commits);
        assert_eq!(
            churn.weighted_commits.to_string().len(),
            decimals.len().min(churn.weighted_commits.to_string().len()),
            "weighted_commits should be rounded to at most 2 decimal places"
        );
    }

    // ── ChurnTrend serde ────────────────────────────────────────

    #[test]
    fn trend_serde_serialization() {
        assert_eq!(
            serde_json::to_string(&ChurnTrend::Accelerating).unwrap(),
            "\"accelerating\""
        );
        assert_eq!(
            serde_json::to_string(&ChurnTrend::Stable).unwrap(),
            "\"stable\""
        );
        assert_eq!(
            serde_json::to_string(&ChurnTrend::Cooling).unwrap(),
            "\"cooling\""
        );
    }

    // ── parse_git_log: author tracking ──────────────────────────

    #[test]
    fn parse_git_log_extracts_author_email() {
        let root = Path::new("/project");
        let output = "1700000000|alice@example.com\n10\t5\tsrc/index.ts\n";
        let (result, pool) = parse_git_log(output, root);
        assert_eq!(pool, vec!["alice@example.com".to_string()]);
        let churn = &result[&PathBuf::from("/project/src/index.ts")];
        assert_eq!(churn.authors.len(), 1);
        let alice = &churn.authors[&0];
        assert_eq!(alice.commits, 1);
        assert_eq!(alice.first_commit_ts, 1_700_000_000);
        assert_eq!(alice.last_commit_ts, 1_700_000_000);
    }

    #[test]
    fn parse_git_log_intern_dedupes_authors() {
        let root = Path::new("/project");
        let output = "\
1700000000|alice@example.com
1\t0\ta.ts
1700100000|bob@example.com
2\t1\tb.ts
1700200000|alice@example.com
3\t2\tc.ts
";
        let (_result, pool) = parse_git_log(output, root);
        assert_eq!(pool.len(), 2);
        assert!(pool.contains(&"alice@example.com".to_string()));
        assert!(pool.contains(&"bob@example.com".to_string()));
    }

    #[test]
    fn parse_git_log_aggregates_per_author() {
        let root = Path::new("/project");
        // alice touches index.ts twice, bob once.
        let output = "\
1700000000|alice@example.com
1\t0\tsrc/index.ts
1700100000|bob@example.com
2\t0\tsrc/index.ts
1700200000|alice@example.com
1\t1\tsrc/index.ts
";
        let (result, pool) = parse_git_log(output, root);
        let churn = &result[&PathBuf::from("/project/src/index.ts")];
        assert_eq!(churn.commits, 3);
        assert_eq!(churn.authors.len(), 2);

        let alice_idx =
            u32::try_from(pool.iter().position(|a| a == "alice@example.com").unwrap()).unwrap();
        let alice = &churn.authors[&alice_idx];
        assert_eq!(alice.commits, 2);
        assert_eq!(alice.first_commit_ts, 1_700_000_000);
        assert_eq!(alice.last_commit_ts, 1_700_200_000);
    }

    #[test]
    fn parse_git_log_legacy_bare_timestamp_still_parses() {
        // Backwards-compat path: header has no `|email` suffix.
        let root = Path::new("/project");
        let output = "1700000000\n10\t5\tsrc/index.ts\n";
        let (result, pool) = parse_git_log(output, root);
        assert!(pool.is_empty());
        let churn = &result[&PathBuf::from("/project/src/index.ts")];
        assert_eq!(churn.commits, 1);
        assert!(churn.authors.is_empty());
    }

    // ── intern_author ──────────────────────────────────────────

    #[test]
    fn intern_author_returns_existing_index() {
        let mut pool = Vec::new();
        let mut index = FxHashMap::default();
        let i1 = intern_author("alice@x", &mut pool, &mut index);
        let i2 = intern_author("alice@x", &mut pool, &mut index);
        assert_eq!(i1, i2);
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn intern_author_assigns_sequential_indices() {
        let mut pool = Vec::new();
        let mut index = FxHashMap::default();
        assert_eq!(intern_author("alice@x", &mut pool, &mut index), 0);
        assert_eq!(intern_author("bob@x", &mut pool, &mut index), 1);
        assert_eq!(intern_author("carol@x", &mut pool, &mut index), 2);
        assert_eq!(intern_author("alice@x", &mut pool, &mut index), 0);
    }
}
