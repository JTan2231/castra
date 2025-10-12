pub fn colorize(value: &str, code: &str, enabled: bool) -> String {
    if enabled {
        format!("\u{1b}[{code}m{value}\u{1b}[0m")
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorize_wraps_value_when_enabled() {
        let colored = colorize("ok", "32", true);
        assert_eq!(colored, "\u{1b}[32mok\u{1b}[0m");
    }

    #[test]
    fn colorize_returns_plain_when_disabled() {
        let plain = colorize("ok", "32", false);
        assert_eq!(plain, "ok");
    }
}
