use std::fmt::Write;

const BASE_PROMPT: &str = r#"You are operating as the vizier, the coordinating agent responsible for managing distributed work across several VMs and host-level environments.

The harness is active and provides the ambient execution context and event routing—assume it transparently handles observation, lifecycle, and artifact streaming. You do not invoke the harness or refer to it; it’s simply there.

Your role is to delegate. Most substantive actions occur remotely, within the VMs you control (e.g., vm-alpha, vm-beta), using standard UNIX tools and protocols available to you—ssh, scp, tar, journalctl, systemctl, rsync, etc. You may also perform limited coordination on the host machine when it advances your orchestration duties (e.g., gathering logs, synthesizing results, packaging artifacts).

Treat each operative as a clear, goal-driven task:
	•	Objective: what outcome the system expects (e.g., deploy new build to vm-alpha and verify service health).
	•	Means: the primitives at your disposal to achieve it, framed as canonical commands or sequences.
	•	Delegation: specify what runs remotely versus what you handle on the host.
	•	Observability: ensure each action yields a traceable outcome (exit status, logs, artifacts). The environment will automatically surface these.

Maintain an orchestrator’s perspective: plan, delegate, observe, synthesize. You coordinate the flow of work, not just issue isolated commands. Avoid over-explaining execution details—describe intent and execution strategy clearly so the system can proceed coherently.
"#;

/// SSH endpoint details for a VM managed by the vizier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmEndpoint {
    name: String,
    user: String,
    host: String,
    port: u16,
    auth_hint: Option<String>,
    status: Option<String>,
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
}

/// Renders the runtime prompt for the vizier agent.
#[derive(Default)]
pub struct PromptBuilder {
    endpoints: Vec<VmEndpoint>,
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
            return output;
        }

        for endpoint in &self.endpoints {
            let mut line = String::new();
            write!(
                &mut line,
                "- {}: ssh {}@{} -p {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null",
                endpoint.name, endpoint.user, endpoint.host, endpoint.port
            )
            .expect("writing to string should not fail");

            if let Some(auth_hint) = endpoint.auth_hint.as_ref() {
                write!(&mut line, " [{}]", auth_hint).expect("writing to string should not fail");
            }

            if let Some(status) = endpoint.status.as_ref() {
                write!(&mut line, "; status={}", status)
                    .expect("writing to string should not fail");
            }

            line.push('\n');
            output.push_str(&line);
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
        assert!(prompt.starts_with("You are operating as the vizier"));
        assert!(prompt.contains("# Operational Context"));
        assert!(prompt.contains("- No active VMs reported"));
    }

    #[test]
    fn renders_sorted_endpoints_with_optional_fields() {
        let endpoints = vec![
            VmEndpoint::new("vm-beta", "castra", "10.0.0.20")
                .with_port(10022)
                .with_auth_hint("-i bootstrap-key")
                .with_status("drain pending"),
            VmEndpoint::new("vm-alpha", "ubuntu", "vm-alpha.internal")
                .with_status("bootstrap"),
            VmEndpoint::new("vm-gamma", "root", "192.168.1.10"),
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
        assert!(lines[0].contains(
            "-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
        ));

        assert!(lines[1].starts_with(
            "- vm-beta: ssh castra@10.0.0.20 -p 10022 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
        ));
        assert!(lines[1].contains("[-i bootstrap-key]"));
        assert!(lines[1].contains("; status=drain pending"));

        assert!(lines[2].starts_with(
            "- vm-gamma: ssh root@192.168.1.10 -p 22 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"
        ));
        assert!(!lines[2].contains("[")); // No auth hint
        assert!(!lines[2].contains("; status="));
    }
}
