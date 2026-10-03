//! Literal argument arrays for automatic recovery, quoted for the pane's shell.

use crate::display::side_panel::{PathQuote, drop_text_for_paths};

pub(crate) fn parse(value: &str) -> Option<Vec<String>> {
    if value.trim().is_empty() {
        return Some(Vec::new());
    }
    let args: Vec<String> = serde_json::from_str(value).ok()?;
    if args.is_empty() {
        return Some(args);
    }
    drop_text_for_paths(&args, PathQuote::Posix)?;
    Some(args)
}

pub(crate) fn append(command: String, value: &str, shell: PathQuote) -> Option<String> {
    let args = parse(value)?;
    if args.is_empty() {
        return Some(command);
    }
    let quoted = if matches!(shell, PathQuote::CommandPrompt) {
        let mut quoted = String::new();
    // CMD passes quotes to the CLI's argv parser. A quoted argument ending in
    // a backslash needs doubled trailing slashes before its closing quote.
        for arg in &args {
            let mut item = drop_text_for_paths(std::slice::from_ref(arg), shell)?;
            if item.starts_with('"') {
                let trailing = arg.chars().rev().take_while(|ch| *ch == '\\').count();
                item.insert_str(item.len() - 2, &"\\".repeat(trailing));
            }
            quoted.push_str(&item);
        }
        quoted
    } else {
        drop_text_for_paths(&args, shell)?
    };
    if quoted.len() > 64 * 1024 {
        return None;
    }
    Some(format!("{command} {}", quoted.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_arguments_cannot_become_shell_expressions() {
        let value = r#"["--config", "name=a b;$(touch nope)&|", "it's"]"#;
        assert_eq!(
            append("codex resume id".into(), value, PathQuote::Posix).unwrap(),
            "codex resume id --config 'name=a b;$(touch nope)&|' 'it'\\''s'"
        );
        assert_eq!(
            append("codex resume id".into(), value, PathQuote::PowerShell).unwrap(),
            "codex resume id --config 'name=a b;$(touch nope)&|' 'it''s'"
        );
        assert_eq!(
            append("codex resume id".into(), r#"["a b\\"]"#, PathQuote::CommandPrompt)
                .unwrap(),
            "codex resume id \"a b\\\\\""
        );
        for value in [r#"["%PATH%"]"#, r#"["!VAR!"]"#, r#"["a\"b"]"#] {
            assert!(append("codex resume id".into(), value, PathQuote::CommandPrompt).is_none());
        }
        for value in ["--yolo", r#"[1]"#, r#"[""]"#, r#"["\n"]"#] {
            assert!(parse(value).is_none());
        }
    }
}
