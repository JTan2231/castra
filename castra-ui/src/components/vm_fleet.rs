use crate::state::{VirtualMachine, VmFleetState};
use gpui::{div, px, prelude::*, rgb, Styled};

pub fn vm_card(vm: &VirtualMachine) -> gpui::Div {
    let indicator_color = if vm.is_online() {
        rgb(0x2f9b4b)
    } else {
        rgb(0x9b3f3f)
    };
    let status_label = if vm.is_online() { "ONLINE" } else { "OFFLINE" };

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
                .text_color(rgb(0x8c8c8c))
                .child(format!("Project: {}", vm.project())),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x707070))
                .child(format!("Last: {}", vm.last_message())),
        )
}

pub fn vm_columns(vm_fleet: &VmFleetState) -> (Vec<gpui::Div>, Vec<gpui::Div>) {
    let mut cards: Vec<_> = vm_fleet
        .virtual_machines()
        .iter()
        .map(vm_card)
        .collect();

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
