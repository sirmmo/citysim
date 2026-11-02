# Skins: re-dressing the city for an era or a world

A skin changes how the city looks and what things are called. It never
changes the simulation, so any skin works with any city or save file.

## Selection

1. `--skin <id>` (or **View > Skin** in the game) forces a skin; `--skin none` disables skins.
2. Otherwise the city's **world** is matched against each skin's `applies.worlds`.
   The world comes from the GeoJSON's top-level `world` (or `ofm:world`) member,
   or from `--world` on import. This is meant for OpenFantasyMap worlds.
3. Otherwise the city's **year** is matched against `applies.years`. The year
   is the `--date` of a historical import (OpenHistoryMap / OpenHistoricalMap).
4. Otherwise no skin is used: the classic OpenCity look.

Skins are tried in the order listed in `assets/skins/index.toml`.

## What a skin can change today

| Key | Effect |
|---|---|
| `look.sky`, `look.water` | Background and water colours |
| `look.tint`, `look.saturation` | Colour treatment of every model and of the terrain |
| `look.ui_accent`, `look.ui_panel` | egui interface colours |
| `look.vehicles` | Hide cars, e.g. before 1900 or in a fantasy world |
| `[labels]` | Tool names, by tool id (`coal_plant = "Watermill"`) |
| `disabled_tools` | Tools that do not exist in that era or world |
| `[models]` | Graphic code to replacement `.ac` model |
| any file under the skin folder | Replaces the base file with the same relative path, e.g. `skins/fantasy/graphism/gui/road.png` |

Tool ids: `residential`, `commercial`, `industrial`, `road`, `power_line`,
`coal_plant`, `nuclear_plant`, `fire_station`, `police`, `hospital`,
`school`, `park`, `trees`, `raise`, `lower`, `bulldozer`.

Replacement models keep the footprint of the model they replace, because the
simulation uses the base catalog's sizes.

## Shipped examples

* **antiquity**: years -1000 to 500. Roman towns built from **0 A.D. art**:
  houses, storehouses, farmsteads, civic centres, barracks, fortress, wall
  towers, fields, mines and five tree species. See below for installing the art.
* **renaissance**: years 501 to 1799. Faded warm palette, no vehicles, no
  nuclear plant. The coal plant becomes a "Watermill" and power lines become a "Mill race".
* **industrial**: 1800-1945. Grey skies, coal age names, no nuclear plant.
* **fantasy**: worlds `Eldoria`, `*fantasy*`, `ofm:*`. Vivid palette, magic
  vocabulary ("Mage tower", "Ley line", "Dragon forge"), no vehicles.

Except for antiquity, these reuse OpenCity's modern-looking models, so they
are a mood change, not a full reskin.

## 0 A.D. art (antiquity skin)

The 0 A.D. models are CC BY-SA 3.0, so they are not committed. Install them with:

```sh
scripts/bake-0ad.sh [ART_DIR]
```

`ART_DIR` is the folder Ovian's `tools/zeroad/bake.sh` produces, by default
`/srv/ovian/web/projects/diorama/public/art`. That folder holds 0 A.D. actors
at a pinned commit, with variants, props, textures and footprints resolved.
[`tools/zeroad/antiquity.toml`](../tools/zeroad/antiquity.toml) maps OpenCity
graphic codes to 0 A.D. structures or gaia actors for one civilisation.
[`bake_0ad.py`](../tools/zeroad/bake_0ad.py) runs in a pinned
`python:3.12.7-slim` container with pycollada 0.8, NumPy 1.26.4 and Pillow 11.0.0. It:

* reads the COLLADA meshes and converts them from Z-up to Y-up;
* attaches props such as tree crowns at their prop points;
* applies the civilisation's player colour where the texture's alpha marks it;
* scales each model to the footprint of the code it replaces;
* writes AC3D models, PNG textures, `models.toml` and `ATTRIBUTION.txt` into
  `assets/skins/antiquity/`.

Without the baked art, the antiquity skin still applies its colours, names
and rules. Switching `civ` (e.g. `athen`, `gaul`, `han`) in the mapping
dresses the town as another civilisation.

![A Roman town with 0 A.D. art](antiquity.png)

## Assessment: what a full reskin needs

**The bottleneck is 3D content, not code.** One complete era or world set
needs about 60 models:

* 10 levels for each of residential, commercial and industrial, so 30 models.
* 15 road pieces.
* Power plants, the four services, a park and 3 to 5 trees.

The rest of the pipeline (selection, overrides, colour, names, rules) is in
place. Ways to get there, roughly by effort:

1. **More 0 A.D. mappings.** The bake covers 15 civilisations, so more era
   skins (Greek, Celtic, Han, Mauryan) are a mapping file each. Medieval and
   later eras need other sources.
2. **glTF loading plus CC0 kits.** Add a glTF loader next to the AC3D one
   (the `gltf` crate) and map existing CC0 low-poly kits onto the graphic
   codes through `[models]`. Kenney's town, castle and fantasy kits are
   examples. This is the fastest route to a convincing medieval or fantasy look.
3. **Procedural buildings from map tags.** OHM and OSM buildings carry
   `building:levels`, `roof:shape`, `building:material`, `building:colour`
   and `start_date`. A generator could extrude footprints with era-appropriate
   roofs and materials. Imported cities would then look like their source,
   not like generic zone models. It is more work, but it scales to any era
   and fits OHM data especially well.
4. **World styles from OpenFantasyMap.** OFM worlds already carry styling
   (georender rulesets). A converter could derive `look` colours and terrain
   palettes from a world's ruleset, so each world gets a matching skin
   automatically. This needs an OFM world/ruleset endpoint or export, which is
   not available to this project yet.
5. **Era rules (optional).** If eras should also play differently, add a
   rules overlay per skin: costs, growth ranges, and what "power" means
   (water mills before 1880). It is a small, separate change in
   `opencity-core`, kept out of skins on purpose so saves stay compatible.
6. **Living history.** Imported cities could start their calendar at the
   import year. Crossing an era boundary would then switch the skin, and
   newly available tools would unlock, so a city plays through history.

Fonts, terrain textures per biome, road textures (the antiquity skin still
has modern asphalt), and sound packs fit the same override mechanism and can
be added as needed.
