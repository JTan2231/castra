use crate::state::AppState;

pub enum Command {
    Help,
    Agents,
    Switch { target: Option<String> },
    Empty,
    Unknown(String),
}

pub enum CommandOutcome {
    None,
}

impl Default for CommandOutcome {
    fn default() -> Self {
        CommandOutcome::None
    }
}

pub fn handle(input: &str, state: &mut AppState) -> CommandOutcome {
    match parse_command(input) {
        Command::Help => {
            state.push_system_message("Commands: /agents • /switch <agent> • /help");
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
        "switch" => {
            let target = parts.next().map(|value| value.to_string());
            Command::Switch { target }
        }
        other => Command::Unknown(other.to_string()),
    }
}
