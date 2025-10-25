use chrono::Local;
use gpui::SharedString;

fn current_timestamp() -> SharedString {
    Local::now().format("%H:%M:%S").to_string().into()
}

#[derive(Clone)]
pub struct ChatMessage {
    timestamp: SharedString,
    speaker: SharedString,
    text: SharedString,
}

impl ChatMessage {
    pub fn new<S: Into<SharedString>, T: Into<SharedString>>(speaker: S, text: T) -> Self {
        Self {
            timestamp: current_timestamp(),
            speaker: speaker.into(),
            text: text.into(),
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

#[derive(Clone)]
pub struct VirtualMachine {
    name: String,
    online: bool,
    project: String,
    last_message: String,
}

impl VirtualMachine {
    pub fn new(name: &str, online: bool, project: &str, last_message: &str) -> Self {
        Self {
            name: name.to_string(),
            online,
            project: project.to_string(),
            last_message: last_message.to_string(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_online(&self) -> bool {
        self.online
    }

    pub fn project(&self) -> &str {
        &self.project
    }

    pub fn last_message(&self) -> &str {
        &self.last_message
    }
}

#[derive(Default)]
pub struct ChatState {
    messages: Vec<ChatMessage>,
}

impl ChatState {
    pub fn push_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
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

pub struct VmFleetState {
    virtual_machines: Vec<VirtualMachine>,
}

impl VmFleetState {
    pub fn virtual_machines(&self) -> &[VirtualMachine] {
        &self.virtual_machines
    }
}

impl Default for VmFleetState {
    fn default() -> Self {
        Self {
            virtual_machines: vec![
                VirtualMachine::new("vm-alpha", true, "Broker Ops", "Negotiated handshake 42s ago"),
                VirtualMachine::new("vm-bravo", true, "Discovery", "Captured new cluster manifest"),
                VirtualMachine::new(
                    "vm-charlie",
                    false,
                    "Shutdown",
                    "Graceful shutdown phase 2",
                ),
                VirtualMachine::new("vm-delta", true, "Image Prep", "Alpine profile verified"),
                VirtualMachine::new("vm-echo", true, "Ports Audit", "Ports 2201, 7722 active"),
                VirtualMachine::new(
                    "vm-foxtrot",
                    false,
                    "Recovery",
                    "Awaiting broker reachability",
                ),
                VirtualMachine::new("vm-golf", true, "Command Bridge", "Dispatched /status to core"),
                VirtualMachine::new("vm-hotel", true, "Observability", "Last log line 09:24:11"),
            ],
        }
    }
}

#[derive(Default)]
pub struct UiState {
    sidebar_visible: bool,
}

impl UiState {
    pub fn sidebar_visible(&self) -> bool {
        self.sidebar_visible
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
    }
}

pub struct AppState {
    chat: ChatState,
    roster: RosterState,
    vm_fleet: VmFleetState,
    ui: UiState,
}

impl AppState {
    pub fn new() -> Self {
        let mut state = Self {
            chat: ChatState::default(),
            roster: RosterState::default(),
            vm_fleet: VmFleetState::default(),
            ui: UiState::default(),
        };
        state.push_system_message("Welcome to Castra. Type /help to discover commands.");
        state
    }

    pub fn chat(&self) -> &ChatState {
        &self.chat
    }

    pub fn roster(&self) -> &RosterState {
        &self.roster
    }

    pub fn vm_fleet(&self) -> &VmFleetState {
        &self.vm_fleet
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
        let speaker = speaker.into();
        let text = text.into();
        self.chat.push_message(ChatMessage::new(speaker, text));
    }

    pub fn push_system_message<T: Into<String>>(&mut self, text: T) {
        self.push_message("SYSTEM", text);
    }

    pub fn push_user_command(&mut self, text: &str) {
        self.push_message("USER", text.to_string());
    }

    pub fn push_user_entry(&mut self, text: &str) {
        let target = self.roster.active_agent().label();
        let speaker = format!("USER→{}", target);
        self.push_message(speaker, text.to_string());
    }

    pub fn push_agent_echo(&mut self, text: &str) {
        let label = self.roster.active_agent().label();
        self.push_message(label, format!("You said: {}", text));
    }

    pub fn agent_index_by_id(&self, id: &str) -> Option<usize> {
        self.roster
            .agents()
            .iter()
            .position(|agent| agent.id().eq_ignore_ascii_case(id))
    }
}
