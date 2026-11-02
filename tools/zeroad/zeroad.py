"""Readers for the original 0 A.D. data files (simulation templates, actors).

Only what the OpenCity bake needs: template inheritance for names, actors and
footprints, and actor variants with props, textures and materials.
"""

from __future__ import annotations

import copy
import functools
import json
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from pathlib import Path

PUBLIC = "binaries/data/mods/public"
# Variants that are not the idle look of a building or tree.
SKIP_VARIANTS = ("destruction", "death", "dead", "garrison", "fire", "burn", "ruin", "carry", "gather", "attack", "packed", "winter", "snow")


class ZeroAD:
    def __init__(self, root: Path):
        self.root = Path(root)
        self.public = self.root / PUBLIC
        self.art = self.public / "art"
        self.templates = self.public / "simulation" / "templates"

    # ---------------------------------------------------------------- templates
    def has_template(self, name: str) -> bool:
        return (self.templates / f"{name}.xml").is_file()

    @functools.lru_cache(maxsize=None)
    def template(self, name: str) -> ET.Element:
        """Template with its parents (and `a|b` mixins) merged in."""
        node = ET.parse(self.templates / f"{name}.xml").getroot()
        parent = node.get("parent")
        if not parent:
            return node
        base = self._chain(parent)
        merge(base, node)
        return base

    def _chain(self, parent: str) -> ET.Element:
        names = parent.split("|")
        base = copy.deepcopy(self.template(self._resolve(names[-1])))
        for mixin in reversed(names[:-1]):
            name = self._resolve(mixin)
            if self.has_template(name):
                merge(base, self.template(name))
        return base

    def _resolve(self, name: str) -> str:
        return name if self.has_template(name) or not self.has_template(f"mixins/{name}") else f"mixins/{name}"

    def text(self, template: str, path: str, default: str = "") -> str:
        el = self.template(template).find(path)
        return (el.text or "").strip() if el is not None and el.text else default

    def footprint(self, template: str) -> tuple[float, float] | None:
        node = self.template(template)
        sq = node.find("Footprint/Square")
        if sq is not None:
            return float(sq.get("width")), float(sq.get("depth"))
        ci = node.find("Footprint/Circle")
        if ci is not None:
            r = 2 * float(ci.get("radius"))
            return r, r
        return None

    def civ_name(self, civ: str) -> str:
        if self.has_template(f"special/players/{civ}"):
            name = self.text(f"special/players/{civ}", "Identity/GenericName")
            if name:
                return name
        p = self.public / "simulation" / "data" / "civs" / f"{civ}.json"
        if p.is_file():
            return json.loads(p.read_text(encoding="utf-8")).get("Name", civ)
        return civ

    # ---------------------------------------------------------------- actors
    def actor_file(self, name: str) -> Path:
        return self.art / "actors" / (name if name.endswith(".xml") else f"{name}.xml")

    def actor(self, name: str, max_looks: int = 4) -> "Actor | None":
        path = self.actor_file(name)
        if not path.is_file():
            return None
        root = ET.parse(path).getroot()
        if root.tag == "qualitylevels":  # newer format: pick the highest quality
            actors = root.findall("actor")
            root = actors[-1] if actors else root
        groups = [g.findall("variant") for g in root.findall("group")]
        choices = [idle(vs) for vs in groups if vs]
        if not choices:
            return None
        vary = next((i for i, c in enumerate(choices) if len(c) > 1), None)
        n = min(max_looks, len(choices[vary])) if vary is not None else 1
        looks = []
        for k in range(n):
            look = Look()
            for i, c in enumerate(choices):
                v = c[k] if i == vary else max(c, key=frequency)
                for layer in self._variant_layers(v):
                    look.apply(layer)
            looks.append(look)
        material = (root.findtext("material") or "default.xml").strip()
        return Actor(name, looks, material_kind(material))

    def _variant_layers(self, el: ET.Element) -> list[dict]:
        layers = []
        if el.get("file"):
            p = self.art / "variants" / el.get("file")
            if p.is_file():
                layers.extend(self._variant_layers(ET.parse(p).getroot()))
        layer: dict = {}
        mesh = el.find("mesh")
        if mesh is not None and mesh.text:
            layer["mesh"] = mesh.text.strip()
        tex = el.find("textures")
        if tex is not None:
            layer["textures"] = {t.get("name", "baseTex"): t.get("file") for t in tex if t.get("file")}
        props = el.find("props")
        if props is not None:
            layer["props"] = [(p.get("attachpoint", "root"), p.get("actor", "")) for p in props]
        layers.append(layer)
        return layers

    def texture_file(self, rel: str) -> Path | None:
        stem = self.art / "textures" / "skins" / Path(rel).with_suffix("")
        for ext in (Path(rel).suffix, ".png", ".dds", ".tga"):
            p = stem.with_suffix(ext) if ext else None
            if p and p.is_file():
                return p
        return None

    def texture_patterns(self, rel: str) -> list[str]:
        stem = (Path(f"/{PUBLIC}/art/textures/skins") / rel).with_suffix("")
        return [f"{stem}.*"]

    def mesh_file(self, rel: str) -> Path:
        return self.art / "meshes" / rel


def merge(dst: ET.Element, src: ET.Element) -> None:
    """Layers `src` onto `dst` (0 A.D. template semantics, simplified)."""
    for k, v in src.attrib.items():
        if k not in ("parent", "replace", "merge", "op", "filtered", "disable"):
            dst.set(k, v)
    if (src.text or "").strip():
        dst.text = src.text
    for child in src:
        existing = dst.find(child.tag)
        if child.get("disable") is not None:
            if existing is not None:
                dst.remove(existing)
            continue
        if existing is None or child.get("replace") is not None:
            if existing is not None:
                dst.remove(existing)
            if child.get("merge") is not None and existing is None:
                continue
            dst.append(copy.deepcopy(child))
        else:
            merge(existing, child)


def frequency(v: ET.Element) -> float:
    try:
        return float(v.get("frequency", "0") or 0)
    except ValueError:
        return 0.0


def idle(vs: list[ET.Element]) -> list[ET.Element]:
    ok = [v for v in vs if not any(s in (v.get("name") or "").lower() for s in SKIP_VARIANTS)]
    return [v for v in ok if frequency(v) > 0] or ok[:1] or vs[:1]


@dataclass
class Look:
    mesh: str | None = None
    textures: dict[str, str] = field(default_factory=dict)
    props: list[tuple[str, str]] = field(default_factory=list)

    def apply(self, layer: dict) -> None:
        if "mesh" in layer:
            self.mesh = layer["mesh"]
        self.textures.update(layer.get("textures", {}))
        if "props" in layer:
            points = {p for p, _ in layer["props"]}
            self.props = [p for p in self.props if p[0] not in points] + [(p, a) for p, a in layer["props"] if a]

    @property
    def base_texture(self) -> str | None:
        return self.textures.get("baseTex") or next(iter(self.textures.values()), None)


@dataclass
class Actor:
    name: str
    looks: list[Look]
    material: str


def material_kind(material: str) -> str:
    m = material.lower()
    if m.startswith("player_trans"):
        return "player"
    if "trans" in m and not m.startswith("no_trans"):
        return "alphatest"
    return "opaque"
