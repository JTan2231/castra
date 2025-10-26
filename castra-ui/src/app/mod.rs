pub mod actions;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use crate::{
    components::shell::{render as render_shell, roster_rows},
    controller::command,
    input::prompt::{PromptEvent, PromptInput},
    state::AppState,
};
use async_channel::{Sender, unbounded};
use castra::{
    Error as CastraError,
    core::{
        broker,
        diagnostics::{Diagnostic, Severity as DiagnosticSeverity},
        events::Event,
        operations,
        options::{BrokerOptions, ConfigLoadOptions, UpOptions},
        outcome::{OperationOutput, UpOutcome},
        reporter::Reporter,
        runtime::{BrokerHandle, BrokerLaunchRequest, BrokerLauncher},
    },
};
use gpui::{
    AppContext, AsyncApp, Context, Entity, FocusHandle, Focusable, IntoElement, MouseDownEvent,
    Render, Task, WeakEntity, Window,
};

pub struct ChatApp {
    state: AppState,
    prompt: Entity<PromptInput>,
}

impl ChatApp {
    pub fn new(prompt: Entity<PromptInput>) -> Self {
        Self {
            state: AppState::new(),
            prompt,
        }
    }

    pub fn prompt_focus_handle(&self, cx: &mut Context<Self>) -> FocusHandle {
        self.prompt.focus_handle(cx)
    }

    pub fn focus_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus_handle = self.prompt.focus_handle(cx);
        self.prompt
            .update(cx, |input, _| input.move_cursor_to_end());
        window.focus(&focus_handle);
        cx.notify();
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.state.toggle_sidebar();
        cx.notify();
    }

    pub fn switch_agent_by_slot(&mut self, slot: usize, cx: &mut Context<Self>) {
        if slot == 0 {
            return;
        }
        let index = slot - 1;
        if self.activate_agent_at_index(index) {
            self.announce_active_agent();
            cx.notify();
        }
    }

    fn activate_agent_at_index(&mut self, index: usize) -> bool {
        self.state.switch_agent(index)
    }

    fn announce_active_agent(&mut self) {
        let label = self.state.active_agent_label();
        self.state
            .push_system_message(format!("Active agent set to {}", label));
    }

    pub fn on_prompt_event(&mut self, event: &PromptEvent, cx: &mut Context<Self>) {
        match event {
            PromptEvent::Submitted(text) => {
                self.handle_submission(text, cx);
            }
        }
    }

    fn handle_submission(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.starts_with('/') {
            self.handle_command(text, cx);
        } else {
            self.handle_plain_text(text);
            cx.notify();
        }
    }

    fn handle_plain_text(&mut self, text: &str) {
        self.state.push_user_entry(text);
        self.state.push_agent_echo(text);
    }

    fn handle_command(&mut self, text: &str, cx: &mut Context<Self>) {
        self.state.push_user_command(text);
        match command::handle(text, &mut self.state) {
            command::CommandOutcome::Up => self.start_up(cx),
            command::CommandOutcome::None => cx.notify(),
        }
    }

    fn start_up(&mut self, cx: &mut Context<Self>) {
        if let Err(message) = self.state.begin_up_operation() {
            self.state.push_system_message(message);
            cx.notify();
            return;
        }

        let config_path = match default_quickstart_config_path() {
            Ok(path) => path,
            Err(message) => {
                self.state.complete_up_failure(&message);
                self.state.push_system_message(message);
                cx.notify();
                return;
            }
        };

        self.state.push_system_message(format!(
            "Launching quickstart workspace from {}...",
            config_path.display()
        ));
        cx.notify();

        let async_app = cx.to_async();
        let weak_entity = cx.entity().downgrade();
        let (event_tx, event_rx) = unbounded::<Event>();
        {
            let receiver = event_rx;
            let weak = weak_entity.clone();
            async_app
                .spawn(move |app: &mut AsyncApp| pump_events(receiver, weak, app.clone()))
                .detach();
        }

        let options = build_up_options(&config_path);
        let launcher = Arc::new(UiBrokerLauncher::default());
        let background = cx.background_spawn({
            let sender = event_tx.clone();
            let launcher = Arc::clone(&launcher);
            async move {
                let mut reporter = UiEventReporter::new(sender);
                let launcher_ref: &dyn BrokerLauncher = launcher.as_ref();
                operations::up_with_launcher(options, launcher_ref, Some(&mut reporter))
            }
        });

        drop(event_tx);

        {
            let weak = weak_entity;
            async_app
                .spawn(move |app: &mut AsyncApp| await_completion(background, weak, app.clone()))
                .detach();
        }
    }

    fn handle_up_event(&mut self, event: Event, cx: &mut Context<Self>) {
        if let Some(message) = self.state.handle_up_event(&event) {
            self.state.push_system_message(message);
        }
        cx.notify();
    }

    fn finish_up(
        &mut self,
        outcome: Result<OperationOutput<UpOutcome>, CastraError>,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            Ok(output) => {
                self.state.complete_up_success();
                self.log_diagnostics(&output.diagnostics);
                let summary = summarize_up(&output.value);
                self.state.push_system_message(summary);
            }
            Err(error) => {
                let message = format!("UP failed: {error}");
                self.state.complete_up_failure(&message);
                self.state.push_system_message(message);
            }
        }
        cx.notify();
    }

    fn log_diagnostics(&mut self, diagnostics: &[Diagnostic]) {
        for diagnostic in diagnostics {
            let rendered = format_diagnostic(diagnostic);
            self.state.push_system_message(rendered);
        }
    }
}

impl Render for ChatApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roster_rows = if self.state.sidebar_visible() {
            Some(roster_rows(self.state.roster(), |index| {
                cx.listener(
                    move |chat: &mut ChatApp,
                          _: &MouseDownEvent,
                          _window: &mut Window,
                          cx: &mut Context<ChatApp>| {
                        if chat.activate_agent_at_index(index) {
                            chat.announce_active_agent();
                        }
                        cx.notify();
                    },
                )
            }))
        } else {
            None
        };

        render_shell(&self.state, &self.prompt, roster_rows)
    }
}

async fn pump_events(
    receiver: async_channel::Receiver<Event>,
    weak: WeakEntity<ChatApp>,
    mut app: AsyncApp,
) {
    while let Ok(event) = receiver.recv().await {
        if weak
            .update(&mut app, |chat, cx| chat.handle_up_event(event.clone(), cx))
            .is_err()
        {
            break;
        }
    }
}

async fn await_completion(
    background: Task<Result<OperationOutput<UpOutcome>, CastraError>>,
    weak: WeakEntity<ChatApp>,
    mut app: AsyncApp,
) {
    let outcome = background.await;
    let _ = weak.update(&mut app, |chat, cx| chat.finish_up(outcome, cx));
}

fn default_quickstart_config_path() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidate = manifest_dir.join("../castra-core/examples/bootstrap-quickstart/castra.toml");
    candidate.canonicalize().map_err(|err| {
        format!(
            "Unable to resolve bootstrap config at {}: {err}",
            candidate.display()
        )
    })
}

fn build_up_options(config_path: &Path) -> UpOptions {
    let mut options = UpOptions::default();
    options.config = ConfigLoadOptions::explicit(config_path.to_path_buf());
    options
}

fn summarize_up(outcome: &UpOutcome) -> String {
    use castra::core::outcome::BootstrapRunStatus;

    let mut parts = Vec::new();
    parts.push(format!(
        "UP complete: {} VM(s) launched",
        outcome.launched_vms.len()
    ));

    if let Some(broker) = &outcome.broker {
        parts.push(format!("broker listening on :{}", broker.config.port));
    }

    if !outcome.bootstraps.is_empty() {
        let mut success = 0usize;
        let mut noop = 0usize;
        let mut skipped = 0usize;
        for run in &outcome.bootstraps {
            match run.status {
                BootstrapRunStatus::Success => success += 1,
                BootstrapRunStatus::NoOp => noop += 1,
                BootstrapRunStatus::Skipped => skipped += 1,
            }
        }
        parts.push(format!(
            "bootstrap summary: {} success • {} noop • {} skipped",
            success, noop, skipped
        ));
    }

    parts.join(" • ")
}

fn format_diagnostic(diagnostic: &Diagnostic) -> String {
    let tag = match diagnostic.severity {
        DiagnosticSeverity::Info => "INFO",
        DiagnosticSeverity::Warning => "WARN",
        DiagnosticSeverity::Error => "ERROR",
    };

    let mut text = format!("[{tag}] {}", diagnostic.message);
    if let Some(path) = &diagnostic.path {
        text.push_str(" (");
        text.push_str(&path.display().to_string());
        text.push(')');
    }
    if let Some(help) = &diagnostic.help {
        text.push_str(" • ");
        text.push_str(help);
    }
    text
}

struct UiEventReporter {
    sender: Sender<Event>,
}

impl UiEventReporter {
    fn new(sender: Sender<Event>) -> Self {
        Self { sender }
    }
}

impl Reporter for UiEventReporter {
    fn report(&mut self, event: Event) {
        let _ = self.sender.send_blocking(event);
    }
}

#[derive(Default, Clone)]
struct UiBrokerLauncher;

impl BrokerLauncher for UiBrokerLauncher {
    fn launch(&self, request: &BrokerLaunchRequest<'_>) -> castra::Result<Box<dyn BrokerHandle>> {
        let options = BrokerOptions {
            port: request.port,
            pidfile: request.pidfile.clone(),
            logfile: request.logfile.clone(),
            handshake_dir: request.handshake_dir.clone(),
        };

        UiBrokerHandle::spawn(options)
    }
}

struct UiBrokerHandle {
    join: Option<thread::JoinHandle<()>>,
}

impl UiBrokerHandle {
    fn spawn(options: BrokerOptions) -> castra::Result<Box<dyn BrokerHandle>> {
        let handle = thread::Builder::new()
            .name("castra-broker".into())
            .spawn(move || {
                if let Err(err) = broker::run(&options) {
                    eprintln!("Broker exited: {err}");
                }
            })
            .map_err(|err| castra::Error::PreflightFailed {
                message: format!("Failed to spawn broker thread: {err}"),
            })?;

        let boxed: Box<UiBrokerHandle> = Box::new(UiBrokerHandle { join: Some(handle) });
        Ok(boxed as Box<dyn BrokerHandle>)
    }
}

impl BrokerHandle for UiBrokerHandle {
    fn pid(&self) -> Option<u32> {
        Some(std::process::id())
    }

    fn abort(&mut self) -> std::io::Result<()> {
        if let Some(handle) = self.join.take() {
            drop(handle);
        }
        Ok(())
    }
}
