#!/usr/bin/env python3
"""生成 gitsc 的 logo / 应用图标。

标记是一段「两条分支汇入当前提交」的 git 图：
左边蓝色、右边紫色两条线在下方汇合，落在一个绿色的大节点上。
坐标以 1024 画布为准，绘制时 3 倍超采样再缩回来。
"""

import math
import os
import subprocess
import sys

from PIL import Image, ImageDraw, ImageFilter
import numpy as np

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "design")
S = 3          # 超采样倍数
N = 1024       # 逻辑画布
W = N * S

BLUE = (76, 141, 255)
VIOLET = (157, 123, 216)
GREEN = (98, 181, 67)
BG_TOP = (35, 37, 41)
BG_BOTTOM = (23, 24, 27)

TRUNK_X = 396.0
TOP = (TRUNK_X, 300.0)
BOTTOM = (TRUNK_X, 744.0)
TIP = (664.0, 300.0)
NODE_R = 74.0
GREEN_R = 86.0


def bezier(p0, p1, p2, p3, steps=120):
    pts = []
    for i in range(steps + 1):
        t = i / steps
        u = 1.0 - t
        x = u**3 * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t**3 * p3[0]
        y = u**3 * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t**3 * p3[1]
        pts.append((x, y))
    return pts


def arc(cx, cy, r, steps=240):
    return [
        (cx + r * math.cos(2 * math.pi * i / steps), cy + r * math.sin(2 * math.pi * i / steps))
        for i in range(steps + 1)
    ]


def stroke(draw, pts, color, width):
    """把折线加粗成多边形：沿法线偏移两侧再连起来，末端补圆头。

    比 ImageDraw.line(joint="curve") 干净——后者在密集采样点上会出现梳状缺口。
    """
    half = width / 2.0
    left, right = [], []
    n = len(pts)
    for i, (x, y) in enumerate(pts):
        if i == 0:
            dx, dy = pts[1][0] - x, pts[1][1] - y
        elif i == n - 1:
            dx, dy = x - pts[-2][0], y - pts[-2][1]
        else:
            dx, dy = pts[i + 1][0] - pts[i - 1][0], pts[i + 1][1] - pts[i - 1][1]
        length = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / length, dx / length
        left.append((x + nx * half, y + ny * half))
        right.append((x - nx * half, y - ny * half))
    draw.polygon(left + right[::-1], fill=color)
    for p in (pts[0], pts[-1]):
        draw.ellipse([p[0] - half, p[1] - half, p[0] + half, p[1] + half], fill=color)


def background():
    yy, xx = np.mgrid[0:W, 0:W].astype(np.float32) / S
    t = (yy / N).clip(0, 1)[..., None]
    top = np.array(BG_TOP, dtype=np.float32)
    bot = np.array(BG_BOTTOM, dtype=np.float32)
    img = top * (1 - t) + bot * t

    # 左上角一盏很暗的冷光，让纯色底不显得平。
    d = np.sqrt((xx - 330) ** 2 + (yy - 250) ** 2) / 620.0
    glow = np.clip(1.0 - d, 0, 1)[..., None] ** 2
    img = img + glow * np.array([26, 32, 44], dtype=np.float32)

    arr = np.clip(img, 0, 255).astype(np.uint8)
    return Image.fromarray(arr, "RGB").convert("RGBA")


def main():
    canvas = background()
    mask = Image.new("L", (W, W), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, W - 1, W - 1], radius=236 * S, fill=255)
    canvas.putalpha(mask)

    # 标记：一条主干 + 一条旁支。绿色节点是当前提交（HEAD），紫色是旁支的最新提交。
    trunk = [(TRUNK_X * S, TOP[1] * S), (TRUNK_X * S, BOTTOM[1] * S)]
    branch = [(TRUNK_X * S, 636 * S)] + [
        (x * S, y * S)
        for x, y in bezier((TRUNK_X, 636), (486, 634), (TIP[0], 560), (TIP[0], 366))
    ]

    lw = 88 * S

    # 外发光：先把线画到透明层，模糊后叠在底图下面。
    glow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    stroke(gd, branch, VIOLET + (110,), lw)
    stroke(gd, trunk, BLUE + (110,), lw)
    glow = glow.filter(ImageFilter.GaussianBlur(52 * S))
    canvas = Image.alpha_composite(canvas, glow)

    layer = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    # 先画旁支、再画主干：交汇处由主干盖住，接缝更干净。
    stroke(d, branch, VIOLET + (255,), lw)
    stroke(d, trunk, BLUE + (255,), lw)

    for (cx, cy), color, r in [
        (TOP, BLUE, NODE_R),
        (TIP, VIOLET, NODE_R),
        (BOTTOM, GREEN, GREEN_R),
    ]:
        d.ellipse(
            [(cx - r) * S, (cy - r) * S, (cx + r) * S, (cy + r) * S],
            fill=color + (255,),
        )

    canvas = Image.alpha_composite(canvas, layer)

    # 内描边，压住底图边缘。
    edge = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    ImageDraw.Draw(edge).rounded_rectangle(
        [3 * S, 3 * S, W - 3 * S, W - 3 * S],
        radius=234 * S,
        outline=(255, 255, 255, 16),
        width=6 * S,
    )
    canvas = Image.alpha_composite(canvas, edge)

    icon = canvas.resize((N, N), Image.LANCZOS)
    icon.save(os.path.join(OUT, "logo-1024.png"))

    for size in (512, 256, 128, 64, 32, 16):
        icon.resize((size, size), Image.LANCZOS).save(os.path.join(OUT, f"logo-{size}.png"))

    # macOS .icns
    iconset = os.path.join(OUT, "gitsc.iconset")
    os.makedirs(iconset, exist_ok=True)
    for base in (16, 32, 128, 256, 512):
        icon.resize((base, base), Image.LANCZOS).save(
            os.path.join(iconset, f"icon_{base}x{base}.png")
        )
        icon.resize((base * 2, base * 2), Image.LANCZOS).save(
            os.path.join(iconset, f"icon_{base}x{base}@2x.png")
        )
    subprocess.run(
        ["iconutil", "-c", "icns", iconset, "-o", os.path.join(OUT, "gitsc.icns")],
        check=True,
    )

    # Windows .ico（多尺寸）
    icon.save(
        os.path.join(OUT, "gitsc.ico"),
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )

    # 透明底的纯标记，给应用内标题栏用（不带圆角底板）。
    mark = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    md = ImageDraw.Draw(mark)
    stroke(md, branch, VIOLET + (255,), lw)
    stroke(md, trunk, BLUE + (255,), lw)
    for (cx, cy), color, r in [
        (TOP, BLUE, NODE_R),
        (TIP, VIOLET, NODE_R),
        (BOTTOM, GREEN, GREEN_R),
    ]:
        md.ellipse(
            [(cx - r) * S, (cy - r) * S, (cx + r) * S, (cy + r) * S],
            fill=color + (255,),
        )
    mark = mark.resize((N, N), Image.LANCZOS)
    mark.crop((190, 160, 850, 900)).resize((264, 296), Image.LANCZOS).save(
        os.path.join(OUT, "mark-256.png")
    )

    docs = os.path.join(ROOT, "docs")
    os.makedirs(docs, exist_ok=True)
    icon.resize((512, 512), Image.LANCZOS).save(os.path.join(docs, "logo.png"))
    print("logo written to", OUT)


if __name__ == "__main__":
    sys.exit(main())
