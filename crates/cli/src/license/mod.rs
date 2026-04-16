//! `fallow license` subcommand: activate, status, refresh, deactivate.
//!
//! All entry points are dispatched from [`run`]. Network-bound flows
//! (`refresh`, `activate --trial`) fetch a JWT from `api.fallow.cloud` and
//! then pass it through the same offline verifier used by the local activation
//! path. Local flows (`activate <jwt>`, `status`, `deactivate`) are fully
//! wired against [`fallow_license`].
//!
//! # Public key
//!
//! The Ed25519 verification key is compiled in at [`PUBLIC_KEY_BYTES`].

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use ed25519_dalek::VerifyingKey;
use fallow_license::{
    DEFAULT_HARD_FAIL_DAYS, Feature, LicenseError, LicenseStatus, current_unix_seconds,
    default_license_path, normalize_jwt, verify_jwt,
};
use serde::Deserialize;

/// Ed25519 verification key for fallow license JWT validation.
pub const PUBLIC_KEY_BYTES: [u8; 32] = [
    179, 203, 218, 13, 98, 63, 103, 172, 91, 108, 23, 122, 27, 101, 200, 182, 174, 117, 160, 41,
    167, 151, 66, 171, 13, 61, 148, 65, 181, 144, 24, 120,
];
const DEFAULT_API_URL: &str = "https://api.fallow.cloud";
const NETWORK_EXIT_CODE: u8 = 7;
const CONNECT_TIMEOUT_SECS: u64 = 5;
const TOTAL_TIMEOUT_SECS: u64 = 10;

/// Subcommands for `fallow license`.
#[derive(Debug)]
pub enum LicenseSubcommand {
    /// Install a license JWT into `~/.fallow/license.jwt`.
    Activate(ActivateArgs),
    /// Print active license tier, seats, features, days remaining.
    Status,
    /// Fetch a fresh JWT from `api.fallow.cloud` (network-only). Stub.
    Refresh,
    /// Remove the local license file.
    Deactivate,
}

/// Arguments for `fallow license activate`.
#[derive(Debug, Clone, Default)]
pub struct ActivateArgs {
    /// JWT passed directly as a positional argument.
    pub raw_jwt: Option<String>,
    /// Path to a file containing the JWT.
    pub from_file: Option<PathBuf>,
    /// Read JWT from stdin.
    pub from_stdin: bool,
    /// Start a 30-day email-gated trial in one step (stub).
    pub trial: bool,
    /// Email used for the trial flow (required when `trial = true`).
    pub email: Option<String>,
}

#[derive(serde::Serialize)]
struct TrialRequest<'a> {
    email: &'a str,
}

#[derive(Debug, Deserialize)]
struct JwtResponse {
    jwt: String,
    /// Optional ISO-8601 trial expiry timestamp returned by the backend for
    /// `/v1/auth/license/trial`. Present only on the trial flow; `refresh`
    /// responses omit it. We surface it to the user on trial activation so
    /// they do not need to parse the JWT to see the trial end date.
    #[serde(default, rename = "trialEndsAt")]
    trial_ends_at: Option<String>,
}

/// Dispatch a `fallow license <sub>` invocation.
pub fn run(subcommand: &LicenseSubcommand) -> ExitCode {
    match subcommand {
        LicenseSubcommand::Activate(args) => run_activate(args),
        LicenseSubcommand::Status => run_status(),
        LicenseSubcommand::Refresh => run_refresh(),
        LicenseSubcommand::Deactivate => run_deactivate(),
    }
}

fn run_activate(args: &ActivateArgs) -> ExitCode {
    if args.trial {
        return run_trial(args.email.as_deref());
    }
    let jwt = match read_jwt(args) {
        Ok(jwt) => jwt,
        Err(msg) => {
            eprintln!("fallow license: {msg}");
            return ExitCode::from(2);
        }
    };
    let key = match verifying_key() {
        Ok(k) => k,
        Err(msg) => {
            eprintln!("fallow license: {msg}");
            return ExitCode::from(2);
        }
    };
    match verify_jwt(&jwt, &key, current_unix_seconds(), DEFAULT_HARD_FAIL_DAYS) {
        Ok(status) => {
            if let Err(msg) = persist_jwt(&jwt) {
                eprintln!("fallow license: {msg}");
                return ExitCode::from(2);
            }
            print_status(&status);
            ExitCode::SUCCESS
        }
        Err(LicenseError::Truncated { .. }) => {
            eprintln!(
                "fallow license: {}",
                LicenseError::Truncated { actual: jwt.len() }
            );
            ExitCode::from(3)
        }
        Err(err) => {
            eprintln!("fallow license: failed to verify JWT: {err}");
            ExitCode::from(3)
        }
    }
}

fn run_status() -> ExitCode {
    let key = match verifying_key() {
        Ok(k) => k,
        Err(msg) => {
            eprintln!("fallow license: {msg}");
            return ExitCode::from(2);
        }
    };
    match fallow_license::load_and_verify(&key, DEFAULT_HARD_FAIL_DAYS) {
        Ok(status) => {
            print_status(&status);
            match status {
                LicenseStatus::HardFail { .. } | LicenseStatus::Missing => ExitCode::from(3),
                _ => ExitCode::SUCCESS,
            }
        }
        Err(err) => {
            eprintln!("fallow license: {err}");
            ExitCode::from(3)
        }
    }
}

fn run_refresh() -> ExitCode {
    match refresh_active_license() {
        Ok(status) => {
            print_status(&status);
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("fallow license refresh: {message}");
            ExitCode::from(NETWORK_EXIT_CODE)
        }
    }
}

fn run_trial(email: Option<&str>) -> ExitCode {
    let Some(email) = email else {
        eprintln!("fallow license activate --trial requires --email <addr>");
        return ExitCode::from(2);
    };
    match activate_trial(email) {
        Ok(status) => {
            print_status(&status);
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("fallow license activate --trial: {message}");
            ExitCode::from(NETWORK_EXIT_CODE)
        }
    }
}

fn run_deactivate() -> ExitCode {
    let path = default_license_path();
    if !path.exists() {
        println!("fallow license: no license file at {}", path.display());
        return ExitCode::SUCCESS;
    }
    match std::fs::remove_file(&path) {
        Ok(()) => {
            println!("fallow license: removed {}", path.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("fallow license: failed to remove {}: {err}", path.display());
            ExitCode::from(2)
        }
    }
}

fn read_jwt(args: &ActivateArgs) -> Result<String, String> {
    if let Some(jwt) = args.raw_jwt.as_deref() {
        return Ok(normalize_jwt(jwt));
    }
    if let Some(path) = args.from_file.as_deref() {
        let raw = std::fs::read_to_string(path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        return Ok(normalize_jwt(&raw));
    }
    if args.from_stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|err| format!("failed to read stdin: {err}"))?;
        return Ok(normalize_jwt(&buf));
    }
    Err(
        "no JWT provided. Pass it as a positional argument, --from-file <path>, or pipe via stdin (`-`).".to_owned(),
    )
}

fn persist_jwt(jwt: &str) -> Result<(), String> {
    let path = write_jwt(jwt)?;
    println!("fallow license: stored at {}", path.display());
    Ok(())
}

fn write_jwt(jwt: &str) -> Result<PathBuf, String> {
    let path = default_license_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    std::fs::write(&path, jwt)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    restrict_license_permissions(&path)?;
    Ok(path)
}

/// Restrict the license file to owner-only read/write on Unix platforms.
///
/// The JWT is a bearer token; anyone who can read the file can use the
/// license. Home directories are typically 0700/0750 already, but setting
/// 0600 on the file itself is defense-in-depth for shared environments. No-op
/// on Windows (NTFS ACLs follow the parent directory).
#[cfg(unix)]
fn restrict_license_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)
        .map_err(|err| format!("failed to set permissions on {}: {err}", path.display()))
}

#[cfg(not(unix))]
fn restrict_license_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Construct the compiled-in Ed25519 verification key.
///
/// Crate-internal so other CLI subcommands (e.g. `fallow coverage setup`)
/// can also detect license state without re-implementing key construction.
pub fn verifying_key() -> Result<VerifyingKey, String> {
    VerifyingKey::from_bytes(&PUBLIC_KEY_BYTES)
        .map_err(|err| format!("invalid compiled-in public key: {err}"))
}

pub fn activate_trial(email: &str) -> Result<LicenseStatus, String> {
    let mut response = api_agent()
        .post(&api_url("/v1/auth/license/trial"))
        .send_json(TrialRequest { email })
        .map_err(|err| format!("failed to request a trial: {err}"))?;
    if !response.status().is_success() {
        return Err(http_status_message(&mut response, "trial"));
    }
    store_verified_jwt(&mut response, "trial")
}

pub fn refresh_active_license() -> Result<LicenseStatus, String> {
    let current = load_current_jwt()?;
    let mut response = api_agent()
        .post(&api_url("/v1/auth/license/refresh"))
        .header("Authorization", &format!("Bearer {current}"))
        .send_empty()
        .map_err(|err| format!("failed to refresh the current license: {err}"))?;
    if !response.status().is_success() {
        return Err(http_status_message(&mut response, "refresh"));
    }
    store_verified_jwt(&mut response, "refresh")
}

fn api_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(CONNECT_TIMEOUT_SECS)))
        .timeout_global(Some(Duration::from_secs(TOTAL_TIMEOUT_SECS)))
        .http_status_as_error(false)
        .build()
        .new_agent()
}

fn api_url(path: &str) -> String {
    let base = std::env::var("FALLOW_API_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_API_URL.to_owned());
    format!("{}{path}", base.trim_end_matches('/'))
}

fn load_current_jwt() -> Result<String, String> {
    match fallow_license::load_raw_jwt() {
        Ok(Some(jwt)) => Ok(jwt),
        Ok(None) => Err(
            "no license found. Run: fallow license activate --trial --email you@company.com"
                .to_owned(),
        ),
        Err(err) => Err(format!("failed to read the current license: {err}")),
    }
}

fn store_verified_jwt(
    response: &mut impl ResponseBodyReader,
    operation: &str,
) -> Result<LicenseStatus, String> {
    let payload: JwtResponse = response
        .read_json()
        .map_err(|err| format!("failed to parse {operation} response: {err}"))?;

    let jwt = normalize_jwt(&payload.jwt);
    let status = verify_downloaded_jwt(&jwt)?;
    let path = write_jwt(&jwt)?;
    println!("fallow license: stored at {}", path.display());
    if let Some(trial_ends_at) = payload.trial_ends_at.as_deref() {
        let trimmed = trial_ends_at.trim();
        if !trimmed.is_empty() {
            println!("fallow license: trial ends at {trimmed}");
        }
    }
    Ok(status)
}

#[derive(Debug, Deserialize, Default)]
struct ErrorEnvelope {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// Map a backend error-code + operation pair to an actionable user-facing
/// hint. Returns `None` for unknown codes; callers fall back to the generic
/// "HTTP N: body" shape.
fn actionable_error_hint(operation: &str, code: &str) -> Option<&'static str> {
    match (operation, code) {
        ("refresh", "token_stale") => Some(
            "your stored license is too stale to refresh. Reactivate with: fallow license activate --trial --email <addr>",
        ),
        ("refresh", "invalid_token") => Some(
            "your stored license token is missing required claims. Reactivate with: fallow license activate --trial --email <addr>",
        ),
        ("refresh" | "trial", "unauthorized") => Some(
            "authentication failed. Reactivate with: fallow license activate --trial --email <addr>",
        ),
        ("trial", "rate_limit_exceeded") => Some(
            "trial creation is rate-limited to 5 per hour per IP. Wait an hour or retry from a different network (in CI, start the trial locally and set FALLOW_LICENSE on the runner).",
        ),
        _ => None,
    }
}

fn http_status_message(response: &mut impl ResponseBodyReader, operation: &str) -> String {
    let status = response.status();
    let body = response.read_to_string().unwrap_or_else(|_| String::new());
    let envelope: Option<ErrorEnvelope> = serde_json::from_str(&body).ok();
    if let Some(envelope) = envelope.as_ref()
        && let Some(code) = envelope.code.as_deref()
        && let Some(hint) = actionable_error_hint(operation, code)
    {
        return format!("{hint} (HTTP {status}, code {code})");
    }
    let body_suffix = match envelope.as_ref().and_then(|e| e.message.as_deref()) {
        Some(message) if !message.trim().is_empty() => format!(": {}", message.trim()),
        _ if !body.trim().is_empty() => format!(": {}", body.trim()),
        _ => String::new(),
    };
    format!("{operation} request failed with HTTP {status}{body_suffix}")
}

trait ResponseBodyReader {
    fn status(&self) -> u16;
    fn read_json(&mut self) -> Result<JwtResponse, ureq::Error>;
    fn read_to_string(&mut self) -> Result<String, ureq::Error>;
}

impl ResponseBodyReader for http::Response<ureq::Body> {
    fn status(&self) -> u16 {
        self.status().as_u16()
    }

    fn read_json(&mut self) -> Result<JwtResponse, ureq::Error> {
        self.body_mut().read_json()
    }

    fn read_to_string(&mut self) -> Result<String, ureq::Error> {
        self.body_mut().read_to_string()
    }
}

fn verify_downloaded_jwt(jwt: &str) -> Result<LicenseStatus, String> {
    let key = verifying_key()?;
    match verify_jwt(jwt, &key, current_unix_seconds(), DEFAULT_HARD_FAIL_DAYS) {
        Ok(status) => Ok(status),
        Err(LicenseError::Truncated { .. }) => {
            Err(format!("{}", LicenseError::Truncated { actual: jwt.len() }))
        }
        Err(err) => Err(format!("failed to verify JWT: {err}")),
    }
}

fn print_status(status: &LicenseStatus) {
    match status {
        LicenseStatus::Valid {
            claims,
            days_until_expiry,
        } => {
            println!(
                "license: VALID, tier={} seats={} features={} days_until_expiry={}",
                claims.tier,
                claims.seats,
                claims.features.join(","),
                days_until_expiry
            );
            if let Some(refresh_after) = claims.refresh_after
                && current_unix_seconds() >= refresh_after
            {
                println!(
                    "  refresh suggested now: fallow license refresh (prevents CI breakage before expiry)"
                );
            }
        }
        LicenseStatus::ExpiredWarning {
            claims,
            days_since_expiry,
        } => {
            println!(
                "license: EXPIRED ({days_since_expiry} days ago), analysis still runs in the warning window. \
                 Refresh: fallow license refresh"
            );
            println!(
                "  tier={} seats={} features={}",
                claims.tier,
                claims.seats,
                claims.features.join(",")
            );
        }
        LicenseStatus::ExpiredWatermark {
            claims,
            days_since_expiry,
        } => {
            println!(
                "license: EXPIRED ({days_since_expiry} days ago), output will show a watermark until refreshed. \
                 Refresh: fallow license refresh"
            );
            println!(
                "  tier={} seats={} features={}",
                claims.tier,
                claims.seats,
                claims.features.join(",")
            );
        }
        LicenseStatus::HardFail {
            days_since_expiry, ..
        } => {
            println!(
                "license: EXPIRED ({days_since_expiry} days ago, past grace window), paid features blocked. \
                 Refresh: fallow license refresh, or fallow license activate --trial --email <addr>"
            );
        }
        LicenseStatus::Missing => {
            println!(
                "license: NOT FOUND. Start a 30-day trial: fallow license activate --trial --email you@company.com"
            );
        }
    }
    if status.permits(&Feature::ProductionCoverage) {
        println!("  → production_coverage: ENABLED");
    } else {
        println!("  → production_coverage: disabled (upgrade or refresh)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_jwt_prefers_raw_arg() {
        let args = ActivateArgs {
            raw_jwt: Some("a.b.c".into()),
            ..Default::default()
        };
        assert_eq!(read_jwt(&args).unwrap(), "a.b.c");
    }

    #[test]
    fn read_jwt_normalizes_whitespace() {
        let args = ActivateArgs {
            raw_jwt: Some("a  .b\nc".into()),
            ..Default::default()
        };
        assert_eq!(read_jwt(&args).unwrap(), "a.bc");
    }

    #[test]
    fn read_jwt_errors_when_no_source() {
        let args = ActivateArgs::default();
        assert!(read_jwt(&args).is_err());
    }

    #[test]
    fn run_trial_without_email_errors() {
        let exit = run_trial(None);
        assert_eq!(format!("{exit:?}"), format!("{:?}", ExitCode::from(2)));
    }

    struct StubResponse {
        status: u16,
        body: String,
    }

    impl ResponseBodyReader for StubResponse {
        fn status(&self) -> u16 {
            self.status
        }

        fn read_json(&mut self) -> Result<JwtResponse, ureq::Error> {
            unreachable!("error-path tests do not read JSON")
        }

        fn read_to_string(&mut self) -> Result<String, ureq::Error> {
            Ok(std::mem::take(&mut self.body))
        }
    }

    #[test]
    fn refresh_token_stale_hint_points_to_reactivation() {
        let mut response = StubResponse {
            status: 401,
            body: r#"{"error":true,"message":"token stale","code":"token_stale"}"#.to_owned(),
        };
        let message = http_status_message(&mut response, "refresh");
        assert!(
            message.contains("Reactivate with: fallow license activate --trial"),
            "expected reactivation hint, got: {message}"
        );
        assert!(message.contains("token_stale"));
    }

    #[test]
    fn refresh_invalid_token_hint_points_to_reactivation() {
        let mut response = StubResponse {
            status: 401,
            body: r#"{"error":true,"code":"invalid_token"}"#.to_owned(),
        };
        let message = http_status_message(&mut response, "refresh");
        assert!(message.contains("missing required claims"));
        assert!(message.contains("invalid_token"));
    }

    #[test]
    fn trial_rate_limit_hint_mentions_five_per_hour() {
        let mut response = StubResponse {
            status: 429,
            body: r#"{"error":true,"code":"rate_limit_exceeded"}"#.to_owned(),
        };
        let message = http_status_message(&mut response, "trial");
        assert!(message.contains("5 per hour per IP"));
        assert!(message.contains("FALLOW_LICENSE"));
    }

    #[test]
    fn unknown_code_falls_back_to_backend_message_when_present() {
        let mut response = StubResponse {
            status: 500,
            body: r#"{"error":true,"code":"checkout_error","message":"stripe returned no session url"}"#
                .to_owned(),
        };
        let message = http_status_message(&mut response, "refresh");
        assert!(message.starts_with("refresh request failed with HTTP 500"));
        assert!(
            message.ends_with(": stripe returned no session url"),
            "expected backend message on fallback, got: {message}"
        );
    }

    #[test]
    fn unknown_code_without_message_falls_back_to_raw_body() {
        let mut response = StubResponse {
            status: 500,
            body: r#"{"error":true,"code":"checkout_error"}"#.to_owned(),
        };
        let message = http_status_message(&mut response, "refresh");
        assert!(message.starts_with("refresh request failed with HTTP 500"));
        assert!(message.contains("checkout_error"));
    }

    #[test]
    fn empty_body_still_produces_minimal_message() {
        let mut response = StubResponse {
            status: 502,
            body: String::new(),
        };
        let message = http_status_message(&mut response, "trial");
        assert_eq!(message, "trial request failed with HTTP 502");
    }
}
