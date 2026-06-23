//! Cross-word swipe LM session + adaptive user dictionary management.
//! (Decode-time scoring lives in lib.rs; this is the stateful session API the
//! host calls on commit / reset.)

use super::ImeEngine;

impl ImeEngine {
    pub fn accept_swipe_candidate(&mut self, text: &str) {
        let tokens = self.lm.encode(text);
        self.swipe_lm_session.feed_tokens(&tokens);
        self.swipe_accepted_text.push_str(text);
    }

    /// Replace the session context with the editor's actual surrounding text
    /// (the host reads `getTextBeforeCursor` and pushes it here on focus / cursor
    /// move). Unlike [`accept_swipe_candidate`] which APPENDS what the user typed
    /// through this keyboard, this REPLACES the context with the real field
    /// contents — so the LM also sees pre-existing text, text typed by another
    /// keyboard, or text around a cursor placed mid-document.
    ///
    /// Feeds both context paths: the tap-decode `TextContext` energy term
    /// ([`set_committed_context`](Self::set_committed_context)) and the swipe-LM
    /// reranker history (`swipe_accepted_text`, re-encoded per rerank).
    pub fn set_editor_context(&mut self, text: &str) {
        self.set_committed_context(text);
        self.swipe_lm_session = lm_core::LmSession::new();
        self.swipe_accepted_text.clear();
        if !text.is_empty() {
            let tokens = self.lm.encode(text);
            self.swipe_lm_session.feed_tokens(&tokens);
            self.swipe_accepted_text.push_str(text);
        }
    }

    /// Record a committed candidate into the adaptive user dictionary so that
    /// (reading, text) is ranked higher next time the same reading is typed.
    pub fn learn_commit(&mut self, reading: &str, text: &str) {
        self.personal.learn_commit(reading, text);
    }

    /// Snapshot the learned user dictionary for host-side persistence.
    pub fn user_dictionary(&self) -> Vec<(String, String, u32)> {
        self.personal.user_dictionary()
    }

    /// Restore a persisted user dictionary (called once at startup).
    pub fn load_user_dictionary<I>(&mut self, entries: I)
    where
        I: IntoIterator<Item = (String, String, u32)>,
    {
        self.personal.load_user_dictionary(entries);
    }

    pub fn reset_swipe_session(&mut self) {
        self.swipe_lm_session = lm_core::LmSession::new();
        self.swipe_accepted_text.clear();
    }

    pub fn swipe_session_text(&self) -> &str {
        &self.swipe_accepted_text
    }
}
