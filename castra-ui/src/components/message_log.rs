use crate::state::{ChatState, MessageKind};
use gpui::{
    App, CursorStyle, Div, MouseButton, MouseDownEvent, Styled, Window, div, hsla, prelude::*, px,
    rgb,
};

const SPEAKER_COLUMN_WIDTH_PX: f32 = 140.;
const INDICATOR_COLUMN_WIDTH_PX: f32 = 18.;
const MAX_PREVIEW_CHARS: usize = 80;

pub fn message_log<H, F>(chat: &ChatState, mut attach_handler: F) -> impl IntoElement
where
    F: FnMut(usize) -> Option<H>,
    H: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
{
    let rows: Vec<Div> = if chat.messages().is_empty() {
        vec![placeholder_row()]
    } else {
        chat.messages()
            .iter()
            .enumerate()
            .map(|(index, message)| {
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

                let mut content = if is_collapsible && !is_expanded {
                    collapsed_preview(message)
                } else {
                    message.text().clone()
                };

                // Use accent copy for collapsed summaries so they stand out.
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

                // Ensure empty content still occupies the space cleanly.
                if content.is_empty() {
                    content = "(no output)".into();
                }

                let mut row = row
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
                    if let Some(handler) = attach_handler(index) {
                        row = row
                            .on_mouse_down(MouseButton::Left, handler)
                            .cursor(CursorStyle::PointingHand);
                    }
                }

                row
            })
            .collect()
    };

    div()
        .id("message-log")
        .flex()
        .flex_col()
        .flex_grow()
        .min_h(px(0.))
        .overflow_y_scroll()
        .track_scroll(chat.scroll_handle())
        .px(px(18.))
        .py(px(16.))
        .child(div().flex().flex_col().gap(px(6.)).children(rows))
}

fn collapsed_preview(message: &crate::state::ChatMessage) -> gpui::SharedString {
    let label = message.kind().display_name();
    let preview_line = message
        .text()
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");

    let mut summary = String::new();
    if !preview_line.is_empty() {
        let mut collected = String::new();
        for ch in preview_line.chars().take(MAX_PREVIEW_CHARS) {
            collected.push(ch);
        }
        if preview_line.chars().count() > MAX_PREVIEW_CHARS {
            collected.push('…');
        }
        summary.push_str(&collected);
        summary.push(' ');
    }
    summary.push('(');
    summary.push_str(label);
    summary.push_str(" hidden — click to expand)");
    summary.into()
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
