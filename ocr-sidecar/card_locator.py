#!/usr/bin/env python3
"""
CardLocator: Neural Hextech Augment Card Detector (hexbox-card-centres-v1)
Loads models/own-card-detector.onnx and models/own-card-detector.json.
Detects the 3 Hextech Augment cards on screen regardless of window offset or aspect ratio.
"""
from __future__ import annotations

import itertools
import json
import os
import time
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

import numpy as np
from PIL import Image

CARD_DETECTOR_MODEL = "own-card-detector.onnx"
CARD_DETECTOR_CONFIG = "own-card-detector.json"
PADDING_VALUE = 32
STRIDE = 8
TRIPLET_MIN_SCORE = 0.25


def _model_dirs() -> List[Path]:
    dirs: List[Path] = []
    env_dir = os.environ.get("HEXBOX_CARD_DETECTOR_DIR") or os.environ.get("ARAM_ASSISTANT_OCR_MODEL_DIR")
    if env_dir:
        dirs.append(Path(env_dir))
    here = Path(__file__).resolve().parent
    dirs.extend([
        here.parent / "models",
        here / "models",
        Path.cwd() / "models",
    ])
    return dirs


def find_detector_dir() -> Path:
    for d in _model_dirs():
        if (d / CARD_DETECTOR_MODEL).exists() and (d / CARD_DETECTOR_CONFIG).exists():
            return d
    raise FileNotFoundError(f"Could not locate {CARD_DETECTOR_MODEL} in {_model_dirs()}")


def letterbox_image(image: Image.Image, size: int = 512, padding: int = PADDING_VALUE) -> Tuple[np.ndarray, Dict[str, float]]:
    source = image.convert("RGB") if image.mode != "RGB" else image
    width, height = source.size
    ratio = min(size / max(1, width), size / max(1, height))
    new_w = max(1, int(round(width * ratio)))
    new_h = max(1, int(round(height * ratio)))
    left = (size - new_w) // 2
    top = (size - new_h) // 2

    resized = source.resize((new_w, new_h), Image.Resampling.BILINEAR)
    canvas = Image.new("RGB", (size, size), (padding, padding, padding))
    canvas.paste(resized, (left, top))

    arr = np.asarray(canvas, dtype=np.float32) * (1.0 / 255.0)
    tensor = np.ascontiguousarray(arr.transpose(2, 0, 1)[np.newaxis, ...])
    geometry = {
        "left": float(left),
        "top": float(top),
        "scaleX": float(new_w) / max(1.0, float(width)),
        "scaleY": float(new_h) / max(1.0, float(height)),
        "origWidth": float(width),
        "origHeight": float(height),
        "inputSize": float(size),
    }
    return tensor, geometry


def box_iou(first: Sequence[float], second: Sequence[float]) -> float:
    x1 = max(first[0], second[0])
    y1 = max(first[1], second[1])
    x2 = min(first[2], second[2])
    y2 = min(first[3], second[3])
    inter = max(0.0, x2 - x1) * max(0.0, y2 - y1)
    if inter <= 0.0:
        return 0.0
    area1 = max(0.0, first[2] - first[0]) * max(0.0, first[3] - first[1])
    area2 = max(0.0, second[2] - second[0]) * max(0.0, second[3] - second[1])
    denom = area1 + area2 - inter
    return inter / denom if denom > 0.0 else 0.0


def decode_outputs(
    heat_logits: np.ndarray,
    box_logits: np.ndarray,
    size: int = 512,
    confidence: float = 0.35,
    nms_iou: float = 0.5,
    stride: int = STRIDE,
) -> List[Dict[str, Any]]:
    prob = 1.0 / (1.0 + np.exp(-np.clip(heat_logits, -30.0, 30.0)))
    reg = np.asarray(box_logits, dtype=np.float32)

    if prob.ndim == 4:
        prob = prob[0, 0]
    elif prob.ndim == 3:
        prob = prob[0]
    if reg.ndim == 4:
        reg = reg[0]

    h, w = prob.shape
    padded = np.pad(prob, 1, mode="constant", constant_values=-1.0)
    local_max = np.maximum.reduce([
        padded[0:h, 0:w], padded[0:h, 1:w+1], padded[0:h, 2:w+2],
        padded[1:h+1, 0:w], padded[1:h+1, 1:w+1], padded[1:h+1, 2:w+2],
        padded[2:h+2, 0:w], padded[2:h+2, 1:w+1], padded[2:h+2, 2:w+2],
    ])
    mask = (prob >= confidence) & (prob >= local_max - 1e-6)
    flat_indices = np.flatnonzero(mask)
    if flat_indices.size == 0:
        return []

    scores = prob.ravel()[flat_indices]
    order = np.argsort(-scores, kind="stable")
    candidates: List[Dict[str, Any]] = []

    for idx in order[:64]:
        flat_idx = int(flat_indices[idx])
        score = float(scores[idx])
        iy, ix = divmod(flat_idx, w)
        if reg.shape[0] >= 4:
            offset_x = float(reg[0, iy, ix])
            offset_y = float(reg[1, iy, ix])
            raw_w = float(reg[2, iy, ix])
            raw_h = float(reg[3, iy, ix])
            box_w = (1.0 / (1.0 + np.exp(-np.clip(raw_w, -20.0, 20.0)))) * size if raw_w < 5.0 else raw_w
            box_h = (1.0 / (1.0 + np.exp(-np.clip(raw_h, -20.0, 20.0)))) * size if raw_h < 5.0 else raw_h
        else:
            offset_x, offset_y = 0.0, 0.0
            box_w = float(reg[0, iy, ix]) * size
            box_h = float(reg[1, iy, ix]) * size

        cx = (ix + 0.5 + offset_x) * stride
        cy = (iy + 0.5 + offset_y) * stride
        x1 = max(0.0, min(float(size), cx - box_w * 0.5))
        y1 = max(0.0, min(float(size), cy - box_h * 0.5))
        x2 = max(0.0, min(float(size), cx + box_w * 0.5))
        y2 = max(0.0, min(float(size), cy + box_h * 0.5))
        if x2 > x1 + 4 and y2 > y1 + 4:
            candidates.append({"box": [x1, y1, x2, y2], "score": score})

    kept: List[Dict[str, Any]] = []
    for cand in candidates:
        if all(box_iou(cand["box"], k["box"]) < nms_iou for k in kept):
            kept.append(cand)
    return kept


def restore_boxes(detections: List[Dict[str, Any]], geometry: Dict[str, float]) -> List[Dict[str, Any]]:
    left = geometry["left"]
    top = geometry["top"]
    sx = max(1e-6, geometry["scaleX"])
    sy = max(1e-6, geometry["scaleY"])
    orig_w = geometry["origWidth"]
    orig_h = geometry["origHeight"]

    restored = []
    for det in detections:
        bx = det["box"]
        x1 = max(0.0, min(orig_w, (bx[0] - left) / sx))
        y1 = max(0.0, min(orig_h, (bx[1] - top) / sy))
        x2 = max(0.0, min(orig_w, (bx[2] - left) / sx))
        y2 = max(0.0, min(orig_h, (bx[3] - top) / sy))
        if x2 > x1 + 8 and y2 > y1 + 8:
            restored.append({
                "box": [round(x1, 1), round(y1, 1), round(x2, 1), round(y2, 1)],
                "score": round(float(det["score"]), 4),
            })
    return restored


def choose_triplet(detections: List[Dict[str, Any]]) -> Optional[List[Dict[str, Any]]]:
    """Three cards of one row: similar height, aligned, evenly spaced, each scoring high enough."""
    strong = [d for d in detections if d["score"] >= TRIPLET_MIN_SCORE]
    if len(strong) < 3:
        return None

    best_group = None
    best_score = -1e9
    for group in itertools.combinations(strong[:12], 3):
        ordered = sorted(group, key=lambda d: (d["box"][0] + d["box"][2]) * 0.5)
        boxes = [d["box"] for d in ordered]
        heights = [b[3] - b[1] for b in boxes]
        widths = [b[2] - b[0] for b in boxes]
        mean_h = sum(heights) / 3.0
        mean_w = sum(widths) / 3.0
        if mean_h <= 10 or mean_w <= 10:
            continue

        centres_x = [(b[0] + b[2]) * 0.5 for b in boxes]
        centres_y = [(b[1] + b[3]) * 0.5 for b in boxes]
        gaps = [centres_x[1] - centres_x[0], centres_x[2] - centres_x[1]]
        if min(gaps) < mean_w * 0.6:
            continue

        y_spread = (max(centres_y) - min(centres_y)) / mean_h
        h_spread = (max(heights) - min(heights)) / mean_h
        gap_spread = abs(gaps[1] - gaps[0]) / max(1.0, sum(gaps) * 0.5)
        if y_spread > 0.35 or h_spread > 0.35 or gap_spread > 0.40:
            continue

        conf_sum = sum(d["score"] for d in ordered)
        score = conf_sum - (y_spread * 1.5 + h_spread * 1.2 + gap_spread * 1.2)
        if score > best_score:
            best_score = score
            best_group = ordered

    return best_group


class CardLocator:
    def __init__(self, model_dir: Optional[Path] = None, threads: int = 2):
        import onnxruntime as ort

        self.model_dir = model_dir or find_detector_dir()
        config_path = self.model_dir / CARD_DETECTOR_CONFIG
        self.config = json.loads(config_path.read_text(encoding="utf-8"))
        self.size = int(self.config.get("input", {}).get("shape", [1, 3, 512, 512])[-1])
        self.confidence = float(self.config.get("confidence", 0.35))
        self.nms_iou = float(self.config.get("nmsIou", 0.5))
        self.stride = int(self.config.get("stride", STRIDE))

        opts = ort.SessionOptions()
        opts.intra_op_num_threads = max(1, threads)
        opts.inter_op_num_threads = 1
        opts.enable_cpu_mem_arena = True
        opts.add_session_config_entry("session.intra_op.allow_spinning", "0")
        opts.add_session_config_entry("session.inter_op.allow_spinning", "0")

        model_path = self.model_dir / self.config.get("model", CARD_DETECTOR_MODEL)
        self.session = ort.InferenceSession(
            str(model_path),
            sess_options=opts,
            providers=["CPUExecutionProvider"],
        )
        self.input_name = self.session.get_inputs()[0].name

    def detect(self, image: Image.Image) -> List[Dict[str, Any]]:
        tensor, geometry = letterbox_image(image, size=self.size)
        outputs = self.session.run(None, {self.input_name: tensor})
        raw_dets = decode_outputs(
            outputs[0],
            outputs[1],
            size=self.size,
            confidence=self.confidence,
            nms_iou=self.nms_iou,
            stride=self.stride,
        )
        return restore_boxes(raw_dets, geometry)

    def locate(self, image: Image.Image) -> Dict[str, Any]:
        started = time.perf_counter()
        detections = self.detect(image)
        cards = choose_triplet(detections)
        detector_ms = round((time.perf_counter() - started) * 1000.0, 2)

        # Derive title rects from located card boxes (title band sits around 42%..56% of card height)
        title_rects = []
        if cards:
            for c in cards:
                x1, y1, x2, y2 = c["box"]
                cw = x2 - x1
                ch = y2 - y1
                tx = int(round(x1 + cw * 0.08))
                ty = int(round(y1 + ch * 0.42))
                tw = int(round(cw * 0.84))
                th = int(round(max(28.0, ch * 0.14)))
                title_rects.append({"x": tx, "y": ty, "width": tw, "height": th})

        return {
            "cards": cards,
            "title_rects": title_rects,
            "detections": detections,
            "detector_ms": detector_ms,
        }


_locator: Optional[CardLocator] = None
_locator_threads: int = 2


def set_locator_threads(threads: int) -> None:
    global _locator, _locator_threads
    if threads != _locator_threads:
        _locator_threads = threads
        _locator = None


def shared_locator(directory: Optional[Path] = None) -> CardLocator:
    global _locator
    if _locator is None:
        _locator = CardLocator(model_dir=directory or find_detector_dir(), threads=_locator_threads)
    return _locator
