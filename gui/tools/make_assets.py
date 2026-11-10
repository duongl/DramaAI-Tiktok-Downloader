"""Generate the DramaAI Tiktok Downloader icon and banner.

Development-only helper. Requires Pillow:

    python -m pip install pillow
    python gui/tools/make_assets.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "gui" / "assets"
PINK = (255, 45, 111)
ORANGE = (255, 122, 61)
DARK = (13, 10, 16)

FONT_BOLD = r"C:\Windows\Fonts\segoeuib.ttf"
FONT_REGULAR = r"C:\Windows\Fonts\segoeui.ttf"


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def vertical_gradient(size, top, bottom):
    width, height = size
    image = Image.new("RGB", size)
    draw = ImageDraw.Draw(image)
    for y in range(height):
        draw.line([(0, y), (width, y)], fill=lerp(top, bottom, y / max(1, height - 1)))
    return image


def rounded_mask(size, radius):
    mask = Image.new("L", size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size[0] - 1, size[1] - 1], radius=radius, fill=255)
    return mask


def make_icon():
    size = 1024
    gradient = vertical_gradient((size, size), PINK, ORANGE)

    # soft dark vignette at the bottom
    shade = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    sd = ImageDraw.Draw(shade)
    for i in range(220):
        alpha = int(38 * (i / 220))
        sd.rectangle([0, size - 220 + i, size, size - 220 + i + 1], fill=(20, 4, 12, alpha))
    gradient = Image.alpha_composite(gradient.convert("RGBA"), shade)

    icon = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    icon.paste(gradient, (0, 0), rounded_mask((size, size), 220))

    draw = ImageDraw.Draw(icon)
    # white play triangle, rounded visually with circles
    triangle = [(365, 300), (365, 724), (742, 512)]
    draw.polygon(triangle, fill=(255, 255, 255, 255))
    for point, radius in [((365, 300), 34), ((365, 724), 34), ((742, 512), 34)]:
        draw.ellipse(
            [point[0] - radius, point[1] - radius, point[0] + radius, point[1] + radius],
            fill=(255, 255, 255, 255),
        )

    icon = icon.resize((256, 256), Image.Resampling.LANCZOS)
    icon.save(ASSETS / "icon.png", "PNG", optimize=True)
    icon.save(
        ASSETS / "icon.ico",
        format="ICO",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    return icon


def spaced_text(draw, xy, text, font, fill, spacing):
    x, y = xy
    for char in text:
        draw.text((x, y), char, font=font, fill=fill)
        x += draw.textlength(char, font=font) + spacing


def make_banner(icon):
    width, height = 1600, 500
    banner = vertical_gradient((width, height), (11, 9, 14), DARK).convert("RGBA")

    # colored glows on the right
    glow = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gd.ellipse([1010, -210, 1730, 510], fill=(255, 45, 111, 70))
    gd.ellipse([1180, 120, 1760, 700], fill=(255, 122, 61, 55))
    glow = glow.filter(ImageFilter.GaussianBlur(90))
    banner = Image.alpha_composite(banner, glow)

    # subtle dotted texture
    dots = Image.new("RGBA", (width, height), (0, 0, 0, 0))
    dd = ImageDraw.Draw(dots)
    for y in range(40, height - 30, 34):
        for x in range(60, 980, 34):
            dd.ellipse([x, y, x + 2, y + 2], fill=(255, 255, 255, 12))
    banner = Image.alpha_composite(banner, dots)

    draw = ImageDraw.Draw(banner)
    font_brand = ImageFont.truetype(FONT_BOLD, 112)
    font_sub = ImageFont.truetype(FONT_BOLD, 38)
    font_body = ImageFont.truetype(FONT_REGULAR, 27)
    font_pill = ImageFont.truetype(FONT_REGULAR, 20)

    draw.text((88, 96), "Drama", font=font_brand, fill=(255, 255, 255))
    drama_width = draw.textlength("Drama", font=font_brand)
    draw.text((92 + drama_width, 96), "AI", font=font_brand, fill=ORANGE)

    spaced_text(draw, (94, 238), "TIKTOK DOWNLOADER", font_sub, (255, 145, 120), 7)
    draw.text(
        (94, 314),
        "11+ nền tảng phim ngắn  •  Tải video & phụ đề",
        font=font_body,
        fill=(190, 185, 198),
    )

    # pill
    pill_text = "Rust GUI  •  Python core"
    pill_w = draw.textlength(pill_text, font=font_pill) + 40
    draw.rounded_rectangle([94, 382, 94 + pill_w, 430], radius=24, outline=(255, 92, 138, 190), width=2, fill=(38, 18, 29, 180))
    draw.text((114, 393), pill_text, font=font_pill, fill=(255, 190, 205))

    # icon on the right
    icon_big = icon.resize((300, 300), Image.Resampling.LANCZOS)
    shadow = Image.new("RGBA", (360, 360), (0, 0, 0, 0))
    sd = ImageDraw.Draw(shadow)
    sd.rounded_rectangle([20, 20, 340, 340], radius=70, fill=(255, 45, 111, 90))
    shadow = shadow.filter(ImageFilter.GaussianBlur(35))
    banner.alpha_composite(shadow, (1190, 96))
    banner.alpha_composite(icon_big, (1220, 106))

    # gradient footer line
    line = Image.new("RGBA", (width, 6), (0, 0, 0, 0))
    ld = ImageDraw.Draw(line)
    for x in range(width):
        ld.line([(x, 0), (x, 6)], fill=lerp(PINK, ORANGE, x / max(1, width - 1)) + (255,))
    banner.alpha_composite(line, (0, height - 6))

    banner.convert("RGB").save(ROOT / "banner.png", "PNG", optimize=True)


if __name__ == "__main__":
    ASSETS.mkdir(parents=True, exist_ok=True)
    icon = make_icon()
    make_banner(icon)
    print("Generated:", ASSETS / "icon.png", ASSETS / "icon.ico", ROOT / "banner.png")
