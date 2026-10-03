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

pub(crate) fn append(
    command: String,
    value: &str,
    shell: PathQuote,
    legacy_powershell: bool,
) -> Option<String> {
    let args = parse(value)?;
    if args.is_empty() {
        return Some(command);
    }
    #[cfg(windows)]
    let args = if legacy_powershell {
        args.iter()
            .map(|arg| {
                let quoted =
                    crate::platform::elevation::quoted_argument(std::ffi::OsStr::new(arg)).ok()?;
                String::from_utf16(&quoted).ok()
            })
            .collect::<Option<Vec<_>>>()?
    } else {
        args
    };
    #[cfg(not(windows))]
    let _ = legacy_powershell;
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
            append("codex resume id".into(), value, PathQuote::Posix, false).unwrap(),
            "codex resume id --config 'name=a b;$(touch nope)&|' 'it'\\''s'"
        );
        assert_eq!(
            append("codex resume id".into(), value, PathQuote::PowerShell, false).unwrap(),
            "codex resume id --config 'name=a b;$(touch nope)&|' 'it''s'"
        );
        assert_eq!(
            append("codex resume id".into(), r#"["a b\\"]"#, PathQuote::CommandPrompt, false)
                .unwrap(),
            "codex resume id \"a b\\\\\""
        );
        for value in [r#"["%PATH%"]"#, r#"["!VAR!"]"#, r#"["a\"b"]"#] {
            assert!(
                append("codex resume id".into(), value, PathQuote::CommandPrompt, false).is_none()
            );
        }
        for value in ["--yolo", r#"[1]"#, r#"[""]"#, r#"["\n"]"#] {
            assert!(parse(value).is_none());
        }
    }
}

#[cfg(all(test, windows))]
mod native_tests {
    use super::*;

    #[test]
    fn windows_powershell_and_pwsh_deliver_the_exact_native_argument_array() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("argv.rs");
        let executable = root.path().join("argv probe.exe");
        std::fs::write(
            &source,
            r#"fn main() { print!("{:?}", std::env::args().skip(1).collect::<Vec<_>>()); }"#,
        )
        .unwrap();
        assert!(
            std::process::Command::new("rustc")
                .arg(&source)
                .arg("-o")
                .arg(&executable)
                .status()
                .unwrap()
                .success()
        );
        let args = [
            "--config",
            "model_provider=\"custom\"",
            "a\"b",
            "a b\\",
            "x\\\"y",
            "it's & $(Write-Output nope)",
        ];
        let value = serde_json::to_string(&args).unwrap();
        let quoted_exe = drop_text_for_paths(
            &[executable.to_string_lossy().into_owned()],
            PathQuote::PowerShell,
        )
        .unwrap();
        for (shell, legacy) in [("powershell.exe", true), ("pwsh.exe", false)] {
            let line = append(
                format!("& {}", quoted_exe.trim_end()),
                &value,
                PathQuote::PowerShell,
                legacy,
            )
            .unwrap();
            let mut command = std::process::Command::new(shell);
            command.args(["-NoProfile", "-NonInteractive", "-Command", &line]);
            let output = crate::platform::process_output::read_cancellable(
                command,
                std::time::Duration::from_secs(15),
                8192,
                &|| false,
            )
            .unwrap();
            assert_eq!(String::from_utf8(output).unwrap(), format!("{args:?}"), "{shell}: {line}");
        }
    }
}
