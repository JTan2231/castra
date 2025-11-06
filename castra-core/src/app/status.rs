use crate::Result;
use crate::cli::StatusArgs;
use crate::core::operations;
use crate::core::options::StatusOptions;
use crate::core::outcome::{ProjectStatusOutcome, StatusOutcome};
use crate::core::project::format_config_warnings;
use crate::core::status::format_uptime;
use std::fmt::Write as _;
use std::io::{self, IsTerminal};
use std::path::PathBuf;

use super::common::{config_load_options, emit_diagnostics, split_config_warnings};

pub fn handle_status(args: StatusArgs, config_override: Option<&PathBuf>) -> Result<()> {
    let options = StatusOptions {
        config: config_load_options(config_override, args.skip_discovery, "status")?,
        workspace: args.workspace.clone(),
    };

    let output = operations::status(options, None)?;

    let (config_warnings, other) = split_config_warnings(&output.diagnostics);
    if let Some(message) = format_config_warnings(&config_warnings) {
        eprint!("{message}");
    }
    emit_diagnostics(&other);

    let use_color = io::stdout().is_terminal();
    let rendered = render_status(&output.value, use_color);
    print!("{rendered}");

    Ok(())
}

fn render_status(outcome: &StatusOutcome, use_color: bool) -> String {
    let mut out = String::new();

    if outcome.projects.is_empty() {
        out.push_str("No active workspaces detected.\n");
        out.push_str(&render_status_legend());
        return out;
    }

    let multi = outcome.aggregated || outcome.projects.len() > 1;
    for (idx, project) in outcome.projects.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        if multi {
            let mut header = project.project_name.clone();
            if let Some(id) = &project.workspace_id {
                header.push_str(&format!(" ({id})"));
            }
            writeln!(out, "=== {header} ===").unwrap();
        }
        out.push_str(&render_project_body(project, use_color));
    }

    out.push('\n');
    out.push_str(&render_status_legend());
    out
}

fn render_project_body(project: &ProjectStatusOutcome, use_color: bool) -> String {
    let mut out = String::new();
    writeln!(
        out,
        "Project: {} ({})",
        project.project_name,
        project.project_path.display()
    )
    .unwrap();
    if let Some(config_path) = &project.config_path {
        writeln!(out, "Config path: {}", config_path.display()).unwrap();
    }
    writeln!(out, "Config version: {}", project.config_version).unwrap();
    if let Some(id) = &project.workspace_id {
        writeln!(out, "Workspace ID: {id}").unwrap();
    }
    if let Some(state_root) = &project.state_root {
        writeln!(out, "State root: {}", state_root.display()).unwrap();
    }
    let reachability_note = if project.reachable {
        "Running VMs detected"
    } else {
        "No running VMs detected"
    };
    writeln!(out, "Guests: {reachability_note}.").unwrap();
    out.push('\n');

    if project.rows.is_empty() {
        writeln!(out, "No VMs defined in configuration.").unwrap();
        return out;
    }

    let cpu_mem: Vec<String> = project
        .rows
        .iter()
        .map(|row| format!("{}/{}", row.cpus, row.memory))
        .collect();

    let vm_width = project
        .rows
        .iter()
        .map(|row| row.name.len())
        .max()
        .unwrap_or(2)
        .max("VM".len());
    let state_width = project
        .rows
        .iter()
        .map(|row| row.state.len())
        .max()
        .unwrap_or(5)
        .max("STATE".len());
    let cpu_mem_width = cpu_mem
        .iter()
        .map(|value| value.len())
        .max()
        .unwrap_or(1)
        .max("CPU/MEM".len());
    let uptime_width = project
        .rows
        .iter()
        .map(|row| format_uptime(row.uptime).len())
        .max()
        .unwrap_or(1)
        .max("UPTIME".len());

    writeln!(
        out,
        "{:<vm_width$}  {:<state_width$}  {:>cpu_mem_width$}  {:>uptime_width$}  {}",
        "VM",
        "STATE",
        "CPU/MEM",
        "UPTIME",
        "FORWARDS",
        vm_width = vm_width,
        state_width = state_width,
        cpu_mem_width = cpu_mem_width,
        uptime_width = uptime_width,
    )
    .unwrap();

    for (idx, row) in project.rows.iter().enumerate() {
        let state = style_state(&row.state, state_width, use_color);
        writeln!(
            out,
            "{:<vm_width$}  {}  {:>cpu_mem_width$}  {:>uptime_width$}  {}",
            row.name,
            state,
            cpu_mem[idx],
            format_uptime(row.uptime),
            row.forwards,
            vm_width = vm_width,
            cpu_mem_width = cpu_mem_width,
            uptime_width = uptime_width,
        )
        .unwrap();
    }

    out
}

fn render_status_legend() -> String {
    "Legend: STATE derives from VM pidfiles; CPU/MEM reflect configured values.\n\
UPTIME shows hh:mm:ss based on host monotonic time; FORWARDS lists host→guest ports.\n\
Exit codes: 0 on success; non-zero if any VM reports an error state.\n"
        .to_string()
}

fn style_state(state: &str, width: usize, use_color: bool) -> String {
    if !use_color {
        return format!("{:width$}", state, width = width);
    }

    let code = match state {
        "running" => "32",
        "starting" | "shutting_down" => "33",
        "error" => "31",
        _ => "37",
    };
    colorize(state, code, width)
}

fn colorize(text: &str, code: &str, width: usize) -> String {
    format!("\u{001b}[{code}m{:width$}\u{001b}[0m", text, width = width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::outcome::VmStatusRow;
    use std::time::Duration;

    fn sample_vm(name: &str) -> VmStatusRow {
        VmStatusRow {
            name: name.to_string(),
            state: "running".to_string(),
            cpus: 1,
            memory: "512 MiB".to_string(),
            uptime: Some(Duration::from_secs(5)),
            forwards: "—".to_string(),
        }
    }

    fn sample_project(name: &str, workspace_id: Option<&str>) -> ProjectStatusOutcome {
        ProjectStatusOutcome {
            project_path: PathBuf::from(format!("/tmp/{name}.toml")),
            project_name: name.to_string(),
            config_version: "0.2.0".to_string(),
            reachable: true,
            rows: vec![sample_vm(&format!("{name}-vm"))],
            workspace_id: workspace_id.map(|id| id.to_string()),
            state_root: Some(PathBuf::from(format!("/state/{name}"))),
            config_path: Some(PathBuf::from(format!("/tmp/{name}.toml"))),
        }
    }

    #[test]
    fn render_status_single_project_without_header() {
        let project = sample_project("demo", None);
        let outcome = StatusOutcome {
            projects: vec![project],
            aggregated: false,
        };

        let rendered = render_status(&outcome, false);
        assert!(rendered.contains("Project: demo (/tmp/demo.toml)"));
        assert!(!rendered.contains("=== demo"));
        assert!(rendered.contains("Guests: Running VMs detected."));
    }

    #[test]
    fn render_status_multiple_projects_includes_headers() {
        let p1 = sample_project("alpha", Some("alpha-1"));
        let p2 = sample_project("beta", Some("beta-2"));
        let outcome = StatusOutcome {
            projects: vec![p1, p2],
            aggregated: true,
        };

        let rendered = render_status(&outcome, false);
        assert!(rendered.contains("=== alpha (alpha-1) ==="));
        assert!(rendered.contains("=== beta (beta-2) ==="));
    }
}
