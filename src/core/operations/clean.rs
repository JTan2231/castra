use std::collections::{HashMap, HashSet, hash_map::Entry};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};

use crate::core::diagnostics::{Diagnostic, Severity};
use crate::core::events::{CleanupKind, CleanupManagedImageEvidence, Event};
use crate::core::options::{CleanOptions, CleanScope, ConfigLoadOptions, ProjectSelector};
use crate::core::outcome::{
    CleanOutcome, CleanupAction, OperationOutput, OperationResult, SkipReason, StateRootCleanup,
};
use crate::core::project::config_state_root;
use crate::core::reporter::Reporter;
use crate::core::runtime::{
    BrokerProcessState, broker_handshake_dir_from_root, inspect_broker_state, inspect_vm_state,
};
use crate::core::status;

use serde_json::Value;

use super::{ReporterProxy, load_project_for_operation};

pub(super) fn clean(
    options: CleanOptions,
    reporter: Option<&mut dyn Reporter>,
) -> OperationResult<CleanOutcome> {
    let mut diagnostics = Vec::new();
    let mut events = Vec::new();
    let mut reporter = ReporterProxy::new(reporter, &mut events);

    let mut state_root_results = Vec::new();

    match options.scope.clone() {
        CleanScope::Workspace(selector) => {
            let cleanup = clean_workspace(selector, &options, &mut reporter, &mut diagnostics)?;
            if let Some(result) = cleanup {
                state_root_results.push(result);
            }
        }
        CleanScope::Global { projects_root } => {
            if !projects_root.exists() {
                diagnostics.push(
                    Diagnostic::new(
                        Severity::Info,
                        format!(
                            "Global projects root {} does not exist; nothing to clean.",
                            projects_root.display()
                        ),
                    )
                    .with_help("Run `castra clean --workspace` within a project to clean a single state root."),
                );
            } else {
                let entries =
                    fs::read_dir(&projects_root).map_err(|err| Error::PreflightFailed {
                        message: format!(
                            "Failed to list projects root {}: {err}",
                            projects_root.display()
                        ),
                    })?;
                for entry in entries {
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(err) => {
                            diagnostics.push(Diagnostic::new(
                                Severity::Warning,
                                format!(
                                    "Failed to read entry under {}: {err}",
                                    projects_root.display()
                                ),
                            ));
                            continue;
                        }
                    };
                    let path = entry.path();
                    if path.is_dir() {
                        let selector = ProjectSelector::StateRoot(path.clone());
                        if let Some(result) =
                            clean_workspace(selector, &options, &mut reporter, &mut diagnostics)?
                        {
                            state_root_results.push(result);
                        }
                    }
                }
            }
        }
    }

    let outcome = CleanOutcome {
        dry_run: options.dry_run,
        state_roots: state_root_results,
    };

    let total_reclaimed: u64 = outcome
        .state_roots
        .iter()
        .map(|cleanup| cleanup.reclaimed_bytes)
        .sum();
    if outcome.state_roots.is_empty() {
        reporter.emit(Event::Message {
            severity: Severity::Info,
            text: "No matching state roots found.".to_string(),
        });
    } else if options.dry_run {
        reporter.emit(Event::Message {
            severity: Severity::Info,
            text: "Dry run complete; no files were removed.".to_string(),
        });
    } else {
        reporter.emit(Event::Message {
            severity: Severity::Info,
            text: format!(
                "Reclaimed {total_reclaimed} bytes across {} state root(s).",
                outcome.state_roots.len()
            ),
        });
    }

    Ok(OperationOutput::new(outcome)
        .with_diagnostics(diagnostics)
        .with_events(events))
}

fn clean_workspace(
    selector: ProjectSelector,
    options: &CleanOptions,
    reporter: &mut ReporterProxy<'_, '_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Option<StateRootCleanup>> {
    match selector {
        ProjectSelector::Config(config) => {
            clean_using_config(config, options, reporter, diagnostics).map(Some)
        }
        ProjectSelector::StateRoot(path) => {
            if !path.exists() {
                diagnostics.push(Diagnostic::new(
                    Severity::Info,
                    format!(
                        "State root {} does not exist; nothing to clean.",
                        path.display()
                    ),
                ));
                Ok(None)
            } else {
                clean_state_root(
                    None,
                    path,
                    Vec::new(),
                    Vec::new(),
                    options,
                    reporter,
                    diagnostics,
                )
                .map(Some)
            }
        }
    }
}

fn clean_using_config(
    config: ConfigLoadOptions,
    options: &CleanOptions,
    reporter: &mut ReporterProxy<'_, '_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<StateRootCleanup> {
    let (project, _synthetic) = load_project_for_operation(&config, diagnostics)?;
    let state_root = config_state_root(&project);

    ensure_not_running_config(&project, options.force, diagnostics)?;

    let overlays: HashSet<PathBuf> = project.vms.iter().map(|vm| vm.overlay.clone()).collect();
    let overlay_list = overlays.into_iter().collect::<Vec<_>>();
    let vm_names = project
        .vms
        .iter()
        .map(|vm| vm.name.clone())
        .collect::<Vec<_>>();

    clean_state_root(
        Some(project.project_name.clone()),
        state_root,
        overlay_list,
        vm_names,
        options,
        reporter,
        diagnostics,
    )
}

fn ensure_not_running_config(
    project: &crate::config::ProjectConfig,
    force: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let status_snapshot = status::collect_status(project);
    diagnostics.extend(status_snapshot.diagnostics);

    let mut running_vms = status_snapshot
        .rows
        .iter()
        .filter(|row| row.state == "running")
        .map(|row| row.name.clone())
        .collect::<Vec<_>>();
    let broker_running = matches!(
        status_snapshot.broker_state,
        BrokerProcessState::Running { .. }
    );
    if broker_running {
        running_vms.push("broker".to_string());
    }

    if running_vms.is_empty() {
        return Ok(());
    }

    let joined = running_vms.join(", ");
    if force {
        diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("Proceeding with cleanup despite running processes: {joined} (forced)."),
            )
            .with_help("Ensure guests are idle to avoid corrupting state."),
        );
        Ok(())
    } else {
        Err(Error::PreflightFailed {
            message: format!(
                "Detected running processes: {joined}.\nStop them first with `castra down` or rerun with `--force` if you are sure they are already stopped.",
            ),
        })
    }
}

fn ensure_not_running_state_root(
    state_root: &Path,
    force: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    if !state_root.exists() {
        return Ok(());
    }
    let broker_pid = state_root.join("broker.pid");
    let (broker_state, mut broker_warnings) = inspect_broker_state(&broker_pid);
    diagnostics.extend(
        broker_warnings
            .drain(..)
            .map(|warning| Diagnostic::new(Severity::Warning, warning)),
    );

    let mut running = Vec::new();
    if matches!(broker_state, BrokerProcessState::Running { .. }) {
        running.push("broker".to_string());
    }

    if let Ok(entries) = fs::read_dir(state_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("pid")
                && path.file_name().and_then(|name| name.to_str()) != Some("broker.pid")
            {
                let name = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("vm")
                    .to_string();
                let (state, _uptime, mut warnings) = inspect_vm_state(&path, &name);
                diagnostics.extend(
                    warnings
                        .drain(..)
                        .map(|warning| Diagnostic::new(Severity::Warning, warning)),
                );
                if state == "running" {
                    running.push(name);
                }
            }
        }
    }

    if running.is_empty() {
        return Ok(());
    }

    let joined = running.join(", ");
    if force {
        diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!("Proceeding with cleanup despite running processes: {joined} (forced)."),
            )
            .with_help("Ensure guests are idle to avoid corrupting state."),
        );
        Ok(())
    } else {
        Err(Error::PreflightFailed {
            message: format!(
                "Detected running processes: {joined}.\nStop them first or rerun with `--force` if you are sure they are already stopped.",
            ),
        })
    }
}

fn clean_state_root(
    project_name: Option<String>,
    state_root: PathBuf,
    overlays: Vec<PathBuf>,
    vm_names: Vec<String>,
    options: &CleanOptions,
    reporter: &mut ReporterProxy<'_, '_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<StateRootCleanup> {
    ensure_not_running_state_root(&state_root, options.force, diagnostics)?;

    let mut actions = Vec::new();
    let mut reclaimed = 0u64;

    reclaimed += process_target(
        &state_root.join("images"),
        CleanupKind::ManagedImages,
        options,
        reporter,
        &mut actions,
        true,
    )?;

    reclaimed += process_target(
        &state_root.join("logs"),
        CleanupKind::Logs,
        options,
        reporter,
        &mut actions,
        options.include_logs,
    )?;

    reclaimed += process_target(
        &broker_handshake_dir_from_root(&state_root),
        CleanupKind::Handshakes,
        options,
        reporter,
        &mut actions,
        options.include_handshakes,
    )?;

    let pid_candidates = collect_pid_paths(&state_root, &overlays, &vm_names)?;
    for pid in pid_candidates {
        reclaimed += process_target(
            &pid,
            CleanupKind::PidFile,
            options,
            reporter,
            &mut actions,
            true,
        )?;
    }

    if options.include_overlays && !options.managed_only {
        for overlay in overlays {
            reclaimed += process_target(
                &overlay,
                CleanupKind::Overlay,
                options,
                reporter,
                &mut actions,
                true,
            )?;
        }
    } else if !overlays.is_empty() && !options.include_overlays {
        for overlay in overlays {
            actions.push(CleanupAction::Skipped {
                path: overlay,
                reason: if options.managed_only {
                    SkipReason::ManagedOnly
                } else {
                    SkipReason::FlagDisabled
                },
                kind: CleanupKind::Overlay,
            });
        }
    }

    Ok(StateRootCleanup {
        state_root,
        project_name,
        reclaimed_bytes: reclaimed,
        actions,
    })
}

fn collect_pid_paths(
    state_root: &Path,
    overlays: &[PathBuf],
    vm_names: &[String],
) -> Result<Vec<PathBuf>> {
    let mut paths = HashSet::new();
    paths.insert(state_root.join("broker.pid"));
    for name in vm_names {
        paths.insert(state_root.join(format!("{name}.pid")));
    }
    for overlay in overlays {
        if let Some(file_name) = overlay.file_stem().and_then(|stem| stem.to_str()) {
            let vm_name = file_name.split('.').next().unwrap_or(file_name);
            paths.insert(state_root.join(format!("{vm_name}.pid")));
        }
    }

    if let Ok(entries) = fs::read_dir(state_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("pid") {
                paths.insert(path);
            }
        }
    }

    Ok(paths.into_iter().collect())
}

fn process_target(
    path: &Path,
    kind: CleanupKind,
    options: &CleanOptions,
    reporter: &mut ReporterProxy<'_, '_>,
    actions: &mut Vec<CleanupAction>,
    enabled: bool,
) -> Result<u64> {
    if !enabled {
        actions.push(CleanupAction::Skipped {
            path: path.to_path_buf(),
            reason: SkipReason::FlagDisabled,
            kind,
        });
        return Ok(0);
    }

    if options.managed_only && !matches!(kind, CleanupKind::ManagedImages) {
        actions.push(CleanupAction::Skipped {
            path: path.to_path_buf(),
            reason: SkipReason::ManagedOnly,
            kind,
        });
        return Ok(0);
    }

    if !path.exists() {
        actions.push(CleanupAction::Skipped {
            path: path.to_path_buf(),
            reason: SkipReason::Missing,
            kind,
        });
        return Ok(0);
    }

    let evidence = if matches!(kind, CleanupKind::ManagedImages) {
        collect_managed_image_evidence(path)
    } else {
        Vec::new()
    };

    let size = match measure_path(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            actions.push(CleanupAction::Skipped {
                path: path.to_path_buf(),
                reason: SkipReason::Io(err.to_string()),
                kind,
            });
            return Ok(0);
        }
    };

    if options.dry_run {
        reporter.emit(Event::CleanupProgress {
            path: path.to_path_buf(),
            kind,
            bytes: size,
            dry_run: true,
            managed_evidence: evidence,
        });
        actions.push(CleanupAction::Skipped {
            path: path.to_path_buf(),
            reason: SkipReason::DryRun,
            kind,
        });
        return Ok(0);
    }

    let result = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };

    match result {
        Ok(()) => {
            reporter.emit(Event::CleanupProgress {
                path: path.to_path_buf(),
                kind,
                bytes: size,
                dry_run: false,
                managed_evidence: evidence.clone(),
            });
            actions.push(CleanupAction::Removed {
                path: path.to_path_buf(),
                bytes: size,
                kind,
            });
            Ok(size)
        }
        Err(err) => {
            actions.push(CleanupAction::Skipped {
                path: path.to_path_buf(),
                reason: SkipReason::Io(err.to_string()),
                kind,
            });
            Ok(0)
        }
    }
}

fn measure_path(path: &Path) -> io::Result<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_file() || metadata.file_type().is_symlink() {
        Ok(metadata.len())
    } else if metadata.is_dir() {
        let mut total = 0u64;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            total += measure_path(&entry.path())?;
        }
        Ok(total)
    } else {
        Ok(0)
    }
}

fn collect_managed_image_evidence(path: &Path) -> Vec<CleanupManagedImageEvidence> {
    let state_root = match path.parent() {
        Some(parent) => parent,
        None => return Vec::new(),
    };
    let log_path = state_root.join("logs").join("image-manager.log");
    let contents = match fs::read_to_string(&log_path) {
        Ok(contents) => contents,
        Err(_) => return Vec::new(),
    };

    let mut latest: HashMap<(String, String), CleanupManagedImageEvidence> = HashMap::new();

    for line in contents.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("event").and_then(|entry| entry.as_str())
            != Some("managed-image-verification-result")
        {
            continue;
        }
        let (Some(image_id), Some(image_version)) = (
            value.get("image").and_then(|entry| entry.as_str()),
            value.get("version").and_then(|entry| entry.as_str()),
        ) else {
            continue;
        };

        let timestamp = value
            .get("completed_at")
            .and_then(|entry| entry.as_u64())
            .or_else(|| value.get("ts").and_then(|entry| entry.as_u64()));
        let Some(ts) = timestamp else {
            continue;
        };
        let verified_at: SystemTime = UNIX_EPOCH + Duration::from_secs(ts);

        let artifacts = value
            .get("artifacts")
            .and_then(|entry| entry.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|artifact| {
                        artifact
                            .get("filename")
                            .and_then(|field| field.as_str())
                            .map(str::to_string)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let evidence = CleanupManagedImageEvidence {
            image_id: image_id.to_string(),
            image_version: image_version.to_string(),
            log_path: log_path.clone(),
            verified_at,
            artifacts,
        };

        match latest.entry((image_id.to_string(), image_version.to_string())) {
            Entry::Occupied(mut occupied) => {
                if evidence.verified_at > occupied.get().verified_at {
                    occupied.insert(evidence);
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(evidence);
            }
        }
    }

    latest.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn base_options(scope: CleanScope) -> CleanOptions {
        CleanOptions {
            scope,
            dry_run: false,
            include_overlays: false,
            include_logs: true,
            include_handshakes: true,
            managed_only: false,
            force: true,
        }
    }

    #[test]
    fn dry_run_preserves_files() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        fs::create_dir(root.join("images")).expect("images dir");
        fs::create_dir(root.join("logs")).expect("logs dir");
        fs::create_dir(root.join("handshakes")).expect("handshakes dir");

        let mut options = base_options(CleanScope::Workspace(ProjectSelector::StateRoot(
            root.to_path_buf(),
        )));
        options.dry_run = true;
        let result = clean(options, None).expect("clean result");
        assert!(root.join("images").exists());
        assert!(root.join("logs").exists());
        assert!(root.join("handshakes").exists());
        assert!(result.value.dry_run);
        assert!(!result.value.state_roots.is_empty());
        let actions = &result.value.state_roots[0].actions;
        assert!(actions.iter().any(|action| matches!(
            action,
            CleanupAction::Skipped {
                reason: SkipReason::DryRun,
                ..
            }
        )));
    }

    #[test]
    fn cleanup_removes_state_files() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        fs::create_dir(root.join("images")).expect("images dir");
        fs::create_dir(root.join("logs")).expect("logs dir");
        fs::create_dir(root.join("handshakes")).expect("handshakes dir");
        let overlay_path = root.join("overlay.qcow2");
        fs::write(&overlay_path, b"overlay").expect("overlay");
        let pidfile = root.join("vm.pid");
        fs::write(&pidfile, format!("{}\n", std::process::id())).expect("pidfile");

        let mut options = base_options(CleanScope::Workspace(ProjectSelector::StateRoot(
            root.to_path_buf(),
        )));
        options.include_overlays = true;
        let result = clean(options, None).expect("clean result");

        assert!(!root.join("images").exists());
        assert!(!root.join("logs").exists());
        assert!(!root.join("handshakes").exists());
        // Overlay remains because state-root mode lacks overlay metadata.
        assert!(overlay_path.exists());
        assert!(!pidfile.exists());
        assert!(!result.value.state_roots.is_empty());
        assert!(result.value.state_roots[0].reclaimed_bytes > 0);
    }
}
