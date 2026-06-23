use std::path::Path;

use candle_core::{D, DType, Device, IndexOp, Tensor};
use candle_nn::{self, Embedding, LayerNorm, Linear, Module, VarBuilder, VarMap};

use crate::LanguageModel;
use crate::tokenizer::CharacterTokenizer;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MiniGptConfig {
    pub vocab_size: usize,
    pub n_layers: usize,
    pub n_heads: usize,
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub max_seq_len: usize,
}

impl Default for MiniGptConfig {
    fn default() -> Self {
        Self {
            vocab_size: 8192,
            n_layers: 4,
            n_heads: 4,
            hidden_size: 256,
            intermediate_size: 512,
            max_seq_len: 256,
        }
    }
}

struct TransformerBlock {
    ln1: LayerNorm,
    attn_qkv: Linear,
    attn_proj: Linear,
    ln2: LayerNorm,
    ffn_up: Linear,
    ffn_down: Linear,
    n_heads: usize,
    // When true this block uses linear attention (O(1) recurrent inference, no
    // growing KV cache); when false it uses standard softmax attention. The model
    // is a hybrid: mostly linear layers with one softmax layer for quality.
    linear: bool,
}

impl TransformerBlock {
    fn new(
        hidden_size: usize,
        intermediate_size: usize,
        n_heads: usize,
        linear: bool,
        vb: VarBuilder,
    ) -> candle_core::Result<Self> {
        Ok(Self {
            ln1: candle_nn::layer_norm(
                hidden_size,
                candle_nn::LayerNormConfig::default(),
                vb.pp("ln1"),
            )?,
            attn_qkv: candle_nn::linear(hidden_size, 3 * hidden_size, vb.pp("attn_qkv"))?,
            attn_proj: candle_nn::linear(hidden_size, hidden_size, vb.pp("attn_proj"))?,
            ln2: candle_nn::layer_norm(
                hidden_size,
                candle_nn::LayerNormConfig::default(),
                vb.pp("ln2"),
            )?,
            ffn_up: candle_nn::linear(hidden_size, intermediate_size, vb.pp("ffn_up"))?,
            ffn_down: candle_nn::linear(intermediate_size, hidden_size, vb.pp("ffn_down"))?,
            n_heads,
            linear,
        })
    }

    fn forward(&self, x: &Tensor, mask: &Tensor) -> candle_core::Result<Tensor> {
        let residual = x;
        let h = self.ln1.forward(x)?;
        let h = self.self_attention(&h, mask)?;
        let x = (residual + &h)?;

        let residual = &x;
        let h = self.ln2.forward(&x)?;
        let h = self.ffn(&h)?;
        residual + &h
    }

    fn self_attention(&self, x: &Tensor, mask: &Tensor) -> candle_core::Result<Tensor> {
        let (b, seq, hidden) = x.dims3()?;
        let head_dim = hidden / self.n_heads;

        let qkv = self.attn_qkv.forward(x)?;
        let qkv = qkv.reshape((b, seq, 3, self.n_heads, head_dim))?;

        // `.contiguous()` after each transpose: candle's batched matmul rejects
        // non-contiguous operands once batch > 1 (fine at batch=1 inference, fails
        // at batch=32 training).
        let q = qkv
            .narrow(2, 0, 1)?
            .squeeze(2)?
            .transpose(1, 2)?
            .contiguous()?;
        let k = qkv
            .narrow(2, 1, 1)?
            .squeeze(2)?
            .transpose(1, 2)?
            .contiguous()?;
        let v = qkv
            .narrow(2, 2, 1)?
            .squeeze(2)?
            .transpose(1, 2)?
            .contiguous()?;

        let out = if self.linear {
            linear_attention(&q, &k, &v)?
        } else {
            let scale = (head_dim as f64).sqrt();
            let kt = k.transpose(D::Minus2, D::Minus1)?.contiguous()?;
            let attn = q.matmul(&kt)?;
            let attn = attn.affine(1.0 / scale, 0.0)?;
            let attn = attn.broadcast_add(mask)?;
            let attn = candle_nn::ops::softmax(&attn, D::Minus1)?;
            attn.matmul(&v)?
        };
        let out = out
            .transpose(1, 2)?
            .contiguous()?
            .reshape((b, seq, hidden))?;
        self.attn_proj.forward(&out)
    }

    fn ffn(&self, x: &Tensor) -> candle_core::Result<Tensor> {
        let h = self.ffn_up.forward(x)?;
        let h = h.gelu_erf()?;
        self.ffn_down.forward(&h)
    }
}

/// Causal linear attention (Katharopoulos et al.). Feature map φ(x)=elu(x)+1 makes
/// scores non-negative, so attention is `φ(q)·φ(k)` normalized by the running sum
/// — equivalent to a recurrent state `S_t = S_{t-1} + φ(k_t)⊗v_t` that updates in
/// O(1) per token at inference (no softmax, no growing KV cache). q/k/v: (b,h,n,d).
fn linear_attention(q: &Tensor, k: &Tensor, v: &Tensor) -> candle_core::Result<Tensor> {
    let n = q.dim(D::Minus2)?;
    let qf = q.elu(1.0)?.affine(1.0, 1.0)?; // elu(x)+1 > 0
    let kf = k.elu(1.0)?.affine(1.0, 1.0)?;
    let kt = kf.transpose(D::Minus2, D::Minus1)?.contiguous()?;
    let scores = qf.matmul(&kt)?; // (b,h,n,n) = φ(q)·φ(k)
    // Multiplicative lower-triangular causal mask (1 where key ≤ query).
    let mut mask = vec![0f32; n * n];
    for i in 0..n {
        for value in mask.iter_mut().skip(i * n).take(i + 1) {
            *value = 1.0;
        }
    }
    let causal = Tensor::from_vec(mask, (n, n), q.device())?;
    let scores = scores.broadcast_mul(&causal)?;
    let denom = scores.sum_keepdim(D::Minus1)?.affine(1.0, 1e-6)?; // (b,h,n,1)
    let ctx = scores.matmul(&v.contiguous()?)?; // (b,h,n,d)
    ctx.broadcast_div(&denom)
}

/// Per-layer recurrent inference state. Linear layers keep an O(1) running state
/// (S = Σ φ(k)⊗v, z = Σ φ(k)); the single softmax layer keeps a small KV cache.
#[derive(Clone)]
enum LayerStreamState {
    Linear { s: Tensor, z: Tensor },
    Softmax { k: Vec<Tensor>, v: Vec<Tensor> },
}

/// Whole-model recurrent state for incremental decoding. Cloned to fork per
/// candidate, so the shared prefix is only processed once.
#[derive(Clone)]
struct StreamState {
    pos: usize,
    layers: Vec<LayerStreamState>,
}

pub struct MiniGptModel {
    config: MiniGptConfig,
    tokenizer: CharacterTokenizer,
    token_embedding: Embedding,
    position_embedding: Embedding,
    layers: Vec<TransformerBlock>,
    ln_final: LayerNorm,
    lm_head: Linear,
    device: Device,
}

impl MiniGptModel {
    pub fn new(
        config: MiniGptConfig,
        tokenizer: CharacterTokenizer,
        vb: VarBuilder,
    ) -> candle_core::Result<Self> {
        let device = vb.device().clone();

        let token_embedding =
            candle_nn::embedding(config.vocab_size, config.hidden_size, vb.pp("token_emb"))?;
        let position_embedding =
            candle_nn::embedding(config.max_seq_len, config.hidden_size, vb.pp("pos_emb"))?;

        // Hybrid: every layer is linear-attention except the LAST, which is full
        // softmax attention. One global-attention layer recovers most of the quality
        // a pure-linear stack loses, while inference stays dominated by O(1) layers.
        let mut layers = Vec::with_capacity(config.n_layers);
        for i in 0..config.n_layers {
            let is_linear = i + 1 < config.n_layers;
            layers.push(TransformerBlock::new(
                config.hidden_size,
                config.intermediate_size,
                config.n_heads,
                is_linear,
                vb.pp(format!("layers.{i}")),
            )?);
        }

        let ln_final = candle_nn::layer_norm(
            config.hidden_size,
            candle_nn::LayerNormConfig::default(),
            vb.pp("ln_final"),
        )?;
        let lm_head = candle_nn::linear(config.hidden_size, config.vocab_size, vb.pp("lm_head"))?;

        Ok(Self {
            config,
            tokenizer,
            token_embedding,
            position_embedding,
            layers,
            ln_final,
            lm_head,
            device,
        })
    }

    pub fn random(
        config: MiniGptConfig,
        tokenizer: CharacterTokenizer,
    ) -> candle_core::Result<Self> {
        let device = Device::Cpu;
        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, DType::F32, &device);
        Self::new(config, tokenizer, vb)
    }

    pub fn load(
        path: &Path,
        config: MiniGptConfig,
        tokenizer: CharacterTokenizer,
        device: &Device,
    ) -> candle_core::Result<Self> {
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[path], DType::F32, device)? };
        Self::new(config, tokenizer, vb)
    }

    /// Convenience loader: reads `config.json`, `tokenizer.json`, and
    /// `model.safetensors` from a directory and builds a CPU model. Keeps candle
    /// types out of downstream crates that only have the directory path.
    pub fn load_from_dir(root: &Path) -> candle_core::Result<Self> {
        let to_err = |e: std::io::Error| candle_core::Error::Msg(e.to_string());
        let config_bytes = std::fs::read(root.join("config.json")).map_err(to_err)?;
        let config: MiniGptConfig = serde_json::from_slice(&config_bytes)
            .map_err(|e| candle_core::Error::Msg(e.to_string()))?;
        let tokenizer =
            CharacterTokenizer::from_json_path(&root.join("tokenizer.json")).map_err(to_err)?;
        Self::load(
            &root.join("model.safetensors"),
            config,
            tokenizer,
            &Device::Cpu,
        )
    }

    pub fn config(&self) -> &MiniGptConfig {
        &self.config
    }

    fn forward_logits(&self, token_ids: &[u32]) -> candle_core::Result<Tensor> {
        let seq_len = token_ids.len().min(self.config.max_seq_len);
        let ids = &token_ids[token_ids.len().saturating_sub(seq_len)..];

        let tokens = Tensor::new(ids, &self.device)?.unsqueeze(0)?;
        let positions = Tensor::arange(0u32, seq_len as u32, &self.device)?.unsqueeze(0)?;

        let tok_emb = self.token_embedding.forward(&tokens)?;
        let pos_emb = self.position_embedding.forward(&positions)?;
        let mut x = (&tok_emb + &pos_emb)?;

        let mask = Self::causal_mask(seq_len, &self.device)?;

        for layer in &self.layers {
            x = layer.forward(&x, &mask)?;
        }

        let x = self.ln_final.forward(&x)?;
        self.lm_head.forward(&x)
    }

    fn head_dim(&self) -> usize {
        self.config.hidden_size / self.config.n_heads
    }

    fn init_stream(&self) -> candle_core::Result<StreamState> {
        let (nh, hd) = (self.config.n_heads, self.head_dim());
        let mut layers = Vec::with_capacity(self.layers.len());
        for layer in &self.layers {
            layers.push(if layer.linear {
                LayerStreamState::Linear {
                    s: Tensor::zeros((nh, hd, hd), DType::F32, &self.device)?,
                    z: Tensor::zeros((nh, hd), DType::F32, &self.device)?,
                }
            } else {
                LayerStreamState::Softmax {
                    k: Vec::new(),
                    v: Vec::new(),
                }
            });
        }
        Ok(StreamState { pos: 0, layers })
    }

    /// Consumes one token, updating `state` in place; returns log-probs for the
    /// NEXT token. Replicates `forward_logits` one position at a time: linear layers
    /// fold the token into their O(1) state, the softmax layer appends to its cache.
    fn stream_step(&self, state: &mut StreamState, token: u32) -> candle_core::Result<Tensor> {
        let (nh, hd, hidden) = (self.config.n_heads, self.head_dim(), self.config.hidden_size);
        let pos = state.pos.min(self.config.max_seq_len - 1);

        let tok_emb = self
            .token_embedding
            .forward(&Tensor::new(&[token], &self.device)?)?;
        let pos_emb = self
            .position_embedding
            .forward(&Tensor::new(&[pos as u32], &self.device)?)?;
        let mut x = (tok_emb + pos_emb)?; // (1, hidden)

        for (li, layer) in self.layers.iter().enumerate() {
            let h = layer.ln1.forward(&x)?;
            let qkv = layer.attn_qkv.forward(&h)?.reshape((3, nh, hd))?;
            let q = qkv.narrow(0, 0, 1)?.squeeze(0)?; // (nh, hd)
            let k = qkv.narrow(0, 1, 1)?.squeeze(0)?;
            let v = qkv.narrow(0, 2, 1)?.squeeze(0)?;

            let attn = match &mut state.layers[li] {
                LayerStreamState::Linear { s, z } => {
                    let qf = q.elu(1.0)?.affine(1.0, 1.0)?;
                    let kf = k.elu(1.0)?.affine(1.0, 1.0)?;
                    let outer = kf.unsqueeze(2)?.broadcast_mul(&v.unsqueeze(1)?)?; // (nh,hd,hd)
                    *s = (&*s + outer)?;
                    *z = (&*z + &kf)?;
                    let num = qf.unsqueeze(1)?.matmul(s)?.squeeze(1)?; // (nh, hd)
                    let den = qf.mul(z)?.sum_keepdim(1)?.affine(1.0, 1e-6)?; // (nh, 1)
                    num.broadcast_div(&den)?
                }
                LayerStreamState::Softmax { k: kc, v: vc } => {
                    kc.push(k);
                    vc.push(v);
                    let kk = Tensor::stack(kc, 0)?.transpose(0, 1)?.contiguous()?; // (nh, T, hd)
                    let vv = Tensor::stack(vc, 0)?.transpose(0, 1)?.contiguous()?;
                    let scale = (hd as f64).sqrt();
                    let scores = q
                        .unsqueeze(1)?
                        .matmul(&kk.transpose(1, 2)?.contiguous()?)?
                        .squeeze(1)?
                        .affine(1.0 / scale, 0.0)?; // (nh, T)
                    let attn = candle_nn::ops::softmax(&scores, D::Minus1)?;
                    attn.unsqueeze(1)?.matmul(&vv)?.squeeze(1)? // (nh, hd)
                }
            };
            let out = layer.attn_proj.forward(&attn.reshape((1, hidden))?)?;
            x = (x + out)?;
            let h2 = layer.ln2.forward(&x)?;
            let h2 = layer.ffn(&h2)?;
            x = (x + h2)?;
        }
        state.pos += 1;
        let logits = self.lm_head.forward(&self.ln_final.forward(&x)?)?; // (1, vocab)
        candle_nn::ops::log_softmax(&logits, D::Minus1)?.squeeze(0)
    }

    /// `log P(candidate | prefix)` for each candidate. The prefix is run through the
    /// recurrent state ONCE; each candidate then forks that state and steps its own
    /// tokens (O(1) per linear layer), instead of a full forward per candidate.
    fn score_continuations_impl(
        &self,
        prefix: &[u32],
        candidates: &[&[u32]],
    ) -> candle_core::Result<Vec<f32>> {
        let mut state = self.init_stream()?;
        let mut pending = None;
        for &t in prefix {
            pending = Some(self.stream_step(&mut state, t)?);
        }
        let Some(base) = pending else {
            return Ok(vec![0.0; candidates.len()]); // empty prefix: no context
        };
        let mut out = Vec::with_capacity(candidates.len());
        for cand in candidates {
            if cand.is_empty() {
                out.push(0.0);
                continue;
            }
            let mut st = state.clone();
            let mut dist = base.clone();
            let mut logp = 0.0f32;
            for (j, &c) in cand.iter().enumerate() {
                logp += dist.get(c as usize)?.to_scalar::<f32>()?;
                if j + 1 < cand.len() {
                    dist = self.stream_step(&mut st, c)?;
                }
            }
            out.push(logp);
        }
        Ok(out)
    }

    /// Batched teacher-forcing forward for training. `tokens` is a `(batch, seq)`
    /// tensor of u32 ids; returns logits `(batch, seq, vocab)`. Unlike
    /// `forward_logits`, this keeps the full sequence (every position predicts the
    /// next token) so a single forward yields a loss over the whole window.
    pub fn forward_batch(&self, tokens: &Tensor) -> candle_core::Result<Tensor> {
        let (batch, seq) = tokens.dims2()?;
        let positions = Tensor::arange(0u32, seq as u32, &self.device)?
            .unsqueeze(0)?
            .broadcast_as((batch, seq))?
            .contiguous()?;
        let tok_emb = self.token_embedding.forward(tokens)?;
        let pos_emb = self.position_embedding.forward(&positions)?;
        let mut x = (&tok_emb + &pos_emb)?;
        let mask = Self::causal_mask(seq, &self.device)?;
        for layer in &self.layers {
            x = layer.forward(&x, &mask)?;
        }
        let x = self.ln_final.forward(&x)?;
        self.lm_head.forward(&x)
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    fn causal_mask(seq_len: usize, device: &Device) -> candle_core::Result<Tensor> {
        let mut data = vec![0f32; seq_len * seq_len];
        for i in 0..seq_len {
            for j in (i + 1)..seq_len {
                data[i * seq_len + j] = f32::NEG_INFINITY;
            }
        }
        Tensor::from_vec(data, (1, 1, seq_len, seq_len), device)
    }

    fn last_logprobs(&self, token_ids: &[u32]) -> candle_core::Result<Vec<f32>> {
        if token_ids.is_empty() {
            return Ok(vec![0.0; self.config.vocab_size]);
        }
        let logits = self.forward_logits(token_ids)?;
        let seq_len = logits.dim(1)?;
        let last = logits.narrow(1, seq_len - 1, 1)?.squeeze(1)?.squeeze(0)?;
        let log_probs = candle_nn::ops::log_softmax(&last, D::Minus1)?;
        log_probs.to_vec1::<f32>()
    }
}

impl LanguageModel for MiniGptModel {
    fn next_token_logprobs(&self, token_ids: &[u32]) -> Vec<f32> {
        self.last_logprobs(token_ids)
            .unwrap_or_else(|_| vec![0.0; self.config.vocab_size])
    }

    fn score_sequence(&self, token_ids: &[u32]) -> f32 {
        if token_ids.len() < 2 {
            return 0.0;
        }
        let Ok(logits) = self.forward_logits(token_ids) else {
            return 0.0;
        };
        let Ok(log_probs) = candle_nn::ops::log_softmax(&logits, D::Minus1) else {
            return 0.0;
        };
        let mut total = 0.0f32;
        for i in 0..token_ids.len() - 1 {
            let target = token_ids[i + 1] as usize;
            if let Ok(lp) = log_probs.i((0, i, target)) {
                if let Ok(val) = lp.to_scalar::<f32>() {
                    total += val;
                }
            }
        }
        total
    }

    fn predict_top_k(&self, token_ids: &[u32], k: usize) -> Vec<(u32, f32)> {
        let Ok(logprobs) = self.last_logprobs(token_ids) else {
            return Vec::new();
        };
        let mut indexed: Vec<(u32, f32)> = logprobs
            .iter()
            .enumerate()
            .map(|(i, &lp)| (i as u32, lp))
            .collect();
        indexed.sort_by(|a, b| b.1.total_cmp(&a.1));
        indexed.truncate(k);
        indexed
    }

    fn encode(&self, text: &str) -> Vec<u32> {
        self.tokenizer.encode(text)
    }

    fn decode(&self, token_ids: &[u32]) -> String {
        self.tokenizer.decode(token_ids)
    }

    fn vocab_size(&self) -> usize {
        self.config.vocab_size
    }

    fn score_continuations(&self, prefix: &[u32], candidates: &[&[u32]]) -> Vec<f32> {
        // Recurrent path: prefix processed once, each candidate stepped in O(1)/token.
        self.score_continuations_impl(prefix, candidates)
            .unwrap_or_else(|_| vec![0.0; candidates.len()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_model() -> MiniGptModel {
        let config = MiniGptConfig {
            vocab_size: 128,
            n_layers: 2,
            n_heads: 2,
            hidden_size: 32,
            intermediate_size: 64,
            max_seq_len: 64,
        };
        let tokenizer =
            CharacterTokenizer::from_vocab(&(0x20u8..=0x7E).map(|b| b as char).collect::<Vec<_>>());
        MiniGptModel::random(config, tokenizer).expect("model init")
    }

    #[test]
    fn forward_produces_logprobs_for_vocab() {
        let model = test_model();
        let ids = model.encode("hello");
        let logprobs = model.next_token_logprobs(&ids);
        assert_eq!(logprobs.len(), 128);
        let sum: f32 = logprobs.iter().map(|lp| lp.exp()).sum();
        assert!((sum - 1.0).abs() < 0.01);
    }

    #[test]
    fn score_sequence_returns_finite() {
        let model = test_model();
        let ids = model.encode("test");
        let score = model.score_sequence(&ids);
        assert!(score.is_finite());
        assert!(score <= 0.0);
    }

    #[test]
    fn incremental_continuations_match_parallel() {
        // The recurrent stream_step path must reproduce the parallel forward exactly.
        let model = test_model();
        let prefix = model.encode("the quick brown");
        let cand = model.encode(" fox");
        let inc = model.score_continuations(&prefix, &[&cand[..]])[0];
        let mut full = prefix.clone();
        full.extend_from_slice(&cand);
        let parallel = model.score_sequence(&full) - model.score_sequence(&prefix);
        assert!(
            (inc - parallel).abs() < 1e-2,
            "incremental {inc} vs parallel {parallel}"
        );
    }

    #[test]
    fn predict_top_k_returns_ordered() {
        let model = test_model();
        let ids = model.encode("ab");
        let top = model.predict_top_k(&ids, 5);
        assert_eq!(top.len(), 5);
        for window in top.windows(2) {
            assert!(window[0].1 >= window[1].1);
        }
    }

    #[test]
    fn encode_decode_roundtrips_ascii() {
        let model = test_model();
        let text = "hello world";
        let ids = model.encode(text);
        let decoded = model.decode(&ids);
        assert_eq!(decoded, text);
    }
}
