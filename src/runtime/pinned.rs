use std::path::PathBuf;
use crate::app::App;

pub struct PinnedFile {
    pub path: PathBuf,
    pub display: String,          // relative path shown in UI
    pub content: String,          // snapshot at last warmup / last send
    pub mtime: Option<std::time::SystemTime>,
    pub changed: bool,            // mtime differs from snapshot
}

impl App {
    /// Check mtime of all pinned files and update `changed` flag.
    pub fn check_pinned_files(&mut self) {
        for pf in &mut self.pinned_files {
            let new_mtime = pf.path.metadata().ok().and_then(|m| m.modified().ok());
            pf.changed = new_mtime != pf.mtime;
        }
    }

    /// Generate diffs for changed pinned files, update snapshots, return note for llm_content.
    pub(crate) fn collect_pinned_diffs(&mut self) -> String {
        let mut notes: Vec<String> = Vec::new();
        for pf in &mut self.pinned_files {
            let Ok(new_content) = std::fs::read_to_string(&pf.path) else { continue };
            let new_mtime = pf.path.metadata().ok().and_then(|m| m.modified().ok());
            if new_mtime == pf.mtime && new_content == pf.content { continue; }
            let diff = file_diff(&pf.content, &new_content, &pf.display);
            if !diff.is_empty() {
                notes.push(format!("[{} changed since your last response]\n```diff\n{}\n```", pf.display, diff.trim()));
            }
            pf.content = new_content;
            pf.mtime   = new_mtime;
            pf.changed = false;
        }
        notes.join("\n\n")
    }

    pub fn pin_file(&mut self, display: String) {
        if self.pinned_files.iter().any(|pf| pf.display == display) { return; }
        let path = self.working_dir.join(&display);
        let Ok(content) = std::fs::read_to_string(&path) else { return };
        let mtime = path.metadata().ok().and_then(|m| m.modified().ok());
        self.pinned_files.push(PinnedFile { path, display, content, mtime, changed: false });
        self.trigger_warmup();
    }

    pub fn unpin_file(&mut self, display: &str) {
        self.pinned_files.retain(|pf| pf.display != display);
        self.trigger_warmup();
    }
}

/// Generate a unified diff between two content strings using the `diff` command.
fn file_diff(old: &str, new: &str, label: &str) -> String {
    let dir = std::env::temp_dir();
    let old_path = dir.join("cognilite_pin_old.txt");
    let new_path = dir.join("cognilite_pin_new.txt");
    let _ = std::fs::write(&old_path, old);
    let _ = std::fs::write(&new_path, new);
    let out = std::process::Command::new("diff")
        .args(["-u", "--label", &format!("a/{label}"), "--label", &format!("b/{label}")])
        .arg(&old_path).arg(&new_path).output();
    let _ = std::fs::remove_file(&old_path);
    let _ = std::fs::remove_file(&new_path);
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => String::new(),
    }
}
