use std::fmt::Write;

const BASE_PROMPT: &str = r#"You are operating as the workspace coordinator, the agent responsible for managing distributed work across several VMs and the surrounding host environment.

The harness supplies the execution context and event routing—assume it transparently handles observation, lifecycle, and artifact streaming. Do not reference the harness directly; work through the affordances it exposes.

Dispatch remote work through the harness-generated wrappers listed under `script=` in the operational context. Each wrapper delegates to `vm_commands.sh`, exporting the correct SSH target, port, identity, and options for that VM. Use the helper as follows:

  vm_commands.sh send [--wait] "<command text>"
    • Include --wait to stream stdout/stderr until the command finishes; omit it to detach and inspect later via view-output.
  vm_commands.sh launch_subagent "<prompt>"
    • Shortcut for spawning a Codex turn remotely (`codex exec --json --dangerously-bypass-approvals-and-sandbox "<prompt>"`) using the same run tracking as send.
  vm_commands.sh interrupt <pgid>
  vm_commands.sh list
  vm_commands.sh view-output <run_id> [stdout|stderr|both]

Wrappers set `SSH_TARGET` (required) along with optional `SSH_PORT`, `SSH_EXTRA_OPTS`, and `SSH_STRICT=1` when strict host key checking is needed. If a wrapper is unavailable or you must retarget, export these variables yourself before invoking `./vm_commands.sh`. Each run captures stdout/stderr artifacts keyed by `run_id`; revisit them with `vm_commands.sh view-output` when reporting status or verifying results.

Assume complete administrative control over every VM placed under your supervision—and only those VMs. You may install packages, reshape configuration, and spawn or retire long-lived processes at will, but do not alter hosts outside the declared set. Prefer canonical UNIX session primitives—systemd-run, tmux, screen, nohup, journalctl, systemctl, rsync, scp—for managing concurrent tasks and inspecting their state. Use them to keep multiple agents running simultaneously without disturbing one another, and to surface their session details for later inspection.

Your role is to coordinate. Most substantive actions occur remotely, within the VMs you control (e.g., vm-alpha, vm-beta), using the standard UNIX tools above. You may also perform limited work on the host when it advances your orchestration duties (e.g., gathering logs, synthesizing results, packaging artifacts).

Treat each operative as a clear, goal-driven task:
	•	Objective: what outcome the system expects (e.g., deploy a new build to vm-alpha and verify service health).
	•	Means: the primitives at your disposal to achieve it, framed as canonical commands or sequences.
	•	Delegation: specify what runs remotely versus what you handle on the host.
	•	Session control: describe how you will establish, name, and inspect long-running activity (systemd unit, tmux session, etc.).
	•	Observability: ensure each action yields a traceable outcome (exit status, logs, artifacts). The environment will automatically surface these.

Maintain an orchestrator’s perspective: plan, delegate, observe, synthesize. You coordinate the flow of work, not just issue isolated commands. Avoid over-explaining execution details—describe intent and execution strategy clearly so the system can proceed coherently.
"#;

/// SSH endpoint details for a managed VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmEndpoint {
    name: String,
    user: String,
    host: String,
    port: u16,
    auth_hint: Option<String>,
    status: Option<String>,
    wrapper_script: Option<String>,
}

impl VmEndpoint {
    /// Construct a new endpoint with the default SSH port (22).
    pub fn new<N, U, H>(name: N, user: U, host: H) -> Self
    where
        N: Into<String>,
        U: Into<String>,
        H: Into<String>,
    {
        Self {
            name: name.into(),
            user: user.into(),
            host: host.into(),
            port: 22,
            auth_hint: None,
            status: None,
            wrapper_script: None,
        }
    }

    /// Override the default SSH port.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Supply an auth hint (identity file label, agent forwarding, etc.).
    pub fn with_auth_hint<S: Into<String>>(mut self, hint: S) -> Self {
        self.auth_hint = Some(hint.into());
        self
    }

    /// Attach an operational status note.
    pub fn with_status<S: Into<String>>(mut self, status: S) -> Self {
        self.status = Some(status.into());
        self
    }

    /// Attach a filesystem path to the harness-generated wrapper script.
    pub fn with_wrapper_script<S: Into<String>>(mut self, script: S) -> Self {
        self.wrapper_script = Some(script.into());
        self
    }
}

/// Renders the runtime prompt for the workspace coordinator.
#[derive(Default)]
pub struct PromptBuilder {
    endpoints: Vec<VmEndpoint>,
    bootstrap_scripts: Vec<(String, String)>,
}

impl PromptBuilder {
    /// Create a new prompt builder instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach operational context covering SSH endpoints for active VMs.
    pub fn with_operational_context<I>(mut self, endpoints: I) -> Self
    where
        I: IntoIterator<Item = VmEndpoint>,
    {
        self.endpoints = endpoints.into_iter().collect();
        self.endpoints.sort_by(|a, b| a.name.cmp(&b.name));
        self
    }

    /// Attach resolved bootstrap scripts so the coordinator can inspect payloads.
    pub fn with_bootstrap_scripts(mut self, scripts: Vec<(String, String)>) -> Self {
        self.bootstrap_scripts = scripts;
        self.bootstrap_scripts.sort_by(|a, b| a.0.cmp(&b.0));
        self
    }

    /// Render the final prompt string.
    pub fn build(&self) -> String {
        let mut output = String::from(BASE_PROMPT);
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push('\n');
        output.push_str("# Operational Context\n");

        if self.endpoints.is_empty() {
            output.push_str("- No active VMs reported\n");
        } else {
            for endpoint in &self.endpoints {
                let mut line = String::new();
                write!(
                    &mut line,
                    "- {}: ssh {}@{} -p {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null",
                    endpoint.name, endpoint.user, endpoint.host, endpoint.port
                )
                .expect("writing to string should not fail");

                if let Some(auth_hint) = endpoint.auth_hint.as_ref() {
                    write!(&mut line, " [{}]", auth_hint)
                        .expect("writing to string should not fail");
                }

                if let Some(status) = endpoint.status.as_ref() {
                    write!(&mut line, "; status={}", status)
                        .expect("writing to string should not fail");
                }

                if let Some(script) = endpoint.wrapper_script.as_ref() {
                    write!(&mut line, "; script={}", script)
                        .expect("writing to string should not fail");
                }

                line.push('\n');
                output.push_str(&line);
            }
        }

        if !self.bootstrap_scripts.is_empty() {
            output.push('\n');
            output.push_str("# Bootstrap Scripts\n");
            for (index, (vm, script)) in self.bootstrap_scripts.iter().enumerate() {
                output.push_str("## ");
                output.push_str(vm);
                output.push('\n');
                output.push_str("```sh\n");
                output.push_str(script);
                if !script.ends_with('\n') {
                    output.push('\n');
                }
                output.push_str("```\n");
                if index + 1 < self.bootstrap_scripts.len() {
                    output.push('\n');
                }
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_placeholder_when_no_endpoints() {
        let prompt = PromptBuilder::new().build();
        assert!(prompt.starts_with("You are operating as the workspace coordinator"));
        assert!(prompt.contains("# Operational Context"));
        assert!(prompt.contains("- No active VMs reported"));
    }

    #[test]
    fn renders_sorted_endpoints_with_optional_fields() {
        let endpoints = vec![
            VmEndpoint::new("vm-beta", "castra", "10.0.0.20")
                .with_port(10022)
                .with_auth_hint("-i bootstrap-key")
                .with_status("drain pending")
                .with_wrapper_script("/tmp/castra/vm-beta.sh"),
            VmEndpoint::new("vm-alpha", "ubuntu", "vm-alpha.internal")
                .with_status("bootstrap")
                .with_wrapper_script("/tmp/castra/vm-alpha.sh"),
            VmEndpoint::new("vm-gamma", "root", "192.168.1.10")
                .with_wrapper_script("/tmp/castra/vm-gamma.sh"),
        ];

        let prompt = PromptBuilder::new()
            .with_operational_context(endpoints)
            .build();

        let context_start = prompt
            .find("# Operational Context\n")
            .expect("context header present");
        let context = &prompt[context_start..];

        let lines: Vec<&str> = context.lines().skip(1).collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("- vm-alpha: ssh ubuntu@vm-alpha.internal -p 22"));
        assert!(lines[0].contains("; status=bootstrap"));
        assert!(lines[0].contains("-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"));
        assert!(lines[0].contains("; script=/tmp/castra/vm-alpha.sh"));

        assert!(lines[1].starts_with(
            "- vm-beta: ssh castra@10.0.0.20 -p 10022 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
        ));
        assert!(lines[1].contains("[-i bootstrap-key]"));
        assert!(lines[1].contains("; status=drain pending"));
        assert!(lines[1].contains("; script=/tmp/castra/vm-beta.sh"));

        assert!(lines[2].starts_with(
            "- vm-gamma: ssh root@192.168.1.10 -p 22 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
        ));
        assert!(!lines[2].contains("[")); // No auth hint
        assert!(!lines[2].contains("; status="));
        assert!(lines[2].contains("; script=/tmp/castra/vm-gamma.sh"));
    }

    #[test]
    fn renders_bootstrap_scripts_section_sorted_by_vm() {
        let scripts = vec![
            ("vm-beta".to_string(), "echo beta".to_string()),
            ("vm-alpha".to_string(), "echo alpha\nline2".to_string()),
        ];

        let prompt = PromptBuilder::new()
            .with_operational_context(Vec::<VmEndpoint>::new())
            .with_bootstrap_scripts(scripts)
            .build();

        let section_start = prompt
            .find("# Bootstrap Scripts\n")
            .expect("bootstrap scripts section present");
        let section = &prompt[section_start..];

        let alpha_index = section.find("## vm-alpha").expect("vm-alpha present");
        let beta_index = section.find("## vm-beta").expect("vm-beta present");
        assert!(alpha_index < beta_index, "scripts sorted by vm name");

        assert!(
            section.contains("## vm-alpha\n```sh\necho alpha\nline2\n```"),
            "vm-alpha script rendered with newline preserved"
        );
        assert!(
            section.contains("## vm-beta\n```sh\necho beta\n```"),
            "vm-beta script rendered and terminates newline"
        );
    }
}
