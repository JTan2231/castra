use std::ops::Range;

use crate::app::actions::{Backspace, CancelHistory, HistoryNext, HistoryPrev, SendMessage};
use gpui::{
    Bounds, Context, CursorStyle, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, InteractiveElement,
    IntoElement, LayoutId, MouseButton, MouseDownEvent, PaintQuad, ParentElement, Pixels, Point,
    Render, ShapedLine, SharedString, Style, Styled, TextRun, UTF16Selection, Window, div, fill,
    hsla, point, px, relative, rgb, size, white,
};
use unicode_segmentation::UnicodeSegmentation;

pub enum PromptEvent {
    Submitted(String),
}

pub struct PromptInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    cursor: usize,
    history: Vec<String>,
    history_index: Option<usize>,
    draft: Option<SharedString>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl EventEmitter<PromptEvent> for PromptInput {}

impl PromptInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: "enter command or /help".into(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
            draft: None,
            last_layout: None,
            last_bounds: None,
        }
    }

    pub fn move_cursor_to_end(&mut self) {
        self.cursor = self.content.len();
    }

    pub fn submit(&mut self, cx: &mut Context<Self>) -> bool {
        let trimmed = self.content.trim();
        if trimmed.is_empty() {
            return false;
        }
        let submitted = trimmed.to_owned();
        if self
            .history
            .last()
            .map(|last| last != &submitted)
            .unwrap_or(true)
        {
            self.history.push(submitted.clone());
        }
        self.history_index = None;
        self.draft = None;
        self.content = "".into();
        self.cursor = 0;
        cx.emit(PromptEvent::Submitted(submitted));
        cx.notify();
        true
    }

    fn backspace(&mut self, _: &Backspace, _window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_draft_before_edit();
        if self.cursor == 0 {
            return;
        }
        let start = self.previous_boundary(self.cursor);
        let mut updated = self.content.to_string();
        updated.replace_range(start..self.cursor, "");
        self.content = updated.into();
        self.cursor = start;
        cx.notify();
    }

    fn send_action(&mut self, _: &SendMessage, window: &mut Window, cx: &mut Context<Self>) {
        if self.submit(cx) {
            window.focus(&self.focus_handle);
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(bounds) = self.last_bounds else {
            return;
        };
        let Some(layout) = self.last_layout.as_ref() else {
            return;
        };

        let mut relative_x = event.position.x - bounds.left();
        if relative_x < px(0.) {
            relative_x = px(0.);
        }
        let max_x = layout.x_for_index(layout.len());
        if relative_x > max_x {
            relative_x = max_x;
        }

        let index = layout.closest_index_for_x(relative_x);
        self.cursor = index.min(self.content.len());
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn selection_range(&self) -> Range<usize> {
        self.cursor..self.cursor
    }

    fn bounds_for_range_impl(
        &self,
        range_utf16: Range<usize>,
    ) -> Option<(Bounds<Pixels>, ShapedLine)> {
        let bounds = self.last_bounds?;
        let layout = self.last_layout.clone()?;
        let range = self.range_from_utf16(&range_utf16);
        let left = layout.x_for_index(range.start);
        let right = layout.x_for_index(range.end);

        Some((
            Bounds::from_corners(
                point(bounds.left() + left, bounds.top()),
                point(bounds.left() + right, bounds.bottom()),
            ),
            layout,
        ))
    }

    fn store_current_as_draft(&mut self) {
        if self.draft.is_none() {
            self.draft = Some(self.content.clone());
        }
    }

    fn update_content_from_string(&mut self, text: String) {
        let len = text.len();
        self.content = text.into();
        self.cursor = len;
    }

    fn restore_draft(&mut self, cx: &mut Context<Self>) {
        if self.history_index.is_none() && self.draft.is_none() {
            return;
        }
        let draft = self.draft.take().unwrap_or_else(|| "".into());
        let len = draft.len();
        self.content = draft;
        self.cursor = len;
        self.history_index = None;
        cx.notify();
    }

    fn ensure_draft_before_edit(&mut self) {
        if self.history_index.is_some() {
            let draft = self.draft.clone().unwrap_or_else(|| "".into());
            let len = draft.len();
            self.content = draft;
            self.cursor = len;
            self.history_index = None;
            self.draft = None;
        }
    }

    fn history_prev(&mut self, _: &HistoryPrev, _window: &mut Window, cx: &mut Context<Self>) {
        if self.history.is_empty() {
            return;
        }

        let next_index = match self.history_index {
            None => {
                self.store_current_as_draft();
                Some(self.history.len() - 1)
            }
            Some(0) => Some(0),
            Some(idx) => Some(idx - 1),
        };

        if let Some(index) = next_index {
            if let Some(entry) = self.history.get(index).cloned() {
                self.history_index = Some(index);
                self.update_content_from_string(entry);
                cx.notify();
            }
        }
    }

    fn history_next(&mut self, _: &HistoryNext, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(current) = self.history_index else {
            if self.draft.is_some() {
                self.restore_draft(cx);
            }
            return;
        };

        if current + 1 >= self.history.len() {
            self.restore_draft(cx);
            return;
        }

        let index = current + 1;
        if let Some(entry) = self.history.get(index).cloned() {
            self.history_index = Some(index);
            self.update_content_from_string(entry);
            cx.notify();
        }
    }

    fn cancel_history(&mut self, _: &CancelHistory, _window: &mut Window, cx: &mut Context<Self>) {
        self.restore_draft(cx);
    }
}

impl Focusable for PromptInput {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for PromptInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let range = self.selection_range();
        Some(UTF16Selection {
            range: self.range_to_utf16(&range),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_draft_before_edit();
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .unwrap_or_else(|| self.selection_range());
        let mut updated = self.content.to_string();
        updated.replace_range(range.clone(), new_text);
        self.content = updated.into();
        self.cursor = range.start + new_text.len();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range(range_utf16, new_text, window, cx);
        if let Some(range_utf16) = new_selected_range_utf16 {
            let range = self.range_from_utf16(&range_utf16);
            self.cursor = range.end.min(self.content.len());
            cx.notify();
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if let Some((bounds, _)) = self.bounds_for_range_impl(range_utf16.clone()) {
            Some(bounds)
        } else {
            Some(element_bounds)
        }
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let layout = self.last_layout.as_ref()?;
        let mut x = point.x - bounds.left();
        if x < px(0.) {
            x = px(0.);
        }
        let index = layout.closest_index_for_x(x);
        Some(self.offset_to_utf16(index))
    }
}

struct PromptTextElement {
    input: Entity<PromptInput>,
}

impl IntoElement for PromptTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

struct PromptPrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
}

impl Element for PromptTextElement {
    type RequestLayoutState = ();
    type PrepaintState = PromptPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let cursor = input.cursor;
        let style = window.text_style();

        let (display_text, text_color) = if input.content.is_empty() {
            (input.placeholder.clone(), hsla(0., 0., 1., 0.4))
        } else {
            (input.content.clone(), style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &[run], None);

        let cursor_pos = line.x_for_index(cursor);
        let cursor_quad = fill(
            Bounds::new(
                point(bounds.left() + cursor_pos, bounds.top()),
                size(px(2.), bounds.bottom() - bounds.top()),
            ),
            white(),
        );

        PromptPrepaintState {
            line: Some(line),
            cursor: Some(cursor_quad),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        if let Some(cursor) = prepaint.cursor.take() {
            window.paint_quad(cursor);
        }

        if let Some(line) = prepaint.line.take() {
            line.paint(bounds.origin, window.line_height(), window, cx)
                .unwrap();

            self.input.update(cx, |input, _cx| {
                input.last_layout = Some(line);
                input.last_bounds = Some(bounds);
            });
        }
    }
}

impl Render for PromptInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .w_full()
            .items_center()
            .gap_3()
            .key_context("ChatInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .text_sm()
            .px(px(12.))
            .py(px(10.))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::send_action))
            .on_action(cx.listener(Self::history_prev))
            .on_action(cx.listener(Self::history_next))
            .on_action(cx.listener(Self::cancel_history))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .child(div().text_color(rgb(0x8a8a8a)).child(">"))
            .child(
                div()
                    .flex_grow()
                    .child(PromptTextElement { input: cx.entity() }),
            )
    }
}
