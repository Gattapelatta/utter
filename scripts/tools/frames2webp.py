"""Frames folder from record-overlay.swift (frame_NNNN.png + delays.txt) → a looping animated
WebP for the README. Full colour (a GIF has 256), real 60 fps timing.
Needs Pillow: python3 -m venv /tmp/v && /tmp/v/bin/pip install pillow
Usage: python3 scripts/tools/frames2webp.py <frames-dir> <out.webp> [--quality 90] [--until 6.2] [--hold 1.2]
  --quality  1–99 lossy (light to decode, browsers keep 60 fps), 100 = lossless (heavy)
  --until    drop frames after this many seconds (a still tail looks like a stall)
  --hold     how long the last frame stays before the loop restarts, seconds"""
import argparse
import os

from PIL import Image

ap = argparse.ArgumentParser()
ap.add_argument("src")
ap.add_argument("out")
ap.add_argument("--quality", type=int, default=90)
ap.add_argument("--until", type=float, default=None)
ap.add_argument("--hold", type=float, default=1.2)
a = ap.parse_args()

names = sorted(n for n in os.listdir(a.src) if n.endswith(".png"))
delays = [float(x) for x in open(os.path.join(a.src, "delays.txt")).read().split()]
if a.until is not None:
    starts = [sum(delays[:i]) for i in range(len(delays))]
    keep = max(1, sum(1 for s in starts if s < a.until))
    names, delays = names[:keep], delays[:keep]
delays[-1] = a.hold
frames = [Image.open(os.path.join(a.src, n)).convert("RGB") for n in names]
# Whole milliseconds per frame without drift: round the running time, not each frame
# (60 fps → 17, 17, 16, …). Browsers slow frames of 10 ms or less down to 100 ms.
ends = [round(sum(delays[: i + 1]) * 1000) for i in range(len(delays))]
ms = [max(11, e - s) for s, e in zip([0] + ends[:-1], ends)]
lossless = a.quality >= 100
frames[0].save(a.out, save_all=True, append_images=frames[1:], duration=ms, loop=0, method=6,
               lossless=lossless, quality=100 if lossless else a.quality)
print(f"{len(frames)} frames, {ends[-1] / 1000:.2f} s → {a.out} ({os.path.getsize(a.out) // 1024} KB)")
