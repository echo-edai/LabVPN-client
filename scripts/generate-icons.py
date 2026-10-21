#!/usr/bin/env python3
"""生成 LabVPN 应用图标（PNG / ICO / ICNS / 托盘模板图）。

依赖：Pillow（`load_workspace_dependencies` 提供的 Python 已内置）。
用法：python3 scripts/generate-icons.py
输出：
  apps/desktop/src-tauri/icons/{32x32,128x128,128x128@2x,icon}.png
  apps/desktop/src-tauri/icons/icon.ico
  apps/desktop/src-tauri/icons/icon.icns
  apps/desktop/src-tauri/icons/tray.png
  resources/icons/icon-1024.png（源文件）
"""

from __future__ import annotations

import math
import shutil
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "apps" / "desktop" / "src-tauri" / "icons"
SOURCE = ROOT / "resources" / "icons"
ICONS.mkdir(parents=True, exist_ok=True)
SOURCE.mkdir(parents=True, exist_ok=True)

SIZE = 1024
SCALE = 4  # 超采样倍数


def lerp(a: tuple[int, int, int], b: tuple[int, int, int], t: float) -> tuple[int, int, int]:
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def squircle_mask(size: int, radius_ratio: float = 0.225) -> Image.Image:
    """近似的 macOS 圆角矩形遮罩（超采样后缩小，边缘平滑）。"""
    big = size * SCALE
    radius = int(big * radius_ratio)
    mask = Image.new("L", (big, big), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle([0, 0, big - 1, big - 1], radius=radius, fill=255)
    return mask.resize((size, size), Image.LANCZOS)


def draw_icon(size: int = SIZE) -> Image.Image:
    """绘制应用图标：深蓝渐变底 + 白色盾牌 + 分流双箭头。"""
    big = size * SCALE
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))

    # ---- 背景：三停靠点对角线渐变 ----
    gradient = Image.new("RGB", (big, big))
    try:
        import numpy as np  # 可选：有 numpy 时逐像素渐变，效率高、无色带

        yy, xx = np.mgrid[0:big, 0:big]
        t = np.clip((xx / big) * 0.55 + (yy / big) * 0.45, 0.0, 1.0)
        stops = [(6, 18, 40), (18, 62, 138), (37, 99, 235)]
        pos = [0.0, 0.62, 1.0]
        arr = np.zeros((big, big, 3), dtype=np.float32)
        for channel in range(3):
            arr[:, :, channel] = np.interp(
                t, pos, [stop[channel] for stop in stops]
            )
        gradient = Image.fromarray(arr.astype("uint8"), mode="RGB")
    except Exception:  # pragma: no cover - 无 numpy 时的降级路径
        gdraw = ImageDraw.Draw(gradient)
        top_left = (6, 18, 40)
        bottom_right = (37, 99, 235)
        for y in range(0, big, 2):
            for x in range(0, big, 16):
                blend = min(1.0, (x / big) * 0.55 + (y / big) * 0.45)
                gdraw.rectangle([x, y, x + 16, y + 2], fill=lerp(top_left, bottom_right, blend))
    img.paste(gradient, (0, 0))

    # ---- 右上角冷光 ----
    glow = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    gdraw = ImageDraw.Draw(glow)
    gdraw.ellipse(
        [int(big * 0.35), -int(big * 0.35), int(big * 1.30), int(big * 0.72)],
        fill=(96, 165, 250, 90),
    )
    glow = glow.filter(ImageFilter.GaussianBlur(big // 14))
    img.alpha_composite(glow)

    # ---- 背景网格点 ----
    dots = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    ddraw = ImageDraw.Draw(dots)
    step = big // 18
    for gy in range(step, big, step):
        for gx in range(step, big, step):
            ddraw.ellipse([gx - 2, gy - 2, gx + 2, gy + 2], fill=(255, 255, 255, 30))
    img.alpha_composite(dots)

    # ---- 主体层 ----
    layer = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    draw = ImageDraw.Draw(layer)
    cx = big / 2

    top = big * 0.175
    bottom = big * 0.835
    half_w = big * 0.235
    shoulder_y = big * 0.545
    shield = [
        (cx - half_w, top),
        (cx + half_w, top),
        (cx + half_w, shoulder_y),
        (cx, bottom),
        (cx - half_w, shoulder_y),
    ]
    outline_w = max(2, int(big * 0.018))

    # 盾牌内侧微光填充
    inner = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    idraw = ImageDraw.Draw(inner)
    idraw.polygon(shield, fill=(255, 255, 255, 26))
    inner = inner.filter(ImageFilter.GaussianBlur(big / 90))
    layer.alpha_composite(inner)

    draw.line(
        shield + [shield[0]],
        fill=(255, 255, 255, 240),
        width=outline_w,
        joint="curve",
    )

    # 分流线路
    line_w = max(2, int(big * 0.021))
    line_color = (191, 219, 254, 255)
    source = (cx, big * 0.335)
    center = (cx, big * 0.455)
    left_end = (cx - big * 0.135, big * 0.615)
    right_end = (cx + big * 0.135, big * 0.615)
    draw.line([source, center, left_end], fill=line_color, width=line_w, joint="curve")
    draw.line([source, center, right_end], fill=line_color, width=line_w, joint="curve")

    def arrowhead(tip, origin, color, length_ratio=0.055):
        angle = math.atan2(tip[1] - origin[1], tip[0] - origin[0])
        length = big * length_ratio
        for delta in (math.radians(150), math.radians(-150)):
            end = (
                tip[0] + length * math.cos(angle + delta),
                tip[1] + length * math.sin(angle + delta),
            )
            draw.line([tip, end], fill=color, width=max(2, int(big * 0.019)), joint="curve")

    arrowhead(left_end, center, line_color)
    arrowhead(right_end, center, line_color)

    node_r = big * 0.030
    for point in (source, left_end, right_end):
        draw.ellipse(
            [point[0] - node_r, point[1] - node_r, point[0] + node_r, point[1] + node_r],
            fill=(255, 255, 255, 255),
        )
    center_r = big * 0.036
    draw.ellipse(
        [center[0] - center_r, center[1] - center_r, center[0] + center_r, center[1] + center_r],
        fill=(96, 165, 250, 255),
    )
    layer.alpha_composite(
        Image.new("RGBA", (big, big), (0, 0, 0, 0))
    )

    # 中心节点外圈
    draw.ellipse(
        [
            center[0] - center_r * 1.55,
            center[1] - center_r * 1.55,
            center[0] + center_r * 1.55,
            center[1] + center_r * 1.55,
        ],
        outline=(147, 197, 253, 150),
        width=max(2, int(big * 0.006)),
    )
    layer = layer.filter(ImageFilter.GaussianBlur(big / 2200))
    img.alpha_composite(layer)

    # 顶部高光
    sheen = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    sdraw = ImageDraw.Draw(sheen)
    sdraw.ellipse(
        [-big * 0.30, -big * 0.62, big * 1.30, big * 0.28],
        fill=(255, 255, 255, 22),
    )
    sheen = sheen.filter(ImageFilter.GaussianBlur(big // 26))
    img.alpha_composite(sheen)

    # 圆角遮罩
    mask = squircle_mask(big, radius_ratio=0.225)
    img.putalpha(mask)
    return img.resize((size, size), Image.LANCZOS)


def draw_tray(size: int = 32) -> Image.Image:
    """macOS 托盘模板图：纯黑 + 透明（系统会自动反色）。"""
    big = size * 8
    img = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    cx = big / 2
    half_w = big * 0.30
    top = big * 0.12
    bottom = big * 0.90
    shield = [
        (cx - half_w, top),
        (cx + half_w, top),
        (cx + half_w, big * 0.55),
        (cx, bottom),
        (cx - half_w, big * 0.55),
    ]
    draw.line(shield + [shield[0]], fill=(0, 0, 0, 255), width=int(big * 0.075), joint="curve")
    start = (cx, big * 0.37)
    split_y = big * 0.50
    left_end = (cx - big * 0.14, big * 0.66)
    right_end = (cx + big * 0.14, big * 0.66)
    width = int(big * 0.06)
    draw.line([start, (cx, split_y), left_end], fill=(0, 0, 0, 255), width=width, joint="curve")
    draw.line([start, (cx, split_y), right_end], fill=(0, 0, 0, 255), width=width, joint="curve")
    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    base = draw_icon(SIZE)
    base.save(SOURCE / "icon-1024.png")

    base.resize((512, 512), Image.LANCZOS).save(ICONS / "icon.png")
    base.resize((32, 32), Image.LANCZOS).save(ICONS / "32x32.png")
    base.resize((128, 128), Image.LANCZOS).save(ICONS / "128x128.png")
    base.resize((256, 256), Image.LANCZOS).save(ICONS / "128x128@2x.png")
    base.resize((512, 512), Image.LANCZOS).save(ICONS / "512x512.png")
    base.resize((1024, 1024), Image.LANCZOS).save(ICONS / "icon-1024.png")

    # ICO（Windows）
    ico_sizes = [(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
    base.save(ICONS / "icon.ico", sizes=ico_sizes)

    # ICNS（macOS）：用 iconutil 组装 iconset
    with tempfile.TemporaryDirectory() as tmp:
        iconset = Path(tmp) / "LabVPN.iconset"
        iconset.mkdir()
        mapping = {
            "icon_16x16.png": 16,
            "icon_16x16@2x.png": 32,
            "icon_32x32.png": 32,
            "icon_32x32@2x.png": 64,
            "icon_128x128.png": 128,
            "icon_128x128@2x.png": 256,
            "icon_256x256.png": 256,
            "icon_256x256@2x.png": 512,
            "icon_512x512.png": 512,
            "icon_512x512@2x.png": 1024,
        }
        for name, px in mapping.items():
            base.resize((px, px), Image.LANCZOS).save(iconset / name)
        if shutil.which("iconutil"):
            subprocess.run(
                ["iconutil", "-c", "icns", str(iconset), "-o", str(ICONS / "icon.icns")],
                check=True,
            )
        else:
            print("⚠️ 未找到 iconutil，跳过 ICNS 生成（macOS 打包时需要）")

    # 托盘图（黑/白模板）
    draw_tray(32).save(ICONS / "tray.png")
    draw_tray(32).save(ICONS / "tray@2x.png")
    print(f"✅ 图标已生成：{ICONS}")
    for file in sorted(ICONS.glob("*")):
        print(f"   {file.name}: {file.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
