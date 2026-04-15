use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::process::{Command, Stdio};

use ed25519_dalek::VerifyingKey;
use fallow_config::OutputFormat;
use fallow_cov_protocol::{
    CallState, Confidence, CoverageSource, PROTOCOL_VERSION, Request, Response, StaticFile,
    StaticFindings, StaticFunction, Verdict, Watermark,
};
use fallow_license::{
    DEFAULT_HARD_FAIL_DAYS, Feature, LicenseStatus, load_and_verify, load_raw_jwt,
};
use globset::GlobSet;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::error::emit_error;
use crate::health::ProductionCoverageOptions;
use crate::health_types::{
    ProductionCoverageAction, ProductionCoverageConfidence, ProductionCoverageFinding,
    ProductionCoverageHotPath, ProductionCoverageMessage, ProductionCoverageReport,
    ProductionCoverageState, ProductionCoverageSummary, ProductionCoverageVerdict,
    ProductionCoverageWatermark,
};
use crate::license::verifying_key;

type FunctionLocations = FxHashMap<(String, String), u32>;

pub fn prepare_options(
    path: &Path,
    min_invocations_hot: u64,
    output: OutputFormat,
) -> Result<ProductionCoverageOptions, ExitCode> {
    let key = match verifying_key() {
        Ok(key) => key,
        Err(message) => return Err(emit_error(&message, 3, output)),
    };
    let status = match load_and_verify(&key, DEFAULT_HARD_FAIL_DAYS) {
        Ok(status) => status,
        Err(err) => return Err(emit_error(&format!("license: {err}"), 3, output)),
    };
    let jwt = match load_raw_jwt() {
        Ok(Some(jwt)) => jwt,
        Ok(None) => {
            return Err(emit_error(
                "No license found. Run: fallow license activate --trial --email you@company.com",
                3,
                output,
            ));
        }
        Err(err) => return Err(emit_error(&format!("license: {err}"), 3, output)),
    };

    validate_license_status(&status, &key, output)?;

    Ok(ProductionCoverageOptions {
        path: path.to_path_buf(),
        min_invocations_hot,
        license_jwt: jwt,
        watermark: if status.show_watermark() {
            Some(ProductionCoverageWatermark::LicenseExpiredGrace)
        } else {
            None
        },
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "sidecar invocation needs the same filter context as health analysis"
)]
pub fn analyze(
    options: &ProductionCoverageOptions,
    root: &Path,
    modules: &[fallow_types::extract::ModuleInfo],
    file_paths: &FxHashMap<fallow_types::discover::FileId, &PathBuf>,
    ignore_set: &GlobSet,
    changed_files: Option<&FxHashSet<PathBuf>>,
    ws_root: Option<&Path>,
    top: Option<usize>,
    quiet: bool,
    output: OutputFormat,
) -> Result<ProductionCoverageReport, ExitCode> {
    let sidecar = discover_sidecar().map_err(|message| emit_error(&message, 4, output))?;
    let (request, locations) = build_request(
        options,
        root,
        modules,
        file_paths,
        ignore_set,
        changed_files,
        ws_root,
    )
    .map_err(|message| emit_error(&message, 5, output))?;
    let response = run_sidecar(&sidecar, &request, quiet, output)?;
    let report = convert_response(response, &locations, options.watermark);
    let _ = top;
    Ok(report)
}

fn validate_license_status(
    status: &LicenseStatus,
    _key: &VerifyingKey,
    output: OutputFormat,
) -> Result<(), ExitCode> {
    match status {
        LicenseStatus::Missing => Err(emit_error(
            "No license found. Run: fallow license activate --trial --email you@company.com",
            3,
            output,
        )),
        LicenseStatus::HardFail {
            days_since_expiry, ..
        } => Err(emit_error(
            &format!(
                "license expired {days_since_expiry} days ago. Refresh with: fallow license refresh"
            ),
            3,
            output,
        )),
        _ if !status.permits(&Feature::ProductionCoverage) => Err(emit_error(
            "License is valid but does not include 'production_coverage'. Upgrade at fallow.tools/upgrade.",
            3,
            output,
        )),
        _ => Ok(()),
    }
}

pub fn discover_sidecar() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("FALLOW_COV_BIN") {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let canonical = canonical_sidecar_path();
    if canonical.is_file() {
        return Ok(canonical);
    }

    if let Some(path) = find_on_path("fallow-cov") {
        return Ok(path);
    }

    Err(
        "Sidecar binary fallow-cov not found. Install with: npm install -g @fallow-cli/fallow-cov"
            .to_owned(),
    )
}

pub fn canonical_sidecar_path() -> PathBuf {
    let home = std::env::var("HOME").map_or_else(|_| PathBuf::from("."), PathBuf::from);
    let binary = if cfg!(windows) {
        "fallow-cov.exe"
    } else {
        "fallow-cov"
    };
    home.join(".fallow").join("bin").join(binary)
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var).find_map(|dir| {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
        if cfg!(windows) {
            let exe = dir.join(format!("{binary}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
        None
    })
}

fn build_request(
    options: &ProductionCoverageOptions,
    root: &Path,
    modules: &[fallow_types::extract::ModuleInfo],
    file_paths: &FxHashMap<fallow_types::discover::FileId, &PathBuf>,
    ignore_set: &GlobSet,
    changed_files: Option<&FxHashSet<PathBuf>>,
    ws_root: Option<&Path>,
) -> Result<(Request, FunctionLocations), String> {
    let mut files = Vec::new();
    let mut locations = FxHashMap::default();
    for module in modules {
        let Some(&path) = file_paths.get(&module.file_id) else {
            continue;
        };
        let relative = path.strip_prefix(root).unwrap_or(path);
        if ignore_set.is_match(relative) {
            continue;
        }
        if let Some(changed) = changed_files
            && !changed.contains(path.as_path())
        {
            continue;
        }
        if let Some(ws) = ws_root
            && !path.starts_with(ws)
        {
            continue;
        }
        if module.complexity.is_empty() {
            continue;
        }
        let functions = module
            .complexity
            .iter()
            .map(|function| {
                locations.insert(
                    (path.to_string_lossy().into_owned(), function.name.clone()),
                    function.line,
                );
                StaticFunction {
                    name: function.name.clone(),
                    start_line: function.line,
                    end_line: function.line.saturating_add(function.line_count),
                    cyclomatic: u32::from(function.cyclomatic),
                }
            })
            .collect();
        files.push(StaticFile {
            path: path.to_string_lossy().into_owned(),
            functions,
        });
    }

    let coverage_sources = collect_coverage_sources(&options.path)?;

    Ok((
        Request {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            license: fallow_cov_protocol::License {
                jwt: options.license_jwt.clone(),
            },
            project_root: root.to_string_lossy().into_owned(),
            coverage_sources,
            static_findings: StaticFindings { files },
            options: fallow_cov_protocol::Options {
                include_hot_paths: true,
                min_invocations_for_hot: Some(options.min_invocations_hot),
            },
        },
        locations,
    ))
}

fn collect_coverage_sources(path: &Path) -> Result<Vec<CoverageSource>, String> {
    if !path.is_dir() {
        return Ok(vec![if looks_like_istanbul(path) {
            CoverageSource::Istanbul {
                path: path.to_string_lossy().into_owned(),
            }
        } else {
            CoverageSource::V8 {
                path: path.to_string_lossy().into_owned(),
            }
        }]);
    }

    let entries = std::fs::read_dir(path).map_err(|err| {
        format!(
            "failed to read coverage directory {}: {err}",
            path.display()
        )
    })?;
    let mut json_files = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|entry| entry.is_file() && entry.extension() == Some(OsStr::new("json")))
        .collect::<Vec<_>>();
    json_files.sort();

    if json_files.is_empty() {
        return Ok(vec![CoverageSource::V8Dir {
            path: path.to_string_lossy().into_owned(),
        }]);
    }

    let mut sources = Vec::with_capacity(json_files.len());
    for file in json_files {
        if looks_like_istanbul(&file) {
            sources.push(CoverageSource::Istanbul {
                path: file.to_string_lossy().into_owned(),
            });
        } else {
            sources.push(CoverageSource::V8 {
                path: file.to_string_lossy().into_owned(),
            });
        }
    }
    Ok(sources)
}

fn looks_like_istanbul(path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name == "coverage-final.json")
}

fn run_sidecar(
    sidecar: &Path,
    request: &Request,
    quiet: bool,
    output: OutputFormat,
) -> Result<Response, ExitCode> {
    let mut child = Command::new(sidecar)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            emit_error(
                &format!("failed to spawn {}: {err}", sidecar.display()),
                4,
                output,
            )
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(err) = serde_json::to_writer(&mut stdin, request) {
            return Err(emit_error(
                &format!("failed to serialize sidecar request: {err}"),
                4,
                output,
            ));
        }
        if let Err(err) = stdin.flush() {
            return Err(emit_error(
                &format!("failed to flush sidecar request: {err}"),
                4,
                output,
            ));
        }
    }

    let output_data = child
        .wait_with_output()
        .map_err(|err| emit_error(&format!("failed to wait for sidecar: {err}"), 4, output))?;

    if !output_data.stderr.is_empty() && !quiet {
        let stderr = String::from_utf8_lossy(&output_data.stderr);
        eprint!("{stderr}");
    }

    match output_data.status.code() {
        Some(0) => {}
        Some(4) => {
            return Err(emit_error(
                &stderr_message(&output_data.stderr, "sidecar protocol mismatch"),
                4,
                output,
            ));
        }
        Some(5) => {
            return Err(emit_error(
                &stderr_message(
                    &output_data.stderr,
                    "failed to parse production coverage input",
                ),
                5,
                output,
            ));
        }
        Some(6) => {
            return Err(emit_error(
                &stderr_message(&output_data.stderr, "sidecar internal error"),
                6,
                output,
            ));
        }
        Some(code) => {
            return Err(emit_error(
                &stderr_message(&output_data.stderr, "sidecar execution failed"),
                u8::try_from(code).unwrap_or(4),
                output,
            ));
        }
        None => {
            return Err(emit_error("sidecar terminated by signal", 4, output));
        }
    }

    let response: Response = serde_json::from_slice(&output_data.stdout).map_err(|err| {
        emit_error(
            &format!("failed to parse sidecar response: {err}"),
            4,
            output,
        )
    })?;

    let supported_major = PROTOCOL_VERSION.split('.').next().unwrap_or("0");
    let response_major = response.protocol_version.split('.').next().unwrap_or("0");
    if response_major != supported_major {
        let message = if response_major > supported_major {
            format!(
                "sidecar emits protocol v{}; this fallow supports up to v{}. Upgrade fallow.",
                response.protocol_version, PROTOCOL_VERSION
            )
        } else {
            format!(
                "sidecar emits protocol v{}; this fallow requires v{}+. Upgrade @fallow-cli/fallow-cov.",
                response.protocol_version, PROTOCOL_VERSION
            )
        };
        return Err(emit_error(&message, 4, output));
    }

    Ok(response)
}

fn stderr_message(stderr: &[u8], fallback: &str) -> String {
    let message = String::from_utf8_lossy(stderr).trim().to_owned();
    if message.is_empty() {
        fallback.to_owned()
    } else {
        message
    }
}

fn convert_response(
    response: Response,
    locations: &FunctionLocations,
    watermark: Option<ProductionCoverageWatermark>,
) -> ProductionCoverageReport {
    let mut findings = response
        .findings
        .into_iter()
        .filter_map(|finding| {
            let state = map_state(&finding.state);
            if matches!(state, ProductionCoverageState::Called) {
                return None;
            }
            let line = locations
                .get(&(finding.file.clone(), finding.function.clone()))
                .copied();
            Some(ProductionCoverageFinding {
                path: PathBuf::from(finding.file),
                function: finding.function,
                line,
                state,
                invocations: finding.invocations,
                confidence: map_confidence(&finding.confidence),
                actions: finding
                    .actions
                    .into_iter()
                    .map(|action| ProductionCoverageAction {
                        kind: action.kind,
                        description: action.description,
                        auto_fixable: action.auto_fixable,
                    })
                    .collect(),
            })
        })
        .collect::<Vec<_>>();

    findings.sort_by(|left, right| {
        state_rank(left.state)
            .cmp(&state_rank(right.state))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.function.cmp(&right.function))
    });

    let mut hot_paths = response
        .hot_paths
        .into_iter()
        .map(|entry| ProductionCoverageHotPath {
            line: locations
                .get(&(entry.file.clone(), entry.function.clone()))
                .copied(),
            path: PathBuf::from(entry.file),
            function: entry.function,
            invocations: entry.invocations,
        })
        .collect::<Vec<_>>();
    hot_paths.sort_by(|left, right| {
        right
            .invocations
            .cmp(&left.invocations)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.function.cmp(&right.function))
    });

    ProductionCoverageReport {
        verdict: map_verdict(&response.verdict),
        summary: ProductionCoverageSummary {
            functions_total: response.summary.functions_total as usize,
            functions_called: response.summary.functions_called as usize,
            functions_never_called: response.summary.functions_never_called as usize,
            functions_coverage_unavailable: response.summary.functions_coverage_unavailable
                as usize,
            percent_dead_in_production: response.summary.percent_dead_in_production,
        },
        findings,
        hot_paths,
        watermark: watermark.or_else(|| response.watermark.as_ref().map(map_watermark)),
        warnings: response
            .warnings
            .into_iter()
            .map(|warning| ProductionCoverageMessage {
                code: warning.code,
                message: warning.message,
            })
            .collect(),
    }
}

fn map_state(state: &CallState) -> ProductionCoverageState {
    match state {
        CallState::Called => ProductionCoverageState::Called,
        CallState::NeverCalled => ProductionCoverageState::NeverCalled,
        CallState::CoverageUnavailable => ProductionCoverageState::CoverageUnavailable,
        CallState::Unknown => ProductionCoverageState::Unknown,
    }
}

fn map_confidence(confidence: &Confidence) -> ProductionCoverageConfidence {
    match confidence {
        Confidence::High => ProductionCoverageConfidence::High,
        Confidence::Medium => ProductionCoverageConfidence::Medium,
        Confidence::Low => ProductionCoverageConfidence::Low,
        Confidence::Unknown => ProductionCoverageConfidence::Unknown,
    }
}

fn map_verdict(verdict: &Verdict) -> ProductionCoverageVerdict {
    match verdict {
        Verdict::Clean => ProductionCoverageVerdict::Clean,
        Verdict::HotPathChangesNeeded => ProductionCoverageVerdict::HotPathChangesNeeded,
        Verdict::ColdCodeDetected => ProductionCoverageVerdict::ColdCodeDetected,
        Verdict::LicenseExpiredGrace => ProductionCoverageVerdict::LicenseExpiredGrace,
        Verdict::Unknown => ProductionCoverageVerdict::Unknown,
    }
}

fn map_watermark(watermark: &Watermark) -> ProductionCoverageWatermark {
    match watermark {
        Watermark::TrialExpired => ProductionCoverageWatermark::TrialExpired,
        Watermark::LicenseExpiredGrace => ProductionCoverageWatermark::LicenseExpiredGrace,
        Watermark::Unknown => ProductionCoverageWatermark::Unknown,
    }
}

fn state_rank(state: ProductionCoverageState) -> u8 {
    match state {
        ProductionCoverageState::NeverCalled => 0,
        ProductionCoverageState::CoverageUnavailable => 1,
        ProductionCoverageState::Called => 2,
        ProductionCoverageState::Unknown => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::{collect_coverage_sources, looks_like_istanbul};
    use fallow_cov_protocol::CoverageSource;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn detects_istanbul_file_by_name() {
        assert!(looks_like_istanbul(
            PathBuf::from("coverage-final.json").as_path()
        ));
        assert!(!looks_like_istanbul(
            PathBuf::from("coverage.json").as_path()
        ));
    }

    #[test]
    fn directory_with_istanbul_and_v8_files_expands_to_per_file_sources() {
        let root = make_temp_dir("coverage-sources");
        std::fs::create_dir_all(&root)
            .unwrap_or_else(|err| panic!("failed to create temp dir: {err}"));
        std::fs::write(root.join("coverage-final.json"), "{}")
            .unwrap_or_else(|err| panic!("failed to write istanbul file: {err}"));
        std::fs::write(root.join("chunk-1.json"), "{\"result\":[]}")
            .unwrap_or_else(|err| panic!("failed to write v8 file: {err}"));

        let sources = collect_coverage_sources(&root)
            .unwrap_or_else(|err| panic!("failed to collect coverage sources: {err}"));

        assert_eq!(sources.len(), 2);
        assert!(matches!(
            &sources[0],
            CoverageSource::V8 { path } if path.ends_with("chunk-1.json")
        ));
        assert!(matches!(
            &sources[1],
            CoverageSource::Istanbul { path } if path.ends_with("coverage-final.json")
        ));

        std::fs::remove_dir_all(&root)
            .unwrap_or_else(|err| panic!("failed to clean temp dir {}: {err}", root.display()));
    }

    fn make_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|err| panic!("clock went backwards: {err}"))
            .as_nanos();
        std::env::temp_dir().join(format!("fallow-cli-{name}-{}-{nanos}", std::process::id()))
    }
}
