pub mod actions;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::{
    components::shell::{render as render_shell, roster_rows},
    controller::command,
    input::prompt::{PromptEvent, PromptInput},
    ssh::{HANDSHAKE_BANNER, SshEvent, SshManager, SshStream},
    state::AppState,
};
use async_channel::{Receiver, Sender, unbounded};
use castra::{
    Error as CastraError,
    core::{
        broker,
        diagnostics::{Diagnostic, Severity as DiagnosticSeverity},
        events::Event,
        operations,
        options::{BrokerOptions, ConfigLoadOptions, DownOptions, UpOptions, VmLaunchMode},
        outcome::{DownOutcome, OperationOutput, UpOutcome},
        reporter::Reporter,
        runtime::{BrokerHandle, BrokerLaunchRequest, BrokerLauncher},
    },
};
use gpui::{
    AppContext, AsyncApp, Context, Entity, FocusHandle, Focusable, IntoElement, MouseDownEvent,
    Render, Task, WeakEntity, Window,
};

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
    ssh_manager: SshManager,
    ssh_events: Option<Receiver<SshEvent>>,
    shutdown: Arc<ShutdownState>,
}

impl ChatApp {
    pub fn new(prompt: Entity<PromptInput>, shutdown: Arc<ShutdownState>) -> Self {
        let (ssh_tx, ssh_rx) = unbounded();
        Self {
            state: AppState::new(),
            prompt,
            ssh_manager: SshManager::new(ssh_tx),
            ssh_events: Some(ssh_rx),
            shutdown,
        }
    }

    pub fn initialize(&mut self, cx: &mut Context<Self>) {
        if let Some(receiver) = self.ssh_events.take() {
            let async_app = cx.to_async();
            let weak = cx.entity().downgrade();
            async_app
                .spawn(move |app: &mut AsyncApp| pump_ssh(receiver, weak, app.clone()))
                .detach();
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

        match self.ssh_manager.ensure_connection(&vm) {
            Ok(_) => {
                if let Err(err) = self.ssh_manager.send_line(&vm, &payload) {
                    self.state
                        .push_system_message(format!("{label}: failed to send command - {err}"));
                }
            }
            Err(err) => {
                self.state.push_system_message(format!("{label}: {err}"));
            }
        }

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
            command::CommandOutcome::Codex { vm, payload } => {
                self.dispatch_codex(vm, payload, cx);
            }
            command::CommandOutcome::None => cx.notify(),
        }
    }

    fn start_up(&mut self, cx: &mut Context<Self>) {
        if let Err(message) = self.state.begin_up_operation() {
            self.state.push_system_message(message);
            cx.notify();
            return;
        }

        self.ssh_manager.reset();

        let config_path = match default_quickstart_config_path() {
            Ok(path) => path,
            Err(message) => {
                self.state.complete_up_failure(&message);
                self.state.push_system_message(message);
                cx.notify();
                return;
            }
        };

        self.state.set_config_path(config_path.clone());
        self.shutdown.record_config(config_path.clone());

        if let Err(message) = self.preflight_cleanup(&config_path) {
            self.state.complete_up_failure(&message);
            self.state.push_system_message(message);
            cx.notify();
            return;
        }

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
                let broker_changed = output.value.broker.changed;
                if vm_changes > 0 || broker_changed {
                    let mut components = Vec::new();
                    if vm_changes > 0 {
                        components.push(format!("{vm_changes} VM(s)"));
                    }
                    if broker_changed {
                        components.push("broker".to_string());
                    }
                    self.state.push_system_message(format!(
                        "Recovered stale {} before launching.",
                        components.join(" and ")
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
        match &event {
            Event::BootstrapPlanned {
                vm, ssh: Some(ssh), ..
            } => {
                self.ssh_manager.register_plan(vm, ssh);
            }
            Event::BootstrapCompleted { vm, .. } => {
                let _ = self.ssh_manager.ensure_connection(vm);
            }
            _ => {}
        }

        if let Some(message) = self.state.handle_up_event(&event) {
            self.state.push_system_message(message);
        }
        cx.notify();
    }

    fn handle_ssh_event(&mut self, event: SshEvent, cx: &mut Context<Self>) {
        match event {
            SshEvent::Connecting { vm, command } => {
                let label = vm.to_uppercase();
                self.state
                    .push_system_message(format!("{label}: establishing SSH bridge ({command})"));
            }
            SshEvent::Connected { vm } => {
                let label = vm.to_uppercase();
                self.state
                    .push_system_message(format!("{label}: SSH bridge established."));
            }
            SshEvent::Output { vm, stream, line } => {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed == HANDSHAKE_BANNER {
                    return;
                }
                let speaker = match stream {
                    SshStream::Stdout => format!("SSH→{}", vm.to_uppercase()),
                    SshStream::Stderr => format!("SSH!→{}", vm.to_uppercase()),
                };
                self.state.push_message(speaker, line);
            }
            SshEvent::ConnectionFailed { vm, error } => {
                let label = vm.to_uppercase();
                self.state
                    .push_system_message(format!("{label}: SSH bridge error — {error}"));
            }
            SshEvent::Disconnected { vm, exit_status } => {
                let label = vm.to_uppercase();
                let status_text = match exit_status {
                    Some(code) => format!("exit status {code}"),
                    None => "terminated".to_string(),
                };
                self.state
                    .push_system_message(format!("{label}: SSH bridge closed ({status_text})."));
            }
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
                let broker_changed = output.value.broker.changed;
                if vm_changes > 0 || broker_changed {
                    let mut parts = Vec::new();
                    if vm_changes > 0 {
                        parts.push(format!("{vm_changes} VM(s)"));
                    }
                    if broker_changed {
                        parts.push("broker".to_string());
                    }
                    self.state.push_system_message(format!(
                        "Shutdown complete: {} terminated.",
                        parts.join(" and ")
                    ));
                } else {
                    self.state
                        .push_system_message("Shutdown complete: nothing was running.");
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

        render_shell(&self.state, &self.prompt, roster_rows, &toasts)
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

async fn pump_ssh(
    receiver: async_channel::Receiver<SshEvent>,
    weak: WeakEntity<ChatApp>,
    mut app: AsyncApp,
) {
    while let Ok(event) = receiver.recv().await {
        if weak
            .update(&mut app, |chat, cx| {
                chat.handle_ssh_event(event.clone(), cx)
            })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn build_up_options_configures_attached_launch_mode() {
        let path = PathBuf::from("castra.toml");
        let options = build_up_options(&path);
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
