use crate::input::prompt::PromptInput;
use gpui::{Entity, Styled, div, prelude::*, px, rgb};

pub fn prompt_container(prompt: &Entity<PromptInput>) -> gpui::Div {
    div()
        .bg(rgb(0x050505))
        .px(px(18.))
        .py(px(12.))
        .child(prompt.clone())
}
