#!/usr/bin/env python3
"""
Compile the OCR sidecar into a standalone program (`ocr/hexbox-ocr[.exe]`) so
the packaged app does not need Python on the user's machine.

Run with the interpreter that has `ocr-sidecar/requirements.txt` and
`pyinstaller` installed:

    ocr-sidecar/.venv/bin/python scripts/build_ocr_runner.py

The output folder `ocr/` is picked up by `apps/desktop/src-tauri/build.rs`
and bundled into the installer.
"""
from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SIDECAR = ROOT / "ocr-sidecar"
OUT_DIR = ROOT / "ocr"
NAME = "hexbox-ocr"


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="hexbox-ocr-build-") as tmp:
        tmp_path = Path(tmp)
        cmd = [
            sys.executable,
            "-m",
            "PyInstaller",
            "--noconfirm",
            "--clean",
            "--onedir",
            "--console",
            "--name",
            NAME,
            "--paths",
            str(SIDECAR),
            "--distpath",
            str(tmp_path / "dist"),
            "--workpath",
            str(tmp_path / "build"),
            "--specpath",
            str(tmp_path),
            str(SIDECAR / "worker.py"),
        ]
        subprocess.run(cmd, check=True, cwd=str(ROOT))
        built = tmp_path / "dist" / NAME
        if OUT_DIR.exists():
            shutil.rmtree(OUT_DIR)
        shutil.copytree(built, OUT_DIR, symlinks=True)

    exe = OUT_DIR / (NAME + (".exe" if sys.platform == "win32" else ""))
    if not exe.exists():
        print(f"expected runner not found: {exe}", file=sys.stderr)
        return 1
    print(f"OCR runner ready: {exe}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
