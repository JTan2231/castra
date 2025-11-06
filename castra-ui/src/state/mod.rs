use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use castra::core::{
    diagnostics::Severity,
    events::{BootstrapStatus, BootstrapStepKind, BootstrapStepStatus, BootstrapTrigger, Event},
};
use castra_harness::{CommandStatus, FileDiff, FileDiffKind, HarnessEvent, PatchStatus, TodoEntry};
use chrono::{DateTime, Local};
use gpui::{ListAlignment, ListOffset, ListState, Pixels, SharedString, px};

use crate::{
    config_catalog::{self, ConfigEntry},
    transcript::TranscriptWriter,
};
const COLLAPSED_PREVIEW_MAX_CHARS: usize = 80;
const DEFAULT_LOG_SOFT_LIMIT: usize = 500;

fn current_timestamp() -> SharedString {
    Local::now().format("%H:%M:%S").to_string().into()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TokenUsageTotals {
    prompt: i64,
    cached: i64,
    completion: i64,
}

impl TokenUsageTotals {
    pub fn add(&mut self, prompt: i64, cached: i64, completion: i64) {
        self.prompt += prompt;
        self.cached += cached;
        self.completion += completion;
    }

    pub fn total(&self) -> i64 {
        self.prompt + self.cached + self.completion
    }

    pub fn is_empty(&self) -> bool {
        self.prompt == 0 && self.cached == 0 && self.completion == 0
    }

    pub fn summary(&self, label: &str) -> String {
        format!(
            "{label}: {} tok (prompt {}, cached {}, completion {})",
            self.total(),
            self.prompt,
            self.cached,
            self.completion
        )
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    System,
    Reasoning,
    Tool,
    User,
    Agent,
    Other,
}

impl MessageKind {
    fn from_speaker(speaker: &str) -> Self {
        let normalized = speaker.trim();
        if normalized.eq_ignore_ascii_case("SYSTEM") {
            MessageKind::System
        } else if normalized.contains('⋯') {
            MessageKind::Reasoning
        } else if normalized.contains("·CMD") {
            MessageKind::Tool
        } else if normalized.starts_with("USER") {
            MessageKind::User
        } else if normalized.contains("CODEX") || normalized.contains("VIZIER") {
            MessageKind::Agent
        } else {
            MessageKind::Other
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            MessageKind::System => "System",
            MessageKind::Reasoning => "Reasoning",
            MessageKind::Tool => "Tool Output",
            MessageKind::User => "User",
            MessageKind::Agent => "Agent",
            MessageKind::Other => "Message",
        }
    }

    pub fn is_collapsible(&self) -> bool {
        matches!(self, MessageKind::Reasoning | MessageKind::Tool)
    }

    pub fn slug(&self) -> &'static str {
        match self {
            MessageKind::System => "system",
            MessageKind::Reasoning => "reasoning",
            MessageKind::Tool => "tool",
            MessageKind::User => "user",
            MessageKind::Agent => "agent",
            MessageKind::Other => "other",
        }
    }
}

#[derive(Clone)]
struct ConfigEntryState {
    entry: ConfigEntry,
    last_failure: Option<String>,
}

impl ConfigEntryState {
    fn matches_path(&self, other: &PathBuf) -> bool {
        &self.entry.path == other
    }

    fn display_name(&self) -> String {
        self.entry.display_name.clone()
    }

    fn summary(&self) -> Option<String> {
        self.entry.summary()
    }

    fn discovery_error(&self) -> Option<String> {
        self.entry.error.clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigSource {
    Catalog,
    Quickstart,
}

#[derive(Clone, Debug)]
pub struct CatalogEntryView {
    pub index: usize,
    pub display_name: String,
    pub summary: Option<String>,
    pub discovery_error: Option<String>,
    pub last_failure: Option<String>,
    pub is_selected: bool,
    pub is_disabled: bool,
    pub source: ConfigSource,
}

#[derive(Clone, Debug)]
pub struct ConfigCatalogView {
    pub entries: Vec<CatalogEntryView>,
    pub hint: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ConfigSelection {
    pub path: PathBuf,
    pub display_name: String,
    pub summary: Option<String>,
}

#[derive(Default)]
pub struct ConfigCatalogState {
    entries: Vec<ConfigEntryState>,
    selected_path: Option<PathBuf>,
    quickstart_path: Option<PathBuf>,
    quickstart_entry: Option<ConfigEntryState>,
    quickstart_failure: Option<String>,
    catalog_root: Option<PathBuf>,
    last_error: Option<String>,
    launching: bool,
}

impl ConfigCatalogState {
    pub fn new(quickstart_path: Option<PathBuf>) -> Self {
        let mut state = Self {
            entries: Vec::new(),
            selected_path: None,
            quickstart_path,
            quickstart_entry: None,
            quickstart_failure: None,
            catalog_root: None,
            last_error: None,
            launching: false,
        };
        state.refresh_quickstart_entry();
        state
    }

    pub fn refresh(&mut self) -> Result<(), String> {
        match config_catalog::discover() {
            Ok(discovery) => {
                self.catalog_root = Some(discovery.root);
                let mut failures: HashMap<PathBuf, Option<String>> = self
                    .entries
                    .iter()
                    .map(|state| (state.entry.path.clone(), state.last_failure.clone()))
                    .collect();
                self.entries = discovery
                    .entries
                    .into_iter()
                    .map(|entry| ConfigEntryState {
                        last_failure: failures.remove(&entry.path).and_then(|value| value),
                        entry,
                    })
                    .collect();
                self.last_error = None;
            }
            Err(err) => {
                self.last_error = Some(err.clone());
                return Err(err);
            }
        }
        self.refresh_quickstart_entry();
        self.sync_selection();
        Ok(())
    }

    pub fn set_launching(&mut self, launching: bool) {
        self.launching = launching;
    }

    pub fn select(&mut self, index: usize) -> Result<ConfigSelection, String> {
        if self.launching {
            return Err("Launch already in progress; wait for completion.".to_string());
        }

        if self.entries.is_empty() {
            if index != 0 {
                return Err("Invalid catalog selection.".to_string());
            }
            let selection = self.quickstart_selection()?;
            self.selected_path = Some(selection.path.clone());
            Ok(selection)
        } else {
            let state = self
                .entries
                .get(index)
                .ok_or_else(|| "Invalid catalog selection.".to_string())?;
            self.selected_path = Some(state.entry.path.clone());
            Ok(ConfigSelection {
                path: state.entry.path.clone(),
                display_name: state.display_name(),
                summary: state.summary(),
            })
        }
    }

    pub fn active_selection(&self) -> Result<ConfigSelection, String> {
        if let Some(path) = self.selected_path.as_ref() {
            if let Some(selection) = self.selection_for_path(path) {
                return Ok(selection);
            }
        }

        self.quickstart_selection()
    }

    pub fn view(&self) -> ConfigCatalogView {
        let mut entries = Vec::new();
        if self.entries.is_empty() {
            if let Some(entry) = self.quickstart_entry.as_ref() {
                let is_selected = self
                    .selected_path
                    .as_ref()
                    .map(|path| entry.matches_path(path))
                    .unwrap_or(true);
                entries.push(CatalogEntryView {
                    index: 0,
                    display_name: entry.display_name(),
                    summary: entry.summary(),
                    discovery_error: entry.discovery_error(),
                    last_failure: self.quickstart_failure.clone(),
                    is_selected,
                    is_disabled: self.launching,
                    source: ConfigSource::Quickstart,
                });
            }
        } else {
            for (index, entry) in self.entries.iter().enumerate() {
                let is_selected = self
                    .selected_path
                    .as_ref()
                    .map(|path| entry.matches_path(path))
                    .unwrap_or(false);
                entries.push(CatalogEntryView {
                    index,
                    display_name: entry.display_name(),
                    summary: entry.summary(),
                    discovery_error: entry.discovery_error(),
                    last_failure: entry.last_failure.clone(),
                    is_selected,
                    is_disabled: self.launching,
                    source: ConfigSource::Catalog,
                });
            }
        }

        let hint = if entries.is_empty() {
            self.catalog_root
                .as_ref()
                .map(|root| format!("Drop castra.toml files into {}.", root.display()))
        } else {
            None
        };

        ConfigCatalogView {
            entries,
            hint,
            last_error: self.last_error.clone(),
        }
    }

    pub fn note_launch_failure(&mut self, path: &PathBuf, message: String) {
        if self
            .quickstart_path
            .as_ref()
            .map(|root| root == path)
            .unwrap_or(false)
        {
            self.quickstart_failure = Some(message.clone());
            if let Some(entry) = self.quickstart_entry.as_mut() {
                entry.last_failure = Some(message);
            }
            return;
        }

        for entry in &mut self.entries {
            if entry.matches_path(path) {
                entry.last_failure = Some(message.clone());
                break;
            }
        }
    }

    pub fn clear_launch_failure(&mut self, path: &PathBuf) {
        if self
            .quickstart_path
            .as_ref()
            .map(|root| root == path)
            .unwrap_or(false)
        {
            self.quickstart_failure = None;
            if let Some(entry) = self.quickstart_entry.as_mut() {
                entry.last_failure = None;
            }
            return;
        }

        for entry in &mut self.entries {
            if entry.matches_path(path) {
                entry.last_failure = None;
                break;
            }
        }
    }

    fn selection_for_path(&self, path: &PathBuf) -> Option<ConfigSelection> {
        if self
            .quickstart_path
            .as_ref()
            .map(|root| root == path)
            .unwrap_or(false)
        {
            return self.quickstart_selection().ok();
        }

        self.entries
            .iter()
            .find(|entry| entry.matches_path(path))
            .map(|entry| ConfigSelection {
                path: entry.entry.path.clone(),
                display_name: entry.display_name(),
                summary: entry.summary(),
            })
    }

    fn quickstart_selection(&self) -> Result<ConfigSelection, String> {
        let path = self
            .quickstart_path
            .as_ref()
            .ok_or_else(|| "Quickstart config unavailable.".to_string())?
            .clone();

        let (display_name, summary) = if let Some(entry) = self.quickstart_entry.as_ref() {
            (entry.display_name(), entry.summary())
        } else {
            ("bootstrap-quickstart".to_string(), None)
        };

        Ok(ConfigSelection {
            path,
            display_name,
            summary,
        })
    }

    fn refresh_quickstart_entry(&mut self) {
        self.quickstart_entry = self.quickstart_path.as_ref().map(|path| ConfigEntryState {
            entry: config_catalog::load_entry(path),
            last_failure: self.quickstart_failure.clone(),
        });
    }

    fn sync_selection(&mut self) {
        if let Some(selected) = self.selected_path.clone() {
            if self.selection_for_path(&selected).is_none() {
                self.selected_path = None;
            }
        }
    }
}

#[derive(Clone)]
pub struct ChatMessage {
    timestamp: SharedString,
    speaker: SharedString,
    text: SharedString,
    kind: MessageKind,
    expanded: bool,
    collapsed_preview: Option<SharedString>,
}

impl ChatMessage {
    pub fn new<S: Into<String>, T: Into<String>>(speaker: S, text: T) -> Self {
        let speaker_string = speaker.into();
        let text_string = text.into();
        let kind = MessageKind::from_speaker(&speaker_string);
        let expanded = !kind.is_collapsible();
        let collapsed_preview = if kind.is_collapsible() {
            Some(Self::build_collapsed_preview(&text_string, kind))
        } else {
            None
        };
        Self {
            timestamp: current_timestamp(),
            speaker: speaker_string.into(),
            text: text_string.into(),
            kind,
            expanded,
            collapsed_preview,
        }
    }

    pub fn timestamp(&self) -> &SharedString {
        &self.timestamp
    }

    pub fn speaker(&self) -> &SharedString {
        &self.speaker
    }

    pub fn text(&self) -> &SharedString {
        &self.text
    }

    pub fn kind(&self) -> MessageKind {
        self.kind
    }

    pub fn is_collapsible(&self) -> bool {
        self.kind.is_collapsible()
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    pub fn toggle_expanded(&mut self) {
        if self.is_collapsible() {
            self.expanded = !self.expanded;
        }
    }

    pub fn collapsed_preview(&self) -> Option<&SharedString> {
        self.collapsed_preview.as_ref()
    }

    fn build_collapsed_preview(text: &str, kind: MessageKind) -> SharedString {
        let label = kind.display_name();
        let preview_line = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("");

        let mut summary = String::new();
        if !preview_line.is_empty() {
            let mut collected = String::new();
            for ch in preview_line.chars().take(COLLAPSED_PREVIEW_MAX_CHARS) {
                collected.push(ch);
            }
            if preview_line.chars().count() > COLLAPSED_PREVIEW_MAX_CHARS {
                collected.push('…');
            }
            summary.push_str(&collected);
            summary.push(' ');
        }
        summary.push('(');
        summary.push_str(label);
        summary.push_str(" hidden — click to expand)");
        summary.into()
    }
}

#[derive(Clone)]
pub struct Agent {
    id: String,
    status: String,
}

impl Agent {
    pub fn new(id: &str, status: &str) -> Self {
        Self {
            id: id.to_string(),
            status: status.to_string(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn label(&self) -> String {
        self.id.to_uppercase()
    }
}

pub struct ChatState {
    messages: Vec<ChatMessage>,
    list_state: ListState,
    stick_to_bottom: bool,
    scroll_dirty: Rc<Cell<bool>>,
    dropped_messages: usize,
    log_soft_limit: usize,
}

impl ChatState {
    pub fn new() -> Self {
        let scroll_dirty = Rc::new(Cell::new(true));
        let list_state = {
            let state = ListState::new(0, ListAlignment::Bottom, px(160.));
            let flag = scroll_dirty.clone();
            state.set_scroll_handler(move |_, _, _| {
                flag.set(true);
            });
            state
        };

        Self {
            messages: Vec::new(),
            list_state,
            stick_to_bottom: true,
            scroll_dirty,
            dropped_messages: 0,
            log_soft_limit: DEFAULT_LOG_SOFT_LIMIT,
        }
    }

    pub fn push_message(&mut self, message: ChatMessage) {
        let insertion_index = self.messages.len();
        self.messages.push(message);
        self.list_state.splice(insertion_index..insertion_index, 1);
        if self.stick_to_bottom {
            self.list_state.scroll_to(ListOffset {
                item_ix: self.messages.len(),
                offset_in_item: px(0.),
            });
        }
        self.scroll_dirty.set(true);
        self.trim_if_needed();
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn list_state(&self) -> &ListState {
        &self.list_state
    }

    pub fn toggle_message_at(&mut self, index: usize) -> bool {
        if let Some(message) = self.messages.get_mut(index) {
            if message.is_collapsible() {
                message.toggle_expanded();
                self.scroll_dirty.set(true);
                self.list_state.splice(index..index + 1, 1);
                return true;
            }
        }
        false
    }

    pub fn refresh_stick_to_bottom(
        &mut self,
        scrollable_threshold_px: f32,
        bottom_tolerance_px: f32,
    ) {
        if !self.scroll_dirty.get() {
            return;
        }
        let max_offset = f32::from(
            self.list_state
                .max_offset_for_scrollbar()
                .height
                .max(Pixels::ZERO),
        );
        let offset = -f32::from(self.list_state.scroll_px_offset_for_scrollbar().y);
        let near_bottom = if max_offset <= scrollable_threshold_px {
            true
        } else {
            (max_offset - offset).abs() <= bottom_tolerance_px
        };

        self.stick_to_bottom = near_bottom;
        self.scroll_dirty.set(false);
    }

    pub fn dropped_messages(&self) -> usize {
        self.dropped_messages
    }

    fn trim_if_needed(&mut self) {
        if self.messages.len() <= self.log_soft_limit {
            return;
        }

        let overflow = self.messages.len() - self.log_soft_limit;
        self.list_state.splice(0..overflow, 0);
        self.messages.drain(0..overflow);
        self.dropped_messages = self.dropped_messages.saturating_add(overflow);
        self.scroll_dirty.set(true);
    }
}

impl Default for ChatState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RosterState {
    agents: Vec<Agent>,
    active_agent: usize,
}

impl RosterState {
    pub fn agents(&self) -> &[Agent] {
        &self.agents
    }

    pub fn active_index(&self) -> usize {
        self.active_agent
    }

    pub fn active_agent(&self) -> &Agent {
        &self.agents[self.active_agent]
    }

    pub fn switch_to(&mut self, index: usize) -> bool {
        if index >= self.agents.len() || index == self.active_agent {
            return false;
        }
        self.active_agent = index;
        true
    }

}

impl Default for RosterState {
    fn default() -> Self {
        Self {
            agents: vec![
                Agent::new("assist", "ONLINE"),
                Agent::new("research", "IDLE"),
                Agent::new("builder", "STANDBY"),
            ],
            active_agent: 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AttentionLevel {
    Idle,
    Progress,
    Success,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VmPhase {
    Pending,
    Planned,
    OverlayPrepared,
    Launching,
    Bootstrapping,
    Ready,
    Failed,
}

impl VmPhase {
    pub fn label(self) -> &'static str {
        match self {
            VmPhase::Pending => "PENDING",
            VmPhase::Planned => "PLANNED",
            VmPhase::OverlayPrepared => "OVERLAY READY",
            VmPhase::Launching => "LAUNCHING",
            VmPhase::Bootstrapping => "BOOTSTRAPPING",
            VmPhase::Ready => "READY",
            VmPhase::Failed => "FAILED",
        }
    }
}

impl fmt::Display for VmPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Clone)]
pub struct VirtualMachine {
    name: String,
    phase: VmPhase,
    attention: AttentionLevel,
    detail: String,
}

impl VirtualMachine {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            phase: VmPhase::Pending,
            attention: AttentionLevel::Idle,
            detail: "Awaiting events...".to_string(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn phase(&self) -> VmPhase {
        self.phase
    }

    pub fn attention(&self) -> AttentionLevel {
        self.attention
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }

    pub fn set_state<T: Into<String>>(
        &mut self,
        phase: VmPhase,
        attention: AttentionLevel,
        detail: T,
    ) {
        self.phase = phase;
        self.attention = attention;
        self.detail = detail.into();
    }
}

pub struct VmFleetState {
    vms: Vec<VirtualMachine>,
    focused: Option<usize>,
}

impl Default for VmFleetState {
    fn default() -> Self {
        Self {
            vms: Vec::new(),
            focused: None,
        }
    }
}

impl VmFleetState {
    pub fn virtual_machines(&self) -> &[VirtualMachine] {
        &self.vms
    }

    pub fn focused_index(&self) -> Option<usize> {
        self.focused
    }

    pub fn focused_vm(&self) -> Option<&VirtualMachine> {
        self.focused.and_then(|index| self.vms.get(index))
    }

    pub fn reset(&mut self) {
        self.vms.clear();
        self.focused = None;
    }

    pub fn ensure_vm(&mut self, name: &str) -> &mut VirtualMachine {
        if let Some(index) = self.vms.iter().position(|vm| vm.name == name) {
            &mut self.vms[index]
        } else {
            self.vms.push(VirtualMachine::new(name));
            let index = self.vms.len() - 1;
            if self.focused.is_none() {
                self.focused = Some(index);
            }
            self.vms.last_mut().expect("new VM inserted")
        }
    }

    pub fn update_vm<T: Into<String>>(
        &mut self,
        name: &str,
        phase: VmPhase,
        attention: AttentionLevel,
        detail: T,
    ) {
        let vm = self.ensure_vm(name);
        vm.set_state(phase, attention, detail);
    }

    #[allow(dead_code)]
    pub fn focus_first(&mut self) -> Option<usize> {
        if self.vms.is_empty() {
            self.focused = None;
            return None;
        }
        if self.focused.is_some() {
            return None;
        }
        self.focused = Some(0);
        Some(0)
    }

    #[allow(dead_code)]
    pub fn focus_vm_at(&mut self, index: usize) -> Option<usize> {
        if index >= self.vms.len() {
            return None;
        }
        if self.focused == Some(index) {
            return None;
        }
        self.focused = Some(index);
        Some(index)
    }

    pub fn focus_next(&mut self) -> Option<usize> {
        if self.vms.is_empty() {
            self.focused = None;
            return None;
        }
        let next = match self.focused {
            Some(current) => (current + 1) % self.vms.len(),
            None => 0,
        };
        if self.focused == Some(next) {
            return None;
        }
        self.focused = Some(next);
        Some(next)
    }

    pub fn focus_prev(&mut self) -> Option<usize> {
        if self.vms.is_empty() {
            self.focused = None;
            return None;
        }
        let len = self.vms.len();
        let prev = match self.focused {
            Some(current) => (current + len - 1) % len,
            None => len - 1,
        };
        if self.focused == Some(prev) {
            return None;
        }
        self.focused = Some(prev);
        Some(prev)
    }

    pub fn counts(&self) -> VmCounts {
        let mut ready = 0usize;
        let mut failed = 0usize;

        for vm in &self.vms {
            match vm.phase {
                VmPhase::Ready => ready += 1,
                VmPhase::Failed => failed += 1,
                _ => {}
            }
        }

        VmCounts {
            total: self.vms.len(),
            ready,
            failed,
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct VmCounts {
    pub total: usize,
    pub ready: usize,
    pub failed: usize,
}

impl VmCounts {
    pub fn in_progress(self) -> usize {
        self.total.saturating_sub(self.ready + self.failed)
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct RuntimePaths {
    state_root: PathBuf,
    log_root: PathBuf,
}

#[allow(dead_code)]
impl RuntimePaths {
    pub fn state_root(&self) -> &PathBuf {
        &self.state_root
    }

    pub fn log_root(&self) -> &PathBuf {
        &self.log_root
    }
}

#[derive(Clone)]
pub enum UpLifecycle {
    Idle,
    Running {
        started_at: DateTime<Local>,
    },
    Success {
        started_at: DateTime<Local>,
        completed_at: DateTime<Local>,
    },
    Failed {
        started_at: Option<DateTime<Local>>,
        reason: String,
    },
}

impl Default for UpLifecycle {
    fn default() -> Self {
        Self::Idle
    }
}

pub struct UpState {
    lifecycle: UpLifecycle,
    vm_fleet: VmFleetState,
    last_error: Option<String>,
    runtime_paths: Option<RuntimePaths>,
    shutdown_in_progress: bool,
}

impl Default for UpState {
    fn default() -> Self {
        Self {
            lifecycle: UpLifecycle::Idle,
            vm_fleet: VmFleetState::default(),
            last_error: None,
            runtime_paths: None,
            shutdown_in_progress: false,
        }
    }
}

impl UpState {
    pub fn is_running(&self) -> bool {
        matches!(self.lifecycle, UpLifecycle::Running { .. })
    }

    pub fn lifecycle(&self) -> UpLifecycle {
        self.lifecycle.clone()
    }

    pub fn start(&mut self) -> bool {
        if self.is_running() {
            return false;
        }
        self.lifecycle = UpLifecycle::Running {
            started_at: Local::now(),
        };
        self.vm_fleet.reset();
        self.last_error = None;
        self.runtime_paths = None;
        self.shutdown_in_progress = false;
        true
    }

    pub fn mark_success(&mut self) {
        let (started_at, prev_error) = match &self.lifecycle {
            UpLifecycle::Running { started_at } => (*started_at, self.last_error.clone()),
            UpLifecycle::Success { started_at, .. } => (*started_at, self.last_error.clone()),
            UpLifecycle::Failed { started_at, .. } => (
                started_at.unwrap_or_else(Local::now),
                self.last_error.clone(),
            ),
            UpLifecycle::Idle => (Local::now(), self.last_error.clone()),
        };

        self.lifecycle = UpLifecycle::Success {
            started_at,
            completed_at: Local::now(),
        };
        self.last_error = prev_error;
    }

    pub fn mark_failure<T: Into<String>>(&mut self, reason: T) {
        let started_at = match &self.lifecycle {
            UpLifecycle::Running { started_at } => Some(*started_at),
            UpLifecycle::Success { started_at, .. } => Some(*started_at),
            UpLifecycle::Failed { started_at, .. } => *started_at,
            UpLifecycle::Idle => None,
        };
        let reason = reason.into();
        self.lifecycle = UpLifecycle::Failed {
            started_at,
            reason: reason.clone(),
        };
        self.last_error = Some(reason);
    }

    pub fn vm_fleet(&self) -> &VmFleetState {
        &self.vm_fleet
    }

    pub fn vm_fleet_mut(&mut self) -> &mut VmFleetState {
        &mut self.vm_fleet
    }

    pub fn note_error<T: Into<String>>(&mut self, message: T) {
        self.last_error = Some(message.into());
    }

    pub fn counts(&self) -> VmCounts {
        self.vm_fleet.counts()
    }

    pub fn set_runtime_paths(&mut self, state_root: PathBuf, log_root: PathBuf) {
        self.runtime_paths = Some(RuntimePaths {
            state_root,
            log_root,
        });
    }

    pub fn clear_runtime_paths(&mut self) {
        self.runtime_paths = None;
    }

    #[allow(dead_code)]
    pub fn runtime_paths(&self) -> Option<&RuntimePaths> {
        self.runtime_paths.as_ref()
    }

    pub fn mark_shutdown_started(&mut self) {
        self.shutdown_in_progress = true;
    }

    pub fn mark_shutdown_complete(&mut self) {
        self.shutdown_in_progress = false;
        self.clear_runtime_paths();
    }

    pub fn shutdown_in_progress(&self) -> bool {
        self.shutdown_in_progress
    }

    pub fn status_line(&self) -> String {
        let counts = self.counts();
        match &self.lifecycle {
            UpLifecycle::Idle => "UP idle".to_string(),
            UpLifecycle::Running { .. } => {
                let mut parts = vec![format!(
                    "UP running: {} ready / {} total",
                    counts.ready, counts.total
                )];
                let in_progress = counts.in_progress();
                if in_progress > 0 {
                    parts.push(format!("{in_progress} in progress"));
                }
                if counts.failed > 0 {
                    parts.push(format!("{} failed", counts.failed));
                }
                if let Some(error) = &self.last_error {
                    parts.push(format!("last error: {error}"));
                }
                parts.join(" • ")
            }
            UpLifecycle::Success {
                started_at,
                completed_at,
            } => {
                let elapsed = (*completed_at - *started_at).num_milliseconds();
                let duration = if elapsed <= 0 {
                    "<1s".to_string()
                } else {
                    format!("{:.1}s", (elapsed as f64) / 1000.0)
                };
                let total = counts.total.max(counts.ready);
                format!(
                    "UP complete in {duration} • {}/{} ready",
                    counts.ready, total
                )
            }
            UpLifecycle::Failed { reason, .. } => format!("UP failed: {reason}"),
        }
    }
}

#[derive(Default)]
pub struct UiState {
    sidebar_visible: bool,
    toasts: Vec<Toast>,
}

impl UiState {
    pub fn sidebar_visible(&self) -> bool {
        self.sidebar_visible
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
    }

    pub fn push_toast<T: Into<String>>(&mut self, message: T) {
        self.prune_toasts();
        self.toasts.push(Toast::new(message));
    }

    pub fn collect_active_toasts(&mut self) -> Vec<String> {
        self.prune_toasts();
        self.toasts
            .iter()
            .map(|toast| toast.message.clone())
            .collect()
    }

    fn prune_toasts(&mut self) {
        let now = Instant::now();
        self.toasts.retain(|toast| !toast.is_expired(now));
    }
}

const TOAST_TTL: Duration = Duration::from_secs(3);

struct Toast {
    message: String,
    created_at: Instant,
}

impl Toast {
    fn new<T: Into<String>>(message: T) -> Self {
        Self {
            message: message.into(),
            created_at: Instant::now(),
        }
    }

    fn is_expired(&self, now: Instant) -> bool {
        now.duration_since(self.created_at) > TOAST_TTL
    }
}

pub struct AppState {
    chat: ChatState,
    roster: RosterState,
    up: UpState,
    ui: UiState,
    config_catalog: ConfigCatalogState,
    codex_thread_id: Option<String>,
    config_path: Option<PathBuf>,
    transcript_writer: Option<Arc<TranscriptWriter>>,
    transcript_error_reported: bool,
    codex_usage: TokenUsageTotals,
    codex_turn_active: bool,
}

impl AppState {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_transcript(None, None)
    }

    pub fn with_transcript(
        transcript_writer: Option<Arc<TranscriptWriter>>,
        quickstart_path: Option<PathBuf>,
    ) -> Self {
        let mut state = Self {
            chat: ChatState::default(),
            roster: RosterState::default(),
            up: UpState::default(),
            ui: UiState::default(),
            config_catalog: ConfigCatalogState::new(quickstart_path),
            codex_thread_id: None,
            config_path: None,
            transcript_writer,
            transcript_error_reported: false,
            codex_usage: TokenUsageTotals::default(),
            codex_turn_active: false,
        };
        state.push_system_message("Welcome to Castra. Type /help to discover commands.");
        state.push_system_message(
            "Pick a catalog entry or run /up to launch the bootstrap-quickstart workspace.",
        );
        state
    }

    pub fn chat(&self) -> &ChatState {
        &self.chat
    }

    pub fn chat_mut(&mut self) -> &mut ChatState {
        &mut self.chat
    }

    pub fn roster(&self) -> &RosterState {
        &self.roster
    }

    pub fn vm_fleet(&self) -> &VmFleetState {
        self.up.vm_fleet()
    }

    pub fn up_lifecycle(&self) -> UpLifecycle {
        self.up.lifecycle()
    }

    pub fn focused_vm_name(&self) -> Option<String> {
        self.up
            .vm_fleet()
            .focused_vm()
            .map(|vm| vm.name().to_string())
    }

    pub fn focused_vm_label(&self) -> Option<String> {
        self.focused_vm_name().map(|name| name.to_uppercase())
    }

    #[allow(dead_code)]
    pub fn focus_vm_at(&mut self, index: usize) -> Option<String> {
        let new_index = self.up.vm_fleet_mut().focus_vm_at(index)?;
        self.up
            .vm_fleet()
            .virtual_machines()
            .get(new_index)
            .map(|vm| vm.name().to_string())
    }

    pub fn focus_next_vm(&mut self) -> Option<String> {
        let new_index = self.up.vm_fleet_mut().focus_next()?;
        self.up
            .vm_fleet()
            .virtual_machines()
            .get(new_index)
            .map(|vm| vm.name().to_string())
    }

    pub fn focus_prev_vm(&mut self) -> Option<String> {
        let new_index = self.up.vm_fleet_mut().focus_prev()?;
        self.up
            .vm_fleet()
            .virtual_machines()
            .get(new_index)
            .map(|vm| vm.name().to_string())
    }

    pub fn resolve_vm_name(&self, candidate: &str) -> Option<String> {
        let needle = candidate.trim();
        if needle.is_empty() {
            return None;
        }
        self.up
            .vm_fleet()
            .virtual_machines()
            .iter()
            .find(|vm| vm.name().eq_ignore_ascii_case(needle))
            .map(|vm| vm.name().to_string())
    }

    pub fn refresh_config_catalog(&mut self) -> Result<(), String> {
        self.config_catalog.refresh()
    }

    pub fn catalog_view(&self) -> ConfigCatalogView {
        self.config_catalog.view()
    }

    pub fn select_catalog_entry(&mut self, index: usize) -> Result<ConfigSelection, String> {
        self.config_catalog.select(index)
    }

    pub fn active_config_selection(&self) -> Result<ConfigSelection, String> {
        self.config_catalog.active_selection()
    }

    pub fn catalog_note_launch_failure(&mut self, path: &PathBuf, message: String) {
        self.config_catalog.note_launch_failure(path, message);
    }

    pub fn catalog_clear_launch_failure(&mut self, path: &PathBuf) {
        self.config_catalog.clear_launch_failure(path);
    }

    pub fn push_toast<T: Into<String>>(&mut self, message: T) {
        self.ui.push_toast(message);
    }

    pub fn collect_active_toasts(&mut self) -> Vec<String> {
        self.ui.collect_active_toasts()
    }

    pub fn toggle_sidebar(&mut self) {
        self.ui.toggle_sidebar();
    }

    pub fn sidebar_visible(&self) -> bool {
        self.ui.sidebar_visible()
    }

    pub fn active_agent_label(&self) -> String {
        self.roster.active_agent().label()
    }

    pub fn active_agent_index(&self) -> usize {
        self.roster.active_index()
    }

    pub fn switch_agent(&mut self, index: usize) -> bool {
        self.roster.switch_to(index)
    }

    pub fn push_message<S: Into<String>, T: Into<String>>(&mut self, speaker: S, text: T) {
        let message = ChatMessage::new(speaker, text);
        self.chat.push_message(message.clone());
        self.record_transcript(&message);
    }

    pub fn push_system_message<T: Into<String>>(&mut self, text: T) {
        self.push_message("SYSTEM", text);
    }

    fn record_transcript(&mut self, message: &ChatMessage) {
        let Some(writer) = self.transcript_writer.clone() else {
            return;
        };

        if let Err(err) = writer.record(message) {
            eprintln!("castra-ui: failed to write transcript entry: {err}");
            self.transcript_writer = None;
            if !self.transcript_error_reported {
                self.transcript_error_reported = true;
                self.push_system_message(format!("Transcript persistence disabled: {err}"));
            }
        }
    }

    pub fn push_user_command(&mut self, text: &str) {
        self.push_message("USER", text.to_string());
    }

    pub fn push_user_entry(&mut self, text: &str) {
        let target = self.roster.active_agent().label();
        let speaker = format!("USER→{}", target);
        self.push_message(speaker, text.to_string());
    }

    pub fn codex_thread_id(&self) -> Option<String> {
        self.codex_thread_id.clone()
    }

    pub fn apply_harness_event(&mut self, event: &HarnessEvent) {
        let mut record_usage = None;
        match event {
            HarnessEvent::ThreadStarted { thread_id } => {
                self.codex_thread_id = Some(thread_id.clone());
                self.push_system_message(format!("Codex thread ready ({thread_id})"));
            }
            HarnessEvent::AgentMessage { text } => {
                self.push_message("CODEX", text.clone());
            }
            HarnessEvent::Reasoning { text } => {
                self.push_message("CODEX⋯", text.clone());
            }
            HarnessEvent::CommandProgress {
                command,
                output,
                status,
                exit_code,
            } => {
                let status_label = match status {
                    CommandStatus::InProgress => "running",
                    CommandStatus::Completed => "completed",
                    CommandStatus::Failed => "failed",
                };
                let mut message = format!("Codex command {status_label}: {command}");
                if let Some(code) = exit_code {
                    message.push_str(&format!(" (exit {code})"));
                }
                self.push_system_message(message);
                if !output.is_empty() {
                    self.push_message("CODEX·CMD", output.clone());
                }
            }
            HarnessEvent::FileChange { changes, status } => {
                let status_label = match status {
                    PatchStatus::Completed => "applied",
                    PatchStatus::Failed => "failed",
                };
                let summary = render_file_changes(changes);
                self.push_system_message(format!("Codex file changes {status_label}: {summary}"));
            }
            HarnessEvent::TodoList { items } => {
                let summary = render_todo_list(items);
                self.push_system_message(format!("Codex TODO: {summary}"));
            }
            HarnessEvent::Usage {
                prompt_tokens,
                cached_tokens,
                completion_tokens,
            } => {
                record_usage = Some((*prompt_tokens, *cached_tokens, *completion_tokens));
                self.push_system_message(format!(
                    "Codex usage — prompt: {prompt_tokens}, cached: {cached_tokens}, completion: {completion_tokens}"
                ));
            }
            HarnessEvent::Failure { message } => {
                self.push_system_message(format!("Codex failure: {message}"));
            }
        }
        if let Some((prompt, cached, completion)) = record_usage {
            self.codex_usage.add(prompt, cached, completion);
        }
    }

    pub fn agent_index_by_id(&self, id: &str) -> Option<usize> {
        self.roster
            .agents()
            .iter()
            .position(|agent| agent.id().eq_ignore_ascii_case(id))
    }

    pub fn begin_up_operation(&mut self) -> Result<(), &'static str> {
        if self.up.shutdown_in_progress() {
            return Err("Shutdown is in progress; wait for cleanup to finish.");
        }
        if !self.up.start() {
            return Err("An /up operation is already in progress.");
        }
        self.config_catalog.set_launching(true);
        Ok(())
    }

    pub fn complete_up_success(&mut self) {
        self.up.mark_success();
        self.config_catalog.set_launching(false);
        if let Some(path) = self.config_path.clone() {
            self.catalog_clear_launch_failure(&path);
        }
    }

    pub fn complete_up_failure<T: Into<String>>(&mut self, reason: T) {
        let reason_string = reason.into();
        self.up.mark_failure(reason_string.clone());
        self.config_catalog.set_launching(false);
        if let Some(path) = self.config_path.clone() {
            self.catalog_note_launch_failure(&path, reason_string.clone());
        }
    }

    pub fn record_runtime_paths(&mut self, state_root: PathBuf, log_root: PathBuf) {
        self.up.set_runtime_paths(state_root, log_root);
    }

    #[allow(dead_code)]
    pub fn runtime_paths(&self) -> Option<&RuntimePaths> {
        self.up.runtime_paths()
    }

    pub fn mark_shutdown_started(&mut self) {
        self.up.mark_shutdown_started();
    }

    pub fn mark_shutdown_complete(&mut self) {
        self.up.mark_shutdown_complete();
        self.config_catalog.set_launching(false);
    }

    pub fn shutdown_in_progress(&self) -> bool {
        self.up.shutdown_in_progress()
    }

    pub fn set_config_path(&mut self, path: PathBuf) {
        self.config_path = Some(path);
    }

    #[allow(dead_code)]
    pub fn clear_config_path(&mut self) {
        self.config_path = None;
    }

    #[allow(dead_code)]
    pub fn config_path(&self) -> Option<&PathBuf> {
        self.config_path.as_ref()
    }

    pub fn handle_up_event(&mut self, event: &Event) -> Option<String> {
        use AttentionLevel::*;
        use VmPhase::*;

        match event {
            Event::Message { severity, text } => {
                let tag = match severity {
                    Severity::Info => "INFO",
                    Severity::Warning => "WARN",
                    Severity::Error => "ERROR",
                };
                if matches!(severity, Severity::Error) {
                    self.up.note_error(text.clone());
                }
                Some(format!("[{tag}] {text}"))
            }
            Event::BootstrapPlanned {
                vm, action, reason, ..
            } => {
                let attention = if action.is_error() { Error } else { Progress };
                let detail = format!("Plan {} ({reason})", action.describe());
                self.up.vm_fleet_mut()
                    .update_vm(vm, Planned, attention, detail.clone());
                if action.is_error() {
                    self.up.note_error(detail.clone());
                }
                Some(format!("{vm}: bootstrap plan {}", action.describe()))
            }
            Event::OverlayPrepared { vm, overlay_path } => {
                self.up.vm_fleet_mut().update_vm(
                    vm,
                    OverlayPrepared,
                    Progress,
                    format!("Overlay ready at {}", overlay_path.display()),
                );
                Some(format!("{vm}: overlay prepared"))
            }
            Event::VmLaunched { vm, pid } => {
                self.up.vm_fleet_mut().update_vm(
                    vm,
                    Launching,
                    Progress,
                    format!("VM launched (pid {pid})"),
                );
                Some(format!("{vm}: VM launched (pid {pid})"))
            }
            Event::BootstrapStarted { vm, trigger, .. } => {
                let trigger_text = format_trigger(trigger);
                self.up.vm_fleet_mut().update_vm(
                    vm,
                    Bootstrapping,
                    Progress,
                    format!("Bootstrap started ({trigger_text})"),
                );
                Some(format!("{vm}: bootstrap started ({trigger_text})"))
            }
            Event::BootstrapStep {
                vm,
                step,
                status,
                duration_ms,
                detail,
            } => {
                let text = format_step(step, status, *duration_ms, detail.as_deref());
                let attention = if matches!(status, BootstrapStepStatus::Failed) {
                    self.up.note_error(text.clone());
                    Error
                } else {
                    Progress
                };
                self.up
                    .vm_fleet_mut()
                    .update_vm(vm, Bootstrapping, attention, text.clone());
                Some(format!("{vm}: {text}"))
            }
            Event::BootstrapCompleted {
                vm,
                status,
                duration_ms,
                ..
            } => {
                let text = match status {
                    BootstrapStatus::Success => {
                        format!("Bootstrap succeeded in {} ms", duration_ms)
                    }
                    BootstrapStatus::NoOp => {
                        format!("Bootstrap skipped (noop) in {} ms", duration_ms)
                    }
                };
                self.up
                    .vm_fleet_mut()
                    .update_vm(vm, Ready, Success, text.clone());
                Some(format!("{vm}: {text}"))
            }
            Event::BootstrapFailed {
                vm,
                duration_ms,
                error,
            } => {
                let text = format!("Bootstrap failed after {} ms: {error}", duration_ms);
                self.up
                    .vm_fleet_mut()
                    .update_vm(vm, Failed, Error, text.clone());
                self.up.note_error(format!("{vm}: {error}"));
                Some(format!("{vm}: {text}"))
            }
            _ => None,
        }
    }

    pub fn up_status_line(&self) -> String {
        self.up.status_line()
    }

    pub fn codex_turn_active(&self) -> bool {
        self.codex_turn_active
    }

    pub fn set_codex_turn_active(&mut self, active: bool) {
        self.codex_turn_active = active;
    }

    pub fn mark_codex_turn_finished(&mut self) {
        self.codex_turn_active = false;
    }

    pub fn token_usage_summaries(&self) -> Vec<String> {
        let mut summaries = Vec::new();
        if !self.codex_usage.is_empty() {
            summaries.push(self.codex_usage.summary("Codex"));
        }
        summaries
    }
}

fn format_step(
    step: &BootstrapStepKind,
    status: &BootstrapStepStatus,
    duration_ms: u64,
    detail: Option<&str>,
) -> String {
    let status_label = match status {
        BootstrapStepStatus::Success => "success",
        BootstrapStepStatus::Skipped => "skipped",
        BootstrapStepStatus::Failed => "failed",
    };

    let mut text = format!("{:?} {status_label} ({duration_ms} ms)", step);
    if let Some(detail) = detail {
        if !detail.is_empty() {
            text.push_str(": ");
            text.push_str(detail);
        }
    }
    text
}

fn format_trigger(trigger: &BootstrapTrigger) -> String {
    match trigger {
        BootstrapTrigger::Always => "always".to_string(),
        BootstrapTrigger::Auto => "auto".to_string(),
    }
}

fn render_file_changes(changes: &[FileDiff]) -> String {
    if changes.is_empty() {
        return "none".to_string();
    }

    changes
        .iter()
        .map(|diff| format!("{} {}", describe_diff_kind(&diff.kind), diff.path))
        .collect::<Vec<_>>()
        .join(" • ")
}

fn describe_diff_kind(kind: &FileDiffKind) -> &'static str {
    match kind {
        FileDiffKind::Add => "added",
        FileDiffKind::Delete => "removed",
        FileDiffKind::Update => "updated",
    }
}

fn render_todo_list(items: &[TodoEntry]) -> String {
    if items.is_empty() {
        return "none".to_string();
    }

    items
        .iter()
        .map(|item| {
            let status = if item.completed { 'x' } else { ' ' };
            format!("[{status}] {}", item.text)
        })
        .collect::<Vec<_>>()
        .join(" • ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    struct EnvVarGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvVarGuard {
        fn set_path(key: &'static str, value: &Path) -> Self {
            let original = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(ref value) = self.original {
                unsafe {
                    std::env::set_var(self.key, value);
                }
            } else {
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    #[test]
    fn user_entries_route_through_active_agent() {
        let mut state = AppState::new();
        state.push_user_entry("Hello agent");

        let last_message = state
            .chat()
            .messages()
            .last()
            .expect("user entry should append message");
        assert_eq!(last_message.speaker().as_ref(), "USER→ASSIST");
        assert_eq!(state.roster().active_agent().id(), "assist");
    }

    #[test]
    fn up_operation_sets_launching_flag() {
        let mut state = AppState::new();
        state.begin_up_operation().expect("up should start");

        let Err(err) = state.select_catalog_entry(0) else {
            panic!("selection should fail while launch in progress");
        };
        assert!(
            err.contains("Launch already in progress"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn catalog_selection_survives_refresh() {
        let temp_home = TempDir::new().expect("temp dir should exist");
        let _home_guard = EnvVarGuard::set_path("HOME", temp_home.path());

        let config_dir = temp_home.path().join(".castra").join("configs");
        std::fs::create_dir_all(&config_dir).expect("catalog dir should be created");
        let config_path = config_dir.join("sample.toml");
        std::fs::write(
            &config_path,
            r#"version = "0.2.0"

[project]
name = "sample"

[[vms]]
name = "alpine"
base_image = "alpine.qcow2"
cpus = 1
memory = "1 GiB"
"#,
        )
        .expect("config write should succeed");

        let mut state = AppState::with_transcript(None, None);
        state
            .refresh_config_catalog()
            .expect("catalog refresh should succeed");

        let selection = state
            .select_catalog_entry(0)
            .expect("selection should succeed");
        assert_eq!(selection.display_name, "sample");

        state
            .refresh_config_catalog()
            .expect("second refresh should succeed");

        let view = state.catalog_view();
        assert!(
            view.entries
                .first()
                .map(|entry| entry.is_selected)
                .unwrap_or(false),
            "selection should persist after refresh"
        );
    }
}
