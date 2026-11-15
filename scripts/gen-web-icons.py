#!/usr/bin/env python3
"""Draws the icons of the browser build's app manifest (web/icon-*.png):
the site's green tile with a house, at the sizes an installed app needs.
Plain Python, no libraries. Run from the workspace root:
    python3 scripts/gen-web-icons.py
"""
import struct
import zlib

GREEN_TOP, GREEN_BOTTOM = (104, 176, 110), (56, 122, 68)
CREAM, SHADE, DOOR = (244, 232, 200), (214, 198, 160), (120, 92, 60)
K = 3  # samples per pixel and axis, for smooth edges


def colour(u, v, maskable):
    """The colour at (u, v) in 0..1, or None outside the rounded tile."""
    if not maskable:
        r = 0.18
        cx, cy = min(max(u, r), 1 - r), min(max(v, r), 1 - r)
        if (u - cx) ** 2 + (v - cy) ** 2 > r * r:
            return None
    # A maskable icon fills the square and keeps the house in the middle
    # 60 %, where every launcher's mask leaves it whole.
    s = 0.60 if maskable else 0.74
    x, y = (u - 0.5) / s + 0.5, (v - 0.5) / s + 0.5 - 0.02
    if 0.42 <= x <= 0.58 and 0.64 <= y <= 0.94:
        return DOOR
    if 0.60 <= y <= 0.74 and (0.20 <= x <= 0.34 or 0.66 <= x <= 0.80):
        return GREEN_BOTTOM
    if 0.0 <= y <= 0.46 and abs(x - 0.5) <= 0.54 * y / 0.46:
        return CREAM  # the roof
    if 0.12 <= x <= 0.88 and 0.44 <= y <= 0.94:
        return SHADE if y <= 0.52 else CREAM
    return tuple(a + (b - a) * v for a, b in zip(GREEN_TOP, GREEN_BOTTOM))


def icon(path, size, maskable):
    rows = bytearray()
    n = size * K
    for py in range(size):
        rows.append(0)
        for px in range(size):
            r = g = b = a = 0
            for sy in range(K):
                for sx in range(K):
                    c = colour((px * K + sx + 0.5) / n, (py * K + sy + 0.5) / n, maskable)
                    if c:
                        r, g, b, a = r + c[0], g + c[1], b + c[2], a + 1
            if a:
                rows += bytes((round(r / a), round(g / a), round(b / a), round(255 * a / (K * K))))
            else:
                rows += b"\0\0\0\0"

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b"")
    open(path, "wb").write(png)
    print("wrote", path)


icon("web/icon-192.png", 192, False)
icon("web/icon-512.png", 512, False)
icon("web/icon-maskable-512.png", 512, True)
