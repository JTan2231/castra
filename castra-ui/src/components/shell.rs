use crate::{
    input::prompt::PromptInput,
    state::{AppState, RosterState},
};
use gpui::{Entity, Styled, div, prelude::*, px, rgb};

use super::{
    message_log::message_log,
    prompt_shell::prompt_container,
    roster_sidebar::{agent_row, sidebar_container},
    status_footer::status_footer,
    vm_fleet::{vm_column_container, vm_columns},
};

pub fn render(
    state: &AppState,
    prompt: &Entity<PromptInput>,
    roster_rows: Option<Vec<gpui::Div>>,
) -> gpui::Div {
    let operation_status = state.up_status_line();
    let (vm_left_cards, vm_right_cards) = vm_columns(state.vm_fleet());

    let log_container = div()
        .flex()
        .flex_col()
        .flex_grow()
        .min_w(px(0.))
        .child(message_log(state.chat()));

    let mut central_shell = div().flex().flex_grow().min_w(px(0.));

    if let Some(rows) = roster_rows {
        central_shell = central_shell
            .child(sidebar_container(rows))
            .child(div().w(px(1.)).bg(rgb(0x1e1e1e)));
    }

    central_shell = central_shell.child(log_container);

    let mut upper_shell = div().flex().flex_grow().bg(rgb(0x050505));

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

    div()
        .bg(rgb(0x000000))
        .text_color(rgb(0xf5f5f5))
        .flex()
        .flex_col()
        .size_full()
        .p(px(20.))
        .font_family("Menlo")
        .child(upper_shell)
        .child(div().h(px(1.)).bg(rgb(0x1e1e1e)))
        .child(status_footer(
            &state.active_agent_label(),
            &operation_status,
        ))
        .child(div().h(px(1.)).bg(rgb(0x1e1e1e)))
        .child(prompt_container(prompt))
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
