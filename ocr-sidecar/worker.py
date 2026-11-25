#!/usr/bin/env python3
"""
Persistent OCR Worker for HaxAssistant (HexBox).
Provides:
- PersistentOcrRunner: reusable neural card detection (or preset fallback) +
  PaddleOCR ONNX batch recognition + RapidFuzz augment dictionary matching +
  AutoWatcher multi-frame stabilization + dynamic language switching +
  foreground LoL window guard.
- main(): line-delimited JSON-RPC stdin/stdout loop for Tauri desktop integration,
  plus `--oneshot` CLI mode for single-shot fallback when the persistent pipe fails.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Dict, List, Optional

from PIL import Image

from augment_pipeline import (
    AutoWatcher,
    Rect,
    capture_screen_image,
    crop_rect_to_image,
    get_preset,
    match_ocr_texts,
    probe_live_frame,
)
from card_locator import CardLocator, set_locator_threads, shared_locator
from service import PpOcrOnnxRecognizer, resolve_model_paths


def _threads_for_mode(performance_mode: str) -> tuple[int, int]:
    mode = (performance_mode or "balanced").lower()
    if mode == "eco":
        return (2, 1)
    if mode == "performance":
        return (6, 3)
    return (4, 2)


def _normalize_lang(lang: Optional[str]) -> str:
    raw = (lang or "ch").strip().lower()
    if raw.startswith("ko"):
        return "ko"
    if raw.startswith("en"):
        return "en"
    return "ch"


def get_foreground_window_title() -> Optional[str]:
    """Best-effort active window title/process name. Returns None if unavailable."""
    if sys.platform == "win32":
        try:
            import ctypes

            user32 = ctypes.windll.user32
            hwnd = user32.GetForegroundWindow()
            if not hwnd:
                return None
            length = user32.GetWindowTextLengthW(hwnd)
            if length <= 0:
                return None
            buf = ctypes.create_unicode_buffer(length + 1)
            user32.GetWindowTextW(hwnd, buf, length + 1)
            return buf.value
        except Exception:
            return None
    if sys.platform == "darwin":
        try:
            out = subprocess.run(
                [
                    "osascript",
                    "-e",
                    'tell application "System Events" to get name of first application process whose frontmost is true',
                ],
                capture_output=True,
                text=True,
                timeout=0.35,
            )
            if out.returncode == 0 and out.stdout.strip():
                return out.stdout.strip()
        except Exception:
            return None
    return None


def is_lol_foreground_window() -> tuple[bool, Optional[str]]:
    """
    Returns (is_lol_or_unknown, window_title).
    Fails open (True) when OS cannot query foreground window so OCR is never stuck.
    """
    title = get_foreground_window_title()
    if not title:
        return True, None
    lower = title.lower()
    keywords = (
        "league of legends",
        "leagueclient",
        "riot",
        "英雄联盟",
        "리그 오브 레전드",
        "hexbox",
    )
    return any(k in lower for k in keywords), title


class PersistentOcrRunner:
    def __init__(
        self,
        performance_mode: str = "balanced",
        lang: str = "ch",
        backend: str = "onnx",
    ) -> None:
        self.performance_mode = performance_mode
        self.lang = _normalize_lang(lang)
        self.backend = backend
        self._recognizer: Optional[PpOcrOnnxRecognizer] = None
        self._locator: Optional[CardLocator] = None
        self._locator_failed = False
        self.watcher = AutoWatcher()

    def reconfigure(
        self,
        performance_mode: Optional[str] = None,
        lang: Optional[str] = None,
        backend: Optional[str] = None,
    ) -> None:
        next_mode = performance_mode or self.performance_mode
        next_lang = _normalize_lang(lang) if lang else self.lang
        next_backend = backend or self.backend
        if (
            next_mode != self.performance_mode
            or next_lang != self.lang
            or next_backend != self.backend
        ):
            self.performance_mode = next_mode
            self.lang = next_lang
            self.backend = next_backend
            self._recognizer = None
            self._locator = None
            self._locator_failed = False

    def _get_recognizer(self) -> PpOcrOnnxRecognizer:
        if self._recognizer is None:
            ocr_threads, _ = _threads_for_mode(self.performance_mode)
            model_path, keys_path = resolve_model_paths(
                backend=self.backend,
                lang=self.lang,
            )
            # Fall back to Chinese v4 model if requested locale model is absent.
            if not model_path.exists() and self.lang != "ch":
                model_path, keys_path = resolve_model_paths(backend="onnx", lang="ch")
            self._recognizer = PpOcrOnnxRecognizer(
                model_path=model_path,
                keys_path=keys_path,
                threads=ocr_threads,
            )
        return self._recognizer

    def _get_locator(self) -> Optional[CardLocator]:
        if self._locator is not None:
            return self._locator
        if self._locator_failed:
            return None
        try:
            _, loc_threads = _threads_for_mode(self.performance_mode)
            set_locator_threads(loc_threads)
            self._locator = shared_locator()
            return self._locator
        except Exception:
            self._locator_failed = True
            return None

    def _run_rects(
        self,
        image: Image.Image,
        rects: List[Rect],
    ) -> tuple[List[Dict[str, Any]], List[Dict[str, Any]], float, float]:
        recognizer = self._get_recognizer()
        crops = [crop_rect_to_image(image, r) for r in rects]

        t_ocr = time.perf_counter()
        raw_ocr = recognizer.recognize_image_batch(crops)
        ocr_ms = (time.perf_counter() - t_ocr) * 1000.0

        t_match = time.perf_counter()
        matches = match_ocr_texts(raw_ocr)
        match_ms = (time.perf_counter() - t_match) * 1000.0
        return raw_ocr, matches, ocr_ms, match_ms

    def run_pipeline(
        self,
        preset_key: str = "1920x1080@100",
        screen_image_path: Optional[str] = None,
        allow_non_lol_foreground: bool = True,
    ) -> Dict[str, Any]:
        t_start = time.perf_counter()

        # 0. Foreground window guard (for live screen capture only)
        if not screen_image_path and not allow_non_lol_foreground:
            is_lol, fg_title = is_lol_foreground_window()
            if not is_lol:
                return {
                    "status": "skipped",
                    "reason": "non_lol_foreground",
                    "foreground_window": fg_title,
                    "preset_key": preset_key,
                    "lang": self.lang,
                    "result": [],
                    "ocr_matches": [],
                }

        # 1. Acquire image
        t_cap = time.perf_counter()
        capture_meta: Dict[str, Any] = {}
        if screen_image_path:
            img_path = Path(screen_image_path)
            if not img_path.exists():
                return {
                    "status": "error",
                    "error": f"screen image not found: {img_path}",
                    "result": [],
                    "ocr_matches": [],
                }
            with Image.open(img_path) as opened:
                image = opened.convert("RGB")
            capture_meta = {"source": "file", "path": str(img_path), "image_size": list(image.size)}
        else:
            image, capture_meta = capture_screen_image(preset_key=preset_key)
            capture_meta["source"] = "screen"
        capture_ms = (time.perf_counter() - t_cap) * 1000.0

        preset = get_preset(preset_key=preset_key, image_size=image.size)

        # 2. Locate 3 card title rects (neural detector first, fallback to resolution preset)
        t_loc = time.perf_counter()
        locator = self._get_locator()
        locate_source = "preset"
        neural_rects: List[Rect] = []
        neural_cards: Optional[List[Dict[str, Any]]] = None

        if locator is not None:
            try:
                loc_out = locator.locate(image)
                if loc_out.get("cards") and len(loc_out.get("title_rects") or []) == 3:
                    neural_cards = loc_out["cards"]
                    neural_rects = [
                        Rect(
                            x=int(r["x"]),
                            y=int(r["y"]),
                            width=int(r["width"]),
                            height=int(r["height"]),
                        )
                        for r in loc_out["title_rects"]
                    ]
                    locate_source = "neural-card-detector"
            except Exception:
                neural_rects = []

        locate_ms = (time.perf_counter() - t_loc) * 1000.0

        # 3. Recognize & match (if neural title band misses cards, cross-check preset coordinates)
        chosen_rects = neural_rects if neural_rects else list(preset.augment_rects)
        raw_ocr, matches, ocr_ms, match_ms = self._run_rects(image, chosen_rects)
        matched_count = sum(1 for m in matches if m.get("match_name"))

        if locate_source == "neural-card-detector" and matched_count < 3:
            p_raw, p_matches, p_ocr_ms, p_match_ms = self._run_rects(image, list(preset.augment_rects))
            p_matched = sum(1 for m in p_matches if m.get("match_name"))
            ocr_ms += p_ocr_ms
            match_ms += p_match_ms
            if p_matched > matched_count:
                chosen_rects = list(preset.augment_rects)
                raw_ocr = p_raw
                matches = p_matches
                locate_source = "preset-fallback"

        total_ms = (time.perf_counter() - t_start) * 1000.0

        return {
            "status": "ok",
            "preset_key": preset.key,
            "lang": self.lang,
            "locate_source": locate_source,
            "capture": capture_meta,
            "frame_probe": probe_live_frame(image, preset),
            "cards": neural_cards,
            "rects": [r.to_dict() for r in chosen_rects],
            "raw_ocr": raw_ocr,
            "result": matches,
            "ocr_matches": matches,
            "timings": {
                "capture_ms": round(capture_ms, 2),
                "locate_ms": round(locate_ms, 2),
                "ocr_ms": round(ocr_ms, 2),
                "match_ms": round(match_ms, 2),
                "total_ms": round(total_ms, 2),
            },
        }

    def watch_tick(
        self,
        preset_key: str = "1920x1080@100",
        screen_image_path: Optional[str] = None,
        allow_non_lol_foreground: bool = False,
    ) -> Dict[str, Any]:
        cycle = self.run_pipeline(
            preset_key=preset_key,
            screen_image_path=screen_image_path,
            allow_non_lol_foreground=allow_non_lol_foreground,
        )
        if cycle.get("status") != "ok":
            return cycle
        cycle["watcher"] = self.watcher.observe(cycle)
        return cycle

    def reset_watcher(self) -> Dict[str, Any]:
        self.watcher = AutoWatcher()
        return {"status": "ok", "state": self.watcher.state}


def _write_json_line(payload: Dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def _run_oneshot_cli(argv: List[str]) -> int:
    parser = argparse.ArgumentParser(description="HexBox OCR one-shot fallback")
    parser.add_argument("--oneshot", action="store_true")
    parser.add_argument("--preset-key", default="1920x1080@100")
    parser.add_argument("--performance-mode", default="balanced")
    parser.add_argument("--lang", default="ch")
    parser.add_argument("--screen-image", default=None)
    parser.add_argument("--allow-non-lol-foreground", action="store_true")
    args = parser.parse_args(argv)

    runner = PersistentOcrRunner(
        performance_mode=args.performance_mode,
        lang=args.lang,
    )
    out = runner.run_pipeline(
        preset_key=args.preset_key,
        screen_image_path=args.screen_image,
        allow_non_lol_foreground=args.allow_non_lol_foreground or bool(args.screen_image),
    )
    _write_json_line(out)
    return 0 if out.get("status") in ("ok", "skipped") else 1


def main() -> None:
    if "--oneshot" in sys.argv[1:]:
        raise SystemExit(_run_oneshot_cli(sys.argv[1:]))

    runner = PersistentOcrRunner()
    for raw_line in sys.stdin:
        line = raw_line.strip()
        if not line:
            continue
        req_id: Any = None
        try:
            req = json.loads(line)
            req_id = req.get("id")
            method = str(req.get("method") or "")
            params = req.get("params") or {}

            if method == "ping":
                _write_json_line({"id": req_id, "status": "ok", "pong": True, "protocol": 2})
            elif method == "run_pipeline":
                runner.reconfigure(
                    performance_mode=params.get("performance_mode"),
                    lang=params.get("lang"),
                    backend=params.get("backend"),
                )
                out = runner.run_pipeline(
                    preset_key=str(params.get("preset_key") or "1920x1080@100"),
                    screen_image_path=params.get("screen_image_path"),
                    allow_non_lol_foreground=bool(params.get("allow_non_lol_foreground", True)),
                )
                out["id"] = req_id
                _write_json_line(out)
            elif method == "watch_tick":
                runner.reconfigure(
                    performance_mode=params.get("performance_mode"),
                    lang=params.get("lang"),
                    backend=params.get("backend"),
                )
                out = runner.watch_tick(
                    preset_key=str(params.get("preset_key") or "1920x1080@100"),
                    screen_image_path=params.get("screen_image_path"),
                    allow_non_lol_foreground=bool(params.get("allow_non_lol_foreground", False)),
                )
                out["id"] = req_id
                _write_json_line(out)
            elif method == "reset_watcher":
                out = runner.reset_watcher()
                out["id"] = req_id
                _write_json_line(out)
            elif method == "shutdown":
                _write_json_line({"id": req_id, "status": "ok", "shutdown": True})
                break
            else:
                _write_json_line({
                    "id": req_id,
                    "status": "error",
                    "error": f"unknown method: {method}",
                })
        except Exception as exc:
            _write_json_line({
                "id": req_id,
                "status": "error",
                "error": str(exc),
            })


if __name__ == "__main__":
    main()
