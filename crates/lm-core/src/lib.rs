mod model;
mod tokenizer;

pub use model::{MiniGptConfig, MiniGptModel};
pub use tokenizer::CharacterTokenizer;

pub trait LanguageModel: Send + Sync {
    fn next_token_logprobs(&self, token_ids: &[u32]) -> Vec<f32>;
    fn score_sequence(&self, token_ids: &[u32]) -> f32;
    fn predict_top_k(&self, token_ids: &[u32], k: usize) -> Vec<(u32, f32)>;
    fn encode(&self, text: &str) -> Vec<u32>;
    fn decode(&self, token_ids: &[u32]) -> String;
    fn vocab_size(&self) -> usize;

    /// `log P(candidate | prefix)` for each candidate. The default scores each
    /// candidate with an independent full forward; recurrent models override this to
    /// process the shared `prefix` only once and step each candidate in O(1)/token.
    fn score_continuations(&self, prefix: &[u32], candidates: &[&[u32]]) -> Vec<f32> {
        let base = self.score_sequence(prefix);
        candidates
            .iter()
            .map(|cand| {
                if cand.is_empty() {
                    return 0.0;
                }
                let mut full = prefix.to_vec();
                full.extend_from_slice(cand);
                self.score_sequence(&full) - base
            })
            .collect()
    }
}

pub struct NullLanguageModel;

impl LanguageModel for NullLanguageModel {
    fn next_token_logprobs(&self, _token_ids: &[u32]) -> Vec<f32> {
        Vec::new()
    }
    fn score_sequence(&self, _token_ids: &[u32]) -> f32 {
        0.0
    }
    fn predict_top_k(&self, _token_ids: &[u32], _k: usize) -> Vec<(u32, f32)> {
        Vec::new()
    }
    fn encode(&self, _text: &str) -> Vec<u32> {
        Vec::new()
    }
    fn decode(&self, _token_ids: &[u32]) -> String {
        String::new()
    }
    fn vocab_size(&self) -> usize {
        0
    }
}

#[derive(Clone)]
pub struct LmSession {
    token_history: Vec<u32>,
}

impl LmSession {
    pub fn new() -> Self {
        Self {
            token_history: Vec::new(),
        }
    }

    pub fn advance(&mut self, model: &dyn LanguageModel, token_id: u32) -> Vec<f32> {
        self.token_history.push(token_id);
        model.next_token_logprobs(&self.token_history)
    }

    pub fn fork(&self) -> Self {
        self.clone()
    }

    pub fn history(&self) -> &[u32] {
        &self.token_history
    }

    pub fn len(&self) -> usize {
        self.token_history.len()
    }

    pub fn is_empty(&self) -> bool {
        self.token_history.is_empty()
    }

    pub fn feed_tokens(&mut self, tokens: &[u32]) {
        self.token_history.extend_from_slice(tokens);
    }
}
