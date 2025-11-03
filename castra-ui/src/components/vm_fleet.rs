use crate::state::{
    AttentionLevel, CatalogEntryView, ConfigCatalogView, ConfigSource, VirtualMachine, VmFleetState,
};
use gpui::{
    Background, BoxShadow, CursorStyle, MouseButton, Styled, div, hsla, point, prelude::*, px, rgb,
};

pub fn vm_card(vm: &VirtualMachine, is_focused: bool) -> gpui::Div {
    let indicator_color = attention_color(vm.attention());
    let status_label = vm.phase().label();
    let background: Background = if is_focused {
        hsla(0.58, 0.65, 0.22, 0.35).into()
    } else {
        rgb(0x080808).into()
    };
    let mut card = div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .px(px(12.))
        .py(px(10.))
        .bg(background)
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
                        .child(div().w(px(6.)).h(px(6.)).bg(indicator_color).shadow(
                            if is_focused {
                                vec![BoxShadow {
                                    color: hsla(0.58, 0.65, 0.55, 0.35),
                                    offset: point(px(0.), px(0.)),
                                    blur_radius: px(6.),
                                    spread_radius: px(2.),
                                }]
                            } else {
                                Vec::new()
                            },
                        ))
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
        );

    if is_focused {
        card = card
            .border(px(1.))
            .border_color(rgb(0x3a7bd5))
            .shadow(vec![BoxShadow {
                color: hsla(0.58, 0.7, 0.5, 0.3),
                offset: point(px(0.), px(0.)),
                blur_radius: px(10.),
                spread_radius: px(0.),
            }]);
    }

    card
}

pub fn catalog_cards<F, H>(view: &ConfigCatalogView, mut attach_handler: F) -> Vec<gpui::Div>
where
    F: FnMut(usize) -> Option<H>,
    H: Fn(&gpui::MouseDownEvent, &mut gpui::Window, &mut gpui::App) + 'static,
{
    let mut cards: Vec<gpui::Div> = view
        .entries
        .iter()
        .map(|entry| {
            let mut card = config_card(entry);
            if entry.is_disabled {
                card = card.opacity(0.6).cursor(CursorStyle::Arrow);
            } else if let Some(handler) = attach_handler(entry.index) {
                card = card
                    .cursor(CursorStyle::PointingHand)
                    .on_mouse_down(MouseButton::Left, handler);
            } else {
                card = card.cursor(CursorStyle::Arrow);
            }
            card
        })
        .collect();

    if let Some(hint) = view.hint.as_ref() {
        cards.push(hint_card(hint));
    }

    if let Some(error) = view.last_error.as_ref() {
        cards.push(error_card(error));
    }

    if cards.is_empty() {
        cards.push(empty_catalog_card());
    }

    cards
}

fn config_card(entry: &CatalogEntryView) -> gpui::Div {
    let indicator_color = match (
        entry.discovery_error.is_some(),
        entry.last_failure.is_some(),
        entry.is_selected,
    ) {
        (true, _, _) => rgb(0x9b3f3f),
        (false, true, _) => rgb(0x9b6a3f),
        (false, false, true) => rgb(0x3a7bd5),
        _ => rgb(0x5c5c5c),
    };
    let background: Background = if entry.is_selected {
        hsla(0.58, 0.65, 0.22, 0.35).into()
    } else {
        rgb(0x080808).into()
    };
    let source_label = match entry.source {
        ConfigSource::Quickstart => "QUICKSTART",
        ConfigSource::Catalog => "CONFIG",
    };
    let detail_line = entry
        .summary
        .clone()
        .filter(|line| !line.is_empty())
        .or_else(|| {
            if entry.discovery_error.is_none() && entry.last_failure.is_none() {
                Some("Ready to launch".to_string())
            } else {
                None
            }
        });
    let status_line = if let Some(failure) = &entry.last_failure {
        Some(format!("Launch failed — {}", truncate_line(failure, 96)))
    } else if let Some(error) = &entry.discovery_error {
        Some(format!("Config error — {}", truncate_line(error, 96)))
    } else {
        None
    };

    let mut card = div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .px(px(12.))
        .py(px(10.))
        .bg(background)
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
                                .child(entry.display_name.clone()),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x7a7a7a))
                        .child(source_label),
                ),
        );

    if let Some(line) = detail_line {
        card = card.child(div().text_xs().text_color(rgb(0x9a9a9a)).child(line));
    }

    if let Some(line) = status_line {
        card = card.child(div().text_xs().text_color(rgb(0xd87a5c)).child(line));
    }

    if entry.is_selected {
        card = card
            .border(px(1.))
            .border_color(rgb(0x3a7bd5))
            .shadow(vec![BoxShadow {
                color: hsla(0.58, 0.7, 0.5, 0.3),
                offset: point(px(0.), px(0.)),
                blur_radius: px(10.),
                spread_radius: px(0.),
            }]);
    }

    card
}

fn empty_catalog_card() -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .px(px(12.))
        .py(px(10.))
        .bg(rgb(0x080808))
        .border(px(1.))
        .border_color(rgb(0x1e1e1e))
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xf0f0f0))
                .child("No configs discovered."),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x9a9a9a))
                .child("Use the CLI to launch or add castra.toml files to the catalog path."),
        )
}

fn hint_card(hint: &str) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .px(px(12.))
        .py(px(10.))
        .bg(rgb(0x040404))
        .border(px(1.))
        .border_color(rgb(0x2a3a2a))
        .child(div().text_sm().text_color(rgb(0xb4dba3)).child("Hint"))
        .child(
            div()
                .text_xs()
                .text_color(rgb(0x9a9a9a))
                .child(hint.to_string()),
        )
}

fn error_card(message: &str) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .px(px(12.))
        .py(px(10.))
        .bg(rgb(0x120707))
        .border(px(1.))
        .border_color(rgb(0x4a1a1a))
        .child(
            div()
                .text_sm()
                .text_color(rgb(0xff8c8c))
                .child("Catalog error"),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(0xffb1b1))
                .child(truncate_line(message, 120)),
        )
}

fn truncate_line(text: &str, max_len: usize) -> String {
    let mut truncated = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_len {
            truncated.push('…');
            break;
        }
        truncated.push(ch);
    }
    truncated
}

pub fn vm_columns(vm_fleet: &VmFleetState) -> (Vec<gpui::Div>, Vec<gpui::Div>) {
    let focused = vm_fleet.focused_index();
    let mut cards: Vec<_> = vm_fleet
        .virtual_machines()
        .iter()
        .enumerate()
        .map(|(index, vm)| vm_card(vm, focused == Some(index)))
        .collect();

    let split_at = (cards.len() + 1) / 2;
    let right_cards = if split_at < cards.len() {
        cards.split_off(split_at)
    } else {
        Vec::new()
    };

    (cards, right_cards)
}

pub fn vm_column_container(title: &str, cards: Vec<gpui::Div>) -> gpui::Div {
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
                .child(title.to_string()),
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
