#!/usr/bin/env python3
"""
Apply XFeat to an image and save a nice feature overlay.

Usage:
    python xfeat_overlay_no_cv2.py input.jpg --output overlay.png
    python xfeat_overlay_no_cv2.py input.jpg --top-k 2048 --upsample 2.0
    python xfeat_overlay_no_cv2.py input.jpg --device cuda --radius 10
"""

from __future__ import annotations

import argparse
import pathlib

import matplotlib.pyplot as plt
import numpy as np
import torch
from PIL import Image


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser()
    p.add_argument("image", type=str, help="Path to input image")
    p.add_argument(
        "--output",
        type=str,
        default=None,
        help="Path to output overlay image (default: <stem>_xfeat_overlay.png)",
    )
    p.add_argument("--top-k", type=int, default=2048, help="Maximum number of features")
    p.add_argument(
        "--device",
        type=str,
        default="auto",
        choices=["auto", "cpu", "cuda", "mps"],
        help="Inference device",
    )
    p.add_argument(
        "--upsample",
        type=float,
        default=1.0,
        help="Optional image upsampling factor before detection",
    )
    p.add_argument(
        "--radius",
        type=float,
        default=7.0,
        help="Base marker radius in pixels for the overlay",
    )
    p.add_argument("--alpha", type=float, default=0.85, help="Marker transparency")
    p.add_argument("--dpi", type=int, default=180, help="Saved figure DPI")
    p.add_argument("--title", type=str, default="XFeat keypoints", help="Figure title")
    return p.parse_args()


def choose_device(device_arg: str) -> str:
    if device_arg != "auto":
        if device_arg == "cuda" and not torch.cuda.is_available():
            raise RuntimeError("CUDA requested, but torch.cuda.is_available() is False.")
        if device_arg == "mps" and not torch.backends.mps.is_available():
            raise RuntimeError("MPS requested, but torch.backends.mps.is_available() is False.")
        return device_arg

    if torch.cuda.is_available():
        return "cuda"
    if torch.backends.mps.is_available():
        return "mps"
    return "cpu"


def load_image_rgb(path: str) -> np.ndarray:
    img = Image.open(path).convert("RGB")
    return np.array(img)


def maybe_resize(img: np.ndarray, scale: float) -> np.ndarray:
    if abs(scale - 1.0) < 1e-9:
        return img
    h, w = img.shape[:2]
    new_w = max(1, int(round(w * scale)))
    new_h = max(1, int(round(h * scale)))
    pil_img = Image.fromarray(img)
    pil_img = pil_img.resize((new_w, new_h), Image.Resampling.BICUBIC)
    return np.array(pil_img)


def to_torch_image(img_rgb: np.ndarray, device: str) -> torch.Tensor:
    x = torch.from_numpy(img_rgb).float() / 255.0  # H, W, C
    x = x.permute(2, 0, 1).unsqueeze(0).contiguous()  # 1, C, H, W
    return x.to(device)


def load_xfeat(top_k: int):
    model = torch.hub.load(
        "verlab/accelerated_features",
        "XFeat",
        pretrained=True,
        top_k=top_k,
    )
    model.eval()
    return model


@torch.inference_mode()
def run_xfeat(model, image_tensor: torch.Tensor, top_k: int):
    model_device = next(model.parameters()).device
    image_tensor = image_tensor.to(model_device)

    out = model.detectAndCompute(image_tensor, top_k=top_k)[0]
    keypoints = out["keypoints"].detach().cpu().numpy()
    scores = out["scores"].detach().cpu().numpy()
    descriptors = out["descriptors"].detach().cpu().numpy()
    return keypoints, scores, descriptors


def normalize_scores(scores: np.ndarray) -> np.ndarray:
    if len(scores) == 0:
        return scores
    smin = float(scores.min())
    smax = float(scores.max())
    if abs(smax - smin) < 1e-12:
        return np.ones_like(scores)
    return (scores - smin) / (smax - smin)


def make_overlay(
    img_rgb: np.ndarray,
    keypoints_xy: np.ndarray,
    scores: np.ndarray,
    output_path: str,
    title: str,
    base_radius: float,
    alpha: float,
    dpi: int,
) -> None:
    h, w = img_rgb.shape[:2]
    norm_scores = normalize_scores(scores)
    sizes = (0.35 + 1.65 * norm_scores) * (base_radius ** 2)

    fig_w = max(8, w / 140)
    fig_h = max(6, h / 140)
    fig, ax = plt.subplots(figsize=(fig_w, fig_h), dpi=dpi)
    ax.imshow(img_rgb)

    if len(keypoints_xy) > 0:
        sc = ax.scatter(
            keypoints_xy[:, 0],
            keypoints_xy[:, 1],
            c=norm_scores,
            s=sizes,
            cmap="turbo",
            alpha=alpha,
            linewidths=0.35,
            edgecolors="white",
        )
        cbar = fig.colorbar(sc, ax=ax, fraction=0.03, pad=0.015)
        cbar.set_label("normalized XFeat score")
    else:
        ax.text(
            0.5,
            0.5,
            "No features detected",
            transform=ax.transAxes,
            ha="center",
            va="center",
            fontsize=14,
            color="white",
            bbox=dict(facecolor="black", alpha=0.6, boxstyle="round"),
        )

    ax.set_title(f"{title}  |  N={len(keypoints_xy)}")
    ax.set_axis_off()
    fig.tight_layout()
    fig.savefig(output_path, bbox_inches="tight", pad_inches=0.02)
    plt.close(fig)


def main() -> None:
    args = parse_args()
    device = choose_device(args.device)

    input_path = pathlib.Path(args.image)
    if args.output is None:
        output_path = input_path.with_name(f"{input_path.stem}_xfeat_overlay.png")
    else:
        output_path = pathlib.Path(args.output)

    img_rgb = load_image_rgb(str(input_path))
    original_h, original_w = img_rgb.shape[:2]

    img_for_model = maybe_resize(img_rgb, args.upsample)
    x = to_torch_image(img_for_model, device)

    model = load_xfeat(top_k=args.top_k)
    model = model.to(device) if hasattr(model, "to") else model

    keypoints, scores, descriptors = run_xfeat(model, x, top_k=args.top_k)

    if args.upsample != 1.0 and len(keypoints) > 0:
        keypoints = keypoints / float(args.upsample)

    if len(keypoints) > 0:
        keypoints[:, 0] = np.clip(keypoints[:, 0], 0, original_w - 1)
        keypoints[:, 1] = np.clip(keypoints[:, 1], 0, original_h - 1)

    make_overlay(
        img_rgb=img_rgb,
        keypoints_xy=keypoints,
        scores=scores,
        output_path=str(output_path),
        title=args.title,
        base_radius=args.radius,
        alpha=args.alpha,
        dpi=args.dpi,
    )

    print(f"Input:        {input_path}")
    print(f"Output:       {output_path}")
    print(f"Device:       {device}")
    print(f"Upsample:     {args.upsample}")
    print(f"Top-K:        {args.top_k}")
    print(f"Features:     {len(keypoints)}")
    print(f"Descriptors:  {descriptors.shape}")


if __name__ == "__main__":
    main()
