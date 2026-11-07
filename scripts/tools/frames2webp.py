"""Frames folder from record-overlay.swift (frame_NNNN.png + delays.txt) → a looping lossless
animated WebP: every colour and every pixel kept, ~0.5 MB for the README animation (a GIF has
256 colours). Needs Pillow: python3 -m venv /tmp/v && /tmp/v/bin/pip install pillow
Usage: python3 scripts/tools/frames2webp.py <frames-dir> <out.webp>"""
import os
import sys

from PIL import Image

src, out = sys.argv[1], sys.argv[2]
names = sorted(n for n in os.listdir(src) if n.endswith(".png"))
delays = [float(x) for x in open(os.path.join(src, "delays.txt")).read().split()]
frames = [Image.open(os.path.join(src, n)).convert("RGB") for n in names]
ms = [max(20, round(d * 1000)) for d in delays]
frames[0].save(out, save_all=True, append_images=frames[1:], duration=ms, loop=0, lossless=True, method=6)
print(f"{len(frames)} frames → {out} ({os.path.getsize(out) // 1024} KB)")
