use crate::state::{ChatState, MessageKind};
use gpui::{
    App, CursorStyle, Div, MouseButton, MouseDownEvent, Styled, Window, div, hsla, list,
    prelude::*, px, rgb,
};
use std::sync::Arc;

const SPEAKER_COLUMN_WIDTH_PX: f32 = 140.;
const INDICATOR_COLUMN_WIDTH_PX: f32 = 18.;

pub type MessageToggleHandler = Arc<dyn Fn(&MouseDownEvent, &mut Window, &mut App) + 'static>;

pub fn message_log(
    chat: &ChatState,
    toggle_handlers: Arc<Vec<Option<MessageToggleHandler>>>,
) -> impl IntoElement {
    let mut content = div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .flex_grow()
        .min_h(px(0.));

    if chat.dropped_messages() > 0 {
        content = content.child(truncation_notice_row(chat.dropped_messages()));
    }

    if chat.messages().is_empty() {
        content = content.child(placeholder_row());
    } else {
        let messages = chat.messages().to_vec();
        let list_state = chat.list_state().clone();
        let handlers = toggle_handlers.clone();

        let list = list(list_state, move |index, _window, _app| {
            let handler = handlers.get(index).and_then(|entry| entry.clone());
            render_message_row(&messages[index], handler).into_any_element()
        })
        .flex()
        .flex_col()
        .gap(px(6.))
        .flex_grow()
        .min_h(px(0.))
        .w_full();

        content = content.child(list);
    }

    div()
        .id("message-log")
        .flex()
        .flex_col()
        .flex_grow()
        .min_h(px(0.))
        .px(px(18.))
        .py(px(16.))
        .child(content)
}

fn placeholder_row() -> Div {
    div()
        .w_full()
        .flex()
        .items_start()
        .gap(px(10.))
        .text_sm()
        .px(px(6.))
        .py(px(6.))
        .text_color(rgb(0x5c5c5c))
        .child(div().w(px(INDICATOR_COLUMN_WIDTH_PX)))
        .child(div().child("[--:--:--]"))
        .child(div().w(px(SPEAKER_COLUMN_WIDTH_PX)).child("[SYSTEM]"))
        .child(div().child("Awaiting input..."))
}

fn render_message_row(
    message: &crate::state::ChatMessage,
    handler: Option<MessageToggleHandler>,
) -> Div {
    let kind = message.kind();
    let is_collapsible = message.is_collapsible();
    let is_expanded = message.is_expanded();
    let collapsed = is_collapsible && !is_expanded;
    let (speaker_color, text_color, accent_color, background_color) = match kind {
        MessageKind::System => (
            rgb(0xf6c177),
            rgb(0xf9d8a7),
            rgb(0xf6c177),
            if collapsed {
                Some(hsla(32., 0.6, 0.2, 0.4))
            } else {
                Some(hsla(32., 0.45, 0.12, 0.35))
            },
        ),
        MessageKind::Reasoning => (
            rgb(0xa8b6ff),
            rgb(0xc3d0ff),
            rgb(0xa8b6ff),
            if collapsed {
                Some(hsla(225., 0.65, 0.16, 0.45))
            } else {
                Some(hsla(225., 0.45, 0.11, 0.4))
            },
        ),
        MessageKind::Tool => (
            rgb(0x7ddac6),
            rgb(0xa0e4d3),
            rgb(0x7ddac6),
            if collapsed {
                Some(hsla(160., 0.6, 0.14, 0.42))
            } else {
                Some(hsla(160., 0.45, 0.1, 0.38))
            },
        ),
        MessageKind::VizierCommand => (
            rgb(0xcbb3ff),
            rgb(0xdcc8ff),
            rgb(0xcbb3ff),
            if collapsed {
                Some(hsla(260., 0.55, 0.19, 0.42))
            } else {
                Some(hsla(260., 0.4, 0.14, 0.36))
            },
        ),
        MessageKind::User => (
            rgb(0xffa7c4),
            rgb(0xffc0d4),
            rgb(0xffa7c4),
            Some(hsla(340., 0.4, 0.15, 0.3)),
        ),
        MessageKind::Agent | MessageKind::Other => (
            rgb(0xf0f0f0),
            rgb(0xc8c8c8),
            rgb(0x7f7f7f),
            Some(hsla(0., 0., 0.08, 0.4)),
        ),
    };
    let indicator = if is_collapsible {
        if is_expanded { "▼" } else { "▶" }
    } else {
        ""
    };

    let mut row = div()
        .w_full()
        .flex()
        .items_start()
        .gap(px(10.))
        .text_sm()
        .py(px(6.))
        .px(px(6.));

    if let Some(bg) = background_color {
        row = row
            .bg(bg)
            .rounded(px(6.))
            .border(px(1.))
            .border_color(hsla(0., 0., 0.3, 0.35));
    }

    let mut content = if collapsed {
        message
            .collapsed_preview()
            .cloned()
            .unwrap_or_else(|| message.text().clone())
    } else {
        message.text().clone()
    };

    let mut content_color = text_color;
    let mut content_div = div()
        .w_full()
        .flex_1()
        .max_w_full()
        .min_w(px(0.))
        .whitespace_normal();

    if collapsed {
        content_color = accent_color;
        content_div = content_div.italic();
    }

    if content.is_empty() {
        content = "(no output)".into();
    }

    row = row
        .child(
            div()
                .w(px(INDICATOR_COLUMN_WIDTH_PX))
                .flex()
                .justify_center()
                .text_color(accent_color)
                .child(indicator.to_string()),
        )
        .child(
            div()
                .text_color(rgb(0x6a6a6a))
                .child(format!("[{}]", message.timestamp())),
        )
        .child(
            div()
                .w(px(SPEAKER_COLUMN_WIDTH_PX))
                .text_color(speaker_color)
                .child(format!("[{}]", message.speaker())),
        )
        .child(content_div.text_color(content_color).child(content));

    if is_collapsible {
        if let Some(handler) = handler {
            let handler = handler.clone();
            row = row
                .on_mouse_down(MouseButton::Left, move |event, window, app| {
                    handler(event, window, app);
                })
                .cursor(CursorStyle::PointingHand);
        }
    }

    row
}

fn truncation_notice_row(dropped: usize) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .py(px(4.))
        .rounded(px(4.))
        .bg(hsla(240., 0.2, 0.1, 0.45))
        .border(px(1.))
        .border_color(hsla(240., 0.25, 0.2, 0.4))
        .text_xs()
        .text_color(rgb(0xcad3ff))
        .child(format!(
            "{dropped} older message{} hidden to keep the log responsive",
            if dropped == 1 { "" } else { "s" }
        ))
}
