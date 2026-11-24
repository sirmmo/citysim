#!/usr/bin/env python3
"""Writes the composed models that use the Quaternius town buildings
(assets/models/quaternius/town/, see scripts/import-quaternius.py):

* q_11 ... q_29: the homes and shops of the Quaternius skin
  (assets/skins/quaternius/skin.toml). Big plots are built of several
  buildings at the same scale as a small plot's, front rows facing +z and
  back rows turned to face -z, so a block keeps its doors and storeys the
  size of its neighbours'.
* mix_13, mix_14, mix_15, mix_17: the modern skin's houses and apartments,
  where each plot picks a Kenney or a Quaternius building (the little and
  family houses and the shops, com_21 ... com_24, mix them too: see
  house_small.compose, house_family.compose and gen-zone-variety.py).

Every Quaternius building is painted per plot: `paints` give the walls
(material Main), the brickwork (Bricks) and the roofs (Dark) one of the
colourways below.
Run from the repository root:

    python3 scripts/gen-quaternius.py
"""
import os

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "models", "composed")
Q = "../quaternius/town/"
K = "../kenney/"

# (walls, brickwork, roofs) in sRGB: cream, sea green, terracotta, sky blue,
# lilac, mint, slate, mustard, rose; roofs (material Dark) charcoal or brown.
CHARCOAL, BROWN, SLATE = [0.30, 0.30, 0.32], [0.38, 0.28, 0.24], [0.34, 0.36, 0.40]
COLOURWAYS = [
    ([0.93, 0.86, 0.72], [0.76, 0.62, 0.50], BROWN),
    ([0.45, 0.62, 0.58], [0.34, 0.48, 0.45], CHARCOAL),
    ([0.84, 0.50, 0.40], [0.64, 0.36, 0.30], SLATE),
    ([0.56, 0.69, 0.85], [0.42, 0.52, 0.68], CHARCOAL),
    ([0.68, 0.61, 0.80], [0.52, 0.46, 0.64], SLATE),
    ([0.64, 0.83, 0.72], [0.48, 0.66, 0.58], BROWN),
    ([0.36, 0.38, 0.46], [0.26, 0.28, 0.34], CHARCOAL),
    ([0.88, 0.76, 0.44], [0.70, 0.58, 0.32], SLATE),
    ([0.90, 0.66, 0.68], [0.72, 0.48, 0.50], BROWN),
]
PAINTS = "[" + ", ".join("{ Main = [%s, %s, %s], Bricks = [%s, %s, %s], Dark = [%s, %s, %s] }" % (*m, *b, *r) for m, b, r in COLOURWAYS) + "]"


def q(*names):
    return [Q + n + ".obj" for n in names]


def slot(parts, size, at, rotations=None, paints=True, tints=None):
    out = ["[[slot]]", "parts = [" + ", ".join('"%s"' % p for p in parts) + "]", "size = [%s, %s]" % size]
    out.append("at = [%s, 0.0, %s]" % at)
    if rotations:
        out.append("rotations = [%s]" % ", ".join(str(r) for r in rotations))
    if tints:
        out.append("tints = [" + ", ".join("[%s, %s, %s]" % tuple(t) for t in tints) + "]")
    if paints:
        out.append("paints = " + PAINTS)
    return "\n".join(out)


def yard(houses, size=(0.82, 0.64)):
    """A house at the back of a 1 x 1 plot, a tree or nothing and a car or nothing in front."""
    return [
        slot(houses, size, (0.09, 0.05)),
        slot(["", K + "nature/tree_default.obj", K + "nature/tree_oak.obj", K + "nature/tree_fat.obj"], (0.26, 0.26), (0.05, 0.71), [0, 90, 180, 270], False),
        slot(["", K + "car-kit/sedan.obj", K + "car-kit/hatchback-sports.obj", K + "car-kit/van.obj"], (0.14, 0.24), (0.68, 0.71), None, False),
    ]


def cells(parts, w, l, depth=1.0, margin=0.03):
    """A block of w x l one-tile buildings: the row at the front faces +z, the
    back row is turned to face -z (rows between face +z)."""
    out = []
    for z in range(l):
        for x in range(w):
            rot = [180] if (l > 1 and z == 0) else None
            out.append(slot(parts, (1 - 2 * margin, depth - 2 * margin), (x + margin, z * depth + margin), rot))
    return out


def rows(parts, w, l, deep, margin=0.03):
    """Two long rows on a w x l plot, front facing +z and back facing -z,
    with a yard between them."""
    size = (w - 2 * margin, deep)
    return [
        slot(parts, size, (margin, margin), [180]),
        slot(parts, size, (margin, l - margin - deep)),
    ]


LOW = q("1Story", "1Story_GableRoof", "1Story_RoundRoof")
HOUSES = q("2Story_GableRoof", "2Story_RoundRoof", "2Story_2", "2Story")
TOWN = q("2Story", "2Story_Balcony", "2Story_Columns", "2Story_Stairs")
TALL = q("3Story_Small", "3Story_Balcony", "2Story_Center", "3Story_Slim")
FLATS = q("4Story", "4Story_Center", "3Story_Balcony", "6Story_Stack")
SHOPS = q("1Story_Sign", "2Story_Sign", "2Story_Double")
WIDE = q("4Story_Wide_2Doors", "4Story_Wide_2Doors_Roof")

# (file, name, width, length, height, what it is, slots)
SKIN = [
    ("q_11", "Little house", 1, 1, 0.55, "a cottage with its yard", yard(LOW)),
    ("q_12", "Family house", 1, 1, 0.75, "a town house with its yard", yard(HOUSES)),
    ("q_13", "Town house", 1, 1, 1.0, "town houses", cells(TOWN, 1, 1)),
    ("q_14", "Town house", 1, 1, 1.1, "town houses", cells(TOWN + q("3Story_Small"), 1, 1)),
    ("q_15", "Tall town house", 1, 1, 1.5, "tall town houses", cells(TALL, 1, 1)),
    ("q_16", "Terrace", 1, 2, 1.5, "a pair of town houses, back to back", cells(TALL, 1, 2)),
    ("q_17", "Apartments", 1, 1, 2.0, "apartment houses", cells(FLATS, 1, 1)),
    ("q_18", "Apartment block", 2, 2, 2.0, "a block round a courtyard", rows(WIDE, 2, 2, 0.75)),
    ("q_19", "Tall terrace", 5, 1, 2.9, "a row of tall houses", cells(FLATS, 5, 1)),
    ("q_21", "Corner shop", 1, 1, 0.8, "low shops", cells(SHOPS, 1, 1)),
    ("q_22", "Café", 1, 1, 0.8, "low shops", cells(q("1Story_Sign", "2Story_Sign", "1Story"), 1, 1)),
    ("q_23", "Hardware store", 1, 1, 1.0, "shops", cells(q("2Story_Sign", "2Story_Double", "2Story_Center"), 1, 1)),
    ("q_24", "Shops", 2, 1, 1.0, "a pair of shops", cells(SHOPS, 2, 1)),
    ("q_25", "Department store", 4, 4, 3.0, "a big store round its yard", rows(WIDE, 4, 4, 1.55)),
    ("q_26", "Office block", 2, 2, 2.9, "offices", cells(FLATS, 2, 2)),
    ("q_27", "Offices", 2, 2, 1.1, "offices", rows(q("2Story_Wide", "2Story_Wide_2Doors"), 2, 2, 0.9)),
    ("q_28", "Head office", 2, 2, 2.0, "offices", rows(WIDE, 2, 2, 0.8)),
    ("q_29", "City block", 3, 3, 2.9, "a dense block", cells(FLATS, 3, 2, 1.5)),
]

# The modern skin's houses and apartments: the Kenney building it had, or a
# Quaternius one, per plot.
SUB = K + "city-suburban/"
COM = K + "city-commercial/"
KTINTS = [[1, 1, 1], [1.0, 0.93, 0.86], [0.9, 0.95, 1.0], [0.95, 1.0, 0.92]]


def mix(kenney, quaternius, margin=0.06):
    s = slot([kenney] + quaternius, (1 - 2 * margin, 1 - 2 * margin), (margin, margin), None, True, KTINTS[:2])
    return [s]


MIXED = [
    ("mix_13", "Family house", 1, 1, 0.9, "a house", mix(SUB + "building-type-c.obj", q("2Story", "2Story_Balcony"))),
    ("mix_14", "Town house", 1, 1, 0.9, "a house", mix(SUB + "building-type-b.obj", q("2Story_Columns", "2Story_Stairs"))),
    ("mix_15", "Town house", 1, 1, 1.0, "a house", mix(SUB + "building-type-n.obj", q("3Story_Small", "2Story_Center"))),
    ("mix_17", "Apartments", 1, 1, 1.6, "apartments", mix(SUB + "building-type-k.obj", q("3Story_Balcony", "4Story"))),
]


def main():
    for file, name, w, l, h, what, slots in SKIN + MIXED:
        kit = "Quaternius" if file.startswith("q_") else "Kenney or a Quaternius"
        text = "# %s (%s): %s buildings, painted, picked per plot.\n# Written by scripts/gen-quaternius.py. CC0 parts.\n" % (name, what, kit)
        text += "variants = 16\n\n" + "\n\n".join(slots) + "\n"
        with open(os.path.join(OUT, file + ".compose"), "w") as f:
            f.write(text)
        with open(os.path.join(OUT, file + ".conf"), "w") as f:
            f.write("# Composed model (see %s.compose).\nname=%s\nwidth=%d\nlength=%d\nheight=%s\n" % (file, name, w, l, h))
    print("wrote %d compositions" % len(SKIN + MIXED))


if __name__ == "__main__":
    main()
