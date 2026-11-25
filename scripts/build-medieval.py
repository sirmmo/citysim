#!/usr/bin/env python3
"""Assembles whole buildings from the Quaternius Medieval Village MegaKit
(CC0) into assets/models/quaternius/medieval/, one trimmed .obj + .mtl each,
for the medieval and fantasy skins (scripts/gen-medieval.py writes their
compositions). Run from the repository root:

    python3 -I scripts/build-medieval.py "/path/to/Medieval Village MegaKit[Standard]" \
        --ruins "/path/to/Ultimate Modular Ruins Pack - Aug 2021" \
        --kenney /path/to/kenney_fantasy-town-kit_2.0

With --ruins the services get statues, columns and ruins from the Quaternius
Ultimate Modular Ruins Pack (CC0); --kenney copies the Kenney Fantasy Town
Kit models the compositions use (CC0) into assets/models/kenney/fantasy-town/.

The kit is modular: walls are 2 m wide and 3 m storeys (3.12 m with the beam
the next storey stands on), drawn on the line z = 0 with their outside
towards +z and their thickness inside (-z); corner posts stand on the
corners; Roof_RoundTiles_WxL roofs cover a W x L box of walls (ridge along
z) from the top of its walls, and Roof_Front_BrickW fills their gables;
door and window pieces fill the holes of the matching walls; the
Overhang_Side pieces carry an upper storey 2 m out over the street. Every
building here is put together on that grid with its door to +z, like the
other citysim buildings, standing on y = 0 and centred on x = z = 0.

The kit's materials are textured trim sheets; here every material is one
flat colour (sRGB) named so that compositions can repaint it (`paints`):
Plaster, Timber (beams, posts, floors), Stone (plinths, sills, chimneys,
steps), Stonework (rubble walls), Tiles (roofs), Shingles (wooden roofs and
awnings), Paintwork (doors and shutters), Glass, Iron and Vine; for the
services also Water, Soil, Crop, Hay, Paving, Banner, Cloth and Glow. The wood
trim sheet's grey band (stone edges) becomes Stone, by texture coordinate.
Positions are rounded to 3 decimals, shared corners merged, normals and
texture coordinates dropped (the renderer shades faces itself).
"""
import math
import os
import shutil
import sys

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "models", "quaternius", "medieval")
STOREY = 3.0

# Flat colours (sRGB) of the materials, after the kit's textures.
COLOURS = {
    "Plaster": (0.95, 0.90, 0.78),
    "Timber": (0.33, 0.22, 0.14),
    "Stone": (0.60, 0.58, 0.55),
    "Stonework": (0.56, 0.53, 0.48),
    "Tiles": (0.78, 0.34, 0.18),
    "Shingles": (0.47, 0.36, 0.26),
    "Paintwork": (0.42, 0.28, 0.18),
    "Glass": (0.27, 0.33, 0.40),
    "Iron": (0.20, 0.20, 0.22),
    "Vine": (0.33, 0.48, 0.20),
    # Made here, for the services and the fields and crafts.
    "Water": (0.32, 0.47, 0.56),
    "Soil": (0.43, 0.33, 0.23),
    "Crop": (0.56, 0.63, 0.27),
    "Hay": (0.84, 0.71, 0.40),
    "Paving": (0.70, 0.67, 0.62),
    "Banner": (0.66, 0.20, 0.16),
    "Cloth": (0.92, 0.88, 0.78),
    "Glow": (1.00, 0.56, 0.16),
}


def material(kit_material, v, piece):
    """The flat material of a face: the kit's material, the wood trim sheet
    split by the band its texture coordinate `v` falls in."""
    if kit_material == "MI_WoodTrim":
        return "Stone" if (1.0 - v) % 1.0 >= 0.78 else "Timber"
    if kit_material == "MI_WoodTrim_Wear":
        return "Shingles" if piece.startswith(("Roof_Wooden", "Window_Roof")) else "Paintwork"
    return {
        "MI_Plaster": "Plaster",
        "MI_UnevenBrick": "Stonework",
        "MI_Brick": "Stone",
        "MI_RedBrick": "Stone",
        "MI_RockTrim": "Stone",
        "MI_RoundTiles": "Tiles",
        "MI_WindowGlass": "Glass",
        "MI_MetalOrnaments": "Iron",
        "MI_Vine": "Vine",
    }[kit_material]


def ruins_material(kit_material, v, piece):
    """The flat material of a face of the Quaternius Ultimate Modular Ruins
    Pack (statues, columns, arches), coloured by material there."""
    return {
        "Black": "Iron",
        "DarkMetal": "Iron",
        "Metal": "Iron",
        "Fire": "Glow",
        "Wood": "Timber",
        "DarkWood": "Timber",
        "Flag": "Banner",
        "Green": "Vine",
        "Leaf_Texture": "Vine",
        "Texture_Leaves": "Vine",
    }.get(kit_material, "Stone")


class Kit:
    def __init__(self, folder, material_of=material):
        self.dir = os.path.join(folder, "OBJ") if os.path.isdir(os.path.join(folder, "OBJ")) else folder
        self.material_of = material_of
        self.cache = {}

    def piece(self, name):
        """The faces of a kit piece: (material, [(x, y, z), ...])."""
        if name not in self.cache:
            v, vt, cur, faces = [], [], None, []
            with open(os.path.join(self.dir, name + ".obj")) as f:
                for line in f:
                    w = line.split()
                    if not w:
                        continue
                    if w[0] == "v":
                        v.append(tuple(float(x) for x in w[1:4]))
                    elif w[0] == "vt":
                        vt.append(float(w[2]))
                    elif w[0] == "usemtl":
                        cur = w[1]
                    elif w[0] == "f":
                        corners = [c.split("/") for c in w[1:]]
                        uv = [vt[int(c[1]) - 1] for c in corners if len(c) > 1 and c[1]]
                        mean_v = sum(uv) / len(uv) if uv else 0.0
                        faces.append((self.material_of(cur, mean_v, name), [v[int(c[0]) - 1] for c in corners]))
            if name in FUSSY:
                # Glazing bars and mouldings too small to see in a city go,
                # and the panes of each window become one pane.
                glass = [p for m, pts in faces if m == "Glass" for p in pts]
                faces = [(m, pts) for m, pts in faces if m != "Glass" and area(pts) >= 0.01]
                for z in sorted({round(p[2], 2) for p in glass}):
                    hull = convex_hull([(p[0], p[1]) for p in glass if round(p[2], 2) == z])
                    faces.append(("Glass", [(x, y, z) for x, y in hull]))
            if name in SMALL:
                faces = [(m, pts) for m, pts in faces if area(pts) >= SMALL[name]]
            self.cache[name] = faces
        return self.cache[name]


# Windows simplified for the city's scale.
FUSSY = {"Window_Thin_Round1", "Window_Wide_Round1", "Window_Wide_Flat1"}
# Pieces whose faces smaller than this many square metres are dropped.
SMALL = {"Roof_Tower_RoundTiles": 0.005}


def convex_hull(points):
    """The convex hull of 2D points, counter-clockwise (monotone chain)."""
    pts = sorted(set(points))
    if len(pts) < 3:
        return pts

    def half(seq):
        out = []
        for p in seq:
            while len(out) >= 2 and (out[-1][0] - out[-2][0]) * (p[1] - out[-2][1]) - (out[-1][1] - out[-2][1]) * (p[0] - out[-2][0]) <= 0:
                out.pop()
            out.append(p)
        return out[:-1]

    return half(pts) + half(reversed(pts))


def normal(pts):
    """The (unnormalised) normal of a polygon, twice its area long."""
    n = [0.0, 0.0, 0.0]
    for i in range(1, len(pts) - 1):
        u = [pts[i][k] - pts[0][k] for k in range(3)]
        v = [pts[i + 1][k] - pts[0][k] for k in range(3)]
        n = [n[0] + u[1] * v[2] - u[2] * v[1], n[1] + u[2] * v[0] - u[0] * v[2], n[2] + u[0] * v[1] - u[1] * v[0]]
    return n


def facing_down(pts):
    """Whether a face looks down: the city is seen from above, so these go
    (but for UNDERSIDES), and with them a fifth of the faces."""
    n = normal(pts)
    return n[1] < -0.3 * math.sqrt(n[0] * n[0] + n[1] * n[1] + n[2] * n[2])


# Faces under the jettied storeys stay, for a view along the street.
UNDERSIDES = {"Overhang_Roof_Plaster"}


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def area(pts):
    return math.sqrt(sum(c * c for c in normal(pts))) / 2


def num(c):
    """A coordinate rounded to 3 decimals (a millimetre), without trailing zeros."""
    s = ("%.3f" % c).rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def turn(p, deg):
    """p turned by deg about +y (90 takes +z to +x), as compose.rs turns parts."""
    a = math.radians(deg)
    s, c = round(math.sin(a), 9), round(math.cos(a), 9)
    return (p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c)


class Building:
    def __init__(self, kit):
        self.kit = kit
        self.faces = []

    def add(self, name, x=0.0, y=0.0, z=0.0, rot=0, swap=None, scale=(1, 1, 1), keep=None, only=None):
        """Places a kit piece: scaled, turned by rot about +y, moved to
        (x, y, z), some materials swapped for others; `keep(points)`, given
        a face in the piece's own frame, drops the faces it refuses."""
        for mat, pts in self.kit.piece(name):
            if (keep and not keep(pts)) or (only and mat not in only):
                continue
            if name not in UNDERSIDES and facing_down(pts):
                continue
            mat = (swap or {}).get(mat, mat)
            out = []
            for p in pts:
                q = turn((p[0] * scale[0], p[1] * scale[1], p[2] * scale[2]), rot)
                out.append((q[0] + x, q[1] + y, q[2] + z))
            if scale[0] * scale[1] * scale[2] < 0:
                out.reverse()
            self.faces.append((mat, out))

    def box(self, mat, lo, hi):
        """A plain box (a post, a plinth, a counter)."""
        (x0, y0, z0), (x1, y1, z1) = lo, hi
        c = [(x0, y0, z0), (x1, y0, z0), (x1, y0, z1), (x0, y0, z1), (x0, y1, z0), (x1, y1, z0), (x1, y1, z1), (x0, y1, z1)]
        for q in ((0, 1, 2, 3), (4, 7, 6, 5), (0, 4, 5, 1), (1, 5, 6, 2), (2, 6, 7, 3), (3, 7, 4, 0)):
            self.faces.append((mat, [c[i] for i in q]))

    def solid(self, faces, cull=True):
        """(material, polygon) faces of a convex solid, each turned to look
        out from the solid's centre (the renderer shades by winding)."""
        pts = [p for _, f in faces for p in f]
        c = [sum(p[i] for p in pts) / len(pts) for i in range(3)]
        for mat, f in faces:
            n = normal(f)
            fc = [sum(p[i] for p in f) / len(f) for i in range(3)]
            if sum(n[i] * (fc[i] - c[i]) for i in range(3)) < 0:
                f = f[::-1]
            if not (cull and facing_down(f)):
                self.faces.append((mat, list(f)))

    def tube(self, mat, p0, p1, r0, r1=None, n=8, cap=None, phase=0.0):
        """A cylinder, cone or frustum from p0 to p1 (radii r0 and r1) with
        n sides; its end at p1 in `cap` (or mat)."""
        r1 = r0 if r1 is None else r1
        d = [p1[i] - p0[i] for i in range(3)]
        length = math.sqrt(sum(x * x for x in d))
        u = [x / length for x in d]
        a = (1.0, 0.0, 0.0) if abs(u[1]) > 0.9 else (0.0, 1.0, 0.0)
        s = cross(u, a)
        sl = math.sqrt(sum(x * x for x in s))
        s = [x / sl for x in s]
        t = cross(u, s)

        def ring(c, r):
            out = []
            for k in range(n):
                ang = phase + 2 * math.pi * k / n
                out.append(tuple(c[i] + r * (s[i] * math.cos(ang) + t[i] * math.sin(ang)) for i in range(3)))
            return out

        lo, hi = ring(p0, r0), ring(p1, r1)
        faces = []
        for k in range(n):
            j = (k + 1) % n
            faces.append((mat, [lo[k], lo[j], hi[j]] if r1 == 0 else [lo[k], lo[j], hi[j], hi[k]]))
        faces.append((mat, lo))
        if r1 > 0:
            faces.append((cap or mat, hi))
        self.solid(faces)

    def beam(self, mat, p0, p1, w):
        """A square beam w thick from p0 to p1."""
        self.tube(mat, p0, p1, w / math.sqrt(2), n=4, phase=math.pi / 4)

    def gem(self, x, y, z, r, h, mat="Glow"):
        """An eight-faced crystal standing on (x, y, z)."""
        mid = [(x + r, y + h / 2, z), (x, y + h / 2, z + r), (x - r, y + h / 2, z), (x, y + h / 2, z - r)]
        top, bottom = (x, y + h, z), (x, y, z)
        self.solid([(mat, [mid[k], mid[(k + 1) % 4], tip]) for k in range(4) for tip in (top, bottom)], cull=False)

    def merge(self, other, rot=0, dx=0.0, dz=0.0):
        """Another building's faces, turned and moved."""
        for mat, pts in other.faces:
            self.faces.append((mat, [(q[0] + dx, q[1], q[2] + dz) for q in (turn(p, rot) for p in pts)]))

    def turn_all(self, deg):
        self.faces = [(m, [turn(p, deg) for p in pts]) for m, pts in self.faces]

    def bounds(self):
        pts = [p for _, ps in self.faces for p in ps]
        return [min(p[i] for p in pts) for i in range(3)], [max(p[i] for p in pts) for i in range(3)]

    def write(self, name):
        lo, hi = self.bounds()
        # Centred on x and z, standing on y = 0.
        dx, dz, dy = (lo[0] + hi[0]) / 2, (lo[2] + hi[2]) / 2, lo[1]
        index, verts, by_mat = {}, [], {}
        for mat, pts in self.faces:
            face = []
            for p in pts:
                key = tuple(num(c) for c in (p[0] - dx, p[1] - dy, p[2] - dz))
                if key not in index:
                    index[key] = len(verts) + 1
                    verts.append(key)
                i = index[key]
                if not face or face[-1] != i:
                    face.append(i)
            if len(face) > 1 and face[0] == face[-1]:
                face.pop()
            if len(face) >= 3:
                by_mat.setdefault(mat, []).append(face)
        lines = ["# %s: Quaternius Medieval Village MegaKit pieces (CC0), assembled by scripts/build-medieval.py" % name, "mtllib %s.mtl" % name]
        lines += ["v %s %s %s" % v for v in verts]
        for mat in sorted(by_mat):
            lines.append("usemtl %s" % mat)
            lines += ["f " + " ".join(str(i) for i in f) for f in by_mat[mat]]
        with open(os.path.join(OUT, name + ".obj"), "w") as f:
            f.write("\n".join(lines) + "\n")
        mtl = ["# Flat colours in sRGB (scripts/build-medieval.py); compositions repaint them by name"]
        for mat in sorted(by_mat):
            mtl += ["newmtl %s" % mat, "Kd %.3f %.3f %.3f" % COLOURS[mat]]
        with open(os.path.join(OUT, name + ".mtl"), "w") as f:
            f.write("\n".join(mtl) + "\n")
        return sum(len(v) for v in by_mat.values()), (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2])


# Wall tokens: what fills each 2 m of a storey's side, by style.
# D door, R round-headed door, W wide window, A arched window, T narrow
# window, a trailing s adds open shutters and c closed ones; P plain wall,
# G close timbering, O an open arch (a shop front), - nothing.
PLASTER = {
    "D": ["Wall_Plaster_Door_Flat", ("DoorFrame_Flat_WoodDark", 0), ("Door_1_Flat", -0.53)],
    "R": ["Wall_Plaster_Door_Round", ("DoorFrame_Round_WoodDark", 0), ("Door_1_Round", -0.53)],
    "W": ["Wall_Plaster_Window_Wide_Flat", ("Window_Wide_Flat1", 0)],
    "A": ["Wall_Plaster_Window_Wide_Round", ("Window_Wide_Round1", 0)],
    "T": ["Wall_Plaster_Window_Thin_Round", ("Window_Thin_Round1", 0)],
    "P": ["Wall_Plaster_Straight_Base"],
    "G": ["Wall_Plaster_WoodGrid"],
    "O": ["Wall_Arch"],
}
UPPER = dict(PLASTER, W=["Wall_Plaster_Window_Wide_Flat2", ("Window_Wide_Flat1", 0)], P=["Wall_Plaster_Straight"])
BRICK = {
    "D": ["Wall_UnevenBrick_Door_Flat", ("DoorFrame_Flat_WoodDark", 0), ("Door_1_Flat", -0.53)],
    "R": ["Wall_UnevenBrick_Door_Round", ("DoorFrame_Round_WoodDark", 0), ("Door_1_Round", -0.53)],
    "W": ["Wall_UnevenBrick_Window_Wide_Flat", ("Window_Wide_Flat1", 0)],
    "A": ["Wall_UnevenBrick_Window_Wide_Round", ("Window_Wide_Round1", 0)],
    "T": ["Wall_UnevenBrick_Window_Thin_Round", ("Window_Thin_Round1", 0)],
    "P": ["Wall_UnevenBrick_Straight"],
    "G": ["Wall_UnevenBrick_Straight"],
    "O": ["Wall_Arch"],
}
SHUTTERS = {
    ("W", "s"): "WindowShutters_Wide_Flat_Open",
    ("W", "c"): "WindowShutters_Wide_Flat_Closed",
    ("A", "s"): "WindowShutters_Wide_Round_Open",
    ("A", "c"): "WindowShutters_Wide_Round_Closed",
    ("T", "s"): "WindowShutters_Thin_Round_Open",
    ("T", "c"): "WindowShutters_Thin_Round_Closed",
}
# Roofs by (width, length) and the height of their ridge above the walls.
ROOFS = {(4, 4), (4, 6), (4, 8), (6, 6), (6, 8), (6, 10), (6, 12), (6, 14), (8, 8), (8, 10), (8, 12), (8, 14)}
RIDGE = {4: 3.73, 6: 4.89, 8: 6.0}
EAVES = {4: (2.76, -0.52), 6: (4.12, -0.78), 8: (4.98, -0.78)}


def roof_height(w, x):
    """Height of a w-wide tiled roof above its walls at x from the ridge."""
    half, low = EAVES[w]
    return RIDGE[w] - (RIDGE[w] - low) * abs(x) / half


def outside(pts):
    """Whether a face of a wall piece shows outside: not its inner skin."""
    return max(p[2] for p in pts) > -0.25


class House(Building):
    """Storeys of walls on the kit's grid, a tiled roof, and what goes with them."""

    open_front = False
    # Windows as panes of glass only, without frames or shutters (the big
    # buildings of the services, seen from afar).
    plain_windows = False

    def wall(self, token, x, y, z, rot, style):
        kind, extra = token[0], token[1:]
        if kind == "-":
            return
        table = {"plaster": PLASTER, "upper": UPPER, "brick": BRICK}[style]
        wall, *inserts = table[kind]
        # Upper storeys' plinths (the grey base of the ground-floor walls) are plaster.
        swap = {"Stone": "Plaster"} if style == "upper" else None
        # The inside of the wall is never seen (but through an open front).
        self.add(wall, x, y, z, rot, swap, keep=None if self.open_front else outside)
        for name, dx in inserts:
            ox, _, oz = turn((dx, 0, 0), rot)
            plain = self.plain_windows and name.startswith("Window_")
            self.add(name, x + ox, y, z + oz, rot, only={"Glass"} if plain else None)
        if extra in ("s", "c") and not self.plain_windows:
            self.add(SHUTTERS[(kind, extra)], x, y, z, rot)

    def storey(self, y, w, zb, zf, sides, style, corners=True, skip_front_sides=False):
        """A storey of walls round x in [-w/2, w/2], z in [zb, zf]. `sides`
        gives the tokens of the front, right, back and left sides, each read
        from outside, left to right."""
        front, right, back, left = sides
        n, m = w // 2, int(round(zf - zb)) // 2
        for i in range(n):
            self.wall(front[i % len(front)], -w / 2 + 1 + 2 * i, y, zf, 0, style)
            self.wall(back[i % len(back)], w / 2 - 1 - 2 * i, y, zb, 180, style)
        for i in range(m):
            if skip_front_sides and i == 0:
                continue
            self.wall(right[i % len(right)], w / 2, y, zf - 1 - 2 * i, 90, style)
            self.wall(left[(m - 1 - i) % len(left)], -w / 2, y, zf - 1 - 2 * i, 270, style)
        if corners:
            for cx in (-w / 2, w / 2):
                for cz in (zb, zf):
                    self.add("Corner_Exterior_Wood", cx, y, cz)

    def jetty(self, y, w, zf, style):
        """The next storey carried 2 m out over the front: brackets and side
        walls of the overhang, and the boarded soffit under it."""
        kind = "UnevenBrick" if style == "brick" else "Plaster"
        self.add("Overhang_Side_%s_Long_L" % kind, -w / 2 + 1, y, zf)
        self.add("Overhang_Side_%s_Long_R" % kind, w / 2 - 1, y, zf)
        for i in range(w // 2):
            self.add("Overhang_Roof_Plaster", -w / 2 + 1 + 2 * i, y + STOREY, zf + 1)

    def roof(self, y, w, zb, zf, gables=True, swap=None):
        length = int(round(zf - zb))
        assert (w, length) in ROOFS, (w, length)
        half = (w / 2 - 0.3, length / 2 - 0.3)

        def seen(pts):
            # Faces under the roof, over the walls, are never seen.
            cx, cz = sum(p[0] for p in pts) / len(pts), sum(p[2] for p in pts) / len(pts)
            n = normal(pts)
            return not (n[1] < -0.3 * math.sqrt(sum(c * c for c in n)) and abs(cx) < half[0] and abs(cz) < half[1])

        self.add("Roof_RoundTiles_%dx%d" % (w, length), 0, y, (zb + zf) / 2, 0, swap, keep=seen)
        if gables:
            self.add("Roof_Front_Brick%d" % w, 0, y, zf, 0)
            self.add("Roof_Front_Brick%d" % w, 0, y, zb, 180)

    def chimney(self, y, w, x, z, stone="Prop_Chimney2"):
        """A chimney through a w-wide roof at x from the ridge, standing
        a little above it."""
        top = y + RIDGE[w] + 0.9
        h = 3.18 if stone == "Prop_Chimney" else 3.0
        base = y + roof_height(w, abs(x) + 0.5) - 0.3
        self.add(stone, x, base, z, 0, None, (1, (top - base) / h, 1))

    def dormer(self, y, w, z, side=1):
        """A dormer window on the roof's right (side 1) or left (-1) slope."""
        # Its front stands on the roof just inside the wall, so its ridge,
        # 2 m further in, meets the roof's slope.
        x = w / 2 - 2.2
        self.add("Roof_Dormer_RoundTile", side * x, y + roof_height(w, x + 1.9) - 0.2, z, 0 if side > 0 else 180)

    def awning(self, y, x, zf, n=1):
        """Wooden lean-to roofs over the front, n wall widths wide."""
        for i in range(n):
            self.add("Roof_Wooden_2x1", x + 2 * i - (n - 1), y, zf)

    def balcony(self, y, x, zf, n=1):
        """A 1 m deep balcony on n wall widths of the front, on brackets."""
        for i in range(n):
            cx = x + 2 * i - (n - 1)
            self.add("Floor_WoodDark_Half3", cx, y + 0.01, zf + 1)
            self.add("Balcony_Cross_Straight", cx, y, zf)
        for i in range(n + 1):
            self.add("Prop_Support", x - n + 2 * i, y - 2.92, zf, 0, None, (1, 1, 0.5))


def cottage(kit, w, d, style, front, right, back, left, chimney=None):
    b = House(kit)
    b.storey(0, w, -d / 2, d / 2, (front, right, back, left), style)
    b.roof(STOREY, w, -d / 2, d / 2)
    if chimney is not None:
        b.chimney(STOREY, w, *chimney)
    return b


def build_all(kit):
    """name -> Building."""
    out = {}

    # One-storey cottages.
    out["cottage_timber"] = cottage(kit, 4, 6, "plaster", ["Ws", "D"], ["P", "Ts", "P"], ["G", "P"], ["P", "W", "G"], (0.9, -1.5))
    out["cottage_stone"] = cottage(kit, 4, 4, "brick", ["T", "R"], ["Ac", "P"], ["P", "T"], ["P", "Wc"], (-0.9, -1.0))
    out["cottage_wide"] = cottage(kit, 6, 6, "plaster", ["Ws", "D", "Ws"], ["G", "T", "P"], ["P", "G", "P"], ["P", "Ws", "G"], (1.2, 0.5))
    b = cottage(kit, 6, 8, "brick", ["W", "R", "W"], ["P", "Tc", "P", "T"], ["P", "P", "P"], ["T", "P", "Tc", "P"], (-1.2, -2.0))
    b.dormer(STOREY, 6, 1.0)
    out["cottage_long"] = b

    # Two-storey houses: a timber upper storey jettied 2 m over the front.
    b = House(kit)
    b.storey(0, 4, -2, 2, (["W", "D"], ["P", "T"], ["P", "P"], ["T", "P"]), "plaster")
    b.jetty(0, 4, 2, "upper")
    b.storey(STOREY, 4, -2, 4, (["Ws", "Ws"], ["Ts", "G", "P"], ["G", "P"], ["P", "G", "Ts"]), "upper", skip_front_sides=True)
    b.roof(2 * STOREY, 4, -2, 4)
    b.chimney(2 * STOREY, 4, -0.8, -1.0)
    out["house_jetty"] = b

    b = House(kit)
    b.storey(0, 6, -3, 3, (["W", "R", "T"], ["P", "W", "P"], ["P", "T", "P"], ["T", "P", "P"]), "brick")
    b.storey(STOREY, 6, -3, 3, (["Ws", "A", "Ws"], ["Ts", "G", "Ts"], ["G", "P", "G"], ["Ts", "P", "Ts"]), "upper")
    b.roof(2 * STOREY, 6, -3, 3)
    b.balcony(STOREY, 0, 3, 1)
    b.chimney(2 * STOREY, 6, 1.3, -1.5)
    out["house_stone"] = b

    b = House(kit)
    b.storey(0, 6, -3, 3, (["Ws", "D", "W"], ["P", "Ts", "P"], ["P", "G", "P"], ["P", "Ts", "P"]), "plaster")
    b.jetty(0, 6, 3, "upper")
    b.storey(STOREY, 6, -3, 5, (["Ws", "A", "Ws"], ["Ts", "P", "G", "Ts"], ["G", "P", "G"], ["Ts", "G", "P", "Ts"]), "upper", skip_front_sides=True)
    b.roof(2 * STOREY, 6, -3, 5)
    b.dormer(2 * STOREY, 6, -0.5, -1)
    b.chimney(2 * STOREY, 6, 1.3, -1.5)
    out["house_wide"] = b

    # Tall town houses: three storeys, the upper two jettied in turn.
    b = House(kit)
    b.storey(0, 4, -3, 1, (["D", "W"], ["P", "T"], ["P", "P"], ["T", "P"]), "brick")
    b.jetty(0, 4, 1, "upper")
    b.storey(STOREY, 4, -3, 3, (["Ws", "Ts"], ["Ts", "G", "P"], ["G", "P"], ["P", "G", "Ts"]), "upper", skip_front_sides=True)
    b.storey(2 * STOREY, 4, -3, 3, (["Ts", "Ts"], ["P", "Ts", "G"], ["P", "G"], ["G", "Ts", "P"]), "upper")
    b.roof(3 * STOREY, 4, -3, 3)
    b.chimney(3 * STOREY, 4, 0.8, -1.5)
    out["townhouse_narrow"] = b

    b = House(kit)
    b.storey(0, 6, -4, 2, (["W", "R", "W"], ["P", "T", "P"], ["P", "P", "P"], ["P", "T", "P"]), "plaster")
    b.jetty(0, 6, 2, "upper")
    b.storey(STOREY, 6, -4, 4, (["Ws", "Ws", "Ws"], ["Ts", "G", "P", "Ts"], ["G", "P", "G"], ["Ts", "P", "G", "Ts"]), "upper", skip_front_sides=True)
    b.storey(2 * STOREY, 6, -4, 4, (["Ts", "A", "Ts"], ["P", "Ts", "G", "P"], ["P", "G", "P"], ["P", "G", "Ts", "P"]), "upper")
    b.roof(3 * STOREY, 6, -4, 4)
    b.dormer(3 * STOREY, 6, 0.0)
    b.chimney(3 * STOREY, 6, -1.3, -2.0)
    out["townhouse_wide"] = b

    # Longhouse or barn: one long storey of rubble walls, doors at both ends.
    b = cottage(kit, 6, 14, "brick", ["T", "R", "T"], ["P", "T", "P", "P", "T", "P", "P"], ["P", "D", "P"], ["P", "P", "T", "P", "P", "T", "P"], (1.2, -4.0))
    b.dormer(STOREY, 6, 1.0)
    b.dormer(STOREY, 6, -3.0, -1)
    out["longhouse"] = b

    # Tower: four storeys of rubble walls under a conical roof.
    b = House(kit)
    b.storey(0, 4, -2, 2, (["P", "R"], ["T", "P"], ["P", "P"], ["P", "T"]), "brick")
    b.storey(STOREY, 4, -2, 2, (["T", "P"], ["P", "T"], ["T", "P"], ["P", "T"]), "brick")
    b.storey(2 * STOREY, 4, -2, 2, (["P", "T"], ["T", "P"], ["P", "T"], ["T", "P"]), "brick")
    b.storey(3 * STOREY, 4, -2, 2, (["A", "A"], ["A", "A"], ["A", "A"], ["A", "A"]), "brick")
    b.add("Roof_Tower_RoundTiles", 0, 4 * STOREY, 0)
    out["tower"] = b

    # Shops: an open arcade on the street, the family upstairs.
    b = House(kit)
    b.open_front = True
    b.storey(0, 6, -3, 3, (["O", "O", "O"], ["P", "T", "P"], ["P", "D", "P"], ["P", "T", "P"]), "plaster")
    for x in (-2, 0, 2):
        b.add("Prop_Crate", x - 0.3, 0, 2.0, 15 * x, None, (0.7, 0.7, 0.7))
    b.awning(1.85, 0, 3, 3)
    b.storey(STOREY, 6, -3, 3, (["Ws", "Ws", "Ws"], ["Ts", "G", "Ts"], ["G", "P", "G"], ["Ts", "G", "Ts"]), "upper")
    b.roof(2 * STOREY, 6, -3, 3)
    b.chimney(2 * STOREY, 6, -1.3, -1.5)
    out["shop_arcade"] = b

    b = House(kit)
    b.open_front = True
    b.storey(0, 4, -3, 3, (["O", "D"], ["P", "W", "P"], ["P", "P"], ["P", "T", "P"]), "brick")
    b.add("Prop_Crate", -1.2, 0, 2.2, 10, None, (0.6, 0.6, 0.6))
    b.add("Prop_Crate", -0.6, 0, 1.6, 40, None, (0.5, 0.5, 0.5))
    b.awning(1.85, -1, 3, 1)
    b.storey(STOREY, 4, -3, 3, (["Ws", "Ts"], ["Ts", "G", "P"], ["G", "P"], ["P", "G", "Ts"]), "upper")
    b.roof(2 * STOREY, 4, -3, 3)
    out["shop_corner"] = b

    # A market stall: a lean-to of boards on four posts, crates under it.
    b = House(kit)
    for x in (-2, 2):
        b.add("Corner_Exterior_Wood", x, 0, 1.2, 0, None, (0.8, 0.52, 0.8))
        b.add("Corner_Exterior_Wood", x, 0, -1.2, 0, None, (0.8, 0.88, 0.8))
    for x in (-1, 1):
        b.add("Roof_Wooden_2x1", x, 1.6, -1.25, 0, None, (1, 1, 1.75))
    b.box("Timber", (-1.9, 0.0, 0.6), (1.9, 0.9, 1.1))
    b.add("Prop_Crate", -1.2, 0, -0.5, 20, None, (0.6, 0.6, 0.6))
    b.add("Prop_Crate", 0.9, 0, -0.6, -10, None, (0.7, 0.7, 0.7))
    b.add("Prop_Crate", 1.4, 0.9, 0.85, 5, None, (0.3, 0.3, 0.3))
    b.add("Prop_Crate", -0.6, 0.9, 0.85, 30, None, (0.3, 0.3, 0.3))
    out["stall"] = b

    # Tavern: stone ground floor with a wide door, timber upper storey with
    # a balcony, dormers and two chimneys.
    b = House(kit)
    b.storey(0, 8, -5, 3, (["W", "R", "W", "W"], ["P", "W", "P", "T"], ["P", "D", "P", "P"], ["T", "P", "W", "P"]), "brick")
    b.jetty(0, 8, 3, "upper")
    b.storey(STOREY, 8, -5, 5, (["Ws", "A", "A", "Ws"], ["Ts", "G", "Ws", "G", "Ts"], ["G", "Ts", "Ts", "G"], ["Ts", "G", "Ws", "G", "Ts"]), "upper", skip_front_sides=True)
    b.roof(2 * STOREY, 8, -5, 5)
    b.dormer(2 * STOREY, 8, 1.5)
    b.dormer(2 * STOREY, 8, -1.5, -1)
    b.chimney(2 * STOREY, 8, 1.6, -3.0)
    b.chimney(2 * STOREY, 8, -1.6, 2.5)
    b.awning(1.85, -3, 3, 1)
    b.awning(1.85, 3, 3, 1)
    out["tavern"] = b

    # Guild hall: a long hall of three storeys, its long side to the street.
    b = House(kit)
    b.storey(0, 8, -7, 7, (["P", "W", "T", "W"], ["A", "T", "A", "R", "A", "T", "A"], ["P", "T", "T", "P"], ["A", "T", "A", "D", "A", "T", "A"]), "brick")
    b.storey(STOREY, 8, -7, 7, (["Ts", "A", "A", "Ts"], ["Ws", "G", "Ws", "A", "Ws", "G", "Ws"], ["Ts", "G", "G", "Ts"], ["Ws", "G", "Ws", "A", "Ws", "G", "Ws"]), "upper")
    b.storey(2 * STOREY, 8, -7, 7, (["G", "T", "T", "G"], ["T", "G", "T", "G", "T", "G", "T"], ["G", "T", "T", "G"], ["T", "G", "T", "G", "T", "G", "T"]), "upper")
    b.roof(3 * STOREY, 8, -7, 7)
    for z in (-4.0, 4.0):
        b.dormer(3 * STOREY, 8, z)
        b.dormer(3 * STOREY, 8, z, -1)
    b.chimney(3 * STOREY, 8, 1.6, -6.0)
    b.chimney(3 * STOREY, 8, -1.6, 6.0)
    b.turn_all(-90)  # the +x side, with its door, to the front
    b.balcony(STOREY, 0, 4, 2)
    out["guildhall"] = b

    # Inn: a long two-storey range, its side to the street, with an arcade.
    b = House(kit)
    b.open_front = True
    b.storey(0, 6, -6, 6, (["W", "D", "W"], ["O", "O", "O", "R", "O", "O"], ["P", "T", "P"], ["T", "W", "P", "P", "W", "T"]), "plaster")
    b.storey(STOREY, 6, -6, 6, (["Ws", "Ts", "Ws"], ["Ws", "G", "Ws", "A", "Ws", "G"], ["G", "Ts", "G"], ["Ts", "G", "Ws", "G", "Ts", "G"]), "upper")
    b.roof(2 * STOREY, 6, -6, 6)
    for z in (-3.0, 3.0):
        b.dormer(2 * STOREY, 6, z)
    b.chimney(2 * STOREY, 6, -1.3, -4.0)
    b.chimney(2 * STOREY, 6, -1.3, 3.0)
    b.turn_all(-90)
    out["inn"] = b

    # Yard props.
    b = Building(kit)
    b.add("Prop_Wagon", 0, 0, 0)
    out["prop_wagon"] = b
    b = Building(kit)
    for x, z, r, s in ((0, 0, 0, 1.0), (1.1, 0.2, 25, 0.8), (0.5, 1.0, 50, 0.7), (0.0, 0.0, 10, 0.0)):
        if s:
            b.add("Prop_Crate", x, 0, z, r, None, (s, s, s))
    b.add("Prop_Crate", 0.1, 1.06, 0.05, 35, None, (0.6, 0.6, 0.6))
    out["prop_crates"] = b
    b = Building(kit)
    for i in range(2):
        b.add("Prop_WoodenFence_Single" if i % 2 == 0 else "Prop_WoodenFence_Extension1", -1 + 2 * i, 0.03, 0)
    out["prop_fence"] = b
    return out


# ---------------------------------------------------------------------------
# Services, works and the fields and crafts: the same kit, the ruins pack's
# statues and columns, and simple shapes made here (fields, ponds, mounds,
# tents, cranes, standing stones).


class Hall(House):
    """A House with plain windows, for the big buildings of the services."""

    plain_windows = True

    def wall(self, token, x, y, z, rot, style):
        # Plain panels for the close timbering, which costs three times the faces.
        super().wall("P" + token[1:] if token.startswith("G") else token, x, y, z, rot, style)


def tower_block(kit, storeys, tokens=None, spire=1.0, w=4):
    """A square stone tower of `storeys` under the kit's conical roof,
    stretched up by `spire`."""
    b = Hall(kit)
    for k in range(storeys):
        if tokens:
            t = tokens[k % len(tokens)]
        elif k == 0:
            t = (["T", "R"], ["P", "T"], ["P", "P"], ["T", "P"])
        else:
            t = (["T", "P"], ["P", "P"], ["P", "T"], ["P", "P"])
        b.storey(k * STOREY, w, -w / 2, w / 2, t, "brick")
    b.add("Roof_Tower_RoundTiles", 0, storeys * STOREY, 0, 0, None, (w / 4, spire, w / 4))
    return b


def church(kit, w=6, length=12, storeys=2, tower=5, spire=1.7):
    """A nave of rubble walls and tall windows with its tower and spire at
    the west end (+z), the door on the side."""
    b = Hall(kit)
    zb, zf = -length / 2, length / 2
    m = length // 2
    for k in range(storeys):
        side = ["T"] * m if k == 0 else ["A"] * m
        right = (["R"] + side[1:]) if k == 0 else side
        back = ["T", "A", "T"][: w // 2] if w == 6 else ["A"] * (w // 2)
        b.storey(k * STOREY, w, zb, zf, (["P"] * (w // 2), right, back, side), "brick")
    b.roof(storeys * STOREY, w, zb, zf)
    # The belfry (the top storey) has arched openings on every side.
    plain = (["P", "T"], ["P", "P"], ["P", "P"], ["T", "P"])
    t = tower_block(kit, tower, [plain] * (tower - 1) + [(["A", "A"], ["A", "A"], ["A", "A"], ["A", "A"])], spire)
    b.merge(t, 0, 0, zf + 2)
    return b


def long_range(kit, w, length, styles, tokens, door="R", dormers=0, chimneys=0, turn_to_front=True, open_front=False):
    """A long range of `styles` storeys (ground first), its long side (+x
    before the turn) to the front."""
    b = Hall(kit)
    b.open_front = open_front
    m = length // 2
    for k, style in enumerate(styles):
        if style == "open":
            b.storey(0, w, -length / 2, length / 2, (["O"] * (w // 2), ["O"] * m, ["O"] * (w // 2), ["O"] * m), "plaster")
            b.box("Timber", (-w / 2, 2.92, -length / 2), (w / 2, 3.02, length / 2))
            continue
        side = [tokens[k][i % len(tokens[k])] for i in range(m)]
        if k == 0 and door:
            side[m // 2] = door
        gable = ["T" if i % 2 else "P" for i in range(w // 2)]
        b.storey(k * STOREY, w, -length / 2, length / 2, (gable, side, gable, side[::-1]), style)
    top = len(styles) * STOREY
    b.roof(top, w, -length / 2, length / 2)
    for i in range(dormers):
        b.dormer(top, w, -length / 2 + (i + 1) * length / (dormers + 1))
    for i in range(chimneys):
        b.chimney(top, w, -(w / 2 - 1.7), (-1) ** i * (length / 2 - 2.0))
    if turn_to_front:
        b.turn_all(-90)
    return b


def arcade_hall(kit, uppers):
    """A market hall: open arches all round under timbered storeys."""
    b = Hall(kit)
    b.open_front = True
    b.storey(0, 8, -7, 7, (["O"] * 4, ["O"] * 7, ["O"] * 4, ["O"] * 7), "plaster")
    b.box("Timber", (-4, 2.92, -7), (4, 3.02, 7))
    b.box("Paving", (-4.2, 0, -7.2), (4.2, 0.06, 7.2))
    for k in range(uppers):
        b.storey((k + 1) * STOREY, 8, -7, 7, (["Ws", "G", "G", "Ws"], ["Ws", "G", "Ws", "A", "Ws", "G", "Ws"], ["G", "Ts", "Ts", "G"], ["Ws", "G", "Ws", "A", "Ws", "G", "Ws"]), "upper")
    top = (uppers + 1) * STOREY
    b.roof(top, 8, -7, 7)
    for z in (-4.0, 4.0):
        b.dormer(top, 8, z)
        b.dormer(top, 8, z, -1)
    b.turn_all(-90)
    return b


def gatehouse(kit):
    """Two round-roofed towers and the gate between them."""
    b = Hall(kit)
    for x in (-4, 4):
        b.merge(tower_block(kit, 3, [(["T", "P"], ["P", "T"], ["P", "P"], ["T", "P"]), (["T", "T"], ["T", "T"], ["T", "T"], ["T", "T"])]), 0, x, 0)
    c = Hall(kit)
    c.open_front = True
    c.storey(0, 4, -2, 2, (["O", "O"], ["-", "-"], ["O", "O"], ["-", "-"]), "brick", corners=False)
    c.box("Timber", (-2, 2.92, -2), (2, 3.02, 2))
    c.storey(STOREY, 4, -2, 2, (["T", "T"], ["-", "-"], ["T", "T"], ["-", "-"]), "brick", corners=False)
    c.roof(2 * STOREY, 4, -2, 2)
    b.merge(c)
    return b


def playhouse(kit):
    """A galleried playhouse: three ranges round a yard open to the street,
    a stage at the back under a cloth."""
    b = Hall(kit)
    for x in (-6, 6):
        r = Hall(kit)
        r.storey(0, 6, -7, 7, (["W", "D", "W"], ["Ws", "G", "Ws", "G", "Ws", "G", "Ws"], ["P", "G", "P"], ["Ws", "G", "Ws", "G", "Ws", "G", "Ws"]), "plaster")
        for k in (1,):
            r.storey(k * STOREY, 6, -7, 7, (["Ws", "G", "Ws"], ["G", "Ws", "G", "Ws", "G", "Ws", "G"], ["G", "G", "G"], ["G", "Ws", "G", "Ws", "G", "Ws", "G"]), "upper")
        r.roof(2 * STOREY, 6, -7, 7)
        b.merge(r, 0, x, 0)
    back = Hall(kit)
    for k in range(2):
        back.storey(k * STOREY, 6, -3, 3, (["G", "Ws", "G"], ["-", "-", "-"], ["G", "G", "G"], ["-", "-", "-"]), "upper" if k else "plaster", corners=False)
    back.roof(2 * STOREY, 6, -3, 3)
    b.merge(back, 0, 0, -4)
    b.box("Paving", (-3, 0, -1), (3, 0.05, 7))
    b.box("Timber", (-2.6, 0, -1), (2.6, 1.1, 1.4))
    b.box("Banner", (-2.6, 1.1, -0.95), (2.6, 4.2, -0.85))
    for x in (-2.5, 2.5):
        b.beam("Timber", (x, 1.1, 1.3), (x, 4.0, 1.3), 0.18)
    b.box("Banner", (-2.6, 3.9, -0.9), (2.6, 4.05, 1.4))
    return b


def smithy(kit):
    b = Hall(kit)
    b.open_front = True
    b.storey(0, 4, -3, 3, (["O", "O"], ["P", "T", "P"], ["P", "P"], ["P", "T", "P"]), "brick")
    b.roof(STOREY, 4, -3, 3)
    b.chimney(STOREY, 4, 0.9, -1.8)
    b.box("Stone", (-1.6, 0, -2.6), (0.2, 0.9, -1.2))
    b.box("Glow", (-1.3, 0.9, -2.3), (-0.1, 1.0, -1.5))
    b.box("Iron", (0.4, 0.6, 0.6), (1.2, 0.85, 0.9))
    b.box("Timber", (0.6, 0, 0.62), (1.0, 0.6, 0.88))
    b.add("Prop_Crate", -1.2, 0, 1.6, 20, None, (0.5, 0.5, 0.5))
    return b


def shed(kit, w=6, depth=4.2):
    """An open shed: posts and a pitched roof of boards along x."""
    b = Hall(kit)
    for x in range(-w // 2, w // 2 + 1, 2):
        for z in (-depth / 2 + 0.1, depth / 2 - 0.1):
            b.add("Corner_Exterior_Wood", x, 0, z, 0, None, (0.9, 0.92, 0.9))
    sz = depth / 3.0
    for x in range(-w // 2 + 1, w // 2, 2):
        b.add("Roof_Wooden_2x1_Center", x, 2.9, 0, 0, None, (1, 1, sz))
        b.add("Roof_Wooden_2x1_Center_Mirror", x, 2.9, 0, 0, None, (1, 1, sz))
    return b


def well(b, x=0.0, z=0.0):
    b.tube("Stone", (x, 0, z), (x, 0.9, z), 1.0, n=12, cap="Water")
    for dx in (-0.85, 0.85):
        b.beam("Timber", (x + dx, 0.9, z), (x + dx, 2.4, z), 0.16)
    b.beam("Timber", (x - 1.0, 2.3, z), (x + 1.0, 2.3, z), 0.12)
    for sz in (-1, 1):
        b.beam("Shingles", (x - 1.15, 2.45, z), (x + 1.15, 2.45, z), 0.06)
        b.solid([("Shingles", [(x - 1.2, 2.45, z), (x + 1.2, 2.45, z), (x + 1.2, 1.95, z + sz * 0.9), (x - 1.2, 1.95, z + sz * 0.9)]), ("Shingles", [(x - 1.2, 2.4, z), (x + 1.2, 2.4, z), (x + 1.2, 1.9, z + sz * 0.9), (x - 1.2, 1.9, z + sz * 0.9)])])


def field(b, x0, z0, x1, z1, rows, h=0.45):
    """A field of furrowed strips along z."""
    b.box("Soil", (x0, 0, z0), (x1, 0.04, z1))
    wid = (x1 - x0) / rows
    for i in range(rows):
        a, c = x0 + i * wid + 0.08, x0 + (i + 1) * wid - 0.08
        m = (a + c) / 2
        b.solid([("Crop", [(a, 0.04, z0 + 0.2), (a, 0.04, z1 - 0.2), (m, h, z1 - 0.2), (m, h, z0 + 0.2)]),
                 ("Crop", [(c, 0.04, z1 - 0.2), (c, 0.04, z0 + 0.2), (m, h, z0 + 0.2), (m, h, z1 - 0.2)]),
                 ("Crop", [(a, 0.04, z1 - 0.2), (c, 0.04, z1 - 0.2), (m, h, z1 - 0.2)]),
                 ("Crop", [(c, 0.04, z0 + 0.2), (a, 0.04, z0 + 0.2), (m, h, z0 + 0.2)])])


def rail_fence(b, p0, p1, mat="Timber"):
    """Posts and two rails from p0 to p1 (on the ground)."""
    dx, dz = p1[0] - p0[0], p1[1] - p0[1]
    length = math.hypot(dx, dz)
    n = max(1, int(length / 2.5))
    for i in range(n + 1):
        x, z = p0[0] + dx * i / n, p0[1] + dz * i / n
        b.beam(mat, (x, 0, z), (x, 1.1, z), 0.14)
    for y in (0.5, 0.95):
        b.beam(mat, (p0[0], y, p0[1]), (p1[0], y, p1[1]), 0.08)


def stone_wall(b, p0, p1, h=1.4):
    """A low precinct wall with a coping."""
    x0, x1 = sorted((p0[0], p1[0]))
    z0, z1 = sorted((p0[1], p1[1]))
    b.box("Stonework", (x0 - 0.25, 0, z0 - 0.25), (x1 + 0.25, h, z1 + 0.25))
    b.box("Stone", (x0 - 0.32, h, z0 - 0.32), (x1 + 0.32, h + 0.15, z1 + 0.32))


def haystack(b, x, z, r=1.2):
    b.tube("Hay", (x, 0, z), (x, 1.3, z), r, n=10)
    b.tube("Hay", (x, 1.3, z), (x, 2.5, z), r, 0, n=10)


def logs(b, x, z, length=4.0):
    """A pile of logs along x."""
    for row, count in enumerate((4, 3, 2)):
        for i in range(count):
            zz = z + (i - (count - 1) / 2) * 0.5
            b.tube("Timber", (x - length / 2, 0.25 + row * 0.43, zz), (x + length / 2, 0.25 + row * 0.43, zz), 0.25, n=7, cap="Hay")


def barrels(b, x, z, n=3):
    for i in range(n):
        bx = x + (i % 2) * 0.8
        bz = z + (i // 2) * 0.8
        b.tube("Timber", (bx, 0, bz), (bx, 1.0, bz), 0.38, n=8, cap="Paintwork")
        b.tube("Iron", (bx, 0.2, bz), (bx, 0.28, bz), 0.4, n=8)
        b.tube("Iron", (bx, 0.72, bz), (bx, 0.8, bz), 0.4, n=8)


def tent(b, x, z, r=2.0, h=2.0, top=2.2):
    b.tube("Cloth", (x, 0, z), (x, h, z), r, n=10)
    b.tube("Banner", (x, h, z), (x, h + top, z), r * 1.08, 0, n=10)
    b.beam("Timber", (x, h + top - 0.2, z), (x, h + top + 0.8, z), 0.08)
    b.solid([("Banner", [(x, h + top + 0.8, z), (x, h + top + 0.45, z), (x + 0.6, h + top + 0.62, z)]), ("Banner", [(x, h + top + 0.8, z), (x + 0.6, h + top + 0.62, z), (x, h + top + 0.45, z + 0.01)])])


def stand(b, x0, x1, z, depth=3.0, rows=3):
    """Timber stands for watching the lists, under a cloth, facing -z."""
    for i in range(rows):
        b.box("Timber", (x0, 0, z + i * depth / rows), (x1, 0.6 + i * 0.6, z + depth))
    for x in (x0, (x0 + x1) / 2, x1):
        b.beam("Timber", (x, 0, z + depth), (x, 3.6, z + depth), 0.16)
        b.beam("Timber", (x, 0, z), (x, 2.8, z), 0.16)
    b.solid([("Banner", [(x0 - 0.2, 2.8, z - 0.3), (x1 + 0.2, 2.8, z - 0.3), (x1 + 0.2, 3.7, z + depth + 0.2), (x0 - 0.2, 3.7, z + depth + 0.2)]),
             ("Banner", [(x0 - 0.2, 2.75, z - 0.3), (x1 + 0.2, 2.75, z - 0.3), (x1 + 0.2, 3.65, z + depth + 0.2), (x0 - 0.2, 3.65, z + depth + 0.2)])])


def treadwheel_crane(b, x=0.0, z=0.0):
    """A timber crane on the quay: a treadwheel house turning the jib over
    the water (-z)."""
    b.box("Stone", (x - 2.5, 0, z - 2.5), (x + 2.5, 0.5, z + 2.5))
    for dx in (-1.2, 1.2):
        for dz in (-1.2, 1.2):
            b.beam("Timber", (x + dx, 0.5, z + dz), (x + dx * 0.4, 6.0, z + dz * 0.4), 0.25)
    b.tube("Timber", (x, 0.5, z), (x, 7.0, z), 0.3, n=6)
    b.beam("Timber", (x, 6.2, z + 1.5), (x, 7.6, z - 6.0), 0.3)
    b.beam("Timber", (x, 2.5, z - 0.5), (x, 6.4, z - 4.2), 0.2)
    b.beam("Iron", (x, 7.4, z - 5.8), (x, 4.0, z - 5.8), 0.05)
    n = 14
    for k in range(n):
        a0, a1 = 2 * math.pi * k / n, 2 * math.pi * (k + 1) / n
        for side in (-0.7, 0.7):
            p0 = (x + side, 2.9 + 2.2 * math.sin(a0), z + 1.5 + 2.2 * math.cos(a0))
            p1 = (x + side, 2.9 + 2.2 * math.sin(a1), z + 1.5 + 2.2 * math.cos(a1))
            b.beam("Timber", p0, p1, 0.18)
        if k % 2 == 0:
            b.beam("Timber", (x - 0.7, 2.9 + 2.2 * math.sin(a0), z + 1.5 + 2.2 * math.cos(a0)), (x + 0.7, 2.9 + 2.2 * math.sin(a0), z + 1.5 + 2.2 * math.cos(a0)), 0.12)
    b.beam("Timber", (x - 0.9, 2.9, z + 1.5), (x + 0.9, 2.9, z + 1.5), 0.2)


def stones_ring(b, r, n, h=3.2, glow=True):
    """A ring of standing stones, a crystal in the middle."""
    for k in range(n):
        a = 2 * math.pi * k / n
        x, z = r * math.cos(a), r * math.sin(a)
        b.tube("Stone", (x, 0, z), (x, h * (0.85 + 0.15 * (k % 3) / 2), z), 0.55, 0.38, n=4, phase=a)
    if glow:
        b.tube("Stone", (0, 0, 0), (0, 0.8, 0), 1.2, 1.0, n=6)
        b.gem(0, 0.8, 0, 0.6, 2.6)


def build_services(kit, ruins):
    """name -> Building, for the services, works and fields and crafts."""
    out = {}
    out["church"] = church(kit)
    out["chapel"] = church(kit, 4, 8, 2, 3, 1.5)
    out["turret"] = tower_block(kit, 2)
    out["mage_tower"] = tower_block(kit, 6, [(["T", "R"], ["P", "T"], ["P", "P"], ["T", "P"]), (["T", "P"], ["P", "T"], ["T", "P"], ["P", "T"]), (["A", "A"], ["A", "A"], ["A", "A"], ["A", "A"])], 2.2)
    out["mage_tower"].gem(0, 6 * STOREY + 4.86 * 2.2 + 0.2, 0, 0.8, 2.8)
    out["tithe_barn"] = long_range(kit, 8, 14, ["brick", "brick"], [["P", "T", "P"], ["P", "P", "T"]], dormers=0)
    out["market_hall"] = arcade_hall(kit, 1)
    out["infirmary"] = long_range(kit, 6, 14, ["brick", "upper"], [["A", "T"], ["Ws", "G"]], dormers=2, chimneys=2)
    out["college"] = long_range(kit, 6, 14, ["brick", "brick"], [["T", "A"], ["A", "T"]], dormers=3, chimneys=2)
    out["gatehouse"] = gatehouse(kit)
    out["playhouse"] = playhouse(kit)
    out["smithy"] = smithy(kit)
    b = long_range(kit, 8, 10, ["open", "brick"], [["P", "O"], ["T", "P"]], door=None, chimneys=0, turn_to_front=False, open_front=True)
    for x, z in ((-2.5, -3.0), (2.5, -3.0), (0.0, 3.4)):
        b.chimney(2 * STOREY, 8, x, z)
    for x in (-2.0, 2.0):
        b.box("Stone", (x - 1.0, 0, -2.0), (x + 1.0, 1.0, 0.0))
        b.box("Glow", (x - 0.7, 1.0, -1.7), (x + 0.7, 1.1, -0.3))
    out["forge_hall"] = b
    b = shed(kit)
    for i, x in enumerate((-1.8, 0.0, 1.8)):
        b.tube("Timber", (x, 0, -0.9), (x, 0.9, -0.9), 0.7, n=10, cap="Banner")
    for x in (-2.5, 2.5):
        b.beam("Timber", (x, 0, 1.5), (x, 2.2, 1.5), 0.12)
    b.beam("Timber", (-2.5, 2.1, 1.5), (2.5, 2.1, 1.5), 0.08)
    for x in (-1.6, 0.0, 1.6):
        b.box("Banner", (x - 0.6, 0.9, 1.45), (x + 0.6, 2.05, 1.55))
    out["dyeworks"] = b
    b = shed(kit, 4)
    logs(b, 0.0, 4.0)
    logs(b, 0.0, -4.4, 5.0)
    b.box("Soil", (-0.6, 0, -0.6), (0.6, 0.05, 0.6))
    b.beam("Timber", (-1.6, 0.9, 0), (1.6, 0.9, 0), 0.4)
    out["timber_yard"] = b
    b = Hall(kit)
    b.storey(0, 6, -4, 4, (["W", "R", "W"], ["P", "T", "P", "T"], ["P", "P", "P"], ["T", "P", "T", "P"]), "brick")
    b.storey(STOREY, 6, -4, 4, (["Ws", "G", "Ws"], ["G", "Ts", "G", "Ts"], ["G", "G", "G"], ["Ts", "G", "Ts", "G"]), "upper")
    b.roof(2 * STOREY, 6, -4, 4)
    b.chimney(2 * STOREY, 6, 1.3, -2.0)
    b.chimney(2 * STOREY, 6, -1.3, 1.5)
    barrels(b, 3.6, 3.2, 3)
    out["brewhouse"] = b
    b = tower_block(kit, 3, [(["T", "R"], ["P", "T"], ["P", "P"], ["T", "P"]), (["T", "P"], ["P", "T"], ["T", "P"], ["P", "T"]), (["P", "W"], ["P", "P"], ["P", "P"], ["P", "P"])], 1.2)
    out["mill_tower"] = b
    b = Hall(kit)
    b.storey(0, 6, -3, 3, (["W", "D", "T"], ["P", "-", "P"], ["P", "T", "P"], ["T", "P", "W"]), "plaster")
    b.storey(STOREY, 6, -3, 3, (["Ws", "G", "Ws"], ["G", "Ts", "G"], ["G", "P", "G"], ["Ts", "G", "Ts"]), "upper")
    b.roof(2 * STOREY, 6, -3, 3)
    b.add("Wall_UnevenBrick_Straight", 3, 0, 0, 90)
    out["mill_house"] = b

    # Things on the ground.
    b = Building(kit)
    field(b, -4.8, -4.8, 4.8, 4.8, 9)
    out["field"] = b
    b = Building(kit)
    field(b, -4.8, -9.8, 4.8, 9.8, 9)
    out["field_long"] = b
    b = Building(kit)
    for x, z, r in ((0, 0, 1.2), (2.6, 0.4, 1.0), (1.2, 2.2, 1.1)):
        haystack(b, x, z, r)
    out["haystacks"] = b
    b = Building(kit)
    well(b)
    out["well"] = b
    b = Building(kit)
    b.tube("Stone", (0, 0, 0), (0, 0.7, 0), 2.6, 2.6, n=14, cap="Water")
    b.tube("Stone", (0, 0.7, 0), (0, 1.7, 0), 0.45, 0.35, n=8)
    b.tube("Stone", (0, 1.7, 0), (0, 2.0, 0), 0.5, 1.2, n=10, cap="Water")
    b.tube("Stone", (0, 2.0, 0), (0, 2.9, 0), 0.18, 0.12, n=6)
    b.tube("Water", (0, 2.9, 0), (0, 3.2, 0), 0.25, 0.05, n=6)
    out["fountain"] = b
    b = Building(kit)
    b.box("Water", (-5.0, 0, -3.5), (5.0, 0.12, 3.5))
    for (x0, z0, x1, z1) in ((-5.4, -3.9, 5.4, -3.5), (-5.4, 3.5, 5.4, 3.9), (-5.4, -3.5, -5.0, 3.5), (5.0, -3.5, 5.4, 3.5)):
        b.box("Stone", (x0, 0, z0), (x1, 0.35, z1))
    out["pond"] = b
    b = Building(kit)
    for x, z, r, h in ((0, 0, 4.0, 2.2), (4.5, 2.5, 2.8, 1.5), (-3.8, 3.0, 2.4, 1.2)):
        b.tube("Soil", (x, 0, z), (x, h, z), r, r * 0.35, n=7, phase=x)
    b.tube("Hay", (2.0, 0, -3.5), (2.0, 0.9, -3.5), 1.6, 0.6, n=6)
    out["midden"] = b
    b = Building(kit)
    b.tube("Stonework", (0, 0, 0), (0, 5.0, 0), 2.6, 1.7, n=10, cap="Iron")
    b.tube("Stone", (0, 5.0, 0), (0, 6.6, 0), 0.8, 0.7, n=8, cap="Iron")
    b.box("Stone", (-0.9, 0, 2.2), (0.9, 1.9, 2.6))
    b.box("Glow", (-0.6, 0.2, 2.6), (0.6, 1.4, 2.65))
    logs(b, 0.0, 4.6, 3.0)
    out["kiln"] = b
    b = Building(kit)
    b.box("Timber", (-5, 0, -2), (5, 0.9, 2))
    b.box("Timber", (-1.5, 0, -8), (1.5, 0.8, -2))
    for x in (-5, 5):
        for z in (-2, 2):
            b.beam("Timber", (x, 0, z), (x, 1.6, z), 0.3)
    for z in (-7.5, -4.5):
        for x in (-1.5, 1.5):
            b.beam("Timber", (x, 0.0, z), (x, 1.3, z), 0.3)
    out["landing"] = b
    b = Building(kit)
    treadwheel_crane(b)
    out["crane"] = b
    b = Building(kit)
    b.box("Paving", (-20.5, 0, -15.5), (20.5, 0.12, 15.5))
    b.box("Stonework", (-20.5, 0, -15.5), (20.5, 0.6, -14.9))
    for i in range(9):
        x = -18 + 4.5 * i
        b.tube("Iron", (x, 0.6, -15.2), (x, 1.1, -15.2), 0.22, n=6)
    out["quay"] = b
    b = Building(kit)
    stand(b, -6, 6, 0)
    out["stand"] = b
    b = Building(kit)
    b.box("Timber", (-14, 0.9, -0.15), (14, 1.2, 0.15))
    for i in range(15):
        x = -14 + 2 * i
        b.beam("Timber", (x, 0, 0), (x, 1.2, 0), 0.15)
    for x in (-15, 15):
        b.beam("Timber", (x, 0, 0), (x, 4.5, 0), 0.2)
        b.box("Banner", (x + 0.1, 2.6, -0.02), (x + 1.3, 4.4, 0.02))
    out["tilt"] = b
    b = Building(kit)
    tent(b, 0, 0)
    out["tent"] = b
    b = Building(kit)
    tent(b, 0, 0, 3.0, 2.6, 3.0)
    out["pavilion"] = b
    b = Building(kit)
    b.tube("Timber", (0, 0, 0), (0, 9.0, 0), 0.18, 0.12, n=6)
    for k in range(8):
        a = 2 * math.pi * k / 8
        b.beam("Banner" if k % 2 else "Cloth", (0, 8.6, 0), (3.2 * math.cos(a), 0.3, 3.2 * math.sin(a)), 0.07)
    b.tube("Vine", (0, 8.0, 0), (0, 8.6, 0), 0.6, 0.4, n=8)
    out["maypole"] = b
    b = Building(kit)
    stones_ring(b, 6.5, 9)
    out["stone_circle"] = b
    b = Building(kit)
    b.tube("Stone", (0, 0, 0), (0, 2.6, 0), 0.5, 0.34, n=4, phase=0.4)
    b.gem(0, 2.45, 0, 0.35, 1.1)
    out["ley_stone"] = b
    b = Building(kit)
    b.box("Water", (-1.0, 0, -5.0), (1.0, 0.1, 5.0))
    for x in (-1.4, 1.0):
        b.box("Stone", (x, 0, -5.0), (x + 0.4, 0.5, 5.0))
    out["millrace"] = b
    b = Building(kit)
    b.box("Water", (-4.6, 0, -4.6), (4.6, 0.1, 4.6))
    for k in range(70):
        x, z = -4.2 + (k * 37 % 84) / 10.0, -4.2 + (k * 53 % 84) / 10.0
        b.beam("Vine", (x, 0.05, z), (x + 0.1 * math.sin(k), 1.3 + 0.5 * math.sin(k * 1.7), z + 0.1 * math.cos(k)), 0.08)
    out["reeds"] = b
    b = shed(kit, 4)
    barrels(b, -1.4, -1.2, 2)
    b.add("Prop_Crate", 1.0, 0, -0.8, 15, None, (0.6, 0.6, 0.6))
    b.add("Prop_Crate", 1.2, 0, 0.6, 50, None, (0.5, 0.5, 0.5))
    b.tube("Cloth", (-0.2, 0, 1.0), (-0.2, 0.9, 1.0), 0.9, 0.2, n=6)
    out["rag_shed"] = b
    b = Building(kit)
    b.box("Glow", (0, 0.1, -0.12), (10.5, 0.22, 0.12))
    out["ley_band"] = b
    b = Building(kit)
    rail_fence(b, (-5.0, 0.0), (5.0, 0.0))
    out["rail_fence"] = b
    b = Building(kit)
    stone_wall(b, (-5.0, 0.0), (5.0, 0.0))
    out["wall"] = b
    b = Building(kit)
    for x, z in ((-1.5, 0), (1.5, 0)):
        b.tube("Timber", (x, 0, z), (x, 0.9, z), 0.6, n=8, cap="Water")
    b.box("Stone", (-2.6, 0, 1.0), (2.6, 0.7, 1.9))
    b.box("Water", (-2.4, 0.7, 1.1), (2.4, 0.72, 1.8))
    out["trough"] = b
    b = Building(kit)
    b.tube("Stone", (0, 0, 0), (0, 1.4, 0), 0.32, 0.25, n=6, cap="Stone")
    b.tube("Stone", (0, 1.4, 0), (0, 1.75, 0), 0.25, 0.0, n=6)
    b.beam("Timber", (1.2, 0, 0.6), (1.2, 2.6, 0.6), 0.12)
    b.box("Paintwork", (1.25, 1.9, 0.2), (2.4, 2.25, 0.3))
    b.box("Paintwork", (0.1, 2.25, 0.85), (1.15, 2.55, 0.95))
    b.box("Timber", (-1.8, 0.45, 1.2), (0.2, 0.55, 1.7))
    for x in (-1.6, 0.0):
        b.box("Timber", (x, 0, 1.3), (x + 0.15, 0.45, 1.6))
    out["waystone"] = b

    if ruins:
        b = Building(ruins)
        b.add("Wall_ArchRound_Overgrown_Broken", 0, 0, -3)
        b.add("Column_Round", -4.5, 0, 1.0)
        b.add("Column_Round", 4.5, 0, 1.0)
        b.add("Column_Round_Short", -4.5, 0, 4.0)
        b.add("Column_Square", 4.5, 0, 4.0)
        b.add("Wall_Broken", -3.0, 0, 5.5, 20)
        b.add("Floor_Diamond", 0, 0.1, 1.0)
        b.add("Floor_Diamond", 0, 0.1, 3.0)
        out["ruins"] = b
        b = Building(ruins)
        b.add("Statue_Stag", 0.5, 0, 0)
        out["statue_stag"] = b
        b = Building(ruins)
        b.add("Statue_Fox", 0, 0, 0)
        out["statue_fox"] = b
        b = Building(ruins)
        for x in (-3.0, 0.0, 3.0):
            b.add("Column_Round", x, 0, 0)
        out["columns"] = b
    return out


def copy_kenney(folder):
    """Copies the Kenney Fantasy Town Kit models the compositions use, with
    its colour map and licence, into assets/models/kenney/fantasy-town/."""
    src = os.path.join(folder, "Models", "OBJ format")
    out = os.path.join(OUT, "..", "..", "kenney", "fantasy-town")
    os.makedirs(out, exist_ok=True)
    for name in KENNEY:
        for ext in (".obj", ".mtl"):
            shutil.copyfile(os.path.join(src, name + ext), os.path.join(out, name + ext))
    shutil.copyfile(os.path.join(src, "Textures", "colormap.png"), os.path.join(out, "colormap.png"))
    shutil.copyfile(os.path.join(folder, "License.txt"), os.path.join(out, "License.txt"))
    print("copied %d Kenney Fantasy Town Kit models" % len(KENNEY))


# Kenney Fantasy Town Kit models used as they are (colour-mapped).
KENNEY = ["windmill", "watermill", "stall-red", "stall-green", "cart", "cart-high", "banner-red",
          "banner-green", "lantern", "hedge-large", "fence", "tree", "tree-high", "tree-crooked", "tree-high-crooked", "tree-high-round",
          "rock-large", "rock-wide", "pillar-stone"]


def main():
    import argparse

    ap = argparse.ArgumentParser(description="Builds the medieval buildings from the Quaternius Medieval Village MegaKit.")
    ap.add_argument("megakit", help="the Medieval Village MegaKit folder")
    ap.add_argument("--ruins", help="the Ultimate Modular Ruins Pack folder (statues, columns, arches)")
    ap.add_argument("--kenney", help="the Kenney Fantasy Town Kit folder, copied for the compositions")
    ap.add_argument("--only", nargs="*", help="build only these")
    args = ap.parse_args()
    kit = Kit(args.megakit)
    ruins = Kit(args.ruins, ruins_material) if args.ruins else None
    os.makedirs(OUT, exist_ok=True)
    total = 0
    buildings = build_all(kit)
    buildings.update(build_services(kit, ruins))
    for name, b in buildings.items():
        if args.only and name not in args.only:
            continue
        faces, size = b.write(name)
        total += os.path.getsize(os.path.join(OUT, name + ".obj"))
        print("%-18s %5d faces  %.1f x %.1f x %.1f m" % (name, faces, size[0], size[2], size[1]))
    lic = os.path.join(args.megakit, "License_Standard.txt")
    if os.path.exists(lic):
        shutil.copyfile(lic, os.path.join(OUT, "License.txt"))
    if args.ruins and os.path.exists(os.path.join(args.ruins, "License.txt")):
        shutil.copyfile(os.path.join(args.ruins, "License.txt"), os.path.join(OUT, "License-Ruins.txt"))
    if args.kenney:
        copy_kenney(args.kenney)
    print("%.0f KB of models in %s" % (total / 1024, os.path.normpath(OUT)))


if __name__ == "__main__":
    main()
