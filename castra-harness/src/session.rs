use std::collections::HashMap;

use crate::events::{
    CommandExecutionItem, CommandExecutionStatus, FileChangeItem, FileUpdateChange,
    PatchApplyStatus, ThreadEvent, ThreadItem, ThreadItemDetails, Usage,
};

#[derive(Default)]
pub struct SessionState {
    thread_id: Option<String>,
    commands: HashMap<String, CommandEntry>,
}

struct CommandEntry {
    command: String,
    last_output_len: usize,
}

#[derive(Debug, Clone)]
pub enum SessionUpdate {
    ThreadStarted {
        thread_id: String,
    },
    AgentMessage {
        text: String,
    },
    Reasoning {
        text: String,
    },
    CommandProgress {
        command: String,
        output: String,
        status: CommandExecutionStatus,
        exit_code: Option<i32>,
    },
    FileChange {
        changes: Vec<FileUpdateChange>,
        status: PatchApplyStatus,
    },
    TodoList {
        items: Vec<crate::events::TodoItem>,
    },
    Usage {
        usage: Usage,
    },
    Failure {
        message: String,
    },
}

enum ItemStage {
    Started,
    Updated,
    Completed,
}

impl SessionState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, event: ThreadEvent) -> Vec<SessionUpdate> {
        match event {
            ThreadEvent::ThreadStarted(evt) => {
                self.thread_id = Some(evt.thread_id.clone());
                vec![SessionUpdate::ThreadStarted {
                    thread_id: evt.thread_id,
                }]
            }
            ThreadEvent::TurnStarted(_) => Vec::new(),
            ThreadEvent::TurnCompleted(evt) => {
                vec![SessionUpdate::Usage { usage: evt.usage }]
            }
            ThreadEvent::TurnFailed(evt) => {
                vec![SessionUpdate::Failure {
                    message: evt.error.message,
                }]
            }
            ThreadEvent::ItemStarted(evt) => self.handle_item(evt.item, ItemStage::Started),
            ThreadEvent::ItemUpdated(evt) => self.handle_item(evt.item, ItemStage::Updated),
            ThreadEvent::ItemCompleted(evt) => self.handle_item(evt.item, ItemStage::Completed),
            ThreadEvent::Error(evt) => vec![SessionUpdate::Failure {
                message: evt.message,
            }],
        }
    }

    fn handle_item(&mut self, item: ThreadItem, stage: ItemStage) -> Vec<SessionUpdate> {
        match item.details {
            ThreadItemDetails::AgentMessage(message) => {
                vec![SessionUpdate::AgentMessage { text: message.text }]
            }
            ThreadItemDetails::Reasoning(reasoning) => vec![SessionUpdate::Reasoning {
                text: reasoning.text,
            }],
            ThreadItemDetails::CommandExecution(command) => {
                self.handle_command(item.id, command, stage)
            }
            ThreadItemDetails::FileChange(change) => self.handle_file_change(change),
            ThreadItemDetails::TodoList(todo) => {
                vec![SessionUpdate::TodoList { items: todo.items }]
            }
            ThreadItemDetails::Error(error) => vec![SessionUpdate::Failure {
                message: error.message,
            }],
            ThreadItemDetails::McpToolCall(_) | ThreadItemDetails::WebSearch(_) => Vec::new(),
        }
    }

    fn handle_command(
        &mut self,
        item_id: String,
        command: CommandExecutionItem,
        stage: ItemStage,
    ) -> Vec<SessionUpdate> {
        let entry = self
            .commands
            .entry(item_id.clone())
            .or_insert_with(|| CommandEntry {
                command: command.command.clone(),
                last_output_len: 0,
            });
        entry.command = command.command.clone();

        let new_len = command.aggregated_output.len();
        let output = if new_len >= entry.last_output_len {
            command
                .aggregated_output
                .get(entry.last_output_len..)
                .unwrap_or_default()
                .to_string()
        } else {
            command.aggregated_output.clone()
        };
        entry.last_output_len = new_len;

        if matches!(stage, ItemStage::Completed)
            || matches!(
                command.status,
                CommandExecutionStatus::Completed | CommandExecutionStatus::Failed
            )
        {
            self.commands.remove(&item_id);
        }

        vec![SessionUpdate::CommandProgress {
            command: command.command,
            output,
            status: command.status,
            exit_code: command.exit_code,
        }]
    }

    fn handle_file_change(&self, change: FileChangeItem) -> Vec<SessionUpdate> {
        vec![SessionUpdate::FileChange {
            changes: change.changes,
            status: change.status,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{
        CommandExecutionItem, CommandExecutionStatus, ItemCompletedEvent, ItemStartedEvent,
        ItemUpdatedEvent, ThreadEvent, ThreadItem, ThreadItemDetails,
    };

    #[test]
    fn thread_started_emits_update() {
        let mut session = SessionState::new();
        let event = ThreadEvent::ThreadStarted(crate::events::ThreadStartedEvent {
            thread_id: "abc".to_string(),
        });

        let updates = session.apply(event);
        assert!(matches!(
            updates.as_slice(),
            [SessionUpdate::ThreadStarted { thread_id }] if thread_id == "abc"
        ));
    }

    #[test]
    fn command_progress_tracks_output_delta() {
        let mut session = SessionState::new();
        let item = command_item("cmd-1", "ls", "listing", CommandExecutionStatus::InProgress);
        let started = ThreadEvent::ItemStarted(ItemStartedEvent { item });
        let updates = session.apply(started);
        match updates.first() {
            Some(SessionUpdate::CommandProgress { output, .. }) => {
                assert_eq!(output, "listing");
            }
            other => panic!("unexpected update: {other:?}"),
        }

        let updated_item = command_item(
            "cmd-1",
            "ls",
            "listing\nmore",
            CommandExecutionStatus::InProgress,
        );
        let updates = session.apply(ThreadEvent::ItemUpdated(ItemUpdatedEvent {
            item: updated_item,
        }));
        match updates.first() {
            Some(SessionUpdate::CommandProgress { output, .. }) => {
                assert_eq!(output, "\nmore");
            }
            other => panic!("unexpected update: {other:?}"),
        }

        let completed_item = command_item(
            "cmd-1",
            "ls",
            "listing\nmore",
            CommandExecutionStatus::Completed,
        );
        let updates = session.apply(ThreadEvent::ItemCompleted(ItemCompletedEvent {
            item: completed_item,
        }));
        match updates.first() {
            Some(SessionUpdate::CommandProgress { status, .. }) => {
                assert!(matches!(status, CommandExecutionStatus::Completed));
            }
            other => panic!("unexpected update: {other:?}"),
        }

        assert!(session.commands.is_empty());
    }

    fn command_item(
        id: &str,
        command: &str,
        output: &str,
        status: CommandExecutionStatus,
    ) -> ThreadItem {
        ThreadItem {
            id: id.to_string(),
            details: ThreadItemDetails::CommandExecution(CommandExecutionItem {
                command: command.to_string(),
                aggregated_output: output.to_string(),
                exit_code: None,
                status,
            }),
        }
    }
}
