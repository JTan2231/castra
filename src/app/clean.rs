use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::Result;
use crate::cli::CleanArgs;
use crate::core::events::{CleanupKind, CleanupManagedImageEvidence};
use crate::core::operations;
use crate::core::options::{CleanOptions, CleanScope, ProjectSelector};
use crate::core::outcome::{CleanOutcome, CleanupAction, SkipReason};
use crate::core::project::{default_projects_root, format_config_warnings};

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub fn handle_clean(args: CleanArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let scope = if args.global {
        CleanScope::Global {
            projects_root: default_projects_root(),
        }
    } else {
        let selector = if let Some(root) = args.state_root.clone() {
            ProjectSelector::StateRoot(root)
        } else {
            let config = config_load_options(config_override, args.skip_discovery, "clean")?;
            ProjectSelector::Config(config)
        };
        CleanScope::Workspace(selector)
    };

    let options = CleanOptions {
        scope,
        dry_run: args.dry_run,
        include_overlays: args.include_overlays,
        include_logs: !args.no_logs,
        include_handshakes: !args.no_handshakes,
        managed_only: args.managed_only || args.global,
        force: args.force,
    };

    let output = operations::clean(options, None)?;
    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    render_clean(&output.value);
    Ok(())
}

fn render_clean(outcome: &CleanOutcome) {
    if outcome.state_roots.is_empty() {
        if outcome.dry_run {
            println!("Dry run complete; no matching state roots found.");
        } else {
            println!("No matching state roots found.");
        }
        return;
    }

    if outcome.dry_run {
        println!("Dry run: no files were removed.");
        println!();
    }

    let mut total_reclaimed = 0u64;
    for cleanup in &outcome.state_roots {
        println!("State root: {}", cleanup.state_root.display());
        if let Some(name) = &cleanup.project_name {
            println!("  Project: {name}");
        }
        println!("  Reclaimed: {}", format_bytes(cleanup.reclaimed_bytes));
        total_reclaimed += cleanup.reclaimed_bytes;
        if cleanup.actions.is_empty() {
            println!("  Actions: none");
        } else {
            println!("  Actions:");
            for action in &cleanup.actions {
                match action {
                    CleanupAction::Removed {
                        path,
                        bytes,
                        kind,
                        managed_evidence,
                    } => {
                        println!(
                            "    removed {:<15} {} ({})",
                            kind.describe(),
                            path.display(),
                            format_bytes(*bytes)
                        );
                        render_managed_evidence(*kind, managed_evidence);
                    }
                    CleanupAction::Skipped {
                        path,
                        reason,
                        kind,
                        managed_evidence,
                    } => {
                        println!(
                            "    skipped {:<15} {} ({})",
                            kind.describe(),
                            path.display(),
                            format_skip_reason(reason)
                        );
                        render_managed_evidence(*kind, managed_evidence);
                    }
                }
            }
        }
        println!();
    }

    let qualifier = if outcome.dry_run { " (dry run)" } else { "" };
    println!(
        "Total reclaimed: {}{qualifier}.",
        format_bytes(total_reclaimed)
    );
}

fn render_managed_evidence(kind: CleanupKind, evidence: &[CleanupManagedImageEvidence]) {
    if !matches!(kind, CleanupKind::ManagedImages) {
        return;
    }

    if evidence.is_empty() {
        println!("      evidence: none (no managed-image verification records found)");
        return;
    }

    println!("      evidence:");
    for entry in evidence {
        let when = format_verified_at(&entry.verified_at);
        let bytes_text = entry
            .total_bytes
            .map(format_bytes)
            .unwrap_or_else(|| "unknown".to_string());
        println!(
            "        image {}@{} at {} verified {} (artifact bytes {}, log {})",
            entry.image_id,
            entry.image_version,
            entry.root_disk_path.display(),
            when,
            bytes_text,
            entry.log_path.display()
        );
        if let Some(delta) = entry.verification_delta {
            println!("          filesystem delta: {}", format_duration(delta));
        }
        if !entry.artifacts.is_empty() {
            println!("          artifacts: {}", entry.artifacts.join(", "));
        }
    }
}

fn format_verified_at(time: &SystemTime) -> String {
    let datetime: OffsetDateTime = (*time).into();
    match datetime.format(&Rfc3339) {
        Ok(formatted) => formatted,
        Err(_) => "<invalid timestamp>".to_string(),
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_duration(duration: Duration) -> String {
    if duration.as_millis() == 0 {
        return "0ms".to_string();
    }

    if duration.as_secs() >= 3600 {
        format!("{:.1}h", duration.as_secs_f64() / 3600.0)
    } else if duration.as_secs() >= 60 {
        format!("{:.1}m", duration.as_secs_f64() / 60.0)
    } else if duration.as_secs() >= 1 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{}ms", duration.as_millis())
    }
}

fn format_skip_reason(reason: &SkipReason) -> String {
    match reason {
        SkipReason::Missing => "not found".to_string(),
        SkipReason::DryRun => "dry run".to_string(),
        SkipReason::FlagDisabled => "disabled by flags".to_string(),
        SkipReason::ManagedOnly => "limited to managed artifacts".to_string(),
        SkipReason::RunningProcess => {
            "blocked by running process (rerun with --force once stopped)".to_string()
        }
        SkipReason::Io(message) => format!("io error: {message}"),
    }
}
