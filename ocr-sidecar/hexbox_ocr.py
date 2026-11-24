#!/usr/bin/env python3
"""
Single entrypoint for the HexBox OCR sidecar and its CLI modes:
- probe: verify ONNX models, runtime dependencies, and output capabilities JSON
- capture-probe: test screen capture via mss
- service: run single-image or multi-image OCR recognition
- preset: run full 3-card detection + OCR + RapidFuzz dictionary matching
- worker: run persistent Stdin/Stdout JSON-RPC worker for Tauri desktop app
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

os.environ.setdefault("ARAM_ASSISTANT_DATA_DIR", str(HERE.parent / "data"))
os.environ.setdefault("ARAM_ASSISTANT_OCR_MODEL_DIR", str(HERE.parent / "models"))


def run_probe() -> None:
    import numpy
    import onnxruntime
    import PIL
    import rapidfuzz

    from card_locator import find_detector_dir, shared_locator
    from service import resolve_model_paths

    det_dir = find_detector_dir()
    locator = shared_locator(det_dir)
    rec_model, keys_path = resolve_model_paths()

    payload = {
        "protocol": 1,
        "compiled": False,
        "crossPlatformDebuggable": True,
        "models": {
            "card_detector": str(det_dir / "own-card-detector.onnx"),
            "card_detector_input_size": locator.size,
            "ocr_rec_model": str(rec_model),
            "ocr_keys_path": str(keys_path),
            "models_present": rec_model.exists() and keys_path.exists(),
        },
        "modules": {
            "augment_pipeline": True,
            "card_locator": True,
            "service": True,
            "worker": True,
        },
        "runtime": {
            "numpy": numpy.__version__,
            "onnxruntime": onnxruntime.__version__,
            "Pillow": PIL.__version__,
            "rapidfuzz": rapidfuzz.__version__,
        },
    }
    print(json.dumps(payload, ensure_ascii=False, indent=2))


def run_capture_probe() -> None:
    from augment_pipeline import capture_screen_image

    img, ctx = capture_screen_image()
    print(json.dumps({"status": "ok", "width": img.width, "height": img.height, "context": ctx}, ensure_ascii=False))


def run_preset_cli(argv: list[str]) -> None:
    parser = argparse.ArgumentParser(description="Run 3-card Hextech Augment OCR pipeline")
    parser.add_argument("--preset", default="1920x1080@100", help="Resolution preset key")
    parser.add_argument("--image", default=None, help="Optional screenshot path (if omitted, grabs live screen)")
    parser.add_argument("--performance-mode", default="balanced", choices=["eco", "balanced", "performance"])
    args = parser.parse_args(argv)

    from worker import PersistentOcrRunner

    runner = PersistentOcrRunner(performance_mode=args.performance_mode)
    result = runner.run_pipeline(preset_key=args.preset, screen_image_path=args.image)
    print(json.dumps(result, ensure_ascii=False, indent=2))


def main() -> None:
    argv = list(sys.argv[1:])
    if not argv or argv[0] in ("-h", "--help"):
        print("Usage: hexbox_ocr.py <probe|capture-probe|preset|service|worker> [options]")
        return

    mode = argv.pop(0)
    if mode == "probe":
        run_probe()
    elif mode == "capture-probe":
        run_capture_probe()
    elif mode == "preset":
        run_preset_cli(argv)
    elif mode == "service":
        sys.argv = [sys.argv[0]] + argv
        import service

        service.main()
    elif mode == "worker":
        import worker

        worker.main()
    else:
        raise SystemExit(f"unknown OCR mode: {mode} (expected worker, preset, service, probe, or capture-probe)")


if __name__ == "__main__":
    main()
