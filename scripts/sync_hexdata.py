#!/usr/bin/env python3
"""
Hexdata Data Sync & Integrity Tool (`scripts/sync_hexdata.py`).
Supports:
- `--prepare-local`: regenerate frontend-compatible JSONs from extracted offline datasets
- `--verify`: verify `data/manifest.json` presence and report dataset counts
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = ROOT / "data"


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    h.update(path.read_bytes())
    return h.hexdigest()


def verify_dataset() -> int:
    manifest_path = DATA_DIR / "manifest.json"
    if not manifest_path.exists():
        print("ERROR: data/manifest.json not found")
        return 1

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    files = manifest.get("files", [])
    missing = [f for f in files if not (DATA_DIR / f).exists()]
    print(
        json.dumps(
            {
                "version": manifest.get("version"),
                "report_date": manifest.get("report_date"),
                "manifest_sha256": sha256_file(manifest_path),
                "expected_files": len(files),
                "present_files": len(files) - len(missing),
                "missing_files": missing,
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0 if not missing else 2


def main() -> None:
    parser = argparse.ArgumentParser(description="HexBox data sync and verification tool")
    parser.add_argument("--prepare-local", action="store_true", help="Run prepare_dev_data.py locally")
    parser.add_argument("--verify", action="store_true", help="Verify data/manifest.json coverage")
    args = parser.parse_args()

    if args.prepare_local or not args.verify:
        import prepare_dev_data

        prepare_dev_data.main()

    if args.verify or not args.prepare_local:
        raise SystemExit(verify_dataset())


if __name__ == "__main__":
    main()
