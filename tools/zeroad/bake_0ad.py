"""Bake 0 A.D. art (as prepared by Ovian's bake) into an OpenCity skin.

Reads the Ovian art folder (manifest.json, m/*.dae, t/*.webp) and writes, for
each OpenCity graphic code in the mapping, an AC3D model plus PNG textures, and
a generated `models.toml` fragment the skin includes.

    python bake_0ad.py --art /art --config antiquity.toml --out /skin
"""

import argparse
import json
import math
import tomllib
from pathlib import Path

import collada
import numpy as np
from PIL import Image

# OpenCity model footprints (tiles) for the codes we replace; must match assets/graphism.
FOOTPRINTS = {
    16: (1, 2), 18: (2, 2), 19: (5, 1), 24: (2, 1), 25: (4, 4), 26: (2, 2), 27: (2, 2),
    28: (2, 2), 29: (3, 3), 38: (2, 2), 39: (4, 2), 300: (5, 3), 303: (6, 6),
    350: (2, 2), 360: (2, 2), 370: (6, 3), 390: (3, 3),
}
MARGIN = 0.92          # share of the footprint a building may cover
MAX_PROP_DEPTH = 3
SKIP_PROPS = ("props/structures/decals/", "props/special/common/plot_boundary")


def z_up_to_y_up(p):
    # 0 A.D. meshes are authored Z-up: (x, y, z) -> (x, z, -y).
    return np.stack([p[:, 0], p[:, 2], -p[:, 1]], axis=1)


class Baker:
    def __init__(self, art: Path, out: Path, player_colour):
        self.art = art
        self.out = out
        self.manifest = json.loads((art / "manifest.json").read_text())
        self.actors = self.manifest["actors"]
        self.player = np.array(player_colour, dtype=np.float32)
        self.textures = {}
        (out / "textures").mkdir(parents=True, exist_ok=True)

    def texture(self, tex: str, mat: str) -> str | None:
        """Converts a 0 A.D. texture to PNG; returns its path relative to the skin."""
        key = (tex, mat)
        if key in self.textures:
            return self.textures[key]
        src = self.art / "t" / f"{tex}.webp"
        if not src.exists():
            self.textures[key] = None
            return None
        img = np.asarray(Image.open(src).convert("RGBA")).astype(np.float32)
        rgb, a = img[..., :3], img[..., 3:4] / 255.0
        if mat == "player":
            # Alpha marks where the player colour shows through.
            rgb = rgb * a + self.player * (1.0 - a)
            a = np.ones_like(a)
        elif mat not in ("alphatest", "basic_trans", "trans"):
            a = np.ones_like(a)
        out = np.concatenate([rgb, a * 255.0], axis=-1).clip(0, 255).astype(np.uint8)
        name = f"textures/{tex.replace('/', '_')}_{mat}.png"
        Image.fromarray(out, "RGBA").save(self.out / name, optimize=True)
        self.textures[key] = name
        return name

    def mesh(self, path: str):
        """Triangles of a COLLADA file: list of (positions Nx3, uvs Nx2), plus prop points."""
        dae = collada.Collada(str(self.art / "m" / path), ignore=[collada.common.DaeUnsupportedError, collada.common.DaeBrokenRefError])
        tris, props = [], {}
        if dae.scene is None:
            return tris, props
        for geom in dae.scene.objects("geometry"):
            for prim in geom.primitives():
                if not isinstance(prim, (collada.triangleset.BoundTriangleSet, collada.polylist.BoundPolylist, collada.polygons.BoundPolygons)):
                    continue
                ts = prim if isinstance(prim, collada.triangleset.BoundTriangleSet) else prim.triangleset()
                if ts.vertex is None or ts.texcoordset is None or len(ts.texcoordset) == 0:
                    continue
                pos = ts.vertex[ts.vertex_index].reshape(-1, 3)
                uv = ts.texcoordset[0][ts.texcoord_indexset[0]].reshape(-1, 2)
                tris.append((z_up_to_y_up(pos), uv))

        def walk(node, m):
            mat = m @ node.matrix if hasattr(node, "matrix") else m
            name = (getattr(node, "id", None) or getattr(node, "name", None) or "").lower()
            if name.startswith("prop-") or name.startswith("prop_"):
                props[name[5:]] = mat
            for child in getattr(node, "children", []):
                if isinstance(child, collada.scene.Node):
                    walk(child, mat)

        for node in dae.scene.nodes:
            walk(node, np.identity(4))
        return tris, props

    def actor(self, name: str, look: int, depth=0):
        """Flattened (texture, positions, uvs) parts of an actor, props included."""
        actor = self.actors.get(name)
        if not actor or not actor.get("looks"):
            return []
        lk = actor["looks"][look % len(actor["looks"])]
        if not lk.get("mesh"):
            return []
        tex = self.texture(lk["tex"], actor.get("mat", "basic")) if lk.get("tex") else None
        try:
            tris, points = self.mesh(lk["mesh"])
        except Exception as e:  # noqa: BLE001 - one broken mesh should not stop the bake
            print(f"    warning: {lk['mesh']}: {e}")
            return []
        parts = [(tex, p, uv) for p, uv in tris]
        if depth < MAX_PROP_DEPTH:
            for point, prop in lk.get("props", []):
                if prop.startswith(SKIP_PROPS):
                    continue
                m = points.get(point.lower(), np.identity(4))
                for t, p, uv in self.actor(prop, 0, depth + 1):
                    # Prop points are in the parent's Z-up space.
                    zp = np.stack([p[:, 0], -p[:, 2], p[:, 1]], axis=1)
                    zp = (np.c_[zp, np.ones(len(zp))] @ m.T)[:, :3]
                    parts.append((t, z_up_to_y_up(zp), uv))
        return parts

    def write_ac(self, parts, footprint, dest: Path):
        """Scales parts to the footprint (tiles) and writes an AC3D file."""
        allp = np.concatenate([p for _, p, _ in parts])
        lo, hi = allp.min(axis=0), allp.max(axis=0)
        w, l = footprint
        span_x, span_z = max(hi[0] - lo[0], 1e-6), max(hi[2] - lo[2], 1e-6)
        s = min(w * MARGIN / span_x, l * MARGIN / span_z)
        centre = np.array([(lo[0] + hi[0]) / 2, lo[1], (lo[2] + hi[2]) / 2])
        offset = np.array([w / 2, 0.0, l / 2])
        lines = ["AC3Db", 'MATERIAL "white" rgb 1 1 1  amb 0.2 0.2 0.2  emis 0 0 0  spec 0 0 0  shi 10  trans 0', "OBJECT world", f"kids {len(parts)}"]
        for i, (tex, p, uv) in enumerate(parts):
            q = (p - centre) * s + offset
            lines += ["OBJECT poly", f'name "part{i}"']
            if tex:
                lines.append(f'texture "../{tex}"')
            lines.append(f"numvert {len(q)}")
            lines += [f"{x:.5f} {y:.5f} {z:.5f}" for x, y, z in q]
            n = len(q) // 3
            lines.append(f"numsurf {n}")
            for t in range(n):
                lines += ["SURF 0x30", "mat 0", "refs 3"]
                for k in range(3):
                    j = 3 * t + k
                    lines.append(f"{j} {uv[j][0]:.5f} {uv[j][1]:.5f}")
            lines.append("kids 0")
        dest.write_text("\n".join(lines) + "\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--art", type=Path, required=True)
    ap.add_argument("--config", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    cfg = tomllib.loads(args.config.read_text())
    baker = Baker(args.art, args.out, cfg.get("player_colour", [150, 40, 30]))
    civ = baker.manifest["civs"][cfg["civ"]]
    (args.out / "models").mkdir(parents=True, exist_ok=True)
    entries = []
    for code_s, spec in sorted(cfg["models"].items(), key=lambda kv: int(kv[0])):
        code = int(code_s)
        if "structure" in spec:
            actor = civ["structures"][spec["structure"]]["actor"]
        elif "gaia" in spec:
            actor = baker.manifest["gaia"][spec["gaia"]][spec.get("index", 0)]
        else:
            actor = spec["actor"]
        parts = baker.actor(actor, spec.get("look", code))
        if not parts:
            print(f"  {code}: {actor}: no mesh, skipped")
            continue
        name = f"models/{code}.ac"
        baker.write_ac(parts, FOOTPRINTS.get(code, (1, 1)), args.out / name)
        entries.append(f'{code} = "{name}"')
        print(f"  {code}: {actor} ({len(parts)} parts)")
    (args.out / "models.toml").write_text("# Generated by tools/zeroad/bake_0ad.py\n" + "\n".join(entries) + "\n")
    src = baker.manifest.get("source", {})
    (args.out / "ATTRIBUTION.txt").write_text(
        "Models and textures in this folder are converted from 0 A.D. by Wildfire Games\n"
        f"(https://play0ad.com), {src.get('repo', '')} at commit {src.get('commit', '')},\n"
        "via Ovian's bake (meshes unchanged, textures re-encoded) and tools/zeroad/bake_0ad.py\n"
        "(converted to AC3D, scaled, player colour applied, re-encoded as PNG).\n\n"
        "0 A.D. art is licensed under Creative Commons Attribution-ShareAlike 3.0\n"
        "(https://creativecommons.org/licenses/by-sa/3.0/). These derived files are shared\n"
        "under the same licence.\n"
    )
    print(f"baked {len(entries)} models, {len([t for t in baker.textures.values() if t])} textures into {args.out}")


if __name__ == "__main__":
    main()
