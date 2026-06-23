"""Train / quantize / eval a MiniGPT language model for kry keyboard.

The PyTorch model architecture matches the Rust candle implementation
in crates/lm-core/src/model.rs exactly, including weight naming.

Usage:
    python tools/train_lm.py train --corpus datasets/corpus/*.jsonl --tokenizer assets/lm/tokenizer.json
    python tools/train_lm.py quantize --input assets/lm/model.safetensors
    python tools/train_lm.py eval --corpus datasets/corpus/*.jsonl --input assets/lm/model.safetensors
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parents[2]


def cmd_train(args: argparse.Namespace) -> None:
    import torch
    import torch.nn as nn
    from torch.utils.data import DataLoader
    from safetensors.torch import load_file, save_file

    config = load_config(args.config)
    tokenizer = load_tokenizer(args.tokenizer)
    char_to_id = tokenizer["char_to_id"]
    unk_id = tokenizer["special_tokens"]["<unk>"]
    eos_id = tokenizer["special_tokens"]["<eos>"]
    ctx_len = config["max_seq_len"]

    print(f"config: {json.dumps(config)}")
    print(f"vocab_size: {tokenizer['vocab_size']}")

    corpus = load_corpus(args.corpus, char_to_id, unk_id, eos_id, ctx_len)
    print(f"training sequences: {len(corpus)}")
    if not corpus:
        print("ERROR: no training data", file=sys.stderr)
        sys.exit(1)

    device = _best_device()
    print(f"device: {device}")

    model = MiniGPT(config).to(device)
    if args.init_from is not None:
        print(f"initializing from {args.init_from}")
        state = load_file(str(args.init_from))
        model.load_safetensors_dict(state)
    param_count = sum(p.numel() for p in model.parameters())
    print(f"parameters: {param_count:,}")

    max_len = max(len(s) for s in corpus)
    data = torch.zeros(len(corpus), max_len, dtype=torch.long)
    for i, seq in enumerate(corpus):
        data[i, : len(seq)] = torch.tensor(seq, dtype=torch.long)

    loader = DataLoader(
        data,
        batch_size=args.batch_size,
        shuffle=True,
        num_workers=0,
        pin_memory=device.type == "cuda",
    )

    optimizer = torch.optim.AdamW(
        model.parameters(), lr=args.lr, weight_decay=0.01
    )
    total_steps = len(loader) * args.epochs
    scheduler = torch.optim.lr_scheduler.CosineAnnealingLR(
        optimizer, T_max=total_steps, eta_min=args.lr * 0.1
    )
    loss_fn = nn.CrossEntropyLoss(ignore_index=0)

    for epoch in range(1, args.epochs + 1):
        model.train()
        total_loss = 0.0
        total_tokens = 0
        for step, batch in enumerate(loader, 1):
            batch = batch.to(device)
            inputs = batch[:, :-1]
            targets = batch[:, 1:]

            logits = model(inputs)
            loss = loss_fn(logits.reshape(-1, logits.size(-1)), targets.reshape(-1))

            optimizer.zero_grad()
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            optimizer.step()
            scheduler.step()

            n_tokens = (targets != 0).sum().item()
            total_loss += loss.item() * n_tokens
            total_tokens += n_tokens

            if step % 200 == 0:
                avg = total_loss / max(total_tokens, 1)
                ppl = math.exp(min(avg, 20))
                lr = scheduler.get_last_lr()[0]
                print(
                    f"  epoch {epoch} step {step}/{len(loader)} "
                    f"loss={avg:.4f} ppl={ppl:.1f} lr={lr:.2e}"
                )

        avg_loss = total_loss / max(total_tokens, 1)
        ppl = math.exp(min(avg_loss, 20))
        print(f"epoch {epoch}/{args.epochs} loss={avg_loss:.4f} ppl={ppl:.1f}")

    state = model.to_safetensors_dict()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    save_file(state, str(args.output))
    print(f"model saved to {args.output}")


def cmd_quantize(args: argparse.Namespace) -> None:
    import torch
    from safetensors.torch import load_file, save_file

    print(f"loading {args.input}...")
    state = load_file(str(args.input))

    quantized = {}
    for name, tensor in state.items():
        if tensor.dtype == torch.float32 and tensor.dim() >= 2:
            scale = tensor.abs().max() / 127.0
            q = (tensor / scale).round().clamp(-128, 127).to(torch.int8)
            quantized[name] = q
            quantized[name + "._scale"] = scale.unsqueeze(0)
        else:
            quantized[name] = tensor

    args.output.parent.mkdir(parents=True, exist_ok=True)
    save_file(quantized, str(args.output))
    orig_mb = args.input.stat().st_size / 1024 / 1024
    quant_mb = args.output.stat().st_size / 1024 / 1024
    print(f"quantized {orig_mb:.1f}MB -> {quant_mb:.1f}MB")
    print(f"output={args.output}")


def cmd_eval(args: argparse.Namespace) -> None:
    import torch
    from safetensors.torch import load_file

    config = load_config(args.config)
    tokenizer = load_tokenizer(args.tokenizer)
    char_to_id = tokenizer["char_to_id"]
    unk_id = tokenizer["special_tokens"]["<unk>"]
    eos_id = tokenizer["special_tokens"]["<eos>"]
    ctx_len = config["max_seq_len"]

    device = _best_device()
    model = MiniGPT(config).to(device)
    state = load_file(str(args.input))
    model.load_safetensors_dict(state)
    model.eval()

    corpus = load_corpus(args.corpus, char_to_id, unk_id, eos_id, ctx_len)
    print(f"eval sequences: {len(corpus)}")

    loss_fn = torch.nn.CrossEntropyLoss(ignore_index=0)
    total_loss = 0.0
    total_tokens = 0

    with torch.no_grad():
        for seq in corpus:
            t = torch.tensor([seq], device=device)
            logits = model(t[:, :-1])
            targets = t[:, 1:]
            loss = loss_fn(
                logits.reshape(-1, logits.size(-1)), targets.reshape(-1)
            )
            n = (targets != 0).sum().item()
            total_loss += loss.item() * n
            total_tokens += n

    avg = total_loss / max(total_tokens, 1)
    ppl = math.exp(min(avg, 20))
    print(f"eval loss={avg:.4f} perplexity={ppl:.1f}")


def _best_device():
    import torch

    if torch.cuda.is_available():
        return torch.device("cuda")
    if hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
        return torch.device("mps")
    return torch.device("cpu")


def _lazy_module_classes():
    """Define nn.Module subclasses inside a function so torch is only imported
    when actually needed (argparse runs without torch)."""
    import torch
    import torch.nn as nn
    import torch.nn.functional as F

    class TransformerBlock(nn.Module):
        def __init__(self, hidden_size: int, intermediate_size: int, n_heads: int):
            super().__init__()
            self.ln1 = nn.LayerNorm(hidden_size)
            self.attn_qkv = nn.Linear(hidden_size, 3 * hidden_size)
            self.attn_proj = nn.Linear(hidden_size, hidden_size)
            self.ln2 = nn.LayerNorm(hidden_size)
            self.ffn_up = nn.Linear(hidden_size, intermediate_size)
            self.ffn_down = nn.Linear(intermediate_size, hidden_size)
            self.n_heads = n_heads

        def forward(self, x: torch.Tensor, mask: torch.Tensor) -> torch.Tensor:
            h = self.ln1(x)
            h = self._self_attention(h, mask)
            x = x + h
            h = self.ln2(x)
            h = F.gelu(self.ffn_up(h))
            h = self.ffn_down(h)
            return x + h

        def _self_attention(self, x: torch.Tensor, mask: torch.Tensor) -> torch.Tensor:
            b, seq, hidden = x.shape
            head_dim = hidden // self.n_heads

            qkv = self.attn_qkv(x).reshape(b, seq, 3, self.n_heads, head_dim)
            q = qkv[:, :, 0].transpose(1, 2)
            k = qkv[:, :, 1].transpose(1, 2)
            v = qkv[:, :, 2].transpose(1, 2)

            scale = head_dim**0.5
            attn = torch.matmul(q, k.transpose(-2, -1)) / scale + mask
            attn = F.softmax(attn, dim=-1)
            out = torch.matmul(attn, v).transpose(1, 2).reshape(b, seq, hidden)
            return self.attn_proj(out)

    class _MiniGPT(nn.Module):
        def __init__(self, config: dict):
            super().__init__()
            vs = config["vocab_size"]
            hs = config["hidden_size"]
            ims = config["intermediate_size"]
            nl = config["n_layers"]
            nh = config["n_heads"]
            ms = config["max_seq_len"]
            self.config = config

            self.token_emb = nn.Embedding(vs, hs)
            self.pos_emb = nn.Embedding(ms, hs)
            self.layers = nn.ModuleList(
                [TransformerBlock(hs, ims, nh) for _ in range(nl)]
            )
            self.ln_final = nn.LayerNorm(hs)
            self.lm_head = nn.Linear(hs, vs)

        def forward(self, input_ids: torch.Tensor) -> torch.Tensor:
            _b, seq = input_ids.shape
            device = input_ids.device
            positions = torch.arange(seq, device=device).unsqueeze(0)

            x = self.token_emb(input_ids) + self.pos_emb(positions)
            mask = torch.triu(
                torch.full((seq, seq), float("-inf"), device=device), diagonal=1
            ).unsqueeze(0).unsqueeze(0)

            for layer in self.layers:
                x = layer(x, mask)
            return self.lm_head(self.ln_final(x))

        def to_safetensors_dict(self) -> dict:
            state = {}
            state["token_emb.weight"] = self.token_emb.weight.data.cpu()
            state["pos_emb.weight"] = self.pos_emb.weight.data.cpu()
            for i, layer in enumerate(self.layers):
                p = f"layers.{i}"
                for name in ("ln1", "ln2"):
                    ln = getattr(layer, name)
                    state[f"{p}.{name}.weight"] = ln.weight.data.cpu()
                    state[f"{p}.{name}.bias"] = ln.bias.data.cpu()
                for name in ("attn_qkv", "attn_proj", "ffn_up", "ffn_down"):
                    lin = getattr(layer, name)
                    state[f"{p}.{name}.weight"] = lin.weight.data.cpu()
                    state[f"{p}.{name}.bias"] = lin.bias.data.cpu()
            state["ln_final.weight"] = self.ln_final.weight.data.cpu()
            state["ln_final.bias"] = self.ln_final.bias.data.cpu()
            state["lm_head.weight"] = self.lm_head.weight.data.cpu()
            state["lm_head.bias"] = self.lm_head.bias.data.cpu()
            return state

        def load_safetensors_dict(self, state: dict) -> None:
            self.token_emb.weight.data.copy_(state["token_emb.weight"])
            self.pos_emb.weight.data.copy_(state["pos_emb.weight"])
            for i, layer in enumerate(self.layers):
                p = f"layers.{i}"
                for name in ("ln1", "ln2"):
                    ln = getattr(layer, name)
                    ln.weight.data.copy_(state[f"{p}.{name}.weight"])
                    ln.bias.data.copy_(state[f"{p}.{name}.bias"])
                for name in ("attn_qkv", "attn_proj", "ffn_up", "ffn_down"):
                    lin = getattr(layer, name)
                    lin.weight.data.copy_(state[f"{p}.{name}.weight"])
                    lin.bias.data.copy_(state[f"{p}.{name}.bias"])
            self.ln_final.weight.data.copy_(state["ln_final.weight"])
            self.ln_final.bias.data.copy_(state["ln_final.bias"])
            self.lm_head.weight.data.copy_(state["lm_head.weight"])
            self.lm_head.bias.data.copy_(state["lm_head.bias"])

    return _MiniGPT


MiniGPT = None  # set on first use


def _get_model_class():
    global MiniGPT
    if MiniGPT is None:
        MiniGPT = _lazy_module_classes()
    return MiniGPT


def load_config(path: Path) -> dict:
    if path.exists():
        with path.open() as f:
            return json.load(f)
    return {
        "vocab_size": 8192,
        "n_layers": 4,
        "n_heads": 4,
        "hidden_size": 256,
        "intermediate_size": 512,
        "max_seq_len": 256,
    }


def load_tokenizer(path: Path) -> dict:
    with path.open() as f:
        return json.load(f)


def load_corpus(
    corpus_paths: list[Path],
    char_to_id: dict[str, int],
    unk_id: int,
    eos_id: int,
    ctx_len: int,
) -> list[list[int]]:
    all_ids: list[int] = []
    for path in corpus_paths:
        with path.open("r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    obj = json.loads(line)
                    text = obj.get("text", "")
                except json.JSONDecodeError:
                    text = line
                for ch in text:
                    all_ids.append(char_to_id.get(ch, unk_id))
                all_ids.append(eos_id)

    sequences = []
    stride = ctx_len // 2
    for start in range(0, len(all_ids) - ctx_len, stride):
        sequences.append(all_ids[start : start + ctx_len])
    if len(all_ids) >= ctx_len:
        sequences.append(all_ids[-ctx_len:])
    return sequences


def main() -> None:
    parser = argparse.ArgumentParser(description="Train / quantize / eval MiniGPT LM")
    sub = parser.add_subparsers(dest="command", required=True)

    p_train = sub.add_parser("train", help="Train model from corpus")
    p_train.add_argument("--corpus", type=Path, nargs="+", required=True)
    p_train.add_argument(
        "--tokenizer", type=Path,
        default=WORKSPACE / "assets" / "lm" / "tokenizer.json",
    )
    p_train.add_argument(
        "--config", type=Path,
        default=WORKSPACE / "assets" / "lm" / "config.json",
    )
    p_train.add_argument(
        "--output", type=Path,
        default=WORKSPACE / "assets" / "lm" / "model.safetensors",
    )
    p_train.add_argument("--epochs", type=int, default=10)
    p_train.add_argument("--batch-size", type=int, default=64)
    p_train.add_argument("--lr", type=float, default=3e-4)
    p_train.add_argument(
        "--init-from",
        type=Path,
        help="Optional safetensors checkpoint to continue training from",
    )

    p_quant = sub.add_parser("quantize", help="Quantize model to INT8")
    p_quant.add_argument(
        "--input", type=Path,
        default=WORKSPACE / "assets" / "lm" / "model.safetensors",
    )
    p_quant.add_argument(
        "--output", type=Path,
        default=WORKSPACE / "assets" / "lm" / "model-q8.safetensors",
    )

    p_eval = sub.add_parser("eval", help="Evaluate model perplexity")
    p_eval.add_argument(
        "--input", type=Path,
        default=WORKSPACE / "assets" / "lm" / "model.safetensors",
    )
    p_eval.add_argument("--corpus", type=Path, nargs="+", required=True)
    p_eval.add_argument(
        "--tokenizer", type=Path,
        default=WORKSPACE / "assets" / "lm" / "tokenizer.json",
    )
    p_eval.add_argument(
        "--config", type=Path,
        default=WORKSPACE / "assets" / "lm" / "config.json",
    )

    args = parser.parse_args()

    # lazy-init model class only when needed
    global MiniGPT
    if args.command in ("train", "eval"):
        _get_model_class()

    if args.command == "train":
        cmd_train(args)
    elif args.command == "quantize":
        cmd_quantize(args)
    elif args.command == "eval":
        cmd_eval(args)


if __name__ == "__main__":
    main()
