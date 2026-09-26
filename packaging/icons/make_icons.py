"""Regenerate assets/icon.ico (enabled) and assets/icon_off.ico (disabled).

    python packaging/icons/make_icons.py

Needs Pillow plus the Noto Sans and Noto Looped Thai fonts (Debian/Ubuntu:
fonts-noto-core). Every size is drawn on its own, not scaled down from 256 px,
so the 16/20/24 px tray icons stay sharp.
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[2]
FONTS = Path("/usr/share/fonts/truetype/noto")
LATIN = FONTS / "NotoSans-Bold.ttf"
THAI = FONTS / "NotoLoopedThai-Bold.ttf"
SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]

ON = ((0x4C, 0xC2, 0xFF), (0x00, 0x5F, 0xB8))
OFF = ((0x9A, 0x9A, 0x9A), (0x5E, 0x5E, 0x5E))


def tile(size: int, colors) -> Image.Image:
    scale = 4  # supersample, then downsample: anti-aliased edges
    s = size * scale
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    top, bottom = colors
    grad = Image.new("RGBA", (1, s))
    for y in range(s):
        t = y / (s - 1)
        grad.putpixel((0, y), tuple(round(a + (b - a) * t) for a, b in zip(top, bottom)) + (255,))
    grad = grad.resize((s, s))
    mask = Image.new("L", (s, s), 0)
    margin = 0 if size <= 24 else round(s * 0.04)
    ImageDraw.Draw(mask).rounded_rectangle(
        (margin, margin, s - 1 - margin, s - 1 - margin), radius=round(s * 0.24), fill=255
    )
    img.paste(grad, (0, 0), mask)

    draw = ImageDraw.Draw(img)
    white = (255, 255, 255, 255)
    if size <= 20:
        # Too small for two glyphs: the Thai letter alone reads as "Thai typing".
        font = ImageFont.truetype(str(THAI), round(s * 0.78))
        draw.text((s / 2, s * 0.53), "ก", font=font, fill=white, anchor="mm")
    else:
        latin = ImageFont.truetype(str(LATIN), round(s * 0.46))
        thai = ImageFont.truetype(str(THAI), round(s * 0.50))
        draw.text((s * 0.33, s * 0.40), "A", font=latin, fill=white, anchor="mm")
        draw.text((s * 0.67, s * 0.62), "ก", font=thai, fill=white, anchor="mm")
    return img.resize((size, size), Image.LANCZOS)


def build(colors, out: Path) -> None:
    images = [tile(size, colors) for size in SIZES]
    images[-1].save(out, format="ICO", sizes=[(i, i) for i in SIZES], append_images=images[:-1])


if __name__ == "__main__":
    build(ON, ROOT / "assets" / "icon.ico")
    build(OFF, ROOT / "assets" / "icon_off.ico")
    print("wrote assets/icon.ico and assets/icon_off.ico")
