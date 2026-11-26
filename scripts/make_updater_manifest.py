#!/usr/bin/env python3
"""Build the Tauri updater manifest (latest.json) from signed release assets.

Each platform's updater bundle sits next to a minisign `.sig` file produced by
`tauri build` when TAURI_SIGNING_PRIVATE_KEY is set:

    *-setup.exe          windows-x86_64   (NSIS installer)
    *_aarch64.app.tar.gz darwin-aarch64
    *_x64.app.tar.gz     darwin-x86_64
    *.AppImage           linux-x86_64

Usage:
    python scripts/make_updater_manifest.py <assets-dir> --version 1.2.0 \
        --base-url https://github.com/<owner>/<repo>/releases/download/v1.2.0 \
        --out <assets-dir>/latest.json
"""
from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote

PLATFORM_SUFFIXES = [
    ("-setup.exe", "windows-x86_64"),
    ("_aarch64.app.tar.gz", "darwin-aarch64"),
    ("_x64.app.tar.gz", "darwin-x86_64"),
    (".AppImage", "linux-x86_64"),
]


def platform_for(file_name: str) -> str | None:
    for suffix, platform in PLATFORM_SUFFIXES:
        if file_name.endswith(suffix):
            return platform
    return None


def build_manifest(assets: Path, version: str, base_url: str, notes: str) -> dict:
    platforms: dict[str, dict[str, str]] = {}
    for sig in sorted(assets.glob("*.sig")):
        bundle = sig.with_suffix("")  # strip ".sig"
        platform = platform_for(bundle.name)
        if platform is None or not bundle.exists():
            continue
        platforms[platform] = {
            "signature": sig.read_text(encoding="utf-8").strip(),
            "url": f"{base_url.rstrip('/')}/{quote(bundle.name)}",
        }
    return {
        "version": version,
        "notes": notes,
        "pub_date": datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
        "platforms": platforms,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("assets", type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--notes", default="")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    manifest = build_manifest(args.assets, args.version, args.base_url, args.notes)
    if not manifest["platforms"]:
        print("error: no signed updater bundles found", file=sys.stderr)
        return 1
    args.out.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {args.out} for {', '.join(sorted(manifest['platforms']))}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
