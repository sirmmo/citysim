#!/usr/bin/env python3
"""Assembles whole buildings from the Quaternius Medieval Village MegaKit
(CC0) into assets/models/quaternius/medieval/, one trimmed .obj + .mtl each,
for the medieval and fantasy skins (scripts/gen-medieval.py writes their
compositions). Run from the repository root:

    python3 -I scripts/build-medieval.py "/path/to/Medieval Village MegaKit[Standard]"

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
awnings), Paintwork (doors and shutters), Glass, Iron and Vine. The wood
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


class Kit:
    def __init__(self, folder):
        self.dir = os.path.join(folder, "OBJ") if os.path.isdir(os.path.join(folder, "OBJ")) else folder
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
                        mean_v = sum(vt[int(c[1]) - 1] for c in corners) / len(corners)
                        faces.append((material(cur, mean_v, name), [v[int(c[0]) - 1] for c in corners]))
            if name in FUSSY:
                # Glazing bars and mouldings too small to see in a city go,
                # and the panes of each window become one pane.
                glass = [p for m, pts in faces if m == "Glass" for p in pts]
                faces = [(m, pts) for m, pts in faces if m != "Glass" and area(pts) >= 0.01]
                for z in sorted({round(p[2], 2) for p in glass}):
                    hull = convex_hull([(p[0], p[1]) for p in glass if round(p[2], 2) == z])
                    faces.append(("Glass", [(x, y, z) for x, y in hull]))
            self.cache[name] = faces
        return self.cache[name]


# Windows simplified for the city's scale.
FUSSY = {"Window_Thin_Round1", "Window_Wide_Round1", "Window_Wide_Flat1"}


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

    def add(self, name, x=0.0, y=0.0, z=0.0, rot=0, swap=None, scale=(1, 1, 1), keep=None):
        """Places a kit piece: scaled, turned by rot about +y, moved to
        (x, y, z), some materials swapped for others; `keep(points)`, given
        a face in the piece's own frame, drops the faces it refuses."""
        for mat, pts in self.kit.piece(name):
            if keep and not keep(pts):
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
            self.add(name, x + ox, y, z + oz, rot)
        if extra in ("s", "c"):
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


def main():
    kit = Kit(sys.argv[1])
    os.makedirs(OUT, exist_ok=True)
    only = sys.argv[2:]
    total = 0
    for name, b in build_all(kit).items():
        if only and name not in only:
            continue
        faces, size = b.write(name)
        total += os.path.getsize(os.path.join(OUT, name + ".obj"))
        print("%-18s %5d faces  %.1f x %.1f x %.1f m" % (name, faces, size[0], size[2], size[1]))
    lic = os.path.join(sys.argv[1], "License_Standard.txt")
    if os.path.exists(lic):
        shutil.copyfile(lic, os.path.join(OUT, "License.txt"))
    print("%.0f KB of models in %s" % (total / 1024, os.path.normpath(OUT)))


if __name__ == "__main__":
    main()
