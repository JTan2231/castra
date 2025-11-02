use crate::{
    input::prompt::PromptInput,
    state::{AppState, RosterState},
};
use gpui::{Entity, Styled, div, hsla, prelude::*, px, rgb};

use super::{
    message_log::message_log,
    prompt_shell::prompt_container,
    roster_sidebar::{agent_row, sidebar_container},
    status_footer::status_footer,
    vm_fleet::{vm_column_container, vm_columns},
};

pub fn render<H, F>(
    state: &AppState,
    prompt: &Entity<PromptInput>,
    roster_rows: Option<Vec<gpui::Div>>,
    toasts: &[String],
    mut message_toggle: F,
) -> gpui::Div
where
    F: FnMut(usize) -> Option<H>,
    H: Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static,
{
    let operation_status = state.up_status_line();
    let focused_label = state.focused_vm_label();
    let (vm_left_cards, vm_right_cards) = vm_columns(state.vm_fleet());

    let log_container = div()
        .flex()
        .flex_col()
        .flex_grow()
        .min_h(px(0.))
        .min_w(px(0.))
        .child(message_log(state.chat(), |index| message_toggle(index)));

    let mut central_shell = div().flex().flex_grow().min_w(px(0.)).min_h(px(0.));

    if let Some(rows) = roster_rows {
        central_shell = central_shell
            .child(sidebar_container(rows))
            .child(div().w(px(1.)).bg(rgb(0x1e1e1e)));
    }

    central_shell = central_shell.child(log_container);

    let mut upper_shell = div().flex().flex_grow().min_h(px(0.)).bg(rgb(0x050505));

    if !vm_left_cards.is_empty() {
        let left_column = vm_column_container(vm_left_cards);
        upper_shell = upper_shell
            .child(left_column)
            .child(div().w(px(1.)).bg(rgb(0x1e1e1e)));
    }

    upper_shell = upper_shell.child(central_shell);

    if !vm_right_cards.is_empty() {
        let right_column = vm_column_container(vm_right_cards);
        upper_shell = upper_shell
            .child(div().w(px(1.)).bg(rgb(0x1e1e1e)))
            .child(right_column);
    }

    let mut root = div()
        .bg(rgb(0x000000))
        .text_color(rgb(0xf5f5f5))
        .flex()
        .flex_col()
        .size_full()
        .p(px(20.))
        .font_family("Menlo")
        .child(upper_shell);

    if let Some(strip) = toast_strip(toasts) {
        root = root.child(strip);
    }

    root = root
        .child(div().h(px(1.)).bg(rgb(0x1e1e1e)))
        .child(status_footer(
            &state.active_agent_label(),
            focused_label.as_deref(),
            &operation_status,
        ))
        .child(div().h(px(1.)).bg(rgb(0x1e1e1e)))
        .child(prompt_container(prompt));

    root
}

pub fn roster_rows<H, F>(roster: &RosterState, mut attach_handler: F) -> Vec<gpui::Div>
where
    F: FnMut(usize) -> H,
    H: Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static,
{
    roster
        .agents()
        .iter()
        .enumerate()
        .map(|(index, agent)| {
            let is_active = index == roster.active_index();
            let mut row = agent_row(agent, is_active);
            let handler = attach_handler(index);
            row = row.on_mouse_down(gpui::MouseButton::Left, handler);
            row
        })
        .collect()
}

fn toast_strip(toasts: &[String]) -> Option<gpui::Div> {
    if toasts.is_empty() {
        return None;
    }

    let chips: Vec<_> = toasts
        .iter()
        .map(|message| {
            div()
                .bg(hsla(0., 0., 0.15, 0.7))
                .border(px(1.))
                .border_color(hsla(0., 0., 0.4, 0.4))
                .px(px(10.))
                .py(px(6.))
                .rounded(px(4.))
                .text_xs()
                .child(message.to_string())
        })
        .collect();

    Some(
        div()
            .flex()
            .justify_end()
            .gap(px(8.))
            .px(px(18.))
            .py(px(8.))
            .children(chips),
    )
}
