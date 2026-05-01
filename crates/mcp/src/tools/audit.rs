use crate::params::AuditParams;

use super::{VALID_AUDIT_GATES, push_global, push_scope, validation_error_body};

/// Build CLI arguments for the `audit` tool.
pub fn build_audit_args(params: &AuditParams) -> Result<Vec<String>, String> {
    if let Some(ref gate) = params.gate
        && !VALID_AUDIT_GATES.contains(&gate.as_str())
    {
        return Err(validation_error_body(format!(
            "Invalid gate '{gate}'. Valid values: new-only, all"
        )));
    }

    let mut args = vec![
        "audit".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--quiet".to_string(),
        "--explain".to_string(),
    ];

    push_global(
        &mut args,
        params.root.as_deref(),
        params.config.as_deref(),
        params.no_cache,
        params.threads,
    );
    if let Some(ref base) = params.base {
        args.extend(["--base".to_string(), base.clone()]);
    }
    push_scope(&mut args, params.production, params.workspace.as_deref());
    if params.production_dead_code == Some(true) {
        args.push("--production-dead-code".to_string());
    }
    if params.production_health == Some(true) {
        args.push("--production-health".to_string());
    }
    if params.production_dupes == Some(true) {
        args.push("--production-dupes".to_string());
    }
    if let Some(ref gb) = params.group_by {
        args.extend(["--group-by".to_string(), gb.clone()]);
    }
    if let Some(ref gate) = params.gate {
        args.extend(["--gate".to_string(), gate.clone()]);
    }
    if let Some(ref path) = params.dead_code_baseline {
        args.extend(["--dead-code-baseline".to_string(), path.clone()]);
    }
    if let Some(ref path) = params.health_baseline {
        args.extend(["--health-baseline".to_string(), path.clone()]);
    }
    if let Some(ref path) = params.dupes_baseline {
        args.extend(["--dupes-baseline".to_string(), path.clone()]);
    }
    if params.explain_skipped == Some(true) {
        args.push("--explain-skipped".to_string());
    }
    if let Some(max_crap) = params.max_crap {
        args.extend(["--max-crap".to_string(), format!("{max_crap}")]);
    }

    Ok(args)
}
