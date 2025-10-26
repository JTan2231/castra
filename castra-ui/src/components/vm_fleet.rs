use crate::state::{AttentionLevel, VirtualMachine, VmFleetState};
use gpui::{Styled, div, prelude::*, px, rgb};

pub fn vm_card(vm: &VirtualMachine) -> gpui::Div {
    let indicator_color = attention_color(vm.attention());
    let status_label = vm.phase().label();

    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .bg(rgb(0x080808))
        .px(px(12.))
        .py(px(10.))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().w(px(6.)).h(px(6.)).bg(indicator_color))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xf0f0f0))
                                .child(vm.name().to_uppercase()),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x7a7a7a))
                        .child(status_label),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x9a9a9a))
                .child(vm.detail().to_string()),
        )
}

pub fn vm_columns(vm_fleet: &VmFleetState) -> (Vec<gpui::Div>, Vec<gpui::Div>) {
    let mut cards: Vec<_> = vm_fleet.virtual_machines().iter().map(vm_card).collect();

    let split_at = (cards.len() + 1) / 2;
    let right_cards = if split_at < cards.len() {
        cards.split_off(split_at)
    } else {
        Vec::new()
    };

    (cards, right_cards)
}

pub fn vm_column_container(cards: Vec<gpui::Div>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .w(px(240.))
        .bg(rgb(0x050505))
        .child(
            div()
                .px(px(16.))
                .py(px(10.))
                .text_xs()
                .text_color(rgb(0x6a6a6a))
                .child("VM FLEET"),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .px(px(12.))
                .py(px(12.))
                .children(cards),
        )
}

fn attention_color(level: AttentionLevel) -> gpui::Rgba {
    match level {
        AttentionLevel::Idle => rgb(0x5c5c5c),
        AttentionLevel::Progress => rgb(0x3a7bd5),
        AttentionLevel::Success => rgb(0x2f9b4b),
        AttentionLevel::Error => rgb(0x9b3f3f),
    }
}
