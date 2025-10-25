mod app;
mod components;
mod controller;
mod input;
mod state;

use app::{actions::*, ChatApp};
use gpui::{App, AppContext, Application, Bounds, KeyBinding, WindowBounds, WindowHandle, WindowOptions, px, size};
use input::prompt::PromptInput;

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(420.), px(640.)), cx);
        cx.bind_keys([
            KeyBinding::new("backspace", Backspace, None),
            KeyBinding::new("enter", SendMessage, None),
            KeyBinding::new("up", HistoryPrev, None),
            KeyBinding::new("down", HistoryNext, None),
            KeyBinding::new("escape", CancelHistory, None),
            KeyBinding::new("cmd-k", FocusPrompt, None),
            KeyBinding::new("ctrl-l", FocusPrompt, None),
            KeyBinding::new("cmd-b", ToggleSidebar, None),
            KeyBinding::new("ctrl-b", ToggleSidebar, None),
            KeyBinding::new("cmd-1", SwitchAgent1, None),
            KeyBinding::new("cmd-2", SwitchAgent2, None),
            KeyBinding::new("cmd-3", SwitchAgent3, None),
            KeyBinding::new("ctrl-1", SwitchAgent1, None),
            KeyBinding::new("ctrl-2", SwitchAgent2, None),
            KeyBinding::new("ctrl-3", SwitchAgent3, None),
        ]);

        let window: WindowHandle<ChatApp> = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_, cx| {
                    let prompt = cx.new(PromptInput::new);
                    cx.new(|cx| {
                        cx.subscribe(&prompt, |chat: &mut ChatApp, _, event, cx| {
                            chat.on_prompt_event(event, cx);
                        })
                        .detach();
                        ChatApp::new(prompt.clone())
                    })
                },
            )
            .unwrap();

        {
            let window_handle = window.clone();
            cx.on_action(move |_: &ToggleSidebar, cx| {
                window_handle
                    .update(cx, |chat, _window, cx| {
                        chat.toggle_sidebar(cx);
                    })
                    .ok();
            });
        }

        {
            let window_handle = window.clone();
            cx.on_action(move |_: &SwitchAgent1, cx| {
                window_handle
                    .update(cx, |chat, _window, cx| {
                        chat.switch_agent_by_slot(1, cx);
                    })
                    .ok();
            });
        }

        {
            let window_handle = window.clone();
            cx.on_action(move |_: &SwitchAgent2, cx| {
                window_handle
                    .update(cx, |chat, _window, cx| {
                        chat.switch_agent_by_slot(2, cx);
                    })
                    .ok();
            });
        }

        {
            let window_handle = window.clone();
            cx.on_action(move |_: &SwitchAgent3, cx| {
                window_handle
                    .update(cx, |chat, _window, cx| {
                        chat.switch_agent_by_slot(3, cx);
                    })
                    .ok();
            });
        }

        {
            let window_handle = window.clone();
            cx.on_action(move |_: &FocusPrompt, cx| {
                window_handle
                    .update(cx, |chat, window, cx| {
                        chat.focus_prompt(window, cx);
                    })
                    .ok();
            });
        }

        window
            .update(cx, |chat, window, cx| {
                let focus_handle = chat.prompt_focus_handle(cx);
                window.focus(&focus_handle);
                cx.activate(true);
            })
            .unwrap();

        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    });
}
