use std::process::ExitStatus;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HarnessError {
    #[error("failed to spawn codex process: {0}")]
    Spawn(std::io::Error),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to decode codex event: {0}")]
    Json(#[from] serde_json::Error),

    #[error("codex process exited with {status:?}: {message}")]
    Process {
        status: Option<ExitStatus>,
        message: String,
    },

    #[error("event stream closed before completion")]
    ChannelClosed,
}

impl HarnessError {
    pub fn process_failure(status: Option<ExitStatus>, message: impl Into<String>) -> Self {
        Self::Process {
            status,
            message: message.into(),
        }
    }
}
