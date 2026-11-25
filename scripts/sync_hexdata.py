#!/usr/bin/env python3
"""
Hexdata Data Sync & Integrity Tool (`scripts/sync_hexdata.py`).
Supports:
- `--prepare-local`: regenerate frontend-compatible JSONs & derived datasets from offline files
- `--update-manifest`: recompute manifest coverage, file SHA-256 hashes, and `core_payload_hash`
- `--source-url <URL>`: optional remote manifest/file sync (falls back gracefully to local data)
- `--verify`: verify `data/manifest.json` presence, non-empty datasets, and optional SHA-256 hashes
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List

ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = ROOT / "data"


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    h.update(path.read_bytes())
    return h.hexdigest()


def compute_core_payload_hash(files: List[str]) -> str:
    h = hashlib.sha256()
    for fname in sorted(files):
        p = DATA_DIR / fname
        if p.exists():
            h.update(fname.encode("utf-8"))
            h.update(b":")
            h.update(sha256_file(p).encode("utf-8"))
            h.update(b"\n")
    return h.hexdigest()


def update_manifest() -> Dict[str, Any]:
    manifest_path = DATA_DIR / "manifest.json"
    manifest: Dict[str, Any] = (
        json.loads(manifest_path.read_text(encoding="utf-8"))
        if manifest_path.exists()
        else {
            "artifact": "aram-assistant-runtime-data",
            "version": "16.22",
            "schema_version": "1",
            "files": [],
        }
    )

    files: List[str] = list(manifest.get("files", []))
    for extra in ("hero_synergies_counters.json", "augments_full.json", "augment_rarity.json"):
        if (DATA_DIR / extra).exists() and extra not in files:
            files.append(extra)
    files = sorted(set(files))

    champions = json.loads((DATA_DIR / "champions.json").read_text(encoding="utf-8")) if (DATA_DIR / "champions.json").exists() else []
    augments = json.loads((DATA_DIR / "augments_full.json").read_text(encoding="utf-8")) if (DATA_DIR / "augments_full.json").exists() else []
    trios = json.loads((DATA_DIR / "hero_augment_trios.json").read_text(encoding="utf-8")) if (DATA_DIR / "hero_augment_trios.json").exists() else {}

    file_hashes = {
        fname: sha256_file(DATA_DIR / fname)
        for fname in files
        if (DATA_DIR / fname).exists()
    }

    manifest["files"] = files
    manifest["file_hashes"] = file_hashes
    manifest["hero_count"] = len(champions)
    manifest["augment_count"] = len(augments) or manifest.get("augment_count", 0)
    manifest["hero_trio_coverage"] = len(trios)
    manifest["current_patch_hero_trio_coverage"] = len(trios)
    manifest["core_payload_hash"] = compute_core_payload_hash(files)
    manifest["generated_at"] = datetime.now(timezone.utc).isoformat()

    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8")
    return manifest


def sync_from_remote(source_url: str, timeout: int = 10) -> bool:
    base = source_url.rstrip("/")
    try:
        with urllib.request.urlopen(f"{base}/manifest.json", timeout=timeout) as resp:
            remote_manifest = json.loads(resp.read().decode("utf-8"))
    except (urllib.error.URLError, TimeoutError, ValueError) as exc:
        print(f"WARN: remote sync skipped ({exc}); keeping local data.")
        return False

    remote_files: List[str] = remote_manifest.get("files", [])
    remote_hashes: Dict[str, str] = remote_manifest.get("file_hashes", {})
    downloaded = 0

    for fname in remote_files:
        target = DATA_DIR / fname
        expected_hash = remote_hashes.get(fname)
        if target.exists() and expected_hash and sha256_file(target) == expected_hash:
            continue
        try:
            with urllib.request.urlopen(f"{base}/{fname}", timeout=timeout) as resp:
                raw = resp.read()
            if expected_hash and hashlib.sha256(raw).hexdigest() != expected_hash:
                print(f"WARN: hash mismatch for remote {fname}; skipping")
                continue
            target.write_bytes(raw)
            downloaded += 1
        except (urllib.error.URLError, TimeoutError) as exc:
            print(f"WARN: failed to fetch {fname}: {exc}")

    (DATA_DIR / "manifest.json").write_text(
        json.dumps(remote_manifest, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(f"Remote sync finished from {base}: downloaded {downloaded} updated file(s).")
    return True


def verify_dataset(check_hashes: bool = False) -> int:
    manifest_path = DATA_DIR / "manifest.json"
    if not manifest_path.exists():
        print("ERROR: data/manifest.json not found")
        return 1

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    files = manifest.get("files", [])
    missing = [f for f in files if not (DATA_DIR / f).exists()]
    empty_stubs: List[str] = []
    hash_mismatches: List[str] = []
    file_hashes: Dict[str, str] = manifest.get("file_hashes", {})

    for f in files:
        p = DATA_DIR / f
        if not p.exists():
            continue
        if p.stat().st_size <= 4:
            empty_stubs.append(f)
        if check_hashes and f in file_hashes:
            if sha256_file(p) != file_hashes[f]:
                hash_mismatches.append(f)

    report = {
        "version": manifest.get("version"),
        "report_date": manifest.get("report_date"),
        "manifest_sha256": sha256_file(manifest_path),
        "expected_files": len(files),
        "present_files": len(files) - len(missing),
        "missing_files": missing,
        "empty_stubs": empty_stubs,
        "hash_mismatches": hash_mismatches,
        "hero_trio_coverage": manifest.get("hero_trio_coverage", 0),
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if (not missing and not empty_stubs and not hash_mismatches) else 2


def main() -> None:
    parser = argparse.ArgumentParser(description="HexBox data sync and verification tool")
    parser.add_argument("--prepare-local", action="store_true", help="Run prepare_dev_data.py locally")
    parser.add_argument("--update-manifest", action="store_true", help="Recompute manifest hashes and coverage")
    parser.add_argument("--verify", action="store_true", help="Verify data/manifest.json coverage")
    parser.add_argument("--check-hashes", action="store_true", help="Verify per-file SHA-256 hashes")
    parser.add_argument(
        "--source-url",
        default=os.environ.get("HEXBOX_DATA_SYNC_URL", ""),
        help="Optional remote HTTP base URL serving manifest.json and data files",
    )
    args = parser.parse_args()

    if args.source_url:
        sync_from_remote(args.source_url)

    if args.prepare_local or (not args.verify and not args.update_manifest and not args.source_url):
        import prepare_dev_data

        prepare_dev_data.main()

    if args.update_manifest:
        update_manifest()

    if args.verify or args.check_hashes or (not args.prepare_local and not args.update_manifest):
        raise SystemExit(verify_dataset(check_hashes=args.check_hashes))


if __name__ == "__main__":
    main()
