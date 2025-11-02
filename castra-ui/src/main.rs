mod app;
mod codex;
mod components;
mod controller;
mod input;
mod ssh;
mod state;
mod transcript;

use app::{ChatApp, ShutdownState, actions::*};
use ctrlc;
use gpui::{
    App, AppContext, Application, Bounds, KeyBinding, WindowBounds, WindowHandle, WindowOptions,
    px, size,
};
use input::prompt::PromptInput;
use std::process;
use std::sync::Arc;

fn main() {
    let shutdown = Arc::new(ShutdownState::new());
    {
        let shutdown = shutdown.clone();
        ctrlc::set_handler(move || {
            shutdown.run_cleanup_blocking();
            process::exit(0);
        })
        .expect("failed to install signal handler");
    }

    Application::new().run({
        let shutdown = shutdown.clone();
        move |cx: &mut App| {
            let bounds = Bounds::centered(None, size(px(960.), px(720.)), cx);

            {
                let shutdown_for_close = shutdown.clone();
                cx.on_window_closed(move |cx| {
                    if cx.windows().is_empty() {
                        if shutdown_for_close.cleanup_in_progress() {
                            return;
                        }
                        if !shutdown_for_close.run_cleanup_blocking() {
                            cx.quit();
                        }
                    }
                })
                .detach();
            }

            cx.bind_keys([
                KeyBinding::new("backspace", Backspace, None),
                KeyBinding::new("enter", SendMessage, None),
                KeyBinding::new("up", HistoryPrev, None),
                KeyBinding::new("down", HistoryNext, None),
                KeyBinding::new("escape", CancelHistory, None),
                KeyBinding::new("cmd-k", FocusPrompt, None),
                KeyBinding::new("ctrl-l", FocusPrompt, None),
                KeyBinding::new("tab", FocusNextVm, None),
                KeyBinding::new("shift-tab", FocusPrevVm, None),
                KeyBinding::new("cmd-b", ToggleSidebar, None),
                KeyBinding::new("ctrl-b", ToggleSidebar, None),
                KeyBinding::new("cmd-1", SwitchAgent1, None),
                KeyBinding::new("cmd-2", SwitchAgent2, None),
                KeyBinding::new("cmd-3", SwitchAgent3, None),
                KeyBinding::new("ctrl-1", SwitchAgent1, None),
                KeyBinding::new("ctrl-2", SwitchAgent2, None),
                KeyBinding::new("ctrl-3", SwitchAgent3, None),
            ]);

            let shutdown_for_window = shutdown.clone();
            let window: WindowHandle<ChatApp> = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..Default::default()
                    },
                    move |_, cx| {
                        let prompt = cx.new(PromptInput::new);
                        let shutdown = shutdown_for_window.clone();
                        cx.new(move |cx| {
                            cx.subscribe(&prompt, |chat: &mut ChatApp, _, event, cx| {
                                chat.on_prompt_event(event, cx);
                            })
                            .detach();
                            let mut chat = ChatApp::new(prompt.clone(), shutdown.clone());
                            chat.initialize(cx);
                            chat
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

            {
                let window_handle = window.clone();
                cx.on_action(move |_: &FocusNextVm, cx| {
                    window_handle
                        .update(cx, |chat, window, cx| {
                            chat.focus_next_vm(window, cx);
                        })
                        .ok();
                });
            }

            {
                let window_handle = window.clone();
                cx.on_action(move |_: &FocusPrevVm, cx| {
                    window_handle
                        .update(cx, |chat, window, cx| {
                            chat.focus_prev_vm(window, cx);
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

            {
                let window_handle = window.clone();
                let shutdown_for_quit = shutdown.clone();
                cx.on_action(move |_: &Quit, cx| {
                    if window_handle
                        .update(cx, |chat, _window, cx| chat.initiate_shutdown(cx))
                        .is_err()
                    {
                        if !shutdown_for_quit.cleanup_in_progress()
                            && !shutdown_for_quit.run_cleanup_blocking()
                        {
                            cx.quit();
                        }
                    }
                });
            }

            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        }
    });
}
