# Changelog

Release notes in full are on the [releases page](https://github.com/sirmmo/citysim/releases).

## 0.2.26: 0 A.D. buildings fixed
- **Fix:** the ancient cultures (Romans, Athenians, Gauls, Ptolemies, Han, Mauryas) show their 0 A.D. buildings again. Since 0.2.15 they fell back to OpenCity's original models.
- **Fix:** the ancient cultures no longer offer highways, railways or stations, even when their art was baked before rail existed.

## 0.2.25: Day and night, seasons and weather
- **Day and night:** the light turns once a game month, with a warm dawn and dusk and a blue night.
- **Seasons:** the ground colours follow the seasons, white in winter.
- **Weather:** weekly clear, cloudy, rain, storm or snow, with falling rain and snow.
- **Disasters:** the odds follow season and weather (storms and summer fuel fires, rain floods, winter sickness).
- **View menu:** a switch for day and night and weather. The top bar shows the hour, season and weather.
- **Fix:** includes the region server, whose 0.2.24 desktop build failed and was never published.

## 0.2.24: Region server and shared culture (tagged, not published; in 0.2.25)
- **`opencity-cli region-server`** hosts a region for several players.
- **Joining:** File > Join region server… (or `--join URL --player NAME --slot x,y`) claims a town. Each month the games exchange summaries, so neighbours trade and ideas cross the borders.
- **Shared pools:** a resource can be marked `shared`, and ideas are, in both rule sets. Every town takes in a tenth of what the rest of the region makes, alone or online.
- **Region window:** shows who plays which town.

## 0.2.23: Scenarios and goals
- **File > New scenario…** and `--scenario`: three scenarios to start with: First town, Rail baron, and Bologna 1600 from OpenHistoricalMap.
- **🎯 Goals window:** progress, deadlines, and won or lost. Goals are checked monthly and kept in saves.
- **Scenario files** (`assets/scenarios/`) set the map or GeoJSON, rules, skin, money and goals.
- **MCP:** `new_city { scenario }` and a `goals` tool.

## 0.2.22: Budget
- **💰 Budget window:** last month and this month by line (taxes per tax, upkeep per service, policies, trade, loans, the family).
- **Service funding:** 0 to 150% per kind of service, scaling both cost and coverage.
- **Loans:** 6% a year over five years, three at most.
- **MCP:** a `budget` tool.

## 0.2.21: Cousin Vinnie
- **Cheat box:** **Ctrl+Shift+C**, as in SimCity 3000.
- **`CallCousinVinnie`** wires $1,000,000, and the family moves in. They collect protection money monthly, police coverage falls, and blocks burn now and then, until enough police stations drive them out. The family stays in saves.
- **MCP:** a `cheat` tool.

## 0.2.20: Disasters
- **Disasters in the rules (`[[disaster]]`):** how often, radius, effect (destroy, damage, sicken), protecting layer, near water.
- **Shipped disasters:** the modern rules have fire, flood and epidemic. Before 1800, great fire, flood and plague.
- **Coverage protects:** fire coverage cuts the risk from fire, health coverage the risk from sickness. Responders such as fire trucks drive to the scene.
- **Disasters menu:** switches chance disasters off and sets one off by hand. The MCP server has a `disaster` tool.

## 0.2.19: Waterways and harbours
- **Harbours:** a harbour on any lake, river, coast or canal links the places near harbours on the same water. Boats sail between harbours: Kenney cargo ships and tugs today, rowing and sailing boats at a Quay before 1800.
- **Canal tool:** digs empty land below the water line.
- **Rules:** vehicles with `ways = ["water"]` are boats. GeoJSON imports and exports harbours.

## 0.2.18: Regions
- **Regions:** a grid of neighbouring cities, played one at a time (File > New region…, Open region…, `--region`).
- **The 🗺 Region window:** found towns, switch between them (the open one is saved), and see border crossings and this month's trade.
- **Trade:** each month the open town buys what it lacks and sells what it has left over to neighbours with crossings on both sides of their shared edge. Prices come from the rules (`price` on a resource), and commuters count.

## 0.2.17: Service vehicles
- **Coverage by road:** fire, police and health coverage now spreads along the streets (`by_road` emissions). A station covers what its vehicles can drive to.
- **Dispatch:** fire stations send Kenney fire trucks, police stations patrol cars and hospitals ambulances to the buildings they cover. Before 1800 the watch and guards go on foot.
- **`dispatch`** in the rules: model, meshes and how often.

## 0.2.16: Railways
- **Track and stations:** a Railway tool lays track, and a Railway station joins it to the streets. Trains (Kenney diesel and electric trains) run between stations on one line.
- **Linked towns:** under GlassBox rules, places near stations on one line trade as if next door, even in towns no road joins.
- **Level crossings:** cars never use track, but cross it where a road meets it on both sides.
- **Research and eras:** the modern rules get a station unit and a Railways research project. Rail is off before 1800.
- **GeoJSON:** railways and stations from OpenStreetMap and OpenHistoricalMap data (`railway=rail`, `railway=station`) are imported and exported.

## 0.2.15: Kenney city kits
- **Kenney city kits:** modern cities (1946 on, and new cities with no date) are built from the CC0 Kenney City Kit Suburban, Commercial and Industrial: houses, shops, towers, workshops, factories and the coal plant.
- **`.obj` models in skins:** skins can use `.obj` models, scaled to fit the footprint they replace, and shared art under `models/`.
- **Default skins:** `[applies] default = true` picks a skin for cities with no date or world.
- **View > Skin > None** shows OpenCity's original buildings.

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
