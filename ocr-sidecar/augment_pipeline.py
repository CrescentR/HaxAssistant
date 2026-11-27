#!/usr/bin/env python3
"""
Augment Pipeline: Screen Capture, Resolution Presets, UI Occlusion Check,
RapidFuzz Dictionary Matching, and AutoWatcher Multi-Frame Stabilization.
"""
from __future__ import annotations

import json
import os
import re
import sys
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple

import numpy as np
from PIL import Image


@dataclass
class Rect:
    x: int
    y: int
    width: int
    height: int

    def to_bbox(self) -> Tuple[int, int, int, int]:
        return (self.x, self.y, self.x + self.width, self.y + self.height)

    def to_dict(self) -> Dict[str, int]:
        return {"x": self.x, "y": self.y, "width": self.width, "height": self.height}

    def scaled(self, scale_x: float, scale_y: float) -> "Rect":
        return Rect(
            x=int(round(self.x * scale_x)),
            y=int(round(self.y * scale_y)),
            width=max(1, int(round(self.width * scale_x))),
            height=max(1, int(round(self.height * scale_y))),
        )

    def shifted_expanded(self, dx: int = 0, dy: int = 0, dw: int = 0, dh: int = 0) -> "Rect":
        return Rect(
            x=max(0, self.x + dx),
            y=max(0, self.y + dy),
            width=max(1, self.width + dw),
            height=max(1, self.height + dh),
        )


@dataclass
class AugmentPreset:
    key: str
    width: int
    height: int
    scale: int
    anchor_rect: Rect
    augment_rects: List[Rect]
    content_viewport: Optional[Rect] = None

    def to_dict(self) -> Dict[str, Any]:
        return {
            "key": self.key,
            "width": self.width,
            "height": self.height,
            "scale": self.scale,
            "anchor_rect": self.anchor_rect.to_dict(),
            "augment_rects": [r.to_dict() for r in self.augment_rects],
            "content_viewport": self.content_viewport.to_dict() if self.content_viewport else None,
        }

    def scaled_to(self, target_width: int, target_height: int) -> "AugmentPreset":
        if target_width == self.width and target_height == self.height:
            return self
        sx = target_width / float(max(1, self.width))
        sy = target_height / float(max(1, self.height))
        return AugmentPreset(
            key=f"{self.key}->scaled:{target_width}x{target_height}",
            width=target_width,
            height=target_height,
            scale=self.scale,
            anchor_rect=self.anchor_rect.scaled(sx, sy),
            augment_rects=[r.scaled(sx, sy) for r in self.augment_rects],
            content_viewport=self.content_viewport.scaled(sx, sy) if self.content_viewport else None,
        )


@dataclass
class AutoWatcher:
    state: str = "idle"
    last_signature: Tuple[str, ...] = field(default_factory=tuple)
    last_non_empty_signature: Tuple[str, ...] = field(default_factory=tuple)
    consecutive_matches: int = 0
    failures: int = 0
    stable_result: Optional[List[Dict[str, Any]]] = None

    def observe(self, cycle_result: Dict[str, Any]) -> Dict[str, Any]:
        matches = cycle_result.get("ocr_matches", [])
        names = tuple(m.get("match_name") or "" for m in matches)
        all_matched = len(names) == 3 and all(bool(n) for n in names)

        if not all_matched:
            self.failures += 1
            self.consecutive_matches = 0
            if self.failures >= 2:
                self.state = "watching_anchor"
                self.stable_result = None
            return {"state": self.state, "stable": False, "signature": list(names)}

        self.failures = 0
        if names == self.last_signature:
            self.consecutive_matches += 1
        else:
            self.last_signature = names
            self.consecutive_matches = 1

        self.last_non_empty_signature = names
        is_stable = self.consecutive_matches >= 2
        self.state = "stable_result_ready" if is_stable else "matching_done"
        if is_stable:
            self.stable_result = matches
        return {
            "state": self.state,
            "stable": is_stable,
            "consecutive_matches": self.consecutive_matches,
            "signature": list(names),
        }


def resources_data_dir() -> Path:
    env_dir = os.environ.get("ARAM_ASSISTANT_DATA_DIR")
    if env_dir and Path(env_dir).exists():
        return Path(env_dir)
    here = Path(__file__).resolve().parent
    for cand in [here.parent / "data", here / "data", Path.cwd() / "data"]:
        if cand.exists():
            return cand
    return here.parent / "data"


def load_presets() -> List[AugmentPreset]:
    path = resources_data_dir() / "augment_presets.json"
    raw = json.loads(path.read_text(encoding="utf-8"))
    presets = []
    for item in raw:
        ar = item["anchor_rect"]
        presets.append(
            AugmentPreset(
                key=item["key"],
                width=int(item["width"]),
                height=int(item["height"]),
                scale=int(item.get("scale", 100)),
                anchor_rect=Rect(int(ar["x"]), int(ar["y"]), int(ar["width"]), int(ar["height"])),
                augment_rects=[
                    Rect(int(r["x"]), int(r["y"]), int(r["width"]), int(r["height"]))
                    for r in item["augment_rects"]
                ],
            )
        )
    return presets


def get_preset(preset_key: Optional[str] = None, image_size: Optional[Tuple[int, int]] = None) -> AugmentPreset:
    presets = load_presets()
    by_key = {p.key: p for p in presets}
    if preset_key and preset_key in by_key:
        chosen = by_key[preset_key]
        return chosen.scaled_to(*image_size) if image_size else chosen

    if image_size:
        w, h = image_size
        for p in presets:
            if p.width == w and p.height == h:
                return p
        return presets[0].scaled_to(w, h)
    return presets[0]


def mss_shot_to_image(shot: Any) -> Image.Image:
    """RGB image of an mss grab, decoded straight from its BGRA buffer."""
    return Image.frombytes("RGB", shot.size, shot.bgra, "raw", "BGRX")


def foreground_client_region() -> Optional[Dict[str, int]]:
    if sys.platform != "win32":
        return None
    import ctypes
    from ctypes import wintypes

    user32 = ctypes.windll.user32
    user32.GetForegroundWindow.restype = wintypes.HWND
    hwnd = user32.GetForegroundWindow()
    if not hwnd:
        return None
    rect = wintypes.RECT()
    origin = wintypes.POINT(0, 0)
    if not user32.GetClientRect(wintypes.HWND(hwnd), ctypes.byref(rect)):
        return None
    if not user32.ClientToScreen(wintypes.HWND(hwnd), ctypes.byref(origin)):
        return None
    width = rect.right - rect.left
    height = rect.bottom - rect.top
    if width < 320 or height < 200:
        return None
    return {"left": origin.x, "top": origin.y, "width": width, "height": height}


def capture_screen_image(
    preset_key: Optional[str] = None,
    foreground_client: bool = False,
) -> Tuple[Image.Image, Dict[str, Any]]:
    import mss

    with mss.mss() as sct:
        monitors = sct.monitors
        mon = monitors[1] if len(monitors) > 1 else monitors[0]
        region = foreground_client_region() if foreground_client else None
        if region:
            mon = region
        shot = sct.grab(mon)
        img = mss_shot_to_image(shot)
        preset = get_preset(preset_key, img.size)
        return img, {"monitor": mon, "preset_key": preset.key, "image_size": list(img.size)}


def probe_live_frame(image: Image.Image, preset: AugmentPreset) -> Dict[str, Any]:
    """Check whether the 3 card regions exhibit Hextech card UI colors and are not occluded."""
    arr = np.asarray(image.convert("RGB"), dtype=np.float32)
    h, w, _ = arr.shape
    card_signals = 0
    for rect in preset.augment_rects:
        x1 = max(0, min(w - 1, rect.x))
        y1 = max(0, min(h - 1, rect.y))
        x2 = max(x1 + 1, min(w, rect.x + rect.width))
        y2 = max(y1 + 1, min(h, rect.y + rect.height))
        patch = arr[y1:y2, x1:x2]
        mean_luma = float(patch.mean())
        std_luma = float(patch.std())
        if 12.0 <= mean_luma <= 220.0 and std_luma >= 14.0:
            card_signals += 1

    detected = card_signals >= 2
    return {
        "detected": detected,
        "card_signals": card_signals,
        "quality": "probable_augment_frame" if detected else "non_augment_frame",
    }


def crop_rect_to_image(image: Image.Image, rect: Rect) -> Image.Image:
    w, h = image.size
    x1 = max(0, min(w - 1, rect.x))
    y1 = max(0, min(h - 1, rect.y))
    x2 = max(x1 + 1, min(w, rect.x + rect.width))
    y2 = max(y1 + 1, min(h, rect.y + rect.height))
    return image.crop((x1, y1, x2, y2))


def normalize_ocr_text(text: str) -> str:
    cleaned = re.sub(r"[\s·•.,，。!！?？:：;；'\"“”‘’()（）\[\]【】\-_/\\]+", "", text or "")
    return cleaned.lower()


def load_augment_dictionary() -> Tuple[List[Dict[str, Any]], Dict[str, str]]:
    data_dir = resources_data_dir()
    aug_path = data_dir / "augments_full.json"
    if not aug_path.exists():
        aug_path = data_dir / "augments.json"
    augments = json.loads(aug_path.read_text(encoding="utf-8"))

    alias_path = data_dir / "augment_aliases.json"
    raw_aliases = json.loads(alias_path.read_text(encoding="utf-8")) if alias_path.exists() else {}
    alias_map: Dict[str, str] = {}
    for k, v in raw_aliases.items():
        target = v[0] if isinstance(v, list) and v else (v if isinstance(v, str) else "")
        if target:
            alias_map[normalize_ocr_text(k)] = target

    for row in augments:
        name = row.get("augment_name") or row.get("name") or ""
        if name:
            alias_map[normalize_ocr_text(name)] = name
        en = row.get("english_name") or ""
        if en:
            alias_map[normalize_ocr_text(en)] = name

    return augments, alias_map


def match_ocr_texts(ocr_results: Sequence[Dict[str, Any]]) -> List[Dict[str, Any]]:
    from rapidfuzz import fuzz, process

    augments, alias_map = load_augment_dictionary()
    canonical_names = [a.get("augment_name") or a.get("name") for a in augments if a.get("augment_name") or a.get("name")]
    norm_to_canonical = {normalize_ocr_text(n): n for n in canonical_names}

    matched: List[Dict[str, Any]] = []
    for slot_idx, item in enumerate(ocr_results, 1):
        raw_text = (item.get("text") or "").strip()
        ocr_score = float(item.get("score", 0.0))
        norm = normalize_ocr_text(raw_text)

        match_name = None
        match_type = "unmatched"
        confidence = 0.0

        if norm in alias_map:
            match_name = alias_map[norm]
            match_type = "exact" if raw_text == match_name else "alias"
            confidence = max(0.88, ocr_score)
        elif norm and norm_to_canonical:
            best = process.extractOne(norm, list(norm_to_canonical.keys()), scorer=fuzz.WRatio)
            if best and best[1] >= 72.0:
                match_name = norm_to_canonical[best[0]]
                match_type = "fuzzy"
                confidence = round((best[1] / 100.0) * max(0.75, ocr_score), 4)

        matched.append({
            "slot": slot_idx,
            "raw_text": raw_text,
            "normalized_text": norm,
            "match_name": match_name,
            "match_type": match_type,
            "confidence": round(confidence, 4),
            "stable": match_name is not None and confidence >= 0.72,
        })
    return matched
