from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path


WORKSPACE = Path(__file__).resolve().parents[2]

SPECIAL_TOKENS = {"<pad>": 0, "<unk>": 1, "<eos>": 2}
NUM_SPECIAL = len(SPECIAL_TOKENS)

ASCII_RANGE = range(0x20, 0x7F)
CJK_UNIFIED = range(0x4E00, 0xA000)
CJK_EXT_A = range(0x3400, 0x4DC0)
CJK_SYMBOLS = range(0x3000, 0x3040)
FULLWIDTH = range(0xFF00, 0xFF61)


def count_chars(corpus_paths: list[Path], limit: int | None = None) -> Counter:
    counts: Counter = Counter()
    total = 0
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
                    counts[ch] += 1
                total += 1
                if limit and total >= limit:
                    break
            if limit and total >= limit:
                break
    return counts


def build_vocab(
    counts: Counter, vocab_size: int
) -> dict[str, int]:
    vocab: dict[str, int] = {}
    next_id = NUM_SPECIAL

    for code in ASCII_RANGE:
        ch = chr(code)
        if next_id >= vocab_size:
            break
        vocab[ch] = next_id
        next_id += 1

    cjk_from_corpus = []
    for ch, _count in counts.most_common():
        cp = ord(ch)
        if (
            cp in CJK_UNIFIED
            or cp in CJK_EXT_A
            or cp in CJK_SYMBOLS
            or cp in FULLWIDTH
        ):
            cjk_from_corpus.append(ch)

    for ch in cjk_from_corpus:
        if next_id >= vocab_size:
            break
        if ch not in vocab:
            vocab[ch] = next_id
            next_id += 1

    for cp in CJK_UNIFIED:
        if next_id >= vocab_size:
            break
        ch = chr(cp)
        if ch not in vocab:
            vocab[ch] = next_id
            next_id += 1

    for rng in (CJK_SYMBOLS, FULLWIDTH):
        for cp in rng:
            if next_id >= vocab_size:
                break
            ch = chr(cp)
            if ch not in vocab:
                vocab[ch] = next_id
                next_id += 1

    remaining = [
        (ch, cnt)
        for ch, cnt in counts.most_common()
        if ch not in vocab and ord(ch) >= 0x80
    ]
    for ch, _cnt in remaining:
        if next_id >= vocab_size:
            break
        vocab[ch] = next_id
        next_id += 1

    return vocab


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Build character-level vocabulary from corpus"
    )
    parser.add_argument(
        "--corpus",
        type=Path,
        nargs="+",
        required=True,
        help="JSONL corpus files",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=WORKSPACE / "assets" / "lm" / "tokenizer.json",
        help="Output tokenizer JSON",
    )
    parser.add_argument(
        "--vocab-size",
        type=int,
        default=8192,
        help="Target vocabulary size (default: 8192)",
    )
    parser.add_argument(
        "--sample-limit",
        type=int,
        default=None,
        help="Limit number of lines to scan for character frequencies",
    )
    args = parser.parse_args()

    for path in args.corpus:
        if not path.exists():
            print(f"ERROR: corpus file not found: {path}", file=sys.stderr)
            sys.exit(1)

    print(f"scanning {len(args.corpus)} corpus file(s)...")
    counts = count_chars(args.corpus, limit=args.sample_limit)
    print(f"  unique characters: {len(counts)}")

    char_to_id = build_vocab(counts, args.vocab_size)
    id_to_char = {v: k for k, v in char_to_id.items()}

    actual_size = NUM_SPECIAL + len(char_to_id)
    print(f"  vocab size: {actual_size} ({NUM_SPECIAL} special + {len(char_to_id)} chars)")

    tokenizer = {
        "vocab_size": actual_size,
        "special_tokens": SPECIAL_TOKENS,
        "char_to_id": char_to_id,
        "id_to_char": {str(k): v for k, v in id_to_char.items()},
    }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as f:
        json.dump(tokenizer, f, ensure_ascii=False, indent=2)
    print(f"output={args.output}")


if __name__ == "__main__":
    main()
