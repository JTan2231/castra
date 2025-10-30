use crate::state::AppState;

pub enum Command {
    Help,
    Agents,
    Up,
    Switch {
        target: Option<String>,
    },
    Codex {
        target: Option<String>,
        payload: String,
    },
    Empty,
    Unknown(String),
}

pub enum CommandOutcome {
    None,
    Up,
    Codex { vm: String, payload: String },
}

impl Default for CommandOutcome {
    fn default() -> Self {
        CommandOutcome::None
    }
}

pub fn handle(input: &str, state: &mut AppState) -> CommandOutcome {
    match parse_command(input) {
        Command::Help => {
            state.push_system_message("Commands: /up • /agents • /switch <agent> • /help");
        }
        Command::Agents => {
            let listing = state
                .roster()
                .agents()
                .iter()
                .enumerate()
                .map(|(idx, agent)| {
                    let label = agent.label();
                    if idx == state.active_agent_index() {
                        format!("{} ({} • active)", label, agent.status())
                    } else {
                        format!("{} ({})", label, agent.status())
                    }
                })
                .collect::<Vec<_>>()
                .join(" | ");
            state.push_system_message(format!("Available agents: {}", listing));
        }
        Command::Switch { target } => {
            let Some(target) = target else {
                state.push_system_message("Usage: /switch <agent-id>");
                return CommandOutcome::None;
            };

            if let Some(index) = state.agent_index_by_id(&target) {
                if state.switch_agent(index) {
                    let label = state.active_agent_label();
                    state.push_system_message(format!("Active agent set to {}", label));
                } else {
                    let label = state.active_agent_label();
                    state.push_system_message(format!("{} is already active.", label));
                }
            } else {
                state.push_system_message(format!("Unknown agent '{}'. Try /agents.", target));
            }
        }
        Command::Codex { target, payload } => {
            let payload = payload.trim();
            if payload.is_empty() {
                state.push_system_message("Usage: /codex [@vm] <payload>");
                return CommandOutcome::None;
            }

            let vm_name = if let Some(target) = target {
                let trimmed = target.trim();
                if trimmed.is_empty() {
                    state.push_system_message("Usage: /codex [@vm] <payload>");
                    return CommandOutcome::None;
                }
                match state.resolve_vm_name(trimmed) {
                    Some(name) => name,
                    None => {
                        state.push_system_message(format!(
                            "Unknown VM '{}'. Try /codex @vm <payload>.",
                            trimmed
                        ));
                        return CommandOutcome::None;
                    }
                }
            } else if let Some(focused) = state.focused_vm_name() {
                focused
            } else {
                state.push_system_message("No VM focused; press Tab or use /codex @vm ...");
                return CommandOutcome::None;
            };

            return CommandOutcome::Codex {
                vm: vm_name,
                payload: payload.to_string(),
            };
        }
        Command::Up => {
            return CommandOutcome::Up;
        }
        Command::Unknown(other) => {
            if !other.is_empty() {
                state.push_system_message(format!("Unrecognized command '{}'. Try /help.", other));
            }
        }
        Command::Empty => {}
    }

    CommandOutcome::None
}

fn parse_command(input: &str) -> Command {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Command::Empty;
    }

    let mut parts = trimmed.trim_start_matches('/').split_whitespace();
    let command = parts.next().unwrap_or_default();
    match command {
        "" => Command::Empty,
        "help" => Command::Help,
        "agents" => Command::Agents,
        "up" => Command::Up,
        "switch" => {
            let target = parts.next().map(|value| value.to_string());
            Command::Switch { target }
        }
        "codex" => {
            let mut tokens: Vec<_> = parts.map(|value| value.to_string()).collect();
            let mut target = None;

            if let Some(first) = tokens.first() {
                if first.starts_with('@') {
                    let name = first.trim_start_matches('@').to_string();
                    target = Some(name);
                    tokens.remove(0);
                }
            }

            let payload = tokens.join(" ");
            Command::Codex { target, payload }
        }
        other => Command::Unknown(other.to_string()),
    }
}
