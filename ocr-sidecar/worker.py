#!/usr/bin/env python3
"""
Persistent OCR Worker for HaxAssistant (HexBox).
Provides:
- PersistentOcrRunner: reusable neural card detection (or preset fallback) +
  PaddleOCR ONNX batch recognition + RapidFuzz augment dictionary matching +
  AutoWatcher multi-frame stabilization.
- main(): line-delimited JSON-RPC stdin/stdout loop for Tauri desktop integration.
"""
from __future__ import annotations

import json
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


class PersistentOcrRunner:
    def __init__(
        self,
        performance_mode: str = "balanced",
        lang: str = "ch",
        backend: str = "onnx",
    ) -> None:
        self.performance_mode = performance_mode
        self.lang = lang
        self.backend = backend
        self._recognizer: Optional[PpOcrOnnxRecognizer] = None
        self._locator: Optional[CardLocator] = None
        self._locator_failed = False
        self.watcher = AutoWatcher()

    def _get_recognizer(self) -> PpOcrOnnxRecognizer:
        if self._recognizer is None:
            ocr_threads, _ = _threads_for_mode(self.performance_mode)
            model_path, keys_path = resolve_model_paths(
                backend=self.backend,
                lang=self.lang,
            )
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
    ) -> Dict[str, Any]:
        t_start = time.perf_counter()

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
    ) -> Dict[str, Any]:
        cycle = self.run_pipeline(
            preset_key=preset_key,
            screen_image_path=screen_image_path,
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


def main() -> None:
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
                _write_json_line({"id": req_id, "status": "ok", "pong": True, "protocol": 1})
            elif method == "run_pipeline":
                if params.get("performance_mode") and params["performance_mode"] != runner.performance_mode:
                    runner = PersistentOcrRunner(
                        performance_mode=str(params["performance_mode"]),
                        lang=str(params.get("lang") or runner.lang),
                        backend=str(params.get("backend") or runner.backend),
                    )
                out = runner.run_pipeline(
                    preset_key=str(params.get("preset_key") or "1920x1080@100"),
                    screen_image_path=params.get("screen_image_path"),
                )
                out["id"] = req_id
                _write_json_line(out)
            elif method == "watch_tick":
                out = runner.watch_tick(
                    preset_key=str(params.get("preset_key") or "1920x1080@100"),
                    screen_image_path=params.get("screen_image_path"),
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
