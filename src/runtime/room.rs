use crate::app::{App, model_display_name};

impl App {
    pub fn room_share_url(&self) -> Option<String> {
        let room_id = self.room_id.as_ref()?;
        // if connected via --remote, derive base from remote_url
        // otherwise use the embedded local server address
        let base = if !self.remote_url.is_empty() {
            self.remote_url.splitn(2, "/id/").next()
                .unwrap_or(&self.remote_url)
                .trim_end_matches('/')
                .to_string()
        } else {
            format!("ws://{}:{}", crate::DEFAULT_SERVER_HOST, crate::DEFAULT_SERVER_PORT)
        };
        Some(format!("{base}/id/{room_id}"))
    }

    /// Push a live token to the shared room so WS clients can stream it in real time.
    pub fn room_push_token(&self, token: &str) {
        if let Some(ref room) = self.shared_room {
            if let Ok(mut r) = room.lock() {
                r.live_tokens.push_str(token);
                r.live_token_version += 1;
                r.live_user = match self.selected_model.as_deref() {
                    Some(m) if !m.is_empty() => format!("{}#{}", model_display_name(m), self.session_id),
                    _ => self.display_username(),
                };
            }
        }
    }

    /// Append the last message (user or assistant) to the shared room (append-only).
    pub fn room_sync_done(&mut self) {
        if let Some(ref room) = self.shared_room {
            if let Ok(mut r) = room.lock() {
                // append any messages not yet in the room (from room_synced_len onward)
                let new = &self.messages[r.messages.len()..];
                r.messages.extend(new.iter().cloned());
                r.version += 1;
                r.live_tokens.clear();
                r.live_token_version = 0;
                r.live_user.clear();
                self.room_synced_len = r.messages.len();
            }
        }
    }

    /// Append the user message to the room immediately when the user hits Enter.
    pub fn room_sync_user_msg(&mut self) {
        if let Some(ref room) = self.shared_room {
            if let Ok(mut r) = room.lock() {
                let new = &self.messages[r.messages.len()..];
                r.messages.extend(new.iter().cloned());
                r.version += 1;
                r.live_user = self.display_username();
                self.room_synced_len = r.messages.len();
            }
        }
    }

    /// Pull new messages from the shared room into app.messages.
    pub fn poll_room(&mut self) {
        let room = match self.shared_room.as_ref() { Some(r) => r.clone(), None => return };
        let r = match room.lock() { Ok(r) => r, Err(_) => return };
        if r.messages.len() > self.room_synced_len {
            let new = r.messages[self.room_synced_len..].to_vec();
            drop(r);
            for msg in new {
                self.messages.push(msg);
                self.room_synced_len += 1;
            }
            self.auto_scroll = true;
        }
    }
}
