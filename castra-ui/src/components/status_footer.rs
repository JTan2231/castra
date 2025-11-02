use chrono::Local;
use gpui::{Styled, div, prelude::*, px, rgb};

pub fn status_footer(
    active_label: &str,
    focused_label: Option<&str>,
    operation_status: &str,
    token_summaries: &[String],
) -> gpui::Div {
    let status_time = Local::now().format("%H:%M:%S").to_string();
    let (prompt_focus_hint, agent_hint, roster_hint) = if cfg!(target_os = "macos") {
        ("Cmd+K focus prompt", "Cmd+1-3 agents", "Cmd+B roster")
    } else {
        ("Ctrl+L focus prompt", "Ctrl+1-3 agents", "Ctrl+B roster")
    };
    let focus_next_hint = "Tab focus next";
    let focus_prev_hint = "Shift+Tab focus prev";
    let status_hint = format!(
        "{} • Enter ↵ to send • {} • {} • {} • {} • {} • ↑/↓ history • /help",
        operation_status,
        focus_next_hint,
        focus_prev_hint,
        prompt_focus_hint,
        agent_hint,
        roster_hint
    );
    let focused_label = focused_label.unwrap_or("None");
    let tokens_text = if token_summaries.is_empty() {
        None
    } else {
        Some(token_summaries.join(" • "))
    };

    div()
        .flex()
        .justify_between()
        .items_center()
        .px(px(18.))
        .py(px(10.))
        .text_xs()
        .text_color(rgb(0x8a8a8a))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .child(div().child(format!("Active: {}", active_label)))
                .child(div().child(format!("Focused VM: {}", focused_label))),
        )
        .child({
            let mut center = div().flex().items_center().gap(px(10.));
            if let Some(tokens) = tokens_text {
                center = center.child(div().child(tokens));
            }
            center.child(div().child(status_time))
        })
        .child(div().text_right().child(status_hint))
}
