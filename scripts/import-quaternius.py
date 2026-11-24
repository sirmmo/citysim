#!/usr/bin/env python3
"""Copies the Quaternius Ultimate Textured Building Pack's material-coloured
town buildings ("Models with Materials/OBJ") into assets/models/quaternius/town/,
trimmed for the game: positions rounded, normals and texture coordinates
dropped (the renderer shades faces itself, and these models are coloured by
material), and the materials' colours turned from Blender's linear values
into the sRGB the Kenney kits use. Run from the repository root:

    python3 scripts/import-quaternius.py "/path/to/Ultimate Textured Building Pack - Dec 2019"

The pack is CC0 (Quaternius, quaternius.com); its License.txt is copied too.
Every building faces +z (its door and shop windows), like the Kenney ones.
"""
import glob
import os
import sys

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "models", "quaternius", "town")


def srgb(x):
    x = max(0.0, min(1.0, x))
    return 12.92 * x if x <= 0.0031308 else 1.055 * x ** (1 / 2.4) - 0.055


def trim_mtl(src, dst):
    out = []
    for line in open(src):
        w = line.split()
        if w and w[0] == "newmtl":
            out.append("newmtl %s" % w[1])
        elif w and w[0] == "Kd":
            out.append("Kd %.4f %.4f %.4f" % tuple(srgb(float(x)) for x in w[1:4]))
    with open(dst, "w") as f:
        f.write("# Quaternius, CC0; colours in sRGB (scripts/import-quaternius.py)\n" + "\n".join(out) + "\n")


def trim_obj(src, dst, mtl):
    out = ["# Quaternius Ultimate Textured Building Pack, CC0 (scripts/import-quaternius.py)", "mtllib %s" % mtl]
    for line in open(src):
        w = line.split()
        if not w:
            continue
        if w[0] == "v":
            out.append("v %s" % " ".join(("%.4f" % float(x)).rstrip("0").rstrip(".") for x in w[1:4]))
        elif w[0] == "usemtl":
            out.append(line.strip())
        elif w[0] == "f":
            out.append("f " + " ".join(c.split("/")[0] for c in w[1:]))
    with open(dst, "w") as f:
        f.write("\n".join(out) + "\n")


def main():
    pack = sys.argv[1]
    src = os.path.join(pack, "Models with Materials", "OBJ")
    os.makedirs(OUT, exist_ok=True)
    n = 0
    for obj in sorted(glob.glob(os.path.join(src, "*_Mat.obj"))):
        name = os.path.basename(obj)[: -len("_Mat.obj")]
        trim_mtl(obj[:-4] + ".mtl", os.path.join(OUT, name + ".mtl"))
        trim_obj(obj, os.path.join(OUT, name + ".obj"), name + ".mtl")
        n += 1
    for lic in glob.glob(os.path.join(pack, "*.txt")):
        with open(lic) as f, open(os.path.join(OUT, "License.txt"), "w") as g:
            g.write(f.read())
    print("wrote %d buildings to %s" % (n, os.path.normpath(OUT)))


if __name__ == "__main__":
    main()
