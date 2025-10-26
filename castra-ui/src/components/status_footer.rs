use chrono::Local;
use gpui::{Styled, div, prelude::*, px, rgb};

pub fn status_footer(active_label: &str, operation_status: &str) -> gpui::Div {
    let status_time = Local::now().format("%H:%M:%S").to_string();
    let (focus_hint, agent_hint, roster_hint) = if cfg!(target_os = "macos") {
        ("Cmd+K focus", "Cmd+1-3 agents", "Cmd+B roster")
    } else {
        ("Ctrl+L focus", "Ctrl+1-3 agents", "Ctrl+B roster")
    };
    let status_hint = format!(
        "{} • Enter ↵ to send • {} • {} • {} • ↑/↓ history • /help",
        operation_status, focus_hint, agent_hint, roster_hint
    );

    div()
        .flex()
        .justify_between()
        .items_center()
        .px(px(18.))
        .py(px(10.))
        .text_xs()
        .text_color(rgb(0x8a8a8a))
        .child(div().child(format!("Active: {}", active_label)))
        .child(div().child(status_time))
        .child(div().child(status_hint))
}
