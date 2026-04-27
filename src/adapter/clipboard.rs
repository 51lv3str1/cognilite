use std::io::Write;

/// Copies `text` to the system clipboard using the best available method.
pub fn copy(text: &str) -> bool {
    // SSH session — OSC 52 is the only option that reaches the local clipboard
    if std::env::var("SSH_CLIENT").is_ok() || std::env::var("SSH_TTY").is_ok() {
        return osc52(text);
    }

    #[cfg(target_os = "macos")]
    if pipe("pbcopy", &[], text) { return true; }

    #[cfg(target_os = "windows")]
    if pipe("clip", &[], text) { return true; }

    // Linux/BSD — Wayland first, then X11
    if std::env::var("WAYLAND_DISPLAY").is_ok() && pipe("wl-copy", &[], text) {
        return true;
    }
    if std::env::var("DISPLAY").is_ok() && pipe("xclip", &["-selection", "clipboard"], text) {
        return true;
    }

    osc52(text)
}

fn pipe(cmd: &str, args: &[&str], text: &str) -> bool {
    let Ok(mut child) = std::process::Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else { return false };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

fn osc52(text: &str) -> bool {
    let b64 = crate::app::base64_encode(text.as_bytes());
    // Write to stderr — goes to the terminal even while ratatui owns stdout
    let _ = write!(std::io::stderr(), "\x1b]52;c;{b64}\x07");
    let _ = std::io::stderr().flush();
    true
}
