#!/usr/bin/env python3
"""Writes the modern skin's commercial and industrial buildings as composed
models (assets/models/composed/com_*.compose, ind_*.compose): each level
picks, per plot, one of a few Kenney buildings of its size and a tint, and
factories a chimney or a tank, so a street of shops or a row of works is not
one building repeated. Run from the repository root:

    python3 scripts/gen-zone-variety.py

The skin (assets/skins/modern/skin.toml) points graphics 21-29 and 31-39 at
these files. Parts are CC0 (Kenney City Kit Commercial and Industrial).
"""
import os

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "models", "composed")
COM, IND = "../kenney/city-commercial/", "../kenney/city-industrial/"
COM_TINTS = [[1, 1, 1], [1.0, 0.82, 0.7], [0.78, 0.88, 1.0], [0.86, 1.0, 0.86], [1.0, 0.95, 0.74]]
IND_TINTS = [[1, 1, 1], [1.0, 0.86, 0.74], [0.8, 0.88, 0.98], [1.0, 0.96, 0.78]]


def slot(parts, size, at, tints=None, height=None):
    out = ["[[slot]]", "parts = [" + ", ".join('"%s"' % p for p in parts) + "]", "size = [%s, %s]" % size]
    if height:
        out.append("height = %s" % height)
    out.append("at = [%s, 0.0, %s]" % at)
    if tints:
        out.append("tints = [" + ", ".join("[%s, %s, %s]" % tuple(t) for t in tints) + "]")
    return "\n".join(out)


def com(*letters):
    return [COM + "building-%s.obj" % x for x in letters]


def ind(*letters):
    return [IND + "building-%s.obj" % x for x in letters]


def whole(parts, w, l, tints, margin=0.03):
    return slot(parts, (w - 2 * margin, l - 2 * margin), (margin, margin), tints)


# (file, name, width, length, height, what it is, slots)
MODELS = [
    ("com_21", "Corner shop", 1, 1, 1.3, "low shops", [whole(com("c", "a", "d"), 1, 1, COM_TINTS)]),
    ("com_22", "Café", 1, 1, 1.3, "low shops", [whole(com("a", "b", "h"), 1, 1, COM_TINTS)]),
    ("com_23", "Hardware store", 1, 1, 1.7, "low shops", [whole(com("h", "g", "b", "d"), 1, 1, COM_TINTS)]),
    # Two shops side by side on a 2 x 1 plot.
    ("com_24", "Shops", 2, 1, 1.7, "a pair of shops", [
        slot(com("a", "c", "h", "f"), (0.95, 0.94), (0.03, 0.03), COM_TINTS),
        slot(com("d", "b", "g", "c"), (0.95, 0.94), (1.02, 0.03), COM_TINTS[1:] + COM_TINTS[:1]),
    ]),
    ("com_25", "Department store", 4, 4, 4.3, "a big store", [whole(com("n", "j"), 4, 4, COM_TINTS, 0.06)]),
    ("com_26", "Office block", 2, 2, 3.2, "offices", [whole(com("i", "l"), 2, 2, COM_TINTS, 0.04)]),
    ("com_27", "Offices", 2, 2, 1.7, "offices", [whole(com("j", "k", "e"), 2, 2, COM_TINTS, 0.04)]),
    ("com_28", "Head office", 2, 2, 2.1, "offices", [whole(com("k", "n", "j"), 2, 2, COM_TINTS, 0.04)]),
    ("com_29", "Skyscraper", 3, 3, 11.8, "towers", [whole(com("skyscraper-d", "skyscraper-b", "m"), 3, 3, COM_TINTS[:4], 0.08)]),
]


def works(code, name, letters, stack, stack_h):
    """A 1 x 1 works: the shed, and a chimney or nothing at its back corner."""
    return ("ind_%d" % code, name, 1, 1, max(stack_h, 1.0), "works", [
        whole(ind(*letters), 1, 1, IND_TINTS),
        slot(["", IND + stack, ""], (0.14, 0.14) if "basic" in stack else (0.2, 0.2), (0.06, 0.06), None, stack_h),
    ])


MODELS += [
    works(31, "Workshop", ("k", "h", "i"), "chimney-basic.obj", 0.8),
    works(32, "Joinery", ("i", "j", "o"), "chimney-basic.obj", 0.8),
    works(33, "Forge", ("j", "k", "h"), "chimney-basic.obj", 0.95),
    works(34, "Small factory", ("h", "g", "o"), "chimney-basic.obj", 1.0),
    works(35, "Factory", ("o", "e", "j"), "chimney-basic.obj", 1.1),
    works(36, "Mill", ("d", "e", "f"), "chimney-medium.obj", 1.5),
    works(37, "Works", ("n", "f", "d"), "chimney-medium.obj", 1.8),
    ("ind_38", "Plant", 2, 2, 1.9, "a plant with its yard", [
        whole(ind("l", "a", "e", "f"), 2, 2, IND_TINTS, 0.04),
        slot(["", IND + "chimney-medium.obj", IND + "detail-tank.obj"], (0.36, 0.36), (0.06, 0.06)),
    ]),
    ("ind_39", "Heavy plant", 4, 2, 2.2, "a heavy plant", [
        slot(ind("r", "a", "q"), (2.9, 1.9), (0.05, 0.05), IND_TINTS),
        slot([IND + "detail-tank-large.obj", IND + "chimney-large.obj", IND + "building-d.obj"], (0.9, 1.5), (3.02, 0.2), IND_TINTS[:2]),
    ]),
]


def main():
    for file, name, w, l, h, what, slots in MODELS:
        kit = "Commercial" if file.startswith("com") else "Industrial"
        text = "# %s (%s): one of several Kenney City Kit %s buildings and a\n# tint, picked per plot. Written by scripts/gen-zone-variety.py. CC0 parts.\n" % (name, what, kit)
        text += "variants = 10\n\n" + "\n\n".join(slots) + "\n"
        with open(os.path.join(OUT, file + ".compose"), "w") as f:
            f.write(text)
        with open(os.path.join(OUT, file + ".conf"), "w") as f:
            f.write("# Composed model (see %s.compose).\nname=%s\nwidth=%d\nlength=%d\nheight=%s\n" % (file, name, w, l, h))
    print("wrote %d compositions" % len(MODELS))


if __name__ == "__main__":
    main()
