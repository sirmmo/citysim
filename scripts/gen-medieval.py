#!/usr/bin/env python3
"""Writes the composed homes and shops of the medieval and fantasy skins
(med_11 ... med_29 and fan_11 ... fan_29 in assets/models/composed/) from
the buildings that scripts/build-medieval.py assembles out of the Quaternius
Medieval Village MegaKit (assets/models/quaternius/medieval/).

Every building keeps the kit's scale, SCALE metres to a tile, on plots of
any size: a slot's parts share a width (the 4 m houses, 5.5 m over the
eaves, or the 6 m ones, 8.25 m), which fits them all to the same scale, so
a plot's houses have doors and storeys the size of their neighbours'.
Fronts face +z, the street; back rows are turned to face -z and side rows
to face the yard. Each building is painted per plot: its plaster, timber,
roof tiles, rubble walls, doors and shutters, and the cloth of its awnings
and market stalls (materials Plaster, Timber, Tiles, Stonework, Paintwork,
Shingles) take one of the colourways below, muted ones for the medieval
skin and vivid ones for the fantasy skin. Run from the repository root:

    python3 -I scripts/gen-medieval.py
"""
import os

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = os.path.join(ROOT, "assets", "models", "composed")
MODELS = os.path.join(ROOT, "assets", "models", "quaternius", "medieval")
M = "../quaternius/medieval/"
K = "../kenney/nature/"
SCALE = 10.5  # metres to a tile

# Colourways: (plaster, timber, tiles, stonework, paintwork, shingles), sRGB.
# Shingles are the boards of the market stalls and the awnings, here cloth.
MEDIEVAL = [
    ([0.95, 0.90, 0.78], [0.33, 0.22, 0.14], [0.78, 0.34, 0.18], [0.60, 0.57, 0.52], [0.42, 0.28, 0.18], [0.62, 0.22, 0.18]),
    ([0.97, 0.95, 0.90], [0.24, 0.18, 0.13], [0.66, 0.26, 0.16], [0.64, 0.62, 0.58], [0.30, 0.40, 0.28], [0.30, 0.44, 0.30]),
    ([0.93, 0.80, 0.56], [0.36, 0.24, 0.15], [0.42, 0.44, 0.50], [0.66, 0.60, 0.50], [0.52, 0.24, 0.18], [0.74, 0.60, 0.30]),
    ([0.94, 0.80, 0.74], [0.28, 0.20, 0.15], [0.84, 0.48, 0.24], [0.59, 0.56, 0.54], [0.28, 0.36, 0.48], [0.30, 0.38, 0.56]),
    ([0.86, 0.89, 0.90], [0.30, 0.22, 0.16], [0.70, 0.30, 0.20], [0.52, 0.51, 0.49], [0.42, 0.28, 0.18], [0.58, 0.46, 0.34]),
    ([0.90, 0.84, 0.70], [0.40, 0.28, 0.17], [0.58, 0.42, 0.26], [0.64, 0.59, 0.51], [0.36, 0.30, 0.22], [0.56, 0.20, 0.26]),
    ([0.96, 0.92, 0.83], [0.20, 0.16, 0.13], [0.38, 0.40, 0.46], [0.62, 0.61, 0.59], [0.55, 0.36, 0.16], [0.40, 0.48, 0.26]),
    ([0.90, 0.77, 0.60], [0.34, 0.23, 0.15], [0.74, 0.38, 0.22], [0.56, 0.53, 0.48], [0.32, 0.38, 0.30], [0.66, 0.36, 0.20]),
    ([0.93, 0.88, 0.82], [0.38, 0.26, 0.18], [0.80, 0.44, 0.22], [0.61, 0.58, 0.54], [0.46, 0.22, 0.20], [0.44, 0.34, 0.50]),
]
FANTASY = [
    ([0.82, 0.72, 0.96], [0.30, 0.18, 0.30], [0.28, 0.38, 0.80], [0.64, 0.60, 0.76], [0.88, 0.66, 0.15], [0.90, 0.30, 0.60]),
    ([0.70, 0.94, 0.80], [0.22, 0.24, 0.30], [0.56, 0.30, 0.72], [0.57, 0.68, 0.72], [0.82, 0.20, 0.25], [0.95, 0.75, 0.15]),
    ([0.66, 0.84, 0.99], [0.30, 0.20, 0.14], [0.84, 0.28, 0.30], [0.60, 0.64, 0.78], [0.20, 0.58, 0.30], [0.20, 0.70, 0.55]),
    ([0.99, 0.74, 0.82], [0.32, 0.16, 0.22], [0.18, 0.58, 0.58], [0.72, 0.60, 0.68], [0.25, 0.36, 0.84], [0.60, 0.25, 0.85]),
    ([0.99, 0.92, 0.58], [0.36, 0.22, 0.12], [0.24, 0.58, 0.36], [0.74, 0.66, 0.52], [0.68, 0.20, 0.68], [0.90, 0.25, 0.20]),
    ([0.97, 0.96, 0.92], [0.18, 0.20, 0.38], [0.92, 0.58, 0.20], [0.62, 0.66, 0.74], [0.15, 0.46, 0.74], [0.20, 0.45, 0.90]),
    ([0.99, 0.80, 0.56], [0.28, 0.14, 0.12], [0.46, 0.28, 0.66], [0.76, 0.62, 0.54], [0.20, 0.62, 0.56], [0.95, 0.55, 0.10]),
    ([0.76, 0.96, 0.96], [0.20, 0.26, 0.24], [0.82, 0.36, 0.58], [0.58, 0.72, 0.72], [0.92, 0.46, 0.10], [0.35, 0.80, 0.30]),
    ([0.92, 0.82, 0.99], [0.26, 0.18, 0.12], [0.24, 0.50, 0.86], [0.68, 0.64, 0.82], [0.88, 0.25, 0.15], [0.85, 0.20, 0.45]),
]


def paints(ways):
    names = ("Plaster", "Timber", "Tiles", "Stonework", "Paintwork", "Shingles")
    return "[" + ", ".join("{ " + ", ".join("%s = [%s, %s, %s]" % (n, *c) for n, c in zip(names, way)) + " }" for way in ways) + "]"


def service_paints(ways, extra):
    """Colourways of the services: the homes' (but for wooden roofs, which
    stay boards), the cloth of banners, tents and stands, the crops of the
    fields, and the glow of fires (fantasy: of magic)."""
    out = []
    for i, way in enumerate(ways):
        p = dict(zip(("Plaster", "Timber", "Tiles", "Stonework", "Paintwork", "Banner"), way))
        p["Crop"] = extra["crop"][i % len(extra["crop"])]
        p["Glow"] = extra["glow"][i % len(extra["glow"])]
        out.append("{ " + ", ".join("%s = [%s, %s, %s]" % (n, *c) for n, c in p.items()) + " }")
    return "[" + ", ".join(out) + "]"


# Crops and fires per colourway.
MEDIEVAL_EXTRA = {
    "crop": [[0.56, 0.63, 0.27], [0.82, 0.70, 0.36], [0.46, 0.56, 0.24], [0.74, 0.62, 0.30], [0.62, 0.66, 0.34]],
    "glow": [[1.0, 0.56, 0.16]],
}
FANTASY_EXTRA = {
    "crop": [[0.48, 0.72, 0.26], [0.92, 0.78, 0.26], [0.70, 0.52, 0.86], [0.36, 0.66, 0.42], [0.95, 0.60, 0.30]],
    "glow": [[0.35, 0.90, 1.0], [0.80, 0.40, 1.0], [0.40, 1.0, 0.55], [1.0, 0.75, 0.25]],
}


def size_of(name):
    """Width, depth and height of a building, in metres."""
    lo, hi = [1e9] * 3, [-1e9] * 3
    with open(os.path.join(MODELS, name + ".obj")) as f:
        for line in f:
            if line.startswith("v "):
                p = [float(x) for x in line.split()[1:4]]
                lo = [min(a, b) for a, b in zip(lo, p)]
                hi = [max(a, b) for a, b in zip(hi, p)]
    return hi[0] - lo[0], hi[2] - lo[2], hi[1] - lo[1]


# Building families: parts of a slot share a width, so they share a scale.
NARROW_COTTAGES = ["cottage_timber", "cottage_stone"]
WIDE_COTTAGES = ["cottage_wide", "cottage_long"]
NARROW_HOUSES = ["house_jetty", "townhouse_narrow"]
WIDE_HOUSES = ["house_stone", "house_wide"]
TALL_NARROW = ["townhouse_narrow", "house_jetty"]
TALL_WIDE = ["townhouse_wide", "house_wide", "house_stone"]
SHOPS_WIDE = ["shop_arcade", "house_stone"]
TREES = [K + "tree_default.obj", K + "tree_oak.obj", K + "tree_fat.obj", K + "tree_default_fall.obj"]  # Kenney Nature Kit, CC0


class Plot:
    """The slots of one composition."""

    def __init__(self):
        self.slots = []
        self.height = 0.0

    def building(self, names, cx, cz, rot=0, painted=True):
        """Buildings (or yard things, or nothing for "") centred on (cx, cz)
        tiles, turned by rot (0, 90, 180 or 270, or a list to pick from)."""
        sizes = [size_of(n) for n in names if n]
        bw, bl = max(s[0] for s in sizes) / SCALE, max(s[1] for s in sizes) / SCALE
        for n, (w, d, h) in zip([n for n in names if n], sizes):
            s = min(bw / w, bl / d)
            assert abs(s * SCALE - 1) < 0.04, "%s would be scaled by %.2f" % (n, s * SCALE)
            self.height = max(self.height, h / SCALE)
        parts = [M + n + ".obj" if n else "" for n in names]
        self.slots.append((parts, (bw, bl), (cx - bw / 2, cz - bl / 2), rot, painted))

    def prop(self, names, cx, cz, rot=None):
        """Yard things, unpainted."""
        self.building(names, cx, cz, rot, False)

    def tree(self, cx, cz, size):
        """A tree or nothing, fitted into a size x size box."""
        self.slots.append((TREES + [""], (size, size), (cx - size / 2, cz - size / 2), [0, 90, 180, 270], False))

    def kenney(self, name, cx, cz, k, rot=None, y=0.0, kit="fantasy-town"):
        """A Kenney kit model, k metres to the kit's unit, centred on (cx,
        cz) tiles and lifted y metres."""
        lo, hi = [1e9] * 3, [-1e9] * 3
        with open(os.path.join(ROOT, "assets", "models", "kenney", kit, name + ".obj")) as f:
            for line in f:
                if line.startswith("v "):
                    p = [float(x) for x in line.split()[1:4]]
                    lo = [min(a, b) for a, b in zip(lo, p)]
                    hi = [max(a, b) for a, b in zip(hi, p)]
        bw, bl = (hi[0] - lo[0]) * k / SCALE, (hi[2] - lo[2]) * k / SCALE
        self.height = max(self.height, (y + (hi[1] - lo[1]) * k) / SCALE)
        self.slots.append((["../kenney/%s/%s.obj" % (kit, name)], (bw, bl), (cx - bw / 2, cz - bl / 2), rot, False, y / SCALE))

    def text(self, ways, extra=None):
        out = []
        for i, (parts, box, corner, rot, painted, *y) in enumerate(self.slots):
            (bw, bl), (x, z) = box, corner
            ys = "%.3f" % y[0] if y and y[0] else "0.0"
            lines = ["[[slot]]", "parts = [%s]" % ", ".join('"%s"' % p for p in parts), "size = [%.3f, %.3f]" % (bw, bl), "at = [%.3f, %s, %.3f]" % (x, ys, z)]
            if isinstance(rot, list):
                lines.append("rotations = [%s]" % ", ".join(str(r) for r in rot))
            elif rot:
                lines.append("rotations = [%d]" % rot)
            if painted and extra:
                # Each slot starts the colourways further on, so the buildings of
                # one plot are painted alike only by chance.
                k = (3 * i) % len(ways)
                lines.append("paints = " + service_paints(ways[k:] + ways[:k], extra))
            elif painted:
                lines.append("paints = " + paints(ways))
            out.append("\n".join(lines))
        return "\n\n".join(out)


def yard_house(houses, x=0.32):
    """A house on the street with a yard beside it: a tree, a wagon, crates
    or nothing, and a fence or none along the street."""
    p = Plot()
    p.building(houses, x, 0.55)
    p.prop(["", "prop_crates", "prop_wagon"], 0.80, 0.74, [0, 90])
    p.prop(["prop_fence", ""], 0.79, 0.975)
    p.tree(0.79, 0.30, 0.34)
    return p


def one(houses):
    p = Plot()
    p.building(houses, 0.5, 0.52)
    return p


def plots():
    out = {}
    out["11"] = ("Cottage", "a cottage with its yard", yard_house(NARROW_COTTAGES))
    out["12"] = ("Cottage", "a cottage", one(WIDE_COTTAGES))
    out["13"] = ("Timber house", "a timber-framed house with its yard", yard_house(["house_jetty", "cottage_timber"]))
    out["14"] = ("Town house", "a town house", one(WIDE_HOUSES))
    out["15"] = ("Tall house", "a tall town house with its yard", yard_house(TALL_NARROW))
    # Two houses back to back on a long plot.
    p = Plot()
    p.building(WIDE_HOUSES + ["townhouse_wide"], 0.5, 1.5)
    p.building(WIDE_COTTAGES + ["house_stone"], 0.5, 0.5, 180)
    out["16"] = ("Burgage", "a house on the street and another behind it", p)
    out["17"] = ("Tall house", "a tall town house", one(TALL_WIDE))
    # A farmstead: two houses on the street and a barn across the back.
    p = Plot()
    p.building(WIDE_HOUSES + WIDE_COTTAGES, 0.5, 1.5)
    p.building(NARROW_HOUSES + NARROW_COTTAGES, 1.45, 1.6)
    p.building(["longhouse"], 1.0, 0.45, 90)
    p.prop(["prop_wagon", "prop_crates", ""], 1.55, 0.98, [90, 0])
    out["18"] = ("Farmstead", "two houses and a barn round a yard", p)
    # A row of tall houses and a tower.
    p = Plot()
    row = [TALL_WIDE, TALL_NARROW, TALL_WIDE, ["tower"], TALL_WIDE, TALL_NARROW, TALL_WIDE]
    widths = [max(size_of(n)[0] for n in fam) / SCALE for fam in row]
    x = (5 - sum(widths) - 0.004 * (len(row) - 1)) / 2
    for fam, w in zip(row, widths):
        p.building(fam, x + w / 2, 0.6 if fam == ["tower"] else 0.52)
        x += w + 0.004
    out["19"] = ("Row houses", "a row of tall houses and a tower", p)

    # Shops.
    p = Plot()
    p.building(["stall"], 0.28, 0.82)
    p.building(["stall"], 0.73, 0.82)
    p.building(["stall"], 0.3, 0.3, 180)
    p.prop(["prop_wagon", "prop_crates", ""], 0.75, 0.32, [90, 0])
    out["21"] = ("Market stalls", "market stalls", p)
    out["22"] = ("Shop", "a shop with its yard", yard_house(["shop_corner"]))
    out["23"] = ("Shop", "a shop under an arcade", one(SHOPS_WIDE))
    p = Plot()
    p.building(SHOPS_WIDE, 0.43, 0.55)
    p.building(["shop_corner", "house_jetty"], 1.18, 0.55)
    p.building(["stall"], 1.73, 0.82)
    p.prop(["prop_crates", "prop_wagon", ""], 1.73, 0.3, [0, 90])
    out["24"] = ("Shops", "shops and a stall", p)
    # A market square: the guild hall and a tower at the back, shops down
    # both sides facing the square, stalls in the middle.
    p = Plot()
    p.building(["guildhall"], 1.75, 0.55)
    p.building(["tower"], 3.2, 0.4)
    p.building(["tavern"], 0.55, 0.65)
    for z in (1.65, 2.45, 3.25):
        p.building(SHOPS_WIDE + ["house_wide"], 0.48, z, 90)
        p.building(["shop_corner", "townhouse_narrow"], 3.6, z, 270)
    for x, z in ((1.6, 2.0), (2.3, 2.0), (1.6, 2.9), (2.3, 2.9)):
        p.building(["stall"], x, z, 0)
    p.prop(["prop_wagon", "prop_crates"], 2.95, 2.45, [0, 90])
    out["25"] = ("Market square", "a market square with its guild hall", p)
    p = Plot()
    p.building(["tavern"], 0.55, 1.4)
    p.building(["stall"], 1.5, 1.75)
    p.prop(["prop_wagon", "prop_crates"], 1.5, 1.2, [90, 0])
    p.building(WIDE_COTTAGES, 1.48, 0.48, 180)
    p.tree(0.6, 0.35, 0.4)
    out["26"] = ("Tavern", "a tavern with its yard", p)
    p = Plot()
    p.building(["inn"], 1.0, 1.55)
    p.building(["longhouse"], 1.0, 0.42, 270)
    p.prop(["prop_wagon", "prop_crates"], 0.4, 0.98, [90, 0])
    p.tree(1.6, 0.98, 0.36)
    out["27"] = ("Inn", "an inn with its stable", p)
    p = Plot()
    p.building(["guildhall"], 1.0, 1.48)
    p.building(WIDE_HOUSES, 0.48, 0.5, 180)
    p.building(NARROW_HOUSES, 1.5, 0.5, 180)
    out["28"] = ("Guild hall", "a guild hall and houses behind it", p)
    # A big block: the guild hall and a tower on the street, an inn and a
    # tavern behind a yard.
    p = Plot()
    p.building(["guildhall"], 0.85, 2.48)
    p.building(["tower"], 1.9, 2.68)
    p.building(TALL_WIDE, 2.6, 2.5)
    p.building(["inn"], 0.85, 0.42, 180)
    p.building(["tavern"], 2.4, 0.6, 180)
    p.building(["stall"], 1.0, 1.5)
    p.building(["stall"], 1.6, 1.5, 180)
    p.prop(["prop_wagon", "prop_crates"], 0.4, 1.5, [90, 0])
    p.tree(2.4, 1.55, 0.45)
    out["29"] = ("Guild quarter", "a guild hall, a tower, an inn and a tavern", p)
    return out


SIZES = {"11": (1, 1), "12": (1, 1), "13": (1, 1), "14": (1, 1), "15": (1, 1), "16": (1, 2), "17": (1, 1), "18": (2, 2), "19": (5, 1),
         "21": (1, 1), "22": (1, 1), "23": (1, 1), "24": (2, 1), "25": (4, 4), "26": (2, 2), "27": (2, 2), "28": (2, 2), "29": (3, 3)}


# ---------------------------------------------------------------------------
# Services, works, amenities and the fields and crafts: every other code the
# medieval and fantasy skins can show, on the footprint of the OpenCity
# model it replaces, at the homes' scale.


def walls_round(p, x0, z0, x1, z1, part="wall", gaps=()):
    """A precinct wall (or fence) of 10 m lengths round a rectangle in tiles,
    leaving out the lengths listed in gaps (0, 1, ... counted round)."""
    k = 0
    step = 10.0 / SCALE
    for (ax, az, bx, bz) in ((x0, z1, x1, z1), (x1, z1, x1, z0), (x1, z0, x0, z0), (x0, z0, x0, z1)):
        length = abs(bx - ax) + abs(bz - az)
        n = max(1, int(round(length / step)))
        for i in range(n):
            t = (i + 0.5) / n
            cx, cz = ax + (bx - ax) * t, az + (bz - az) * t
            if k not in gaps:
                p.building([part], cx, cz, 0 if az == bz else 90, True)
            k += 1


def trees(p, spots, size=0.36):
    for x, z in spots:
        p.tree(x, z, size)


def fantasy_trees(p, spots, k=3.6):
    for i, (x, z) in enumerate(spots):
        p.kenney(["tree-high", "tree", "tree-crooked", "tree-high-round", "tree-high-crooked"][i % 5], x, z, k, [0, 90, 180, 270])


def windmill(p, cx, cz):
    p.building(["mill_tower"], cx, cz)
    # The Kenney sails turn in the plane of the tower's front.
    p.kenney("windmill", cx, cz + 0.29, 4.2, 90, 4.6)


def services(skin):
    """code -> (name, what, Plot) for one skin ("medieval" or "fantasy")."""
    fan = skin == "fantasy"
    out = {}

    def plot():
        return Plot()

    # Fields and crafts (industrial 31-39).
    p = plot()
    p.building(["field"], 0.5, 0.5)
    out["31"] = ("Field", "a ploughed field", p)
    p = plot()
    p.building(["smithy"], 0.32, 0.55)
    p.prop(["trough", ""], 0.8, 0.75, [90, 0])
    p.prop(["prop_wagon", "prop_crates"], 0.8, 0.25, [90, 0])
    out["32"] = ("Smithy", "a smithy and its yard", p)
    p = plot()
    p.building(["dyeworks"], 0.5, 0.75)
    p.building(["millrace"], 0.5, 0.28, 90)
    p.prop(["prop_crates", ""], 0.85, 0.3, [0, 90])
    out["33"] = ("Dye works", "a dyers' shed with vats and cloth drying, by a mill race", p)
    p = plot()
    p.building(["timber_yard"], 0.3, 0.5)
    p.prop(["prop_wagon", "prop_crates"], 0.78, 0.55, [0, 90])
    out["34"] = ("Timber yard", "a timber yard with its saw pit", p)
    p = plot()
    p.building(["brewhouse"], 0.5, 0.52)
    out["35"] = ("Brewhouse", "a brewhouse and its barrels", p)
    p = plot()
    windmill(p, 0.4, 0.45)
    p.prop(["haystacks", ""], 0.8, 0.27, [0, 90])
    out["36"] = ("Windmill", "a tower windmill", p)
    p = plot()
    p.building(["mill_house"], 0.38, 0.55)
    p.building(["millrace"], 0.88, 0.5)
    p.kenney("watermill", 0.87, 0.55, 2.6)
    out["37"] = ("Watermill", "a watermill on its race", p)
    p = plot()
    p.building(["field_long"], 0.5, 1.0)
    p.building(WIDE_COTTAGES, 1.5, 1.45)
    p.prop(["haystacks"], 1.4, 0.45, [0, 90])
    p.building(["well"], 1.85, 0.8)
    out["38"] = ("Croft", "a cottage, its field and haystacks", p)
    p = plot()
    p.building(["field_long"], 0.5, 1.0)
    p.building(["field_long"], 1.5, 1.0)
    p.building(["tithe_barn"], 3.0, 1.45)
    windmill(p, 2.45, 0.45)
    p.prop(["haystacks"], 3.4, 0.4, [0, 90])
    p.prop(["prop_wagon", "prop_crates"], 3.85, 0.5, [0, 90])
    out["39"] = ("Manor farm", "fields, a great barn and a windmill", p)

    # Power: the tithe barn (fantasy: a mage tower); fantasy forges and circles.
    p = plot()
    if fan:
        p.building(["stone_circle"], 2.5, 1.5)
        p.building(["mage_tower"], 2.5, 1.5)
        p.building(["turret"], 0.7, 0.8)
        p.building(["turret"], 4.3, 0.8)
        p.building(["statue_stag"], 1.3, 2.6)
        p.building(["statue_fox"], 3.7, 2.6)
        fantasy_trees(p, [(0.5, 2.3), (4.5, 2.3), (1.0, 0.4), (4.0, 0.4)])
        out["300"] = ("Mage tower", "a mage tower in a ring of standing stones", p)
    else:
        p.building(["tithe_barn"], 2.4, 2.4)
        p.building(["longhouse"], 1.0, 0.7, 90)
        p.prop(["haystacks"], 3.6, 0.7, [0, 90])
        p.building(["field"], 4.4, 1.9)
        p.prop(["prop_wagon"], 4.2, 0.6, [0, 90])
        p.prop(["prop_crates", "prop_wagon"], 1.0, 2.5, [0, 90])
        walls_round(p, 0.06, 0.06, 3.0, 1.5, "rail_fence", gaps=(1,))
        out["300"] = ("Tithe barn", "a great barn and its yard", p)
    if fan:
        p = plot()
        p.building(["forge_hall"], 1.7, 2.2)
        p.building(["forge_hall"], 4.3, 2.2)
        p.building(["kiln"], 1.0, 4.4)
        p.building(["kiln"], 5.0, 4.4)
        p.building(["smithy"], 3.0, 4.5)
        p.building(["mage_tower"], 3.0, 2.0)
        p.building(["statue_stag"], 3.0, 5.4)
        for x, z in ((0.5, 0.5), (5.5, 0.5), (0.5, 5.5), (5.5, 5.5), (3.0, 0.5)):
            p.kenney("rock-large", x, z, 4.0, [0, 90, 180])
        walls_round(p, 0.08, 0.08, 5.92, 5.92, "wall", gaps=(2, 3))
        out["303"] = ("Dragon forge", "forges, kilns and a tower behind walls", p)
        p = plot()
        p.building(["stone_circle"], 2.0, 2.0)
        for x, z in ((0.5, 0.5), (3.5, 0.5), (0.5, 3.5), (3.5, 3.5)):
            p.building(["ley_stone"], x, z)
        p.building(["columns"], 2.0, 0.5)
        fantasy_trees(p, [(0.6, 2.0), (3.4, 2.0), (2.0, 3.6)])
        out["304"] = ("Sun circle", "a ring of standing stones round a crystal", p)
        p = plot()
        p.building(["forge_hall"], 3.5, 3.0)
        for x, z in ((1.3, 1.3), (5.7, 1.3), (1.3, 4.6), (5.7, 4.6)):
            p.building(["mage_tower"], x, z)
        p.building(["stone_circle"], 3.5, 1.2)
        p.building(["kiln"], 2.0, 3.0)
        p.building(["kiln"], 5.0, 3.0)
        walls_round(p, 0.1, 0.1, 6.9, 5.9, "wall", gaps=(3,))
        out["305"] = ("Arcane foundry", "four mage towers round a foundry", p)
        p = plot()
        windmill(p, 0.45, 0.5)
        out["306"] = ("Windmill", "a tower windmill", p)

    # The park (a well; fantasy: a sacred grove).
    p = plot()
    if fan:
        p.building(["ley_stone"], 0.5, 0.5)
        fantasy_trees(p, [(0.2, 0.25), (0.8, 0.3), (0.25, 0.8), (0.78, 0.8)], 3.0)
        out["320"] = ("Sacred grove", "a standing stone among trees", p)
    else:
        p.building(["well"], 0.45, 0.5)
        p.prop(["trough", ""], 0.5, 0.85)
        trees(p, [(0.15, 0.2), (0.85, 0.25)], 0.3)
        out["320"] = ("Well", "a well with a trough and trees", p)

    # Amenities.
    p = plot()
    p.building(["tilt"], 3.0, 2.6)
    p.building(["stand"], 3.0, 3.65)
    p.building(["stand"], 3.0, 1.45, 180)
    for x in (0.65, 5.35):
        p.building(["pavilion"], x, 2.6)
    for x in (1.2, 3.0, 4.8):
        p.building(["tent"], x, 0.5)
        p.building(["tent"], x, 4.5)
    for x, z in ((0.4, 0.4), (5.6, 0.4), (0.4, 4.6), (5.6, 4.6)):
        if fan:
            fantasy_trees(p, [(x, z)])
        else:
            trees(p, [(x, z)], 0.5)
    walls_round(p, 0.3, 1.95, 5.7, 3.25, "rail_fence", gaps=(6, 13))
    out["340"] = ("Tournament lists" if not fan else "Jousting lists", "the lists with stands, pavilions and tents", p)
    p = plot()
    p.building(["market_hall"], 2.5, 1.7)
    for i, x in enumerate((0.9, 1.7, 2.5, 3.3, 4.1)):
        p.kenney(["stall-red", "stall-green"][i % 2], x, 3.3, 3.2, [0, 180])
    p.building(SHOPS_WIDE, 0.55, 1.7, 90)
    p.building(SHOPS_WIDE, 4.45, 1.7, 270)
    p.building(TALL_WIDE, 1.5, 0.52, 180)
    p.building(TALL_WIDE, 3.5, 0.52, 180)
    p.building(["well"], 2.5, 0.6)
    p.prop(["prop_wagon", "prop_crates"], 4.5, 0.6, [0, 90])
    p.prop(["prop_crates", "prop_wagon"], 0.5, 0.6, [90, 0])
    out["341"] = ("Market hall" if not fan else "Grand bazaar", "a market hall with stalls before it", p)
    p = plot()
    p.building(["chapel"], 1.5, 1.35)
    p.building(["statue_stag"], 0.5, 2.5)
    p.building(["statue_fox"], 2.5, 2.5)
    walls_round(p, 0.06, 0.06, 2.94, 2.94, "wall", gaps=(1,))
    if fan:
        fantasy_trees(p, [(0.5, 0.6), (2.5, 0.6)])
    else:
        trees(p, [(0.5, 0.6), (2.5, 0.6)])
    out["342"] = ("Reliquary" if not fan else "Hall of relics", "a chapel and statues in a walled garden", p)
    p = plot()
    p.building(["guildhall"], 1.5, 1.9)
    p.building(["statue_fox"], 1.5, 0.6)
    for x in (0.4, 2.6):
        p.kenney("lantern", x, 2.75, 2.0)
    p.kenney("stall-green", 0.5, 0.7, 3.2)
    p.kenney("stall-red", 2.5, 0.7, 3.2, 180)
    out["343"] = ("Painters' guild" if not fan else "Enchanters' gallery", "a guild hall, a statue and stalls", p)
    p = plot()
    p.building(["playhouse"], 2.0, 1.6)
    for x in (0.4, 3.6):
        p.kenney("lantern", x, 2.8, 2.0)
    p.prop(["prop_wagon", "prop_crates"], 0.35, 0.6, [90, 0])
    p.building(["tent"], 3.65, 0.5)
    out["344"] = ("Playhouse" if not fan else "Bards' playhouse", "a galleried playhouse", p)
    p = plot()
    p.building(["ruins"], 1.5, 1.6)
    p.building(["columns"], 1.5, 0.45)
    p.building(["statue_stag"], 2.5, 2.5)
    for x, z in ((0.4, 2.6), (2.6, 0.9), (0.5, 0.9)):
        p.kenney("rock-large", x, z, 3.0, [0, 90, 180])
    if fan:
        fantasy_trees(p, [(0.5, 1.8)])
    else:
        trees(p, [(0.5, 1.8)])
    out["345"] = ("Old ruins" if not fan else "Ancient ruins", "Roman ruins, columns and a statue", p)
    if fan:
        p = plot()
        p.building(["inn"], 2.0, 2.0)
        p.building(["pond"], 1.0, 0.6)
        p.building(["fountain"], 3.1, 0.6)
        for x in (0.3, 3.7):
            for z in (1.3, 2.5):
                p.kenney("hedge-large", x, z, 2.4, 90)
        fantasy_trees(p, [(0.3, 0.3), (3.7, 1.9), (2.0, 0.4)])
        out["346"] = ("Elven retreat", "an inn with pools and a fountain", p)
        p = plot()
        p.building(["mage_tower"], 1.0, 1.0)
        for x, z in ((0.3, 0.3), (1.7, 0.3), (0.3, 1.7), (1.7, 1.7)):
            p.building(["ley_stone"], x, z)
        fantasy_trees(p, [(1.0, 0.25), (0.25, 1.0)], 3.0)
        out["347"] = ("Stargazer's tower", "a tall mage tower among standing stones", p)
        p = plot()
        p.building(["maypole"], 3.0, 2.5)
        for x, z in ((1.2, 1.0), (4.8, 1.0), (1.2, 4.0), (4.8, 4.0)):
            p.building(["pavilion"], x, z)
        for x, z in ((3.0, 0.7), (3.0, 4.3)):
            p.building(["tent"], x, z)
        for i, (x, z) in enumerate(((1.6, 2.5), (4.4, 2.5), (2.2, 1.6), (3.8, 1.6), (2.2, 3.4), (3.8, 3.4))):
            p.kenney(["stall-red", "stall-green"][i % 2], x, z, 3.2, [0, 90, 180])
        p.prop(["prop_wagon"], 0.6, 2.5, [0, 90])
        p.prop(["prop_wagon"], 5.4, 2.5, [90, 0])
        out["348"] = ("Faire", "a fair of tents and stalls round a maypole", p)

    # Services and government.
    p = plot()
    p.building(["tower"], 0.45, 0.55)
    p.building(NARROW_COTTAGES, 1.45, 1.45)
    if fan:
        p.building(["fountain"], 0.6, 1.5)
    else:
        p.building(["well"], 0.6, 1.5)
    p.prop(["trough"], 1.45, 0.4, [0, 90])
    out["350"] = ("Watch", "a watch tower, its guardhouse and water", p)
    p = plot()
    p.building(TALL_WIDE, 0.55, 1.5)
    p.building(["turret"], 1.5, 1.5)
    walls_round(p, 0.08, 0.08, 1.92, 0.95, "wall", gaps=(0,))
    if fan:
        fantasy_trees(p, [(0.5, 0.5), (1.5, 0.5)], 3.0)
    else:
        trees(p, [(0.5, 0.5), (1.5, 0.5)], 0.34)
    out["351"] = ("Magistrate's house" if not fan else "Lord's manor", "a stone house with its turret and walled garden", p)
    p = plot()
    p.building(["market_hall"], 1.85, 1.9)
    p.building(["turret"], 3.35, 1.9)
    p.building(["well"], 1.0, 0.6)
    p.kenney("stall-red", 2.4, 0.6, 3.2)
    p.kenney("stall-green", 3.0, 0.6, 3.2)
    for x in (0.4, 3.6):
        p.kenney("lantern", x, 2.8, 2.0)
    out["352"] = ("Town hall" if not fan else "Council hall", "a hall over an open market, with a belfry", p)
    p = plot()
    p.building(NARROW_COTTAGES, 0.4, 0.5)
    p.building(["waystone"], 1.3, 0.75)
    if fan:
        fantasy_trees(p, [(1.55, 0.3)], 3.0)
    else:
        trees(p, [(1.55, 0.3)], 0.34)
    out["353"] = ("Ward house", "a small stone house and a waystone", p)
    p = plot()
    p.building(["gatehouse"], 1.0, 1.6)
    walls_round(p, 0.08, 0.08, 1.92, 1.25, "wall", gaps=(0,))
    p.prop(["prop_wagon", "prop_crates"], 0.55, 0.6, [0, 90])
    p.building(["turret"], 1.45, 0.6)
    out["360"] = ("Town guard" if not fan else "Guard house", "a gatehouse and its walled yard", p)
    p = plot()
    p.building(["infirmary"], 1.15, 2.45)
    p.building(["chapel"], 3.0, 2.2)
    p.building(["infirmary"], 4.85, 2.45)
    p.building(["field"], 1.15, 0.75)
    p.building(["well"], 3.0, 0.55)
    p.building(["pond"], 4.85, 0.75)
    if fan:
        fantasy_trees(p, [(2.3, 0.5), (3.7, 0.5)])
    else:
        trees(p, [(2.3, 0.5), (3.7, 0.5)])
    out["370"] = ("Hospital of St John" if not fan else "Temple of healing", "two infirmary halls, a chapel and a herb garden", p)
    p = plot()
    if fan:
        p.building(["mage_tower"], 0.7, 1.6)
    else:
        p.building(["church"], 0.75, 1.5)
    p.building(["college"], 2.3, 1.5, 270)
    p.building(["well"], 1.55, 0.55)
    if fan:
        fantasy_trees(p, [(1.55, 2.6), (0.6, 0.35)], 3.2)
    else:
        trees(p, [(1.55, 2.6), (1.55, 1.5)])
    out["390"] = ("Cathedral school" if not fan else "Academy of magic", "a church and its school", p)
    p = plot()
    p.building(["college"], 1.5, 2.4)
    p.building(["chapel"], 3.35, 1.6)
    p.building(["well"], 1.5, 1.2)
    walls_round(p, 0.08, 0.08, 2.8, 1.85, "wall", gaps=(0, 1, 2))
    if fan:
        fantasy_trees(p, [(0.6, 0.6), (2.3, 0.6)])
    else:
        trees(p, [(0.6, 0.6), (2.3, 0.6)])
    out["391"] = ("Grammar school" if not fan else "Hall of apprentices", "a school range, its chapel and garden", p)
    p = plot()
    p.building(["college"], 2.0, 3.45)
    p.building(["college"], 0.48, 2.0, 90)
    p.building(["college"], 3.52, 2.0, 270)
    p.building(["college"], 2.0, 0.55)
    p.building(["mage_tower" if fan else "church"], 4.55, 2.0)
    p.building(["statue_stag"], 2.0, 2.0)
    out["392"] = ("College" if not fan else "Arcane university", "a quadrangle of ranges and a chapel", p)
    p = plot()
    p.building(["gatehouse"], 3.5, 4.65)
    p.building(["college"], 1.55, 4.55)
    p.building(["college"], 5.45, 4.55)
    p.building(["college"], 0.48, 2.7, 90)
    p.building(["college"], 6.52, 2.7, 270)
    p.building(["college"], 1.6, 0.5)
    p.building(["college"], 5.4, 0.5)
    p.building(["church"], 3.5, 1.0, 90)
    p.building(["statue_stag"], 2.6, 2.9)
    p.building(["statue_fox"], 4.4, 2.9)
    p.building(["well"], 3.5, 3.0)
    if fan:
        p.building(["mage_tower"], 3.5, 2.2)
        fantasy_trees(p, [(1.8, 2.2), (5.2, 2.2), (1.8, 3.4), (5.2, 3.4)])
    else:
        trees(p, [(1.8, 2.2), (5.2, 2.2), (1.8, 3.4), (5.2, 3.4)])
    out["393"] = ("University" if not fan else "Arcane university", "colleges round two courts, a church and a gatehouse", p)

    # Water and waste.
    p = plot()
    p.building(["pond"], 1.0, 0.5)
    p.building(NARROW_COTTAGES, 0.45, 1.5)
    p.building(["trough"], 1.45, 1.55)
    out["380"] = ("Conduit house" if not fan else "Spring house", "a conduit house and its cistern", p)
    p = plot()
    p.building(["turret"], 0.5, 0.48)
    p.building(["trough"], 0.5, 0.86)
    out["381"] = ("Cistern tower", "a stone cistern tower and a trough", p)
    p = plot()
    for x, z in ((0.85, 0.6), (2.15, 0.6), (0.85, 1.55), (2.15, 1.55)):
        p.building(["pond"], x, z)
    p.building(["mill_house"], 1.5, 2.5)
    p.building(["reeds"], 0.5, 2.5)
    out["382"] = ("Settling ponds" if not fan else "Purifying pools", "ponds, reeds and a sluice house", p)
    p = plot()
    p.building(["midden"], 1.5, 1.0)
    p.building(["midden"], 1.9, 2.1, 180)
    p.prop(["prop_wagon"], 0.6, 2.4, [0, 90])
    p.building(["haystacks"], 0.5, 1.9)
    walls_round(p, 0.08, 0.08, 2.92, 2.92, "rail_fence", gaps=(1,))
    out["383"] = ("Midden", "heaps of muck and rubbish behind a fence", p)
    p = plot()
    p.building(["rag_shed"], 0.55, 1.45)
    p.building(["timber_yard"], 1.5, 1.0)
    p.building(["dyeworks"], 0.6, 0.55)
    p.prop(["prop_wagon", "prop_crates"], 1.0, 1.65, [90, 0])
    out["384"] = ("Rag-and-bone yard" if not fan else "Tinkers' yard", "sheds of rags, scrap and timber", p)
    p = plot()
    p.building(["kiln"], 0.55, 1.45)
    p.building(["kiln"], 1.45, 1.45)
    p.building(["timber_yard"], 1.45, 0.55, 90)
    p.prop(["prop_wagon", "prop_crates"], 0.45, 0.45, [90, 0])
    out["385"] = ("Lime kiln" if not fan else "Salamander kiln", "two kilns and their wood", p)
    if fan:
        p = plot()
        p.building(["fountain"], 1.5, 1.5)
        for x, z in ((0.6, 0.6), (2.4, 0.6)):
            p.building(["pond"], x, z)
        for i, x in enumerate((0.3, 0.9, 2.1, 2.7)):
            p.kenney("hedge-large", x, 2.75, 2.5)
        fantasy_trees(p, [(0.4, 1.9), (2.6, 1.9)])
        out["376"] = ("Water garden", "pools, a fountain and hedges", p)
        p = plot()
        p.building(["millrace"], 0.5, 0.5)
        p.building(["wall"], 0.5, 0.9)
        out["377"] = ("Culvert", "a stone culvert", p)
        p = plot()
        p.building(["reeds"], 0.55, 0.55)
        p.building(["reeds"], 1.45, 1.45)
        fantasy_trees(p, [(1.5, 0.4), (0.4, 1.5)], 3.0)
        out["378"] = ("Reed bed", "pools of reeds", p)

    # Transit.
    p = plot()
    p.building(["waystone"], 0.5, 0.55)
    p.kenney("lantern", 0.15, 0.8, 2.0)
    out["386"] = ("Waystone", "a waystone, a bench and a lantern", p)
    p = plot()
    p.building(["trough"], 0.5, 0.75)
    p.building(["rail_fence"], 0.5, 0.35)
    p.kenney("cart", 0.25, 0.5, 2.4, 90)
    out["387"] = ("Horse post" if not fan else "Coach post", "a trough and hitching rail", p)
    p = plot()
    p.building(NARROW_COTTAGES, 0.42, 0.5)
    p.kenney("lantern", 0.88, 0.85, 2.0)
    out["388"] = ("Undercroft" if not fan else "Delvers' stair", "a stone house over a stair down", p)
    p = plot()
    if fan:
        for x in (1.0, 5.0):
            p.building(["mage_tower"], x, 1.0)
        p.building(["stone_circle"], 3.0, 1.5)
        p.building(["longhouse"], 3.0, 0.45, 90)
        out["397"] = ("Griffin roost", "roosting towers round a ring of stones", p)
    else:
        p.building(["longhouse"], 1.6, 0.42, 90)
        p.building(["longhouse"], 4.4, 0.42, 90)
        for x in (1.0, 2.5, 4.0):
            p.prop(["prop_wagon"], x, 2.2, [90, 0])
        p.building(["haystacks"], 5.4, 1.9)
        p.building(["trough"], 3.0, 1.3)
        p.building(["well"], 0.4, 2.4)
        out["397"] = ("Carriers' yard", "stables and wagons", p)
    p = plot()
    p.building(["landing"], 0.6, 0.35)
    p.building(NARROW_COTTAGES, 1.45, 1.45)
    p.prop(["prop_crates", "prop_wagon"], 0.5, 1.5, [0, 90])
    p.kenney("boat-row-small", 1.5, 0.15, 3.0, [0, 30], 0.0, "watercraft")
    out["396"] = ("Quay", "a landing stage and a boat shed", p)
    p = plot()
    p.building(["quay"], 2.0, 1.5)
    p.building(["crane"], 0.9, 0.6)
    p.building(["crane"], 2.4, 0.6)
    p.building(["tithe_barn"], 2.6, 2.45)
    p.prop(["prop_crates", "prop_wagon"], 0.6, 2.2, [0, 90])
    p.building(["haystacks"], 3.6, 1.4)
    p.kenney("ship-small", 1.6, -0.6, 1.6, [90, 270], -0.5, "watercraft")
    out["398"] = ("Port", "a stone quay with treadwheel cranes and a warehouse", p)
    return out


SERVICE_SIZES = {
    "31": (1, 1), "32": (1, 1), "33": (1, 1), "34": (1, 1), "35": (1, 1), "36": (1, 1), "37": (1, 1), "38": (2, 2), "39": (4, 2),
    "300": (5, 3), "303": (6, 6), "304": (4, 4), "305": (7, 6), "306": (1, 1), "320": (1, 1),
    "340": (6, 5), "341": (5, 4), "342": (3, 3), "343": (3, 3), "344": (4, 3), "345": (3, 3), "346": (4, 3), "347": (2, 2), "348": (6, 5),
    "350": (2, 2), "351": (2, 2), "352": (4, 3), "353": (2, 1), "360": (2, 2), "370": (6, 3),
    "390": (3, 3), "391": (4, 3), "392": (5, 4), "393": (7, 5),
    "376": (3, 3), "377": (1, 1), "378": (2, 2), "380": (2, 2), "381": (1, 1), "382": (3, 3), "383": (3, 3), "384": (2, 2), "385": (2, 2),
    "386": (1, 1), "387": (1, 1), "388": (1, 1), "396": (2, 2), "397": (6, 3), "398": (4, 3),
}


def ley_lines():
    """The fantasy skin's ley lines (power lines): a standing stone with a
    crystal on each tile, a glowing line to the next tile east and/or south."""
    out = {}
    for name, east, south in (("pole", False, False), ("east", True, False), ("south", False, True), ("corner", True, True)):
        p = Plot()
        p.building(["ley_stone"], 0.5, 0.5)
        if east:
            p.building(["ley_band"], 1.0, 0.5)
        if south:
            p.building(["ley_band"], 0.5, 1.0, 90)
        out["ley_" + name] = p
    return out


def fantasy_trees_plot(names):
    p = Plot()
    p.kenney(names[0], 0.5, 0.5, 3.8, [0, 90, 180, 270])
    p.slots[-1] = (["../kenney/fantasy-town/%s.obj" % n for n in names],) + p.slots[-1][1:]
    return p


def write(file, name, what, plot, ways, extra, w, l, skin):
    text = "# %s (%s skin): %s. Quaternius Medieval Village MegaKit pieces put\n" % (name, skin, what)
    text += "# together by scripts/build-medieval.py, with Quaternius ruins and Kenney\n# props. Written by scripts/gen-medieval.py. CC0 parts.\n"
    text += "variants = 16\n\n" + plot.text(ways, extra) + "\n"
    with open(os.path.join(OUT, file + ".compose"), "w") as f:
        f.write(text)
    with open(os.path.join(OUT, file + ".conf"), "w") as f:
        f.write("# Composed model (see %s.compose).\nname=%s\nwidth=%d\nlength=%d\nheight=%.2f\n" % (file, name, w, l, max(plot.height, 0.1)))


def main():
    n = 0
    for prefix, ways, extra, skin in (("med", MEDIEVAL, MEDIEVAL_EXTRA, "medieval"), ("fan", FANTASY, FANTASY_EXTRA, "fantasy")):
        for code, (name, what, plot) in services(skin).items():
            w, l = SERVICE_SIZES[code]
            write("%s_%s" % (prefix, code), name, what, plot, ways, extra, w, l, skin)
            n += 1
    for name, plot in ley_lines().items():
        write("fan_" + name, "Ley line", "a standing stone and its line", plot, FANTASY, FANTASY_EXTRA, 1, 1, "fantasy")
        n += 1
    for name, trees_ in (("tree_tall", ["tree-high", "tree-high-crooked", "tree-high-round"]), ("tree_round", ["tree", "tree-crooked", "tree-high-round"])):
        write("fan_" + name, "Trees", "a Kenney Fantasy Town Kit tree", fantasy_trees_plot(trees_), FANTASY, None, 1, 1, "fantasy")
        n += 1
    for code, (name, what, plot) in plots().items():
        w, l = SIZES[code]
        for prefix, ways, skin in (("med", MEDIEVAL, "medieval"), ("fan", FANTASY, "fantasy")):
            file = "%s_%s" % (prefix, code)
            text = "# %s (%s skin): %s, from Quaternius Medieval Village MegaKit\n" % (name, skin, what)
            text += "# pieces (scripts/build-medieval.py), painted per plot. Written by\n# scripts/gen-medieval.py. CC0 parts.\n"
            text += "variants = 16\n\n" + plot.text(ways) + "\n"
            with open(os.path.join(OUT, file + ".compose"), "w") as f:
                f.write(text)
            with open(os.path.join(OUT, file + ".conf"), "w") as f:
                f.write("# Composed model (see %s.compose).\nname=%s\nwidth=%d\nlength=%d\nheight=%.2f\n" % (file, name, w, l, plot.height))
            n += 1
    print("wrote %d compositions" % n)


if __name__ == "__main__":
    main()
