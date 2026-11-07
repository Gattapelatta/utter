"""Frames folder (record-overlay.swift / hero.swift: frame_NNNN.png + delays.txt) → a looping
animation for the README, by the output's extension:
  .png  — animated PNG: full colour, and Safari plays it at 60 fps (measured 2026-11-07: a 60 fps
          animated WebP ran at a quarter of its speed in Safari, with 4 s freezes).
  .webp — animated WebP (lossy unless --quality 100).
Needs Pillow: python3 -m venv /tmp/v && /tmp/v/bin/pip install pillow
Usage: python3 scripts/tools/frames2anim.py <frames-dir> <out.png|out.webp> [--quality 90] [--until 6.2] [--hold 1.2]
  --quality  WebP only: 1–99 lossy, 100 = lossless
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
frames = [Image.open(os.path.join(a.src, n)).convert("RGBA") for n in names]  # keeps rounded corners
# Whole milliseconds per frame without drift: round the running time, not each frame
# (60 fps → 17, 17, 16, …). Browsers slow frames of 10 ms or less down to 100 ms.
ends = [round(sum(delays[: i + 1]) * 1000) for i in range(len(delays))]
ms = [max(11, e - s) for s, e in zip([0] + ends[:-1], ends)]
if a.out.endswith(".png"):
    # Pillow stores only the changed rectangle of each frame, so a still part costs nothing.
    frames[0].save(a.out, save_all=True, append_images=frames[1:], duration=ms, loop=0, optimize=True)
else:
    lossless = a.quality >= 100
    frames[0].save(a.out, save_all=True, append_images=frames[1:], duration=ms, loop=0, method=6,
                   lossless=lossless, quality=100 if lossless else a.quality)
print(f"{len(frames)} frames, {ends[-1] / 1000:.2f} s → {a.out} ({os.path.getsize(a.out) // 1024} KB)")
