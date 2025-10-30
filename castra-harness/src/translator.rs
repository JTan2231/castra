use crate::events::{
    CommandExecutionStatus, FileUpdateChange, PatchApplyStatus, PatchChangeKind,
    TodoItem as CodexTodoItem,
};

use crate::session::SessionUpdate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchStatus {
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileDiffKind {
    Add,
    Delete,
    Update,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    pub kind: FileDiffKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoEntry {
    pub text: String,
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HarnessEvent {
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
        status: CommandStatus,
        exit_code: Option<i32>,
    },
    FileChange {
        changes: Vec<FileDiff>,
        status: PatchStatus,
    },
    TodoList {
        items: Vec<TodoEntry>,
    },
    Usage {
        prompt_tokens: i64,
        cached_tokens: i64,
        completion_tokens: i64,
    },
    Failure {
        message: String,
    },
}

pub fn translate(update: SessionUpdate) -> Vec<HarnessEvent> {
    match update {
        SessionUpdate::ThreadStarted { thread_id } => {
            vec![HarnessEvent::ThreadStarted { thread_id }]
        }
        SessionUpdate::AgentMessage { text } => {
            vec![HarnessEvent::AgentMessage { text }]
        }
        SessionUpdate::Reasoning { text } => {
            vec![HarnessEvent::Reasoning { text }]
        }
        SessionUpdate::CommandProgress {
            command,
            output,
            status,
            exit_code,
        } => {
            vec![HarnessEvent::CommandProgress {
                command,
                output,
                status: map_command_status(status),
                exit_code,
            }]
        }
        SessionUpdate::FileChange { changes, status } => {
            let diffs = changes.into_iter().map(map_file_diff).collect();
            vec![HarnessEvent::FileChange {
                changes: diffs,
                status: map_patch_status(status),
            }]
        }
        SessionUpdate::TodoList { items } => {
            let mapped = items.into_iter().map(map_todo).collect();
            vec![HarnessEvent::TodoList { items: mapped }]
        }
        SessionUpdate::Usage { usage } => vec![HarnessEvent::Usage {
            prompt_tokens: usage.input_tokens,
            cached_tokens: usage.cached_input_tokens,
            completion_tokens: usage.output_tokens,
        }],
        SessionUpdate::Failure { message } => {
            vec![HarnessEvent::Failure { message }]
        }
    }
}

fn map_command_status(status: CommandExecutionStatus) -> CommandStatus {
    match status {
        CommandExecutionStatus::InProgress => CommandStatus::InProgress,
        CommandExecutionStatus::Completed => CommandStatus::Completed,
        CommandExecutionStatus::Failed => CommandStatus::Failed,
    }
}

fn map_patch_status(status: PatchApplyStatus) -> PatchStatus {
    match status {
        PatchApplyStatus::Completed => PatchStatus::Completed,
        PatchApplyStatus::Failed => PatchStatus::Failed,
    }
}

fn map_file_diff(change: FileUpdateChange) -> FileDiff {
    FileDiff {
        path: change.path,
        kind: map_file_diff_kind(change.kind),
    }
}

fn map_file_diff_kind(kind: PatchChangeKind) -> FileDiffKind {
    match kind {
        PatchChangeKind::Add => FileDiffKind::Add,
        PatchChangeKind::Delete => FileDiffKind::Delete,
        PatchChangeKind::Update => FileDiffKind::Update,
    }
}

fn map_todo(item: CodexTodoItem) -> TodoEntry {
    TodoEntry {
        text: item.text,
        completed: item.completed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{
        CommandExecutionStatus, FileUpdateChange, PatchApplyStatus, PatchChangeKind, Usage,
    };
    use crate::session::SessionUpdate;

    #[test]
    fn translates_command_status() {
        let events = translate(SessionUpdate::CommandProgress {
            command: "ls".to_string(),
            output: String::new(),
            status: CommandExecutionStatus::Completed,
            exit_code: Some(0),
        });

        match events.as_slice() {
            [
                HarnessEvent::CommandProgress {
                    status, exit_code, ..
                },
            ] => {
                assert!(matches!(status, CommandStatus::Completed));
                assert_eq!(*exit_code, Some(0));
            }
            other => panic!("unexpected translation: {other:?}"),
        }
    }

    #[test]
    fn translates_file_change() {
        let updates = translate(SessionUpdate::FileChange {
            changes: vec![FileUpdateChange {
                path: "README.md".to_string(),
                kind: PatchChangeKind::Update,
            }],
            status: PatchApplyStatus::Completed,
        });

        match updates.as_slice() {
            [HarnessEvent::FileChange { changes, status }] => {
                assert!(matches!(status, PatchStatus::Completed));
                assert_eq!(changes[0].path, "README.md");
                assert!(matches!(changes[0].kind, FileDiffKind::Update));
            }
            other => panic!("unexpected translation: {other:?}"),
        }
    }

    #[test]
    fn translates_usage() {
        let updates = translate(SessionUpdate::Usage {
            usage: Usage {
                input_tokens: 10,
                cached_input_tokens: 5,
                output_tokens: 20,
            },
        });

        match updates.as_slice() {
            [
                HarnessEvent::Usage {
                    prompt_tokens,
                    cached_tokens,
                    completion_tokens,
                },
            ] => {
                assert_eq!(*prompt_tokens, 10);
                assert_eq!(*cached_tokens, 5);
                assert_eq!(*completion_tokens, 20);
            }
            other => panic!("unexpected translation: {other:?}"),
        }
    }
}
