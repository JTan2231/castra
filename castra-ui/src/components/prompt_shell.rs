use crate::input::prompt::PromptInput;
use gpui::{Entity, Styled, div, prelude::*, px, rgb};

pub fn prompt_container(
    prompt: &Entity<PromptInput>,
    stop_control: Option<gpui::Div>,
) -> gpui::Div {
    let mut container = div()
        .bg(rgb(0x050505))
        .px(px(18.))
        .py(px(12.))
        .flex()
        .items_center()
        .gap(px(12.));

    if let Some(control) = stop_control {
        container = container.child(control);
    }

    container.child(div().flex_grow().child(prompt.clone()))
}
