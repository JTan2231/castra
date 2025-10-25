use gpui::actions;

actions!(
    chat_actions,
    [
        Backspace,
        SendMessage,
        Quit,
        FocusPrompt,
        HistoryPrev,
        HistoryNext,
        CancelHistory,
        ToggleSidebar,
        SwitchAgent1,
        SwitchAgent2,
        SwitchAgent3,
    ]
);
