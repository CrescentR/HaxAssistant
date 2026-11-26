#!/usr/bin/env python3
"""Download the ONNX OCR models listed in scripts/models.lock.json into models/.

The models are too large for Git, so CI fetches them before bundling. Files
that already exist with the right SHA-256 are left alone.

Usage:
    python scripts/fetch_models.py            # download missing models
    python scripts/fetch_models.py --check    # only verify, exit 1 if any is missing
    python scripts/fetch_models.py --optional # warn instead of failing

Environment:
    HEXBOX_MODELS_URL  override the download base URL from the lock file
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "scripts" / "models.lock.json"
MODELS_DIR = ROOT / "models"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def download(url: str, dest: Path) -> None:
    tmp = dest.with_suffix(dest.suffix + ".part")
    req = urllib.request.Request(url, headers={"User-Agent": "hexbox-build"})
    with urllib.request.urlopen(req, timeout=120) as resp, tmp.open("wb") as out:
        while True:
            chunk = resp.read(1 << 20)
            if not chunk:
                break
            out.write(chunk)
    tmp.replace(dest)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="verify only, no download")
    parser.add_argument("--optional", action="store_true", help="never exit non-zero")
    args = parser.parse_args()

    lock = json.loads(LOCK.read_text(encoding="utf-8"))
    base_url = os.environ.get("HEXBOX_MODELS_URL", lock["base_url"]).rstrip("/")
    MODELS_DIR.mkdir(parents=True, exist_ok=True)

    failed: list[str] = []
    for name, expected in lock["models"].items():
        dest = MODELS_DIR / name
        if dest.exists() and sha256_file(dest) == expected:
            print(f"ok       {name}")
            continue
        if args.check:
            print(f"missing  {name}")
            failed.append(name)
            continue
        url = f"{base_url}/{name}"
        print(f"fetch    {url}")
        try:
            download(url, dest)
        except Exception as exc:  # noqa: BLE001 - report and continue with the rest
            print(f"error    {name}: {exc}", file=sys.stderr)
            failed.append(name)
            continue
        got = sha256_file(dest)
        if got != expected:
            dest.unlink(missing_ok=True)
            print(f"error    {name}: sha256 {got} != {expected}", file=sys.stderr)
            failed.append(name)
        else:
            print(f"ok       {name}")

    if failed:
        msg = f"{len(failed)} model(s) unavailable: {', '.join(failed)}"
        if args.optional:
            print(f"warning: {msg}; the bundle will ship without them", file=sys.stderr)
            return 0
        print(f"error: {msg}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
