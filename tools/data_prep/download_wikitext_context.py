from __future__ import annotations

import argparse
import json
import sys
import urllib.request
from pathlib import Path


WORKSPACE = Path(__file__).resolve().parents[2]
LOCAL_TOOL_PACKAGES = WORKSPACE / ".cache" / "python-tools"
if LOCAL_TOOL_PACKAGES.exists():
    sys.path.insert(0, str(LOCAL_TOOL_PACKAGES))

WIKITEXT_PARQUET_FILES = {
    "train": "https://huggingface.co/datasets/Salesforce/wikitext/resolve/main/wikitext-2-raw-v1/train-00000-of-00001.parquet",
    "validation": "https://huggingface.co/datasets/Salesforce/wikitext/resolve/main/wikitext-2-raw-v1/validation-00000-of-00001.parquet",
    "test": "https://huggingface.co/datasets/Salesforce/wikitext/resolve/main/wikitext-2-raw-v1/test-00000-of-00001.parquet",
}


def main() -> None:
    args = parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    parquet_files = download_parquet_files(args.output_dir)
    jsonl_path = args.output_dir / "wikitext-2-raw-v1.jsonl"
    rows = write_jsonl(parquet_files, jsonl_path)
    print(f"output={jsonl_path} rows={rows}")


def download_parquet_files(output_dir: Path) -> list[tuple[str, Path]]:
    files = []
    for split, url in WIKITEXT_PARQUET_FILES.items():
        path = output_dir / f"{split}.parquet"
        if not path.exists() or path.stat().st_size == 0:
            tmp_path = path.with_suffix(".parquet.tmp")
            if tmp_path.exists():
                tmp_path.unlink()
            request = urllib.request.Request(
                url,
                headers={"User-Agent": "mocha-keyboard-artifact-builder/1.0"},
            )
            with urllib.request.urlopen(request) as response, tmp_path.open("wb") as handle:
                while True:
                    chunk = response.read(1024 * 1024)
                    if not chunk:
                        break
                    handle.write(chunk)
            tmp_path.replace(path)
        files.append((split, path))
    return files


def write_jsonl(parquet_files: list[tuple[str, Path]], output: Path) -> int:
    import pyarrow.parquet as pq

    rows = 0
    with output.open("w", encoding="utf-8", newline="\n") as handle:
        for split, path in parquet_files:
            table = pq.read_table(path, columns=["text"])
            for text in table.column("text").to_pylist():
                text = str(text).strip()
                if not text or text.startswith("="):
                    continue
                handle.write(
                    json.dumps(
                        {
                            "source": "Salesforce/wikitext:wikitext-2-raw-v1",
                            "split": split,
                            "text": text,
                        },
                        ensure_ascii=False,
                        separators=(",", ":"),
                    )
                    + "\n"
                )
                rows += 1
    return rows


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=WORKSPACE / "datasets" / "context" / "en-wikitext-2-raw-v1",
    )
    return parser.parse_args()


if __name__ == "__main__":
    main()
