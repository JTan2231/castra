pub mod actions;

use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::{
    codex::HarnessRunner,
    components::{
        message_log::MessageToggleHandler,
        shell::{render as render_shell, roster_rows},
        vm_fleet::{catalog_cards, vm_columns},
    },
    controller::command,
    input::prompt::{PromptEvent, PromptInput},
    state::{AppState, ConfigSelection, UpLifecycle},
    transcript::TranscriptWriter,
};
use async_channel::{Receiver, Sender, unbounded};
use castra::{
    Error as CastraError,
    core::{
        diagnostics::{Diagnostic, Severity as DiagnosticSeverity},
        events::Event,
        operations,
        options::{ConfigLoadOptions, DownOptions, UpOptions, VmLaunchMode},
        outcome::{DownOutcome, OperationOutput, UpOutcome},
        reporter::Reporter,
    },
    load_project_config,
};
use castra_harness::TurnHandle;
use castra_harness::{HarnessEvent, TurnRequest};
use gpui::{
    AppContext, AsyncApp, Context, Entity, FocusHandle, Focusable, IntoElement, MouseDownEvent,
    Render, Task, WeakEntity, Window,
};

const BROKER_DEPRECATION_MESSAGE: &str =
    "Deprecated: bus/broker have been removed. Connect directly to guest agent sessions via vm_commands.sh wrappers.";

#[derive(Default)]
pub struct ShutdownState {
    inner: Mutex<ShutdownStateInner>,
}

#[derive(Default)]
struct ShutdownStateInner {
    config_path: Option<PathBuf>,
    cleanup_in_progress: bool,
    cleanup_completed: bool,
}

impl ShutdownState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ShutdownStateInner::default()),
        }
    }

    pub fn record_config(&self, path: PathBuf) {
        let mut inner = self.inner.lock().expect("shutdown state poisoned");
        inner.config_path = Some(path);
        inner.cleanup_completed = false;
    }

    pub fn prepare_cleanup(&self) -> Option<DownOptions> {
        let mut inner = self.inner.lock().expect("shutdown state poisoned");
        if inner.cleanup_in_progress || inner.cleanup_completed {
            return None;
        }
        let path = inner.config_path.clone()?;
        inner.cleanup_in_progress = true;
        Some(DownOptions {
            config: ConfigLoadOptions::explicit(path),
            ..DownOptions::default()
        })
    }

    pub fn mark_cleanup_complete(&self) {
        let mut inner = self.inner.lock().expect("shutdown state poisoned");
        inner.cleanup_in_progress = false;
        inner.cleanup_completed = true;
    }

    pub fn cleanup_in_progress(&self) -> bool {
        let inner = self.inner.lock().expect("shutdown state poisoned");
        inner.cleanup_in_progress
    }

    pub fn run_cleanup_blocking(&self) -> bool {
        if let Some(options) = self.prepare_cleanup() {
            let result = operations::down(options, None);
            if let Err(err) = result {
                eprintln!("castra-ui: shutdown via signal failed: {err}");
            }
            self.mark_cleanup_complete();
            true
        } else {
            false
        }
    }
}

pub struct ChatApp {
    state: AppState,
    prompt: Entity<PromptInput>,
    harness_runner: HarnessRunner,
    shutdown: Arc<ShutdownState>,
    codex_handle: Option<Arc<TurnHandle>>,
}

impl ChatApp {
    pub fn new(prompt: Entity<PromptInput>, shutdown: Arc<ShutdownState>) -> Self {
        let workspace_root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let (transcript_root, transcript_warning) = match resolve_transcript_workspace_root() {
            Ok(root) => (root, None),
            Err(err) => {
                eprintln!(
                    "castra-ui: failed to resolve transcript workspace root: {err}; falling back to {}",
                    workspace_root.display()
                );
                (workspace_root.clone(), Some(err))
            }
        };
        let (transcript_writer, transcript_status) = match TranscriptWriter::new(&transcript_root) {
            Ok(writer) => {
                let path = writer.path().to_path_buf();
                let session = writer.session_id().to_string();
                (Some(Arc::new(writer)), Ok((session, path)))
            }
            Err(err) => {
                eprintln!("castra-ui: failed to initialize transcript writer: {err}");
                (None, Err(err.to_string()))
            }
        };
        let (quickstart_path, quickstart_error) = match default_quickstart_config_path() {
            Ok(path) => (Some(path), None),
            Err(message) => (None, Some(message)),
        };
        let mut state = AppState::with_transcript(transcript_writer, quickstart_path);
        if let Some(reason) = transcript_warning {
            state.push_system_message(format!(
                "Transcripts stored in {} (failed to resolve workspace state root: {reason}).",
                workspace_root.display()
            ));
        }
        match transcript_status {
            Ok((session_id, path)) => {
                state.push_system_message(format!(
                    "Recording chat transcript (session {session_id}) to {}",
                    path.display()
                ));
            }
            Err(reason) => state.push_system_message(format!("Transcripts unavailable: {reason}")),
        }

        if let Some(message) = quickstart_error {
            state.push_system_message(format!("Quickstart bundle unavailable: {message}"));
        }

        if let Err(err) = state.refresh_config_catalog() {
            state.push_system_message(format!("Failed to load config catalog: {err}"));
        }

        Self {
            state,
            prompt,
            harness_runner: HarnessRunner::new(),
            shutdown,
            codex_handle: None,
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

    fn ensure_prompt_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        let focus_handle = self.prompt_focus_handle(cx);
        window.focus(&focus_handle);
    }

    pub fn focus_next_vm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(vm) = self.state.focus_next_vm() {
            self.handle_focus_change(vm, window, cx);
        }
    }

    pub fn focus_prev_vm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(vm) = self.state.focus_prev_vm() {
            self.handle_focus_change(vm, window, cx);
        }
    }

    fn handle_focus_change(&mut self, vm: String, window: &mut Window, cx: &mut Context<Self>) {
        let label = vm.to_uppercase();
        self.state.push_toast(format!("Focused VM: {}", label));
        self.ensure_prompt_focus(window, cx);
        cx.notify();
    }

    fn dispatch_codex(&mut self, vm: String, payload: String, cx: &mut Context<Self>) {
        let label = vm.to_uppercase();
        self.state
            .push_message(format!("USER→{}", label), payload.clone());

        let mut request = TurnRequest::new(payload.clone());
        if let Some(thread_id) = self.state.codex_thread_id() {
            request = request.with_resume_thread(thread_id);
        }

        match self.harness_runner.run(request) {
            Ok(job) => {
                let (receiver, handle) = job.into_parts();
                let handle = Arc::new(handle);
                self.codex_handle = Some(handle.clone());
                self.state.set_codex_turn_active(true);
                self.state
                    .push_system_message(format!("Codex engaged for {label}"));
                let async_app = cx.to_async();
                let weak = cx.entity().downgrade();
                async_app
                    .spawn(move |app: &mut AsyncApp| {
                        pump_codex(receiver, handle.clone(), weak, app.clone())
                    })
                    .detach();
            }
            Err(err) => {
                self.state
                    .push_system_message(format!("Codex launch failed: {err}"));
            }
        }

        cx.notify();
    }

    fn request_codex_stop(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.codex_handle.as_ref() {
            match handle.cancel() {
                Ok(()) => {
                    if self.state.codex_turn_active() {
                        self.state
                            .push_system_message("Codex stop requested. Waiting for exit…");
                    }
                    self.state.set_codex_turn_active(false);
                }
                Err(err) => {
                    self.state
                        .push_system_message(format!("Codex stop failed: {err}"));
                }
            }
        } else {
            self.state
                .push_system_message("Codex is not running. Nothing to stop.");
        }
        cx.notify();
    }

    fn finish_codex_turn(&mut self, cx: &mut Context<Self>) {
        let had_handle = self.codex_handle.take().is_some();
        let was_active = self.state.codex_turn_active();
        self.state.mark_codex_turn_finished();
        if had_handle || was_active {
            cx.notify();
        }
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

    fn toggle_message(&mut self, index: usize) -> bool {
        self.state.chat_mut().toggle_message_at(index)
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
            self.handle_plain_text(text, cx);
        }
    }

    fn handle_plain_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        self.state.push_user_entry(text);
        cx.notify();

        if let Some(vm) = self.state.focused_vm_name() {
            self.dispatch_codex(vm, trimmed.to_string(), cx);
        } else {
            self.state
                .push_system_message("Select a VM before dispatching work.".to_string());
            cx.notify();
        }
    }

    fn handle_command(&mut self, text: &str, cx: &mut Context<Self>) {
        self.state.push_user_command(text);
        match command::handle(text, &mut self.state) {
            command::CommandOutcome::Up => self.start_up(cx),
            command::CommandOutcome::Codex { vm, payload } => {
                self.dispatch_codex(vm, payload, cx);
            }
            command::CommandOutcome::None => cx.notify(),
        }
    }

    fn handle_config_click(&mut self, index: usize, cx: &mut Context<Self>) {
        match self.state.select_catalog_entry(index) {
            Ok(_) => self.start_up(cx),
            Err(message) => {
                self.state.push_system_message(message);
                cx.notify();
            }
        }
    }

    fn start_up(&mut self, cx: &mut Context<Self>) {
        let selection = match self.state.active_config_selection() {
            Ok(selection) => selection,
            Err(message) => {
                self.state.push_system_message(message);
                cx.notify();
                return;
            }
        };

        if let Err(message) = self.state.begin_up_operation() {
            self.state.push_system_message(message);
            cx.notify();
            return;
        }

        let ConfigSelection {
            path: config_path,
            display_name,
            summary,
        } = selection;

        self.state.set_config_path(config_path.clone());
        self.state.catalog_clear_launch_failure(&config_path);
        self.shutdown.record_config(config_path.clone());

        if let Err(message) = self.preflight_cleanup(&config_path) {
            self.state.complete_up_failure(&message);
            self.state.push_system_message(message);
            cx.notify();
            return;
        }

        let mut announcement =
            format!("Launching {display_name} from {}...", config_path.display());
        if let Some(detail) = summary {
            announcement.push(' ');
            announcement.push('(');
            announcement.push_str(&detail);
            announcement.push(')');
        }
        self.state.push_system_message(announcement);
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

        let _options = build_up_options(&config_path);
        let background = cx.background_spawn({
            let sender = event_tx.clone();
            async move {
                let mut reporter = UiEventReporter::new(sender);
                reporter.report(Event::Message {
                    severity: DiagnosticSeverity::Warning,
                    text: BROKER_DEPRECATION_MESSAGE.to_string(),
                });
                Err(CastraError::PreflightFailed {
                    message: BROKER_DEPRECATION_MESSAGE.to_string(),
                })
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

    fn preflight_cleanup(&mut self, config_path: &Path) -> Result<(), String> {
        let mut options = DownOptions::default();
        options.config = ConfigLoadOptions::explicit(config_path.to_path_buf());

        match operations::down(options, None) {
            Ok(output) => {
                self.log_diagnostics(&output.diagnostics);
                let vm_changes = output
                    .value
                    .vm_results
                    .iter()
                    .filter(|vm| vm.changed)
                    .count();
                if vm_changes > 0 {
                    self.state.push_system_message(format!(
                        "Recovered stale {vm_changes} VM(s) before launching."
                    ));
                }
                Ok(())
            }
            Err(CastraError::NoActiveWorkspaces) => Ok(()),
            Err(CastraError::WorkspaceConfigUnavailable { .. }) => Ok(()),
            Err(err) => Err(format!("Pre-flight cleanup failed: {err}")),
        }
    }

    fn handle_up_event(&mut self, event: Event, cx: &mut Context<Self>) {
        if let Some(message) = self.state.handle_up_event(&event) {
            self.state.push_system_message(message);
        }
        cx.notify();
    }

    fn handle_codex_event(&mut self, event: HarnessEvent, cx: &mut Context<Self>) {
        self.state.apply_harness_event(&event);
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
                self.state.record_runtime_paths(
                    output.value.state_root.clone(),
                    output.value.log_root.clone(),
                );
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

    pub(crate) fn initiate_shutdown(&mut self, cx: &mut Context<Self>) {
        if self.state.shutdown_in_progress() {
            self.state
                .push_system_message("Shutdown already in progress.");
            cx.notify();
            return;
        }

        match self.shutdown.prepare_cleanup() {
            Some(options) => {
                self.state.mark_shutdown_started();
                self.state.push_system_message("Shutting down workspace...");
                cx.notify();

                let async_app = cx.to_async();
                let weak = cx.entity().downgrade();
                let shutdown = Arc::clone(&self.shutdown);
                let background =
                    cx.background_spawn(async move { operations::down(options, None) });
                async_app
                    .spawn(move |app: &mut AsyncApp| {
                        await_shutdown(background, weak, shutdown, app.clone())
                    })
                    .detach();
            }
            None => {
                cx.quit();
            }
        }
    }

    fn finish_shutdown(
        &mut self,
        outcome: Result<OperationOutput<DownOutcome>, CastraError>,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            Ok(output) => {
                self.log_diagnostics(&output.diagnostics);
                let vm_changes = output
                    .value
                    .vm_results
                    .iter()
                    .filter(|vm| vm.changed)
                    .count();
                if vm_changes > 0 {
                    self.state.push_system_message(format!(
                        "Shutdown complete: {vm_changes} VM(s) terminated."
                    ));
                } else {
                    self.state
                        .push_system_message("Shutdown complete: nothing was running.");
                }
                if let Err(err) = self.state.refresh_config_catalog() {
                    self.state
                        .push_system_message(format!("Failed to refresh config catalog: {err}"));
                }
            }
            Err(error) => {
                let message = format!("Shutdown encountered an error: {error}");
                self.state.push_system_message(message);
            }
        }

        self.state.mark_shutdown_complete();
        self.shutdown.mark_cleanup_complete();
        cx.notify();
        cx.quit();
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
        const SCROLLABLE_THRESHOLD_PX: f32 = 0.5;
        const SCROLL_BOTTOM_TOLERANCE_PX: f32 = 4.0;

        self.state
            .chat_mut()
            .refresh_stick_to_bottom(SCROLLABLE_THRESHOLD_PX, SCROLL_BOTTOM_TOLERANCE_PX);

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

        let toasts = self.state.collect_active_toasts();

        let toggle_handlers: Arc<Vec<Option<MessageToggleHandler>>> = {
            let handlers = self
                .state
                .chat()
                .messages()
                .iter()
                .enumerate()
                .map(|(index, message)| {
                    if message.is_collapsible() {
                        let handler_index = index;
                        let handler = cx.listener(
                            move |chat: &mut ChatApp,
                                  _: &MouseDownEvent,
                                  _window: &mut Window,
                                  cx: &mut Context<ChatApp>| {
                                if chat.toggle_message(handler_index) {
                                    cx.notify();
                                }
                            },
                        );
                        Some(Arc::new(handler) as MessageToggleHandler)
                    } else {
                        None
                    }
                })
                .collect();
            Arc::new(handlers)
        };

        let stop_handler = if self.state.codex_turn_active() {
            Some(cx.listener(
                |chat: &mut ChatApp,
                 _: &MouseDownEvent,
                 _window: &mut Window,
                 cx: &mut Context<ChatApp>| {
                    chat.request_codex_stop(cx);
                },
            ))
        } else {
            None
        };

        let (fleet_title, fleet_columns) = match self.state.up_lifecycle() {
            UpLifecycle::Idle | UpLifecycle::Failed { .. } => {
                let view = self.state.catalog_view();
                let cards = catalog_cards(&view, |index| {
                    let handler = cx.listener(
                        move |chat: &mut ChatApp,
                              _: &MouseDownEvent,
                              _window: &mut Window,
                              cx: &mut Context<ChatApp>| {
                            chat.handle_config_click(index, cx);
                        },
                    );
                    Some(handler)
                });
                ("CONFIG CATALOG", (cards, Vec::new()))
            }
            _ => ("VM FLEET", vm_columns(self.state.vm_fleet())),
        };

        render_shell(
            &self.state,
            &self.prompt,
            roster_rows,
            fleet_title,
            fleet_columns,
            &toasts,
            stop_handler,
            toggle_handlers,
        )
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

async fn pump_codex(
    receiver: Receiver<HarnessEvent>,
    handle: Arc<TurnHandle>,
    weak: WeakEntity<ChatApp>,
    mut app: AsyncApp,
) {
    let _handle = handle;

    while let Ok(event) = receiver.recv().await {
        if weak
            .update(&mut app, |chat, cx| {
                chat.handle_codex_event(event.clone(), cx)
            })
            .is_err()
        {
            break;
        }
    }

    let _ = weak.update(&mut app, |chat, cx| {
        chat.finish_codex_turn(cx);
    });
}

async fn await_completion(
    background: Task<Result<OperationOutput<UpOutcome>, CastraError>>,
    weak: WeakEntity<ChatApp>,
    mut app: AsyncApp,
) {
    let outcome = background.await;
    let _ = weak.update(&mut app, |chat, cx| chat.finish_up(outcome, cx));
}

async fn await_shutdown(
    background: Task<Result<OperationOutput<DownOutcome>, CastraError>>,
    weak: WeakEntity<ChatApp>,
    shutdown: Arc<ShutdownState>,
    mut app: AsyncApp,
) {
    let outcome = background.await;
    if weak
        .update(&mut app, |chat, cx| chat.finish_shutdown(outcome, cx))
        .is_err()
    {
        shutdown.mark_cleanup_complete();
        let _ = app.update(|cx| cx.quit());
    }
}

fn resolve_transcript_workspace_root() -> Result<PathBuf, String> {
    let config_path = default_quickstart_config_path()?;
    let config = load_project_config(&config_path).map_err(|err| {
        format!(
            "unable to load project config at {}: {err}",
            config_path.display()
        )
    })?;
    Ok(config.state_root)
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
    options.launch_mode = VmLaunchMode::Attached;
    options
}

fn summarize_up(outcome: &UpOutcome) -> String {
    use castra::core::outcome::BootstrapRunStatus;

    let mut parts = Vec::new();
    parts.push(format!(
        "UP complete: {} VM(s) launched",
        outcome.launched_vms.len()
    ));

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

#[cfg(test)]
mod tests {
    use super::*;
    use castra::core::options::ConfigSource;
    use std::path::PathBuf;

    #[test]
    fn build_up_options_configures_attached_launch_mode() {
        let path = PathBuf::from("castra.toml");
        let options = build_up_options(&path);
        match &options.config.source {
            ConfigSource::Explicit(explicit) => assert_eq!(explicit, &path),
            other => panic!("expected explicit config path, got {other:?}"),
        }
        assert_eq!(options.launch_mode, VmLaunchMode::Attached);
    }
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
