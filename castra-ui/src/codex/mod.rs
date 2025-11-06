use std::path::PathBuf;

use async_channel::Receiver;

use castra_harness::{
    CodexSession, HarnessConfig, HarnessError, HarnessEvent, TurnHandle, TurnRequest,
    VizierInputFrame,
};

pub struct HarnessRunner {
    session: CodexSession,
}

pub struct HarnessJob {
    pub receiver: Receiver<HarnessEvent>,
    handle: TurnHandle,
}

impl HarnessRunner {
    pub fn new() -> Self {
        let binary = default_binary_path();
        Self::with_binary(binary)
    }

    pub fn with_binary<P: Into<PathBuf>>(binary: P) -> Self {
        let config = HarnessConfig::new(binary);
        let session = CodexSession::new(config);
        Self { session }
    }

    pub fn run(&self, request: TurnRequest) -> Result<HarnessJob, HarnessError> {
        let handle = self.session.run_turn(request)?;
        let receiver = handle.events();
        Ok(HarnessJob { receiver, handle })
    }

    pub fn send_vizier_input<S: Into<String>>(
        &self,
        _vm: S,
        _frame: VizierInputFrame,
    ) -> Result<(), HarnessError> {
        Err(HarnessError::process_failure(
            None,
            "vizier remote input dispatch is not yet implemented",
        ))
    }
}

impl HarnessJob {
    pub fn into_parts(self) -> (Receiver<HarnessEvent>, TurnHandle) {
        (self.receiver, self.handle)
    }
}

fn default_binary_path() -> PathBuf {
    std::env::var_os("CASTRA_CODEX_BINARY")
        .or_else(|| std::env::var_os("CODEX_BINARY"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("codex"))
}
