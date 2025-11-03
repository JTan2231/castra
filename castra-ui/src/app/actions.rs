use gpui::actions;

actions!(
    chat_actions,
    [
        Backspace,
        SendMessage,
        InsertNewline,
        Paste,
        Quit,
        FocusPrompt,
        HistoryPrev,
        HistoryNext,
        CancelHistory,
        FocusNextVm,
        FocusPrevVm,
        ToggleSidebar,
        SwitchAgent1,
        SwitchAgent2,
        SwitchAgent3,
    ]
);
