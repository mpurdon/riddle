#!/usr/bin/env python3
"""Draw Riddle's app icon and hand it to `pnpm tauri icon`.

    python3 scripts/make-icon.py            # writes src-tauri/icons/source.png
    cd apps/desktop && pnpm tauri icon src-tauri/icons/source.png -o src-tauri/icons

The motif is the Sift: one bright Find caught on the mesh while the rest
falls through. It has to survive 16 px in the menu bar and the Dock, so the
whole design is three shapes — a dot, a line, some falling grains — on a dark
warm field. Anything finer than that turns to mush at that size, which is how
the first placeholder (a flat indigo square) became unfindable in the Dock.

Geometry follows Apple's macOS template: a 1024 canvas with the rounded
square inset to 824 and a corner radius of 185, drawn here at 4x and
downsampled so the curves are clean. The palette is the app's own tokens.
"""
from PIL import Image, ImageDraw, ImageFilter

S = 4  # supersample
N = 1024 * S

INK_TOP = (38, 33, 29)
INK_BOTTOM = (10, 9, 8)
AMBER = (245, 158, 11)
AMBER_LIGHT = (253, 208, 113)
SAND = (231, 229, 228)
CLAY = (168, 162, 158)


def px(v: float) -> int:
    """A coordinate given in 1024-space, scaled to the working canvas."""
    return int(round(v * S))


def rounded_mask() -> Image.Image:
    m = Image.new("L", (N, N), 0)
    d = ImageDraw.Draw(m)
    d.rounded_rectangle(
        [px(100), px(100), px(924), px(924)], radius=px(185), fill=255
    )
    return m


def background() -> Image.Image:
    """A warm dark field, lighter at the top so the square has some depth."""
    g = Image.new("RGB", (1, 1024), INK_BOTTOM)
    d = ImageDraw.Draw(g)
    for y in range(1024):
        t = y / 1023
        d.point(
            (0, y),
            fill=tuple(
                int(INK_TOP[i] + (INK_BOTTOM[i] - INK_TOP[i]) * t) for i in range(3)
            ),
        )
    return g.resize((N, N), Image.BILINEAR)


def glow(layer: ImageDraw.ImageDraw, cx: float, cy: float, r: float, rgb, peak: int):
    """Concentric discs standing in for a radial gradient."""
    steps = 60
    for i in range(steps, 0, -1):
        rr = r * i / steps
        a = int(peak * (1 - i / steps) ** 2)
        layer.ellipse(
            [px(cx - rr), px(cy - rr), px(cx + rr), px(cy + rr)], fill=(*rgb, a)
        )


def draw() -> Image.Image:
    icon = background().convert("RGBA")

    halo = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    glow(ImageDraw.Draw(halo), 512, 440, 300, AMBER, 104)
    halo = halo.filter(ImageFilter.GaussianBlur(px(18)))
    icon = Image.alpha_composite(icon, halo)

    d = ImageDraw.Draw(icon)

    # The mesh: five bars with gaps between them, drawn as separate shapes so
    # the gaps are background rather than holes punched in the alpha (the
    # rounded-square mask is applied last and would refill them as black).
    # At 16 px the gaps close up and it reads as one line, which is the point.
    bar_y, bar_h = 556, 30
    x0, x1 = 276, 748
    gaps, gap_w = 4, 32
    seg_w = (x1 - x0 - gaps * gap_w) / (gaps + 1)
    gap_centres = []
    for i in range(gaps + 1):
        sx = x0 + i * (seg_w + gap_w)
        d.rounded_rectangle(
            [px(sx), px(bar_y - bar_h / 2), px(sx + seg_w), px(bar_y + bar_h / 2)],
            radius=px(bar_h / 2),
            fill=SAND,
        )
        if i < gaps:
            gap_centres.append(sx + seg_w + gap_w / 2)

    # The grains that pass through, falling from the gaps they fell through.
    for cx, cy, r in (
        (gap_centres[0], 666, 26),
        (gap_centres[2], 736, 20),
        (gap_centres[3], 680, 15),
    ):
        d.ellipse([px(cx - r), px(cy - r), px(cx + r), px(cy + r)], fill=(*CLAY, 255))

    # The Find: caught on the mesh, the only bright thing in the frame.
    r = 96
    cy = bar_y - bar_h / 2 - r + 3
    d.ellipse([px(512 - r), px(cy - r), px(512 + r), px(cy + r)], fill=AMBER)
    # A highlight on the upper left gives it a little roundness.
    hr = r * 0.42
    d.ellipse(
        [
            px(512 - r * 0.34 - hr),
            px(cy - r * 0.34 - hr),
            px(512 - r * 0.34 + hr),
            px(cy - r * 0.34 + hr),
        ],
        fill=(*AMBER_LIGHT, 170),
    )

    icon.putalpha(rounded_mask())
    return icon.resize((1024, 1024), Image.LANCZOS)


if __name__ == "__main__":
    out = "apps/desktop/src-tauri/icons/source.png"
    draw().save(out)
    print(f"wrote {out}")
