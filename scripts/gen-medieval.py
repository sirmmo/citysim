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

    def text(self, ways):
        out = []
        for parts, (bw, bl), (x, z), rot, painted in self.slots:
            lines = ["[[slot]]", "parts = [%s]" % ", ".join('"%s"' % p for p in parts), "size = [%.3f, %.3f]" % (bw, bl), "at = [%.3f, 0.0, %.3f]" % (x, z)]
            if isinstance(rot, list):
                lines.append("rotations = [%s]" % ", ".join(str(r) for r in rot))
            elif rot:
                lines.append("rotations = [%d]" % rot)
            if painted:
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


def main():
    n = 0
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
