# Skins: re-dressing the city for an era or a world

A skin changes how the city looks and what things are called. It never
changes the simulation, so any skin works with any city or save file.

## Selection

0. **File > New city…** offers an **era** and, where several skins share it, a
   **culture** (for example Antiquity: Romans, Athenians, Gauls, Ptolemies, Han,
   Mauryas). The chosen skin dresses the new city.
1. `--skin <id>` (or **View > Skin** in the game) forces a skin; `--skin none` disables skins.
2. Otherwise the city's **world** is matched against each skin's `applies.worlds`.
   The world comes from the GeoJSON's top-level `world` (or `ofm:world`) member,
   or from `--world` on import. This is meant for OpenFantasyMap worlds.
3. Otherwise the city's **year** is matched against `applies.years`. The year
   is the `--date` of a historical import (OpenHistoryMap / OpenHistoricalMap).
4. Otherwise no skin is used: the classic OpenCity look.

Skins are tried in the order listed in `assets/skins/generated.toml` (written
by art bakes) and then `assets/skins/index.toml`. A skin's `era` and `culture`
keys group it in the new-city dialog.

## What a skin can change today

| Key | Effect |
|---|---|
| `look.sky`, `look.water` | Background and water colours |
| `look.tint`, `look.saturation` | Colour treatment of every model and of the terrain |
| `look.ui_accent`, `look.ui_panel` | egui interface colours |
| `look.vehicles` | Hide cars, e.g. before 1900 or in a fantasy world |
| `look.bridge_pier` | An `.obj` under `models/` (no extension) stood under bridges, two per tile over water, stretched from the bed to the deck; without it bridges rest on plain columns |
| `look.skybox` | An equirectangular sky image (path under the data directory) drawn behind the scene and lit by the hour and the weather; none keeps the plain `sky` colour. |
| `look.smoke` | `true` draws smoke over coal plants, incinerators and industry. |
| `[labels]` | Tool names, by tool id (`coal_plant = "Watermill"`) |
| `disabled_tools` | Tools that do not exist in that era or world |
| `[models]` | Graphic code to replacement model: `.ac` (AC3D), `.obj` (Wavefront, e.g. Kenney kits) or `.compose` (a model composed from parts, below). A path starting with `models/` is shared art under the data folder; any other path is relative to the skin folder |
| any file under the skin folder | Replaces the base file with the same relative path, e.g. `skins/fantasy/graphism/gui/road.png` |

Tool ids: `residential`, `commercial`, `industrial`, `road` (asphalt
street), `highway`, `paved_street`, `dirt_road`, `power_line`,
`coal_plant`, `nuclear_plant`, `fire_station`, `police`, `hospital`,
`school`, `park`, `trees`, `raise`, `lower`, `bulldozer`.

With GlassBox rules, a service tool is named after the unit the rules build
with it (a "Fortified granary" for `coal_plant` in the pre-industrial rules),
and the inspector names every building as the rules do. Rules can hide tools
too (`disabled_tools` in the rules file).

Replacement models keep the footprint of the model they replace, because the
simulation uses the base catalog's sizes. `.obj` models are scaled evenly to
fit that footprint, centred on it, and textured with the `colormap.png` next
to them.

## Composed models

A `.compose` file (TOML) builds a model from parts, so the buildings drawn
with it are not all alike. It lists **slots**; each slot has parts to pick
from (`""` is no part) and optionally colour tints. Every building gets one
combination, picked by its tile, so it keeps its look.

```toml
variants = 16                     # combinations used at most (default 12)

[[slot]]                          # the house, at the back of the plot
parts = ["../kenney/city-suburban/building-type-h.obj", "../kenney/city-suburban/building-type-e.obj"]
size = [0.82, 0.64]               # fitted evenly into this box, in tiles
at = [0.09, 0.0, 0.05]            # then moved by this much (x, y, z)
tints = [[1, 1, 1], [1.0, 0.93, 0.86], [0.9, 0.95, 1.0]]

[[slot]]                          # a tree in the front yard, or nothing
parts = ["", "../kenney/nature/tree_oak.obj"]
size = [0.24, 0.24]
at = [0.05, 0.0, 0.71]
rotations = [0, 90, 180, 270]     # turned about its centre; one is picked

[[slot]]                          # a chimney: a round part stood tall
parts = ["../kenney/factory/piston-round.obj"]
size = [0.3, 0.3]
height = 1.8                      # stretched upright to this, after the fit
at = [0.6, 0.0, 0.2]
```

- **Paths** are relative to the `.compose` file.
- **Fitting:** a part with `size` is scaled evenly into that box. Without
  it, an `.obj` part fills the building's footprint and an `.ac` part keeps
  its own size, like whole models.
- **Front:** buildings face +z (the far end of the plot from the origin),
  where the street usually is.
- **Rotations:** `rotations` lists turns in degrees, one picked per variant
  like a part or a tint; the part turns about the centre of its box (or of
  the footprint) before `at` moves it. A single entry turns every variant
  the same way, which is how the power-line wires are strung north-south.
- **Height:** `height` stretches the part upright to that many tiles after
  the fit, so a square kit part can stand as a tower or a chimney.
- **Variants:** with many combinations, `variants` of them are used,
  spread so they differ in every slot.
- A `.conf` next to the `.compose` file gives its name and size like any
  model's.

![Composed houses in the modern skin](composed.png)

The modern skin draws its little houses (11) and family houses (12) this
way, from `models/composed/`: one of several Kenney houses, tinted, with a
tree and a parked car or neither. Its power lines, trees, parks, harbour,
seaport and water and waste works are compositions too
(`powerline_*.compose`, `tree_*.compose`, `park.compose`, `harbour.compose`,
`seaport.compose`, `pumping_station.compose`, `water_tower.compose`,
`water_treatment.compose`, `dump.compose`, `recycling.compose`,
`incinerator.compose`), and so are its services, power plants and transit
stops (`fire_station.compose`, `hospital.compose`, `nuclear_plant.compose`…).

## Shipped examples

* **modern** (Modern, Kenney city kits): years 1946 on, and every city with
  no date or world (`default = true` in `[applies]`). Houses come from the CC0
  Kenney City Kit Suburban, shops and towers from City Kit Commercial, and
  workshops, factories and the coal plant from City Kit Industrial. Power
  lines are City Kit Roads poles wired tile to tile, bridges stand on its
  pillars, trees and parks come from the Nature Kit, and the harbour and
  seaport are dressed with Watercraft Pack cargo and Factory Kit cranes.
  The services and the nuclear plant keep OpenCity's models. **View >
  Skin > None** brings back OpenCity's own buildings.

* **antiquity-<civ>** (baked, see below): years -1000 to 500, one per culture.
  Towns built from **0 A.D. art**: houses, storehouses, farmsteads, civic
  centres, barracks, fortress, wall towers, fields, mines and five tree species.
  Tools are named after the culture's own buildings (Domus, Oikos, Tegia, Mín Jū…).
* **antiquity**: the same era without 3D art, used when nothing is baked.
  Paved roads (*via strata*) and tracks instead of asphalt.
* **medieval**: years 501 to 1449. Muted palette, no vehicles, no
  electricity; cobbled streets and dirt lanes.
* **renaissance**: years 1450 to 1799. Faded warm palette, no vehicles, no
  nuclear plant, cobbled streets and lanes. With the classic rules the coal
  plant becomes a "Watermill" and power lines become a "Mill race".
* **industrial**: 1800-1945. Grey skies, coal age names, no nuclear plant,
  no highways.
* **fantasy**: worlds `Eldoria`, `*fantasy*`, `ofm:*`. Vivid palette, magic
  vocabulary ("Mage tower", "Ley line", "Dragon forge"), no vehicles, no highways.

Except for antiquity, these reuse OpenCity's modern-looking models, so they
are a mood change, not a full reskin.

## 0 A.D. art (antiquity skins)

The 0 A.D. models are CC BY-SA 3.0, so they are not committed or packaged.
They are converted from the **original 0 A.D. repository**
(`gitea.wildfiregames.com/0ad/0ad`, pinned commit in
[`assets/zeroad/antiquity.toml`](../assets/zeroad/antiquity.toml)) by citysim
itself. No Docker, git or Python is needed:

```sh
opencity-cli bake-0ad          # any install; --from <0 A.D. checkout> works offline
./download-0ad-art.sh          # release package: .sh on Linux, .command on macOS
download-0ad-art.cmd           # release package on Windows
scripts/bake-0ad.sh            # source checkout (cargo run)
```

The game also runs it in the background on first launch, then the cultures
appear in **File > New city…**. `--bake-0ad` forces a new run and
`--no-auto-bake` disables it. The attempt is recorded in
`assets/skins/.bake-0ad-attempted`.

The bake ([`crates/opencity-zeroad`](../crates/opencity-zeroad)) runs in four steps:

1. **Read the data.** It reads simulation templates (with inheritance and
   mixins), civilisation names, actors and variants. The XML files come one
   by one from the repository's `raw` endpoint.
2. **List the needs.** It collects the meshes and textures that the mapped
   structures, trees and props use.
3. **Download.** It fetches only those, about 380 Git LFS files or 115 MB,
   from the `media` endpoint, eight at a time. Downloads are cached in
   `~/.cache/citysim/0ad` (`%LOCALAPPDATA%\citysim\0ad` on Windows), so a
   second run is fast.
4. **Convert.**
   * COLLADA meshes become Y-up AC3D with props attached at their prop
     points, scaled to the footprint of the OpenCity model they replace.
   * PNG/DDS textures get the culture's player colour.
   * Per culture it writes `models.toml`, `skin.toml` (tools named after the
     culture's own buildings) and `ATTRIBUTION.txt` into
     `assets/skins/antiquity-<civ>/`, and lists them in `assets/skins/generated.toml`.

Without the baked art, the plain antiquity skin still applies its colours,
names and rules.

## Assessment: what a full reskin needs

**The bottleneck is 3D content, not code.** One complete era or world set
needs about 60 models:

* 10 levels for each of residential, commercial and industrial, so 30 models.
* 15 road pieces.
* Power plants, the four services, a park and 3 to 5 trees.

The rest of the pipeline (selection, overrides, colour, names, rules) is in
place. Ways to get there, roughly by effort:

1. **More 0 A.D. cultures.** Adding a culture is one line in
   `assets/zeroad/antiquity.toml`. Medieval and
   later eras need other sources.
2. **CC0 kits through `.obj`.** The OBJ loader is in place, and the modern
   skin maps three Kenney city kits onto the graphic codes this way. Kenney's
   town, castle and fantasy kits would give a medieval or fantasy look the
   same way.
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
