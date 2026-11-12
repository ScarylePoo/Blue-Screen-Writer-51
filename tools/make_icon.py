"""Render the program icon (same design as src/register.rs icon_rgba) as
assets/bsw51.ico (all Windows sizes, embedded into bsw51.exe by build.rs)
and assets/bsw51-1024.png.  Needs Pillow.  Run from the repository root:
    python3 tools/make_icon.py
"""
import io, struct
from PIL import Image, ImageDraw, ImageFont

FONT = "assets/fonts/DejaVuSansMono-Bold.ttf"


def render(S, k=4):
    s = S * k
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([0, 0, s - 1, s - 1], radius=S * 0.14 * k, fill=(0, 0, 0xAA, 255))
    f = ImageFont.truetype(FONT, round(S * 0.55 * k))
    d.text(((s - f.getlength("51")) / 2, S * 0.66 * k), "51", font=f, fill=(255, 255, 255, 255), anchor="ls")
    y0, h, x0, x1 = S * 0.78, max(S / 16, 1), S * 0.22, S * 0.42
    d.rectangle([x0 * k, y0 * k, x1 * k - 1, (y0 + h) * k - 1], fill=(0x55, 0xFF, 0xFF, 255))
    return img.resize((S, S), Image.LANCZOS)


def dib(img):
    """32-bit BGRA DIB as stored in .ico files (bottom-up, plus AND mask)."""
    w, h = img.size
    px = img.tobytes()
    rows = []
    for y in range(h - 1, -1, -1):
        r = px[y * w * 4:(y + 1) * w * 4]
        rows.append(b"".join(bytes((r[i + 2], r[i + 1], r[i], r[i + 3])) for i in range(0, len(r), 4)))
    mask_row = ((w + 31) // 32) * 4
    hdr = struct.pack("<IiiHHIIiiII", 40, w, h * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    return hdr + b"".join(rows) + b"\0" * (mask_row * h)


big = render(1024)
big.save("assets/bsw51-1024.png", optimize=True)
sizes = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]
images = []
for S in sizes:
    im = big.resize((S, S), Image.LANCZOS) if S >= 64 else render(S, 16)
    if S >= 256:
        b = io.BytesIO()
        im.save(b, "PNG", optimize=True)
        images.append(b.getvalue())
    else:
        images.append(dib(im))
out = struct.pack("<HHH", 0, 1, len(sizes))
off = 6 + 16 * len(sizes)
for S, data in zip(sizes, images):
    d = 0 if S >= 256 else S
    out += struct.pack("<BBBBHHII", d, d, 0, 0, 1, 32, len(data), off)
    off += len(data)
out += b"".join(images)
open("assets/bsw51.ico", "wb").write(out)
print("assets/bsw51.ico", len(out), "bytes;", sizes)
