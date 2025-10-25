pub mod actions;

use crate::{
    components::shell::{render as render_shell, roster_rows},
    controller::command,
    input::prompt::{PromptEvent, PromptInput},
    state::AppState,
};
use gpui::{Context, Entity, FocusHandle, Focusable, IntoElement, MouseDownEvent, Render, Window};

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
                self.handle_submission(text);
                cx.notify();
            }
        }
    }

    fn handle_submission(&mut self, text: &str) {
        if text.starts_with('/') {
            self.handle_command(text);
        } else {
            self.handle_plain_text(text);
        }
    }

    fn handle_plain_text(&mut self, text: &str) {
        self.state.push_user_entry(text);
        self.state.push_agent_echo(text);
    }

    fn handle_command(&mut self, text: &str) {
        self.state.push_user_command(text);
        command::handle(text, &mut self.state);
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
