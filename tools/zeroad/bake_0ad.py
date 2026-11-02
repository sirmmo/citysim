"""Bake 0 A.D. art from the original repository into OpenCity culture skins.

    python bake_0ad.py --cache /cache/repo --config antiquity.toml --out <skins dir>

1. Shallow, sparse checkout of the pinned 0 A.D. commit: templates, civ data,
   actors and variants only (text).
2. Resolve every actor the mapping needs (variants, props) and list the exact
   meshes and textures they use.
3. Download just those files (Git LFS).
4. Convert: COLLADA meshes to AC3D (Y-up, props attached, scaled to the
   OpenCity footprint), textures to PNG with the culture's player colour.

Writes one skin per culture (`<out>/antiquity-<civ>/`) and lists them in
`<out>/generated.toml`, which the game reads next to `index.toml`.
"""

import argparse
import subprocess
import tomllib
from pathlib import Path

import collada
import numpy as np
from PIL import Image

from zeroad import PUBLIC, ZeroAD

Image.MAX_IMAGE_PIXELS = None
HERE = Path(__file__).resolve().parent

# OpenCity model footprints (tiles) for the codes we replace; must match assets/graphism.
FOOTPRINTS = {
    16: (1, 2), 18: (2, 2), 19: (5, 1), 24: (2, 1), 25: (4, 4), 26: (2, 2), 27: (2, 2),
    28: (2, 2), 29: (3, 3), 38: (2, 2), 39: (4, 2), 300: (5, 3), 303: (6, 6),
    350: (2, 2), 360: (2, 2), 370: (6, 3), 390: (3, 3),
}
MARGIN = 0.92          # share of the footprint a building may cover
MAX_PROP_DEPTH = 3
SKIP_PROPS = ("props/structures/decals/", "props/special/common/plot_boundary", "props/special/common/garrison")
SKIP_POINTS = ("projectile", "garrison", "loaded", "fire", "smoke")


def z_up_to_y_up(p):
    # 0 A.D. meshes are authored Z-up: (x, y, z) -> (x, z, -y).
    return np.stack([p[:, 0], p[:, 2], -p[:, 1]], axis=1)


def wanted_props(look):
    for point, prop in look.props:
        if prop.startswith(SKIP_PROPS) or any(s in point.lower() for s in SKIP_POINTS):
            continue
        yield point, prop


class Baker:
    def __init__(self, z: ZeroAD, out: Path, player_colour, max_texture: int):
        self.z = z
        self.out = out
        self.player = np.array(player_colour, dtype=np.float32)
        self.max_texture = max_texture
        self.textures = {}
        (out / "textures").mkdir(parents=True, exist_ok=True)

    def texture(self, rel: str, mat: str) -> str | None:
        """Converts a 0 A.D. texture to PNG; returns its path relative to the skin."""
        key = (rel, mat)
        if key in self.textures:
            return self.textures[key]
        src = self.z.texture_file(rel)
        if src is None:
            print(f"    warning: texture {rel} missing")
            self.textures[key] = None
            return None
        im = Image.open(src).convert("RGBA")
        if max(im.size) > self.max_texture:
            im.thumbnail((self.max_texture, self.max_texture), Image.LANCZOS)
        img = np.asarray(im).astype(np.float32)
        rgb, a = img[..., :3], img[..., 3:4] / 255.0
        if mat == "player":
            # Alpha marks where the player colour shows through.
            rgb = rgb * a + self.player * (1.0 - a)
            a = np.ones_like(a)
        elif mat != "alphatest":
            a = np.ones_like(a)
        out = np.concatenate([rgb, a * 255.0], axis=-1).clip(0, 255).astype(np.uint8)
        name = f"textures/{str(Path(rel).with_suffix('')).replace('/', '_')}_{mat}.png"
        Image.fromarray(out, "RGBA").save(self.out / name, optimize=True)
        self.textures[key] = name
        return name

    def mesh(self, rel: str):
        """Triangles of a COLLADA file, plus its prop points."""
        dae = collada.Collada(str(self.z.mesh_file(rel)), ignore=[collada.common.DaeUnsupportedError, collada.common.DaeBrokenRefError])
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
            if name.startswith(("prop-", "prop_")):
                props[name[5:]] = mat
            for child in getattr(node, "children", []):
                if isinstance(child, collada.scene.Node):
                    walk(child, mat)

        for node in dae.scene.nodes:
            walk(node, np.identity(4))
        return tris, props

    def actor(self, name: str, look: int, depth=0):
        """Flattened (texture, positions, uvs) parts of an actor, props included."""
        actor = self.z.actor(name)
        if actor is None or not actor.looks:
            return []
        lk = actor.looks[look % len(actor.looks)]
        if not lk.mesh:
            return []
        tex = self.texture(lk.base_texture, actor.material) if lk.base_texture else None
        try:
            tris, points = self.mesh(lk.mesh)
        except Exception as e:  # noqa: BLE001 - one broken mesh should not stop the bake
            print(f"    warning: {lk.mesh}: {e}")
            return []
        parts = [(tex, p, uv) for p, uv in tris]
        if depth < MAX_PROP_DEPTH:
            for point, prop in wanted_props(lk):
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


def mapping(z: ZeroAD, cfg: dict, civ: str):
    """(code, actor, look) for every model the mapping replaces for `civ`."""
    for code_s, spec in sorted(cfg["models"].items(), key=lambda kv: int(kv[0])):
        code = int(code_s)
        if "structure" in spec:
            template = f"structures/{civ}/{spec['structure']}"
            if not z.has_template(template):
                print(f"  {code}: {civ} has no {spec['structure']}, skipped")
                continue
            actor = z.text(template, "VisualActor/Actor")
        elif "gaia" in spec:
            actor = cfg["gaia"][spec["gaia"]][spec.get("index", 0)]
        else:
            actor = spec["actor"]
        if actor:
            yield code, actor, spec.get("look", code)


def needs(z: ZeroAD, actor: str, out: set, depth=0):
    """Repository paths of the meshes and textures an actor (with props) uses."""
    a = z.actor(actor)
    if a is None:
        return
    for lk in a.looks:
        if lk.mesh:
            out.add(f"/{PUBLIC}/art/meshes/{lk.mesh}")
        if lk.base_texture:
            stem = str(Path(lk.base_texture).with_suffix(""))
            out.add(f"/{PUBLIC}/art/textures/skins/{stem}.*")
        if depth < MAX_PROP_DEPTH:
            for _, prop in wanted_props(lk):
                needs(z, prop, out, depth + 1)


def toml_str(v: str) -> str:
    return '"' + v.replace("\\", "\\\\").replace('"', '\\"') + '"'


def skin_toml(z: ZeroAD, civ: str, colour) -> str:
    spec = lambda role, default: z.text(f"structures/{civ}/{role}", "Identity/SpecificName", default) if z.has_template(f"structures/{civ}/{role}") else default  # noqa: E731
    name = z.civ_name(civ)
    labels = {
        "residential": f"{spec('house', 'Houses')} · homes",
        "commercial": f"{spec('civil_centre', 'Market')} · trade",
        "industrial": f"{spec('farmstead', 'Farmstead')} · fields & crafts",
        "coal_plant": f"{spec('fortress', 'Fortress')} · stronghold",
        "police": f"{spec('barracks', 'Barracks')} · garrison",
        "fire_station": f"{spec('wall_tower', 'Tower')} · watch",
        "hospital": "Temple of healing",
        "school": "School",
        "park": "Garden",
        "trees": "Grove",
    }
    accent = ", ".join(str(int(c)) for c in colour)
    lines = [
        "# Generated by tools/zeroad/bake_0ad.py",
        f"name = {toml_str('Antiquity: ' + name)}",
        f"description = {toml_str(name + ' towns built from 0 A.D. art (CC BY-SA 3.0).')}",
        'era = "Antiquity"',
        f"culture = {toml_str(name)}",
        'disabled_tools = ["nuclear_plant", "power_line"]',
        "",
        "[applies]",
        "years = [-1000, 500]",
        "",
        "[look]",
        "sky = [0.70, 0.78, 0.88]",
        "saturation = 0.95",
        "water = [0.18, 0.40, 0.55, 0.60]",
        f"ui_accent = [{accent}]",
        "ui_panel = [36, 28, 24]",
        "vehicles = false",
        "",
        "[labels]",
    ] + [f"{k} = {toml_str(v)}" for k, v in labels.items()]
    return "\n".join(lines) + "\n"


ATTRIBUTION = """Models and textures in this folder are converted from 0 A.D. by Wildfire Games
(https://play0ad.com), {repo} at commit {commit},
by tools/zeroad/bake_0ad.py: COLLADA meshes converted to AC3D (Y-up, props
attached, scaled), textures re-encoded as PNG with the player colour applied.

0 A.D. art is licensed under Creative Commons Attribution-ShareAlike 3.0
(https://creativecommons.org/licenses/by-sa/3.0/). These derived files are shared
under the same licence.
"""


def bake_culture(z: ZeroAD, cfg: dict, civ: str, colour, out: Path):
    baker = Baker(z, out, colour, cfg.get("max_texture", 1024))
    (out / "models").mkdir(parents=True, exist_ok=True)
    entries = []
    for code, actor, look in mapping(z, cfg, civ):
        parts = baker.actor(actor, look)
        if not parts:
            print(f"  {code}: {actor}: no mesh, skipped")
            continue
        name = f"models/{code}.ac"
        baker.write_ac(parts, FOOTPRINTS.get(code, (1, 1)), out / name)
        entries.append(f'{code} = "{name}"')
    (out / "models.toml").write_text("# Generated by tools/zeroad/bake_0ad.py\n" + "\n".join(entries) + "\n")
    (out / "skin.toml").write_text(skin_toml(z, civ, colour))
    (out / "ATTRIBUTION.txt").write_text(ATTRIBUTION.format(**cfg["source"]))
    print(f"  baked {len(entries)} models, {len([t for t in baker.textures.values() if t])} textures")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", type=Path, required=True, help="where the 0 A.D. checkout lives")
    ap.add_argument("--config", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True, help="skins folder")
    args = ap.parse_args()
    cfg = tomllib.loads(args.config.read_text())
    src = cfg["source"]
    # Through `sh`: the executable bit can be lost on Windows bind mounts.
    fetch = ["sh", str(HERE / "fetch.sh"), str(args.cache), src["repo"], src["commit"]]
    subprocess.run(fetch, check=True)
    z = ZeroAD(args.cache)
    cultures = [c for c in cfg["bake"]["cultures"] if z.has_template(f"structures/{c['civ']}/house") or print(f"{c['civ']}: unknown civilisation, skipped")]
    paths: set[str] = set()
    for c in cultures:
        for _, actor, _ in mapping(z, cfg, c["civ"]):
            needs(z, actor, paths)
    listing = args.cache / ".citysim-needs"
    listing.write_text("\n".join(sorted(paths)) + "\n")
    subprocess.run(fetch + [str(listing)], check=True)
    baked = []
    for c in cultures:
        civ = c["civ"]
        print(f"{civ}: {z.civ_name(civ)}")
        bake_culture(z, cfg, civ, c.get("colour", [150, 40, 30]), args.out / f"antiquity-{civ}")
        baked.append(f"antiquity-{civ}")
    (args.out / "generated.toml").write_text(
        "# Generated by tools/zeroad/bake_0ad.py: skins baked from 0 A.D. art.\n"
        "skins = [" + ", ".join(toml_str(b) for b in baked) + "]\n"
    )


if __name__ == "__main__":
    main()
