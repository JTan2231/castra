use crate::state::ChatState;
use gpui::{div, px, prelude::*, rgb, Styled};

const SPEAKER_COLUMN_WIDTH_PX: f32 = 140.;

pub fn message_log(chat: &ChatState) -> impl IntoElement {
    let rows: Vec<_> = if chat.messages().is_empty() {
        vec![div()
            .w_full()
            .text_sm()
            .text_color(rgb(0x5c5c5c))
            .child("[--:--:--] [SYSTEM] Awaiting input...")]
    } else {
        chat
            .messages()
            .iter()
            .map(|message| {
                div()
                    .w_full()
                    .flex()
                    .gap(px(12.))
                    .text_sm()
                    .text_color(rgb(0xeaeaea))
                    .child(
                        div()
                            .text_color(rgb(0x6a6a6a))
                            .child(format!("[{}]", message.timestamp())),
                    )
                    .child(
                        div()
                            .w(px(SPEAKER_COLUMN_WIDTH_PX))
                            .text_color(rgb(0xf0f0f0))
                            .child(format!("[{}]", message.speaker())),
                    )
                    .child(
                        div()
                            .flex_grow()
                            .text_color(rgb(0xc8c8c8))
                            .child(message.text().clone()),
                    )
            })
            .collect()
    };

    div()
        .id("message-log")
        .flex()
        .flex_col()
        .flex_grow()
        .overflow_y_scroll()
        .px(px(18.))
        .py(px(16.))
        .child(div().flex().flex_col().gap(px(6.)).children(rows))
}
