# Changelog

Release notes in full are on the [releases page](https://github.com/sirmmo/citysim/releases).

## 0.2.14: A local MCP server
- **`opencity-cli mcp`** serves the Model Context Protocol over stdio, so AI assistants (Claude Code, Claude Desktop…) can build, simulate and inspect a city.
- **Tools:** new, load and save a city; summary; simulate; list and apply tools; inspect a tile; turn a building; research; taxes and policies; render a PNG; export GeoJSON.
- **`new_city` with `flat: true`** makes an empty flat map.
- Setup is in [docs/MCP.md](docs/MCP.md).

## 0.2.13: Taxes and policies
- **Policy files:** taxes and policies live in their own files (`assets/policies/<id>.toml`), named by `policy_set`.
- **Taxes (`[[tax]]`):** each tax scales what some zones pay. Above its default rate it cuts their demand and satisfaction.
- **Policies (`[[policy]]`):** each policy is a set of stances with bonuses, demand, satisfaction and running costs. A stance can wait for a research project.
- **📜 Policies window:** a slider per tax and a choice of stance per policy. Saves keep both.

## 0.2.12: Kenney vehicles
- **OBJ models:** the game reads `.obj` models, and vehicle definitions take `meshes`, picking one per trip.
- **Kenney kits:** the CC0 car, train and watercraft kits ship under `assets/models/kenney/`.
- **Modern towns** show sedans, SUVs, vans, taxis, sports cars and delivery trucks.

## 0.2.11: Buildings that face the street
- Plants and services turn to face the road beside them when placed.
- **⟳ Turn** in the inspector, or **O**, turns one by hand. Long buildings swap their footprint.

## 0.2.10: Tech trees
- **Tech tree files:** research projects move to their own files (`assets/techs/<id>.toml`), named by `tech_tree`. The format follows the Stronghold experiment.
- **New fields:** branches (`category`), tiers, rival projects (`excludes`) and production bonuses.
- **Rival choices:** a service economy or industrial policy, a free market or a guild charter.
- **New projects:** Computing, Crop rotation and Printing.

## 0.2.9: Ideas and research
- **Pooled resources:** a resource can live in a city-wide pool that isn't on the map.
- **Ideas and research:** cafés and taverns make ideas, and schools turn them into research points.
- **Projects (`[[tech]]`)** unlock buildings and tools. The 🔬 Research window picks one.

## 0.2.8: Vehicles by the rules
- **Vehicles in the rules:** each road network lists its vehicles: their look, the road kinds they may use, their top speed and what they carry.
- **Towns of the past:** people walk and carts carry trade. No cars before 1800, and no carts on highways.

## 0.2.7: Roads of every age
- **Four road kinds:** street, highway, paved street and dirt road, each with its own speed, capacity, width and surface.
- **Pre-industrial rules:** huts to insulae, taverns to exchanges, fields, granaries, wells, walkers and carts.

## 0.2.6: Resource network
- **Network view:** the live GlassBox network on the map (V), with filters, busiest links and shortages.
- **Resource kinds:** for example petrol and electric cars, with fuel, power and pollution trails.

_0.2.5 was tagged but never published._

## 0.2.4: Steady growth and graphs
- **Damped growth:** grace, patience and saturation stop buildings flickering in and out.
- **GlassBox graphs (G):** every series the engine samples, with analysis and CSV export.

## 0.2.3 and 0.2.2: 0 A.D. art
- **0 A.D. art:** downloaded and converted on first launch, with no Docker, git or Python, for the six ancient cultures.

## 0.2.1
- **Fix:** a crash when dense GlassBox buildings grew over their neighbours.

## 0.2.0: GlassBox
- **GlassBox rules engine:** data-driven simulation with the rule language and Extended GlassBox metrics.
- **Eras and cultures:** 0 A.D. art for the ancient cultures.
- **Faster drawing:** GPU instancing and smooth road ribbons.

## 0.1.0
- **First release:** a Rust port of OpenCity (SVN trunk r480).
