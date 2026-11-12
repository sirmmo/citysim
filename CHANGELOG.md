# Changelog

Release notes in full are on the [releases page](https://github.com/sirmmo/citysim/releases).

## 0.2.52: Sounder GlassBox rules
- **Homes need jobs and shops.** Workers and shoppers are `outlets` of a home: a house whose people find no work within reach empties, one with work but no shop gets by and never grows. People stop moving in while those already there look for work.
- **Satisfaction weighs needs by size and importance** (`weight` on a resource: power counts for more than parking), and a need met by less than half starves a building. A shop without customers or a workshop without workers now closes instead of idling at 80 %.
- **Buildings level up only into what can be supplied.** A house does not grow into one that needs water where the mains have none to spare, or rubbish collection no truck can bring. A building starved for long shrinks a level. The inspector says what a building is waiting for, and how many of a home's workers and shoppers found takers.
- **Shops open where customers can reach them:** commercial plots develop only on a street network with shoppers or goods left over.
- **Taxes bite:** a point or two above the default is shrugged off, the top rates empty the town (`steep` on a tax).
- **Running the city costs money:** an administration line in the budget, so much per inhabitant and more per head as the city grows (`[administration]`).
- **Walkable centres** no longer hurt the shops they are meant to help: fewer cars, and shops and workplaces that need half the parking (stances take `needs`).
- **Traffic pollution and noise follow the traffic:** agent rules emit per unit carried, so a busy route is dirtier than a quiet one, and the air quality and noise maps stay on their scales.
- GlassBox scripts can test a zone's demand (`test global demand.residential greater 0`); zone rules leave resting plots alone.
- `opencity-cli simulate --demo --game-rules …` lays out the demo town after the rules are attached, so it gets its waterworks and transit as in the game.
- The pre-industrial rules get the same: outlets for homes, a town hall to pay, steeper taxes.
- Saved cities keep working. Expect towns to change in the first months: idle shops and workshops close, houses that grew past their water supply shrink back.

## 0.2.51: Sounds, sky, smoke and map symbols
- **Interface sounds** (Kenney Interface Sounds): a click when a tool is picked, a low note when something is refused or fails, a chime when a save, a load or a discovery succeeds, a bong when a disaster strikes.
- **Research is announced:** "Discovered: Railways" when a project finishes.
- **A sky behind the city** in the modern skin: a Kenney panorama with clouds at the horizon, lit by the hour and the weather. Skins choose theirs with `look.skybox`.
- **Smoke** rises from coal plants, incinerators and working industry in the modern and industrial skins (`look.smoke`).
- **The region map shows the towns as map symbols** (Kenney Cartography Pack): a tent for a free slot, then a house, a village, a church and a castle as the town grows.

## 0.2.50: Kenney services
- **The modern skin's services and power plants are Kenney scenes too:** a red fire station with its engines on the forecourt, a blue police station with patrol cars, a hospital of two wings with ambulances at the door, a school with a fenced yard and trees, a coal plant with its stacks, tank and coal heaps, a nuclear plant with two cooling towers, reactor and turbine halls; bus, tram and metro stops get shelters, platforms, signs and lamps. With this every building of the modern skin is CC0 Kenney art, apart from the airport.

## 0.2.49: Factory Kit utilities
- **The modern skin's water and waste works are Kenney Factory Kit scenes:** the pumping station is a pump house with its pipe run, valve and tank; the water tower a tank on legs; the treatment plant three basins, a pipe run and a control house; the landfill heaps of cargo and crates with a magnet crane; the recycling centre a sorting machine fed by a conveyor, with a hopper, crates and a robot arm; the incinerator a furnace with its feed hopper and chimney. Each plot picks its own parts, turns and tints.
- Composed models (`.compose`) take `height`: the part is stretched upright to that many tiles after the fit, so a square part can stand as a tower or a chimney.

## 0.2.48: Kenney infrastructure
- **The modern skin dresses the networks too.** Power lines are Kenney City Kit Roads poles, one per tile, wired to the next pole east and south, so a line reads as one run of wire; bridges stand on the kit's pillars, two per tile, stretched from the river bed to the deck. Trees are Nature Kit pines and broad-leaved trees in several shapes, shades and turns; the park is a stone circle with a tree, a bush, flowers and some odd thing, different on every plot; the houses' yard trees come from the same kit. The seaport is a quay with two Factory Kit cranes over the water, stacks of Watercraft Pack containers, cargo, a silo and a buoy; the harbour gets a wooden landing, cargo and a buoy.
- Composed models (`.compose`) take `rotations`: turns in degrees picked per variant like parts and tints, or one turn for every variant.
- `.obj` models coloured by materials (`.mtl` files, as the Nature Kit is) keep their colours; kits with a colour map work as before.
- Skins can name a bridge pier model (`look.bridge_pier`).
- Released as 0.2.48: the 0.2.47 tag never built, GitHub's Ubuntu runners could not reach their package mirror; the release workflow now retries and falls back to the main archive.

## 0.2.46: Regions in the browser
- **Regions work in the browser build.** File > New region… founds a region whose towns live in the browser's storage; Open region… lists the regions saved there; Join region server… reaches an `opencity-cli region-server` over HTTP, and `play/?join=http://host:8077&player=ada&slot=0,0` joins one at once. Desktop and browser players share the same region. The region server now answers the browser's preflight requests, so pages on any host may call it.
- Joining a server no longer freezes the game while it waits (the answer is taken in on a later frame, on every platform); fresh summaries from the server re-agree the month's trade when they arrive.
- **The Android APK is no longer part of the releases:** phones play the browser build. `scripts/android.sh` still builds it by hand.

## 0.2.45: Interface size on phones
- **The interface is drawn at the screen's pixel density.** Phones pack three pixels in a point and the game ignored that, so on them (and on Retina Macs) the menu, panels and toolbar were a third of their size. Both interfaces now scale with the screen: the classic one (Android) draws its toolbar, status bar and menu that way and lays the toolbar out in more columns on short screens; the desktop/browser one tells egui the ratio and hides the City panel on narrow screens to leave room for the map. **View > Size of the interface** picks Normal, Large or Huge on top of that; `--ui-scale 1.5` (or `?ui-scale=1.5` in the browser) does the same.

## 0.2.44: Airport and seaport
- **Seaport** (on the shore, research *Ports*) and **airport** (research *Aviation*): gates to the rest of the world under GlassBox rules. Each tick they bring in up to their `trade` capacity of what the city lacks (the seaport goods and fuel, the airport tourists for the shops and some air freight) and take out what has nowhere to go, at the resource's price, booked as trade purchases and sales; the inspector shows what went through. Under classic rules they are landmarks with upkeep.
- An airliner takes off, circles and lands at every airport. GeoJSON imports build them from `aeroway=aerodrome` and port land use; exports tag them back.
- Sidebar: the *Waterways* group is now *Ports* (harbour, canal, seaport, airport).
- Website: the main page runs the game in place (press play on the screenshot), besides the full-screen link.

## 0.2.43: Rivers and bridges
- **Rivers** on generated maps: the new city dialog (and `--rivers` in the CLI, `rivers` in settings and the MCP `new_city` tool) carves rivers from the high ground down to the sea, a lake or the edge of the map, with banks that slope a step per tile. New maps get one by default; scenarios and the demo town are unchanged.
- **Bridges:** a road or track over water is a bridge, at three times the price of the tile, drawn as a deck on piers above the water; cars and trains cross on it and boats pass underneath. Water and its banks can always be crossed, whatever their slope.
- Buildings can no longer be placed on flat water (lake beds used to count as flat ground).

## 0.2.42: In the browser
- **The game runs in the browser** at <http://ingmmo.com/citysim/play/>, built to WebAssembly from the same code. Saves live in the browser's local storage (one per name), exports download, and the page's query string stands for the command line (`?demo&game-rules=glassbox`). Region servers cannot be joined from there yet.
- **Web files** ship with each release (`citysim-vX.Y.Z-web.zip`) to host the browser game yourself; `scripts/web.sh` builds them, and the website's pages workflow rebuilds them at every release.
- Developers: `cargo run -p xtask -- web-dist <dir>` assembles the wasm, the page and the data files packed into one gzip download (about 22 MB of assets); CI also lints the `wasm32-unknown-unknown` target.

## 0.2.41: Transit across borders
- **Commuting has a limit** in regions: people and lorries go no further from a border crossing than they would inside the town, traffic included. Before, every place on the crossing's streets could trade through it.
- **Transit carries them further:** a tram street, a street with bus stops, or a metro tunnel taken to the edge links the places near its stops to the crossing, at the line's speed.
- **Metro tunnels cross borders:** a tunnel at the edge is a crossing of its own. It joins the neighbour's tunnel, and their end is bored for free.

## 0.2.40: Rules editor: layers and new things
- **Map layers in the graph:** a layer shows the buildings that raise it and those that need it within bounds. How it spreads, how fast it fades and its full-colour value can be edited there.
- **A building's map layers:** what it raises (amount, radius) and what it needs (at least, at most) are edited on its graph, with buttons to add more.
- **New buildings, resources and map layers** from the graph: **+ Building**, **+ Resource**, **+ Map layer**, or **Duplicate…** on a building to start from a copy. The rules file keeps its comments and layout.
- Fix: in a building's graph, the controls below no longer cover the lower needs when it needs more than it makes.

## 0.2.39: Composed models
- **Models composed from parts.** A `.compose` file builds a model from slots of parts to pick from, with colour tints. Every building gets its own combination, picked by its tile, so streets stop repeating one house (docs/SKINS.md#composed-models).
- The modern skin's little houses and family houses now vary from plot to plot: one of several Kenney houses, tinted, with a tree or a parked car in the yard.

## 0.2.38: Metro
- **Metro** (modern GlassBox rules, after the new **Metro** research project). The **Metro line** tool bores tunnels underground, under streets, buildings or open land. **Metro stations** go over a tunnel or beside it.
- Stations on one run of tunnels link the places around them at three times street speed, and traffic never slows them.
- The underground view (**B**) shows the tunnels in orange beside the blue water pipes, and also opens with the metro tools. There, the bulldozer digs up tunnels as well as pipes.
- The inspector says when a metro tunnel runs under a tile.
- The demo town has a metro line with three stations.
- GeoJSON imports and exports `railway=subway` (tunnels) and `railway=subway_entrance`.

## 0.2.37: Traffic
- **Traffic slows trips** (GlassBox rules). Trips by road load the streets they cross, and busy areas make road trips take up to three times longer. A jammed town reaches fewer jobs and shops.
- Trams, trains and boats are not slowed and highways bypass traffic; buses feel half of it. Public transport now relieves the streets as well as reaching farther.
- Inspecting a road shows how full it is and how much slower trips are there. Cars route around busy roads.
- Road traffic now comes from the GlassBox trips instead of the classic estimate.

## 0.2.36: Buses and trams
(0.2.35 was tagged but never published: GitHub retired the macOS runners it built on. Releases now build on macOS 15.)

- **Trams:** the **Tram line** tool lays tracks in streets, and cars still use them. **Tram stops** go beside the tracks. People near two stops on one line ride between them at twice street speed.
- **Buses:** **Bus stops** go beside any street. People near two stops on one street network ride at 1.5 times street speed.
- A ride costs the walk and boarding, so it pays on longer trips: people reach jobs and shops farther away than they could drive to. Trams and buses run between the stops on the map.
- Modern GlassBox rules, unlocked by the new **Public transport** research project. They are not available before 1800.
- **Sidebar:** Rail and Waterways are now separate groups. Rail has the railway, stations, tram lines and tram stops; Waterways has harbours and canals. Bus stops are under Roads.
- The underground view moves from **U** (which raises terrain) to **B**.
- The demo town has a tram line with three stops and four bus stops.
- Rules: a vehicle with `line = "bus"` or `"tram"` runs between stops. New `tram` road kind, `bus` vehicle model.
- GeoJSON imports and exports `highway=bus_stop`, `railway=tram_stop` and `railway=tram`.

## 0.2.34: Pipes underground
- **Water pipes run underground.** Lay them under streets, buildings or open land. Streets no longer carry water by themselves, so lay mains under them.
- **Underground view** (U, or View > Underground, and whenever a Water tool is in hand): the city fades and the pipes show through it. In this view the bulldozer digs up pipes and leaves the surface alone. Outside it, the bulldozer never touches pipes.
- The inspector says when a water main runs under a tile.
- Saves from 0.2.33 move their surface pipes underground on loading. The surface pipe models are gone.
- GeoJSON exports pipes as `man_made=pipeline` with `location=underground`.
- The demo town lays mains under all its streets.

## 0.2.33: Water and waste
- **Water:** pumping stations, water towers and water treatment plants feed the water mains, which run under the streets and through buildings. **Water pipes** cross open land. Level-2 buildings need water; medium and high density buildings also need their sewage treated.
- **Waste:** landfills, recycling centres and incinerators send rubbish trucks to collect from the buildings around them; level-3 buildings need collection.
  - The landfill smells and lowers land value.
  - Recycling sends goods back to the shops.
  - The incinerator makes some power, with smoke.
  - Before 1800 the dump is a midden, served by handcarts.
- **Sidebar submenus:** each family of tools is one row. Its icon picks the last tool used, and its name opens a submenu beside the sidebar: Roads, Rail and water, Power, Water, Waste, Services and Land.
- The demo town has a pumping station, a water tower, a treatment plant, a landfill and a water main.
- GeoJSON imports and exports water towers, water works, wastewater plants, pipelines, landfills, recycling centres and waste incinerators.
- The new models and icons are generated by `scripts/gen-utilities.py` and are CC0.

## 0.2.32: Rules editor
- **Rules editor** (View > Rules editor…): edit a rules set inside the game, including its rules file, scripts, tech tree and policies.
  - **Text:** syntax colours, with every defined id in the colour of its kind. Hints come from what the rules already define, fitted to the key being written (resources for `needs`, networks for `network`, fields for a section, keywords in scripts). Every change is checked, and problems are reported with their file and line.
  - **Graph:** a building with what it needs and makes, a resource with who makes and needs it, or a network with what it carries. A building's amounts can be changed, added or removed right on the graph; the rules file is rewritten with its comments kept.
  - **Try on this city** runs the city on the edited rules at once; **Save** writes the files.

## 0.2.31: Packs to download
- **Ready-made packs:** each release now includes Rail Baron and Bologna 1600 as games of their own, for Linux, Windows and both Macs. Download one from the website (Download > Packs) or the releases page, unpack it and run `opencity`: it opens straight into the pack.

## 0.2.30: The network on its networks
- **Resource network view, redrawn:** flows no longer jump from building to building in arcs. Each link follows the network that carries it, summed per stretch:
  - people, cars, lorries and buses along the streets;
  - trains on the track and boats on the water;
  - power along lines, streets and the buildings it runs through.
- Each resource runs in its own lane on the right of the way it travels, as thick as the flow. A border gate is just one more node feeding its lane at the edge.
- Hovering a stretch shows the resource, the way and the amount per tick.

## 0.2.29: Border gates
- **Crossings carry the trade:** in a region, each crossing joined on both sides is a GlassBox node. Commuters, shoppers, goods and fuel travel in from the edge to the places that need them and out from the places with a surplus. Power runs in and out along lines and streets, and trains carry trade to the stations on a railway crossing's line. You can watch the vehicles come and go at the border.
- A crossing your network doesn't reach carries nothing, and imports are paid only for what gets through.
- The Region window shows what went through each crossing last tick, against what was agreed.
- A town founded in a region slot takes the name the region file gives it.

## 0.2.28: Border crossings
- **Border crossings join up:** in a region, a road, railway or power line built to the edge of the map runs on into the neighbour. It appears on the neighbour's map at the matching place on the shared edge, free of charge. In a region played alone, it appears when that town is next opened or founded. Online, it appears in the other player's game at their next monthly update.
- **Border gates:** each crossing gets a gate at the edge of the map: green when the neighbour has the crossing too, amber while it waits, grey at the edge of the region.
- **Lone maps:** a road built to the edge of a map outside a region brings a message: there is no region server, so no town lies beyond.
- A neighbour's crossing never replaces what you built on that tile, and one you bulldoze stays gone.

## 0.2.27: Packs
- **Packs:** a scenario sealed with its skin and rules, as a game of its own. Use `--pack ID`, or put a `pack.toml` in the data folder. A Game menu (restart, save, quit) replaces File. Packs have no new cities, regions, imports, skin changes or cheats. The game is saved on quit and carries on at the next launch.
- **Packs shipped:** Rail Baron and Bologna 1600.
- **`scripts/make-pack.sh`:** turns any release download into a single-pack game.
- **Mornings:** each month now starts at 8 in the morning, so a new city opens in daylight instead of at midnight.

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
