use crate::input::prompt::PromptInput;
use gpui::{div, px, prelude::*, rgb, Entity, Styled};

pub fn prompt_container(prompt: &Entity<PromptInput>) -> gpui::Div {
    div()
        .bg(rgb(0x050505))
        .px(px(18.))
        .py(px(12.))
        .child(prompt.clone())
}
