use crate::state::Agent;
use gpui::{Styled, div, prelude::*, px, rgb};

pub fn agent_row(agent: &Agent, is_active: bool) -> gpui::Div {
    div()
        .flex()
        .justify_between()
        .items_center()
        .px(px(12.))
        .py(px(8.))
        .gap(px(12.))
        .text_sm()
        .bg(if is_active {
            rgb(0x0d0d0d)
        } else {
            rgb(0x050505)
        })
        .text_color(if is_active {
            rgb(0xf0f0f0)
        } else {
            rgb(0xb8b8b8)
        })
        .child(div().child(agent.label()))
        .child(
            div()
                .text_xs()
                .text_color(if is_active {
                    rgb(0xcccccc)
                } else {
                    rgb(0x707070)
                })
                .child(agent.status().to_string()),
        )
}

pub fn sidebar_container(rows: Vec<gpui::Div>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .w(px(200.))
        .bg(rgb(0x050505))
        .child(
            div()
                .px(px(16.))
                .py(px(10.))
                .text_xs()
                .text_color(rgb(0x6a6a6a))
                .child("AGENTS"),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .px(px(8.))
                .py(px(12.))
                .children(rows),
        )
}
