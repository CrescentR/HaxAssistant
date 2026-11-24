#!/usr/bin/env python3
"""
OCR Sidecar Service: PpOcrOnnxRecognizer (PaddleOCR v4 / v5 ONNX Runtime inference)
Supports ch_PP-OCRv4_rec_int8.onnx, PP-OCRv5_mobile_rec.onnx, and korean_PP-OCRv5_mobile_rec.onnx.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple

import numpy as np
from PIL import Image, ImageOps


def resolve_default_model_dir() -> Path:
    env_dir = os.environ.get("ARAM_ASSISTANT_OCR_MODEL_DIR")
    if env_dir and Path(env_dir).exists():
        return Path(env_dir)
    here = Path(__file__).resolve().parent
    for cand in [here.parent / "models", here / "models", Path.cwd() / "models"]:
        if cand.exists():
            return cand
    return here.parent / "models"


def resolve_model_paths(
    backend: str = "onnx",
    lang: str = "ch",
    rec_model: Optional[str] = None,
    keys_path: Optional[str] = None,
) -> Tuple[Path, Path]:
    base = resolve_default_model_dir()
    if rec_model and keys_path:
        return Path(rec_model), Path(keys_path)
    if lang.startswith("ko"):
        return base / "korean_PP-OCRv5_mobile_rec.onnx", base / "korean_PP-OCRv5_mobile_rec_keys.txt"
    if backend == "onnx-v5" or lang.startswith("en"):
        return base / "PP-OCRv5_mobile_rec.onnx", base / "PP-OCRv5_mobile_rec_keys.txt"
    return base / "ch_PP-OCRv4_rec_int8.onnx", base / "ppocr_keys_v1.txt"


def preprocess_image_in_memory(
    image: Image.Image,
    grayscale: bool = False,
    invert: bool = False,
    max_side: int = 960,
) -> Image.Image:
    img = image.copy().convert("RGB")
    w, h = img.size
    longest = max(w, h)
    if max_side > 0 and longest > max_side:
        ratio = max_side / float(longest)
        img = img.resize((max(1, int(round(w * ratio))), max(1, int(round(h * ratio)))), Image.Resampling.BILINEAR)
    if grayscale:
        img = ImageOps.grayscale(img).convert("RGB")
    if invert:
        img = ImageOps.invert(img)
    return img


class PpOcrOnnxRecognizer:
    def __init__(
        self,
        model_path: Path,
        keys_path: Path,
        img_height: int = 48,
        img_width: int = 320,
        threads: int = 4,
    ):
        import onnxruntime as ort

        if not model_path.exists():
            raise FileNotFoundError(f"rec model not found: {model_path}")
        if not keys_path.exists():
            raise FileNotFoundError(f"keys file not found: {keys_path}")

        self.model_path = model_path
        self.keys_path = keys_path
        self.img_height = img_height
        self.img_width = img_width

        raw_lines = keys_path.read_text(encoding="utf-8").splitlines()
        # Index 0 is CTC blank token; characters start at 1, followed by space token
        self.character = ["blank"] + raw_lines + [" "]

        opts = ort.SessionOptions()
        opts.intra_op_num_threads = max(1, threads)
        opts.inter_op_num_threads = 1
        opts.enable_cpu_mem_arena = True
        opts.add_session_config_entry("session.intra_op.allow_spinning", "0")
        opts.add_session_config_entry("session.inter_op.allow_spinning", "0")

        self.session = ort.InferenceSession(
            str(model_path),
            sess_options=opts,
            providers=["CPUExecutionProvider"],
        )
        self.input_name = self.session.get_inputs()[0].name

    def _preprocess_image(self, image: Image.Image) -> np.ndarray:
        img = image.convert("RGB") if image.mode != "RGB" else image
        w, h = img.size
        ratio = w / float(max(1, h))
        resized_w = min(self.img_width, max(16, int(round(self.img_height * ratio))))
        resized = img.resize((resized_w, self.img_height), Image.Resampling.BILINEAR)

        arr = np.asarray(resized, dtype=np.float32) / 255.0
        arr = (arr - 0.5) / 0.5
        arr = arr.transpose(2, 0, 1)  # (3, H, W)

        padded = np.zeros((3, self.img_height, self.img_width), dtype=np.float32)
        padded[:, :, :resized_w] = arr
        return padded

    def _decode(self, output: np.ndarray) -> Dict[str, Any]:
        # output shape: (T, C)
        preds_idx = output.argmax(axis=-1)
        preds_prob = output.max(axis=-1)

        chars: List[str] = []
        probs: List[float] = []
        prev_idx = 0
        for idx, prob in zip(preds_idx, preds_prob):
            i = int(idx)
            if i != 0 and i != prev_idx and i < len(self.character):
                chars.append(self.character[i])
                probs.append(float(prob))
            prev_idx = i

        text = "".join(chars).strip()
        score = float(np.mean(probs)) if probs else 0.0
        return {
            "text": text,
            "score": round(score, 4),
            "lines": [{"text": text, "score": round(score, 4)}] if text else [],
        }

    def recognize_image(self, image: Image.Image) -> Dict[str, Any]:
        return self.recognize_image_batch([image])[0]

    def recognize_image_batch(self, images: Sequence[Image.Image]) -> List[Dict[str, Any]]:
        if not images:
            return []
        batch = np.stack([self._preprocess_image(img) for img in images], axis=0)
        outputs = self.session.run(None, {self.input_name: batch})[0]
        return [self._decode(outputs[i]) for i in range(len(images))]

    def recognize(self, image_path: Path) -> Dict[str, Any]:
        with Image.open(image_path) as img:
            res = self.recognize_image(img)
        res["image_path"] = str(image_path)
        res["status"] = "ok"
        res["backend"] = "onnx-ppocrv5-rec" if "v5" in self.model_path.name else "onnx-ppocr-rec"
        return res

    def recognize_many(self, image_paths: List[Path]) -> List[Dict[str, Any]]:
        imgs = []
        for p in image_paths:
            with Image.open(p) as im:
                imgs.append(im.copy())
        results = self.recognize_image_batch(imgs)
        backend_name = "onnx-ppocrv5-rec" if "v5" in self.model_path.name else "onnx-ppocr-rec"
        for p, r in zip(image_paths, results):
            r["image_path"] = str(p)
            r["status"] = "ok"
            r["backend"] = backend_name
        return results


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="OCR sidecar.")
    parser.add_argument("--image", type=Path, help="Single image path")
    parser.add_argument("--images", nargs="+", type=Path, help="Multiple image paths")
    parser.add_argument("--lang", default="ch", help="Language: ch / en / ko")
    parser.add_argument("--grayscale", action="store_true")
    parser.add_argument("--invert", action="store_true")
    parser.add_argument("--max-side", type=int, default=960)
    parser.add_argument("--mock", action="store_true")
    parser.add_argument("--backend", choices=["auto", "mock", "paddle", "onnx", "onnx-v5"], default="auto")
    parser.add_argument("--rec-model", type=str, default=None)
    parser.add_argument("--keys-path", type=str, default=None)
    parser.add_argument("--performance-mode", choices=["eco", "balanced", "performance"], default="balanced")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    image_paths: List[Path] = []
    if args.image:
        image_paths.append(args.image)
    if args.images:
        image_paths.extend(args.images)

    if not image_paths:
        print(json.dumps({"status": "error", "error": "missing --image or --images"}, ensure_ascii=False))
        sys.exit(1)

    if args.mock or args.backend == "mock":
        payloads = [
            {
                "status": "ok",
                "image_path": str(p),
                "lines": [{"text": p.stem, "score": 0.99}],
                "text": p.stem,
                "score": 0.99,
                "backend": "mock",
            }
            for p in image_paths
        ]
        print(json.dumps(payloads if len(payloads) > 1 else payloads[0], ensure_ascii=False))
        return

    threads = 2 if args.performance_mode == "eco" else (6 if args.performance_mode == "performance" else 4)
    model_path, keys_path = resolve_model_paths(args.backend, args.lang, args.rec_model, args.keys_path)
    recognizer = PpOcrOnnxRecognizer(model_path, keys_path, threads=threads)
    payloads = recognizer.recognize_many(image_paths)
    print(json.dumps(payloads if len(payloads) > 1 else payloads[0], ensure_ascii=False))


if __name__ == "__main__":
    main()
