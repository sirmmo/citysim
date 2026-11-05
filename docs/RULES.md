# Game rules: the GlassBox engine

New cities can run on the **classic OpenCity rules** or on a **rules set**:
a data file describing everything the city simulates, in the spirit of the
GlassBox engine of SimCity (2013). Pick it in **File > New city… > Rules**,
or with `--game-rules glassbox` (game and CLI). The shipped set is
[`assets/rules/glassbox.toml`](../assets/rules/glassbox.toml); copy it, edit it, and list
your file in `assets/rules/index.toml`.

## The model

Buildings are **units**. Each one:

* **produces** resources every tick, scaled by its **satisfaction**;
* **needs** resources every tick; what arrives decides its satisfaction;
* **sends** its stock of some resources to units that need them;
* **emits** into map layers (pollution, land value, service coverage);
* **requires** layer values at its tile (each unmet condition halves its satisfaction).

**Resources** say how they travel:

| `transport` | Moves | Examples |
|---|---|---|
| `agents` | Along a network, from the nearest senders to receivers, within `max_trip` tiles. A sample of trips is shown as vehicles. | workers, shoppers, goods |
| `grid` | Instantly within each connected part of a network; shared proportionally when short. | power |
| `city` | Into or out of the treasury. | taxes, upkeep |
| `local` | Stays in the unit, used for statistics. | residents |
| `pool` | Into one city-wide stock that keeps from tick to tick. Needs draw on it and research spends it. | ideas, research points |

**Kinds** (subtypes) split an `agents` or `grid` resource into variants that
fill the same slots but weigh on different networks. `car` with kinds
`gasoline` and `electric` becomes three resources: `car`, `car.gasoline` and
`car.electric`.

* **Slots.** A need for `car` is a slot that either kind fills, from the
  nearest sender with any kind in stock. A need for `car.electric` takes only
  electric cars, and is served before slots.
* **Production.** A unit that produces `car` makes its kinds by their
  `share`. A unit can also produce a kind directly.
* **Secondary impacts.** `uses` lists resources a kind needs per unit moved.
  The sender asks for them the next tick through their own networks, and they
  count towards its satisfaction like its other needs. `trail` adds to map
  layers per unit moved and per tile travelled.
* **Balances.** The parent's balances (produced, delivered, unmet,
  undelivered) are totals over its kinds. Zone demand terms on `car` see the
  whole family.
* **Kinds move like their parent:** same transport and network; `max_trip`
  can differ.

```toml
[[resource]]
id = "car"
name = "Cars"
transport = "agents"
network = "road"

  [[resource.subtype]]
  id = "gasoline"
  name = "Petrol cars"
  share = 0.75
  uses = { fuel = 0.25 }                # fuel delivered by road
  trail = { "AirQuality.pm10" = 0.1 }   # air pollution along the route

  [[resource.subtype]]
  id = "electric"
  name = "Electric cars"
  share = 0.25
  uses = { power = 0.3 }                # charging, from the power grid
```

In `.gbx` scripts, `car.{type}` repeats the line once per kind (the Extended
GlassBox `{type}` placeholder). A rule's `condition resource=car` follows every
kind of car.

**Networks** are made of structures (`road`, `power_line`). With
`through_buildings`, adjacent buildings conduct too, as with OpenCity's power.

**Zones** (residential, commercial, industrial) have up to three
**densities**. Each density lists the units that grow on it, level by level.
Zoning cost is set per density. A zone type's **demand** is computed from the
last tick's balances:

* `unmet`: needed but not delivered, e.g. open jobs pull residents in;
* `undelivered`: produced but with nowhere to go, e.g. unemployed workers pull industry in.

When demand is positive, empty plots near roads develop into the density's
first unit. Units with high satisfaction grow to the next unit; unhappy ones
shrink, and empty their plot at the first level (`[growth]` sets the thresholds).

Growth is damped so that buildings do not flicker between states. A building
also never levels up into a unit whose layer `requires` already fail on its
tile, since it would only shrink back:

| `[growth]` key | Default | Effect |
|---|---|---|
| `grace` | 40 | ticks a building stays as built (or as levelled) before it may change again |
| `patience` | 15 | ticks in a row satisfaction must stay past `level_up`/`level_down` before acting |
| `saturation` | 0.1 | a zone stops developing new plots while this share of its settled buildings fail |

An abandoned plot rests for `grace` ticks before it develops again, twice as
long after each further failure there, until a building on it settles.
Agents (workers, shoppers, goods) reach a building from roads up to
`road_access` tiles away, the same distance that lets a plot develop, and
established buildings are served first, so scarce resources do not hop
between buildings at random every tick.

**Services** map a tool to a unit: power plants produce `grid` power, and
fire, police, hospital and school use workers, power and upkeep to emit
coverage layers that bigger buildings require.

## The shipped rules

Two sets ship: **GlassBox** (`glassbox.toml`, modern) and **Pre-industrial
GlassBox** (`pre-industrial.toml`). A rules file can say which years it
suits (`years = [-9999, 1799]`): a new city in an age outside them gets the
first set that fits. It can also hide tools (`disabled_tools`).

### GlassBox (modern)

```
houses ──workers──▶ industry, shops, services        (by road)
houses ──shoppers─▶ shops                            (by road)
houses ──cars─────▶ parking at shops and industry    (by road; petrol or electric)
industry ──fuel───▶ homes of petrol cars             (by road)
industry ──goods──▶ shops                            (by road)
power plants ──power──▶ everything connected         (grid: power lines, roads, buildings)
zones ──taxes──▶ treasury ──upkeep──▶ services and plants
industry, plants ──▶ pollution ──✕ houses
parks, shops ──▶ land value ──▶ bigger homes
fire, police, hospital, school ──▶ coverage ──▶ medium and high density levels
```

Residential demand comes from open jobs, commercial demand from shoppers and
goods with nowhere to go, industrial demand from unemployed workers and shops
short of goods.

### Pre-industrial

```
houses ──workers──▶ fields, crafts, shops, services  (on foot)
houses ──customers─▶ stalls, shops, taverns, markets  (on foot)
fields, farms, granary ──food──▶ houses, taverns, markets
farms ──fodder──▶ horse carts, inns, stables
crafts ──goods──▶ shops, markets
farms, crafts, stables ──carts──▶ markets and workshops (handcarts or horse carts)
wells ──▶ water ──▶ family houses and up
watch, guard, bathhouse, academy ──▶ coverage ──▶ bigger houses and halls
```

No power and no cars. The zones' densities are the town's grades:

| Zone | Low | Medium | High |
|---|---|---|---|
| Residential | Hut, Cottage, Family house | Town house, Merchant's house, Row houses | Tenement, Courtyard house, Insula block |
| Commercial | Market stall, Bakery, Shop & store | Covered market, Tavern, Coaching inn | Caravanserai, Market square, Merchants' exchange |
| Fields & crafts | Grain field, Farm, Farmstead | Quarry, Mine, Stone works | Foundry, Stables, Craft hall |

Services: Fortified granary (the coal-plant tool), Watchtower, Guard house,
Bathhouse, Academy, and the Public well (the park tool).

### Roads

The road network has four kinds of road (`road`, `highway`, `paved_street`,
`dirt_road`). They connect to each other. Agent trips are measured in street
tiles, so slower roads shorten their reach: a dirt road (speed 0.5) halves
it. Units connect through the nearest road they can open onto, which is
never a highway. Between two units within 4 tiles of a highway, a trip costs
4 tiles to get on and off, then a 2.5th of its length.

### Vehicles

A network that lists `[[network.vehicle]]` entries is a vehicle network. The
rules then decide what moves along it and where:

```toml
[[network.vehicle]]
id = "horse_cart"
name = "Horse cart"
model = "horse_cart"     # car, pickup, sports_car, walker, handcart, horse_cart
ways = ["street", "paved_street", "dirt_road"]   # road kinds it may use; none = all
speed = 1.0              # top speed, in street speeds; none = no limit
carries = ["cart.horse", "food", "goods", "fodder"]   # none = any resource on the network
share = 1                # how often it is picked among those fit for a trip
direct = true            # quickest route rather than avoiding traffic
meshes = ["kenney/car-kit/delivery", "kenney/car-kit/truck"]   # .obj models, one per trip (optional)
```

`meshes` draws the vehicle with `.obj` models from `<data>/models/` instead
of `model`'s look; `model` still decides how it moves and which skins hide
it. The game ships the CC0 Kenney kits under `models/kenney/`: `car-kit`
(sedan, suv, van, taxi, police, ambulance, firetruck, delivery, truck,
garbage-truck, tractor…), `train-kit` (locomotives, diesel and electric
trains, trams, carriages) and `watercraft` (boats and ships). The modern
rules use the car kit for cars, taxis, sports cars and delivery trucks.

- **What you see:** each trip shown on the map becomes one of the vehicles
  that carry its resource, picked by `share`. A vehicle that carries a
  resource also carries that resource's kinds.
- **Ways:** the vehicle routes only over the road kinds in `ways`.
- **Top speed:** a vehicle is never faster than its `speed`. Trips of the
  resources it carries are measured at that speed too, using the fastest of
  their vehicles.
- **Highways:** the highway shortcut applies only if one of a resource's
  vehicles may use highways.
- **Unlisted resources:** a resource that no vehicle carries is still
  delivered, but it is not drawn.
- **Defaults:** a road network without vehicles carries cars, pickups and
  sports cars on every road, as before.

The modern rules run cars (workers, shoppers, car trips), sports cars
(shoppers) and delivery trucks (goods and fuel). The pre-industrial rules
have no cars:

| Vehicle | Carries | Ways | Speed |
|---|---|---|---|
| People on foot | workers, customers | lanes, cobbles, streets | 0.8 |
| Handcart | handcarts, food, goods | lanes, cobbles, streets | 0.6 |
| Horse cart | horse carts, food, goods, fodder | lanes, cobbles, streets | 1.0 |

None of them may use a highway, even on a map imported with motorways.

Inspecting a road lists the vehicles allowed on it. Skins that set
`vehicles = false` hide cars and trucks only. People on foot and carts are
always drawn.

### Disasters

```toml
[[disaster]]
id = "fire"
name = "Fire"
every = 6000                         # mean ticks between strikes (0: by hand only)
radius = 2                           # tiles around the spot
effect = "destroy"                   # destroy | damage | sicken
protection = "fire_safety"           # layer that protects (optional)
full = 10.0                          # protection value that protects fully
near_water = false                   # only on low ground near water (floods)
```

- **Where it strikes:** a developed zone building, picked at random (low and
  near water for `near_water`). Every building within `radius` is hit with a
  risk of `1 − protection / full`, never under 5%; neighbours of the spot take
  70% of it.
- **`destroy`** empties the plot, which rests longer than after an abandonment.
- **`damage`** drops the building a level, or empties a first-level plot.
- **`sicken`** drops its satisfaction to 0.1, so it may decline.
- **Responders:** the nearest service whose emissions include the protection
  layer sends its `dispatch` vehicle to the scene. Fire trucks go to fires.
- **Player controls:** the **Disasters** menu switches chance disasters off
  and sets any of them off by hand. The MCP server has a `disaster` tool.
- **Shipped:** the modern rules have fire, flood and epidemic. Before 1800
  there are great fires (wider and more frequent), floods and plague.

### Shared pools (culture)

```toml
[[resource]]
id = "ideas"
transport = "pool"
shared = true                        # spreads across a region
```

A shared pool's production is part of each town's region summary. Each month a
town takes in, per tick, a tenth of what the other towns of its region make of
it. This applies in regions played alone and on a region server
([PLAYING.md](PLAYING.md#playing-a-region-with-others)).

### Waterways and harbours

Water works like rail: a **line** is one connected body of water (a lake, a
river, the sea, a canal), and a **harbour** on its shore is the stop. Its
quay is a water tile at most 2 tiles out, past the sloping shore.

- **Linked places:** places within 8 tiles of harbours on the same water
  trade with each other at speed 2. Boats are slower than trains, but they
  need no track.
- **Which resources sail:** the road network lists a water vehicle for them
  (`ways = ["water"]`). If the network lists no vehicles at all, every resource sails.
- **Boats** sail between harbours over water tiles, drawn with the water
  vehicle's `meshes`: Kenney cargo ships and tugs today, rowing and sailing
  boats before 1800, where the harbour is a `quay`.
- **Canals:** the **Canal** tool digs empty land, with nothing built around
  it, below the water line, at $30 a tile.
- **GeoJSON import:** `landuse=port` and `harbour=yes` become harbours.

```toml
[[network.vehicle]]
id = "ship"
name = "Ship"
model = "car"
ways = ["water"]
carries = ["workers", "shoppers", "goods", "fuel"]
share = 0
meshes = ["kenney/watercraft/ship-cargo-a", "kenney/watercraft/boat-tug-a"]
```

### Regions and trade

A resource with a `price` is traded with the neighbouring cities of a
region (see [PLAYING.md](PLAYING.md#regions)):

```toml
[[resource]]
id = "workers"
transport = "agents"
network = "road"
price = 0.02                         # per unit bought or sold
```

- **Averages:** each city keeps running averages of what it has left over
  (undelivered) and what it lacks (unmet) per tick, measured before any trade.
- **Monthly deals:** each month the open city buys what it lacks from
  neighbours with a surplus, and sells its surplus to neighbours that lack it.
  It trades only through crossings joined on both sides of the shared edge
  (the same tile along it) that carry the resource: its network is made of
  what crosses there. Power (`power_grid`: lines and roads) crosses on lines
  and streets, road agents on streets, and agents that trains carry on track.
- **Gates:** each crossing of a deal becomes a node of the city's network,
  with an even share of the deal. Every tick it sends what was bought to the
  nearest units it reaches that still need it (by agents from the edge tile,
  or into its grid component), and takes what was sold from the nearest
  senders' leftovers. A street gate reaches units on its road component, a
  track gate the units within 8 tiles of a station on its line.
- **Imports** fill unmet needs and are paid at the price, only for what gets through.
- **Exports** take what had nowhere to go, and earn the price.

The shipped rules price workers (commuters), shoppers, goods, fuel and
power; before 1800, workers, shoppers, food and goods.

### Service vehicles and coverage by road

An emission can travel along the streets instead of as the crow flies:

```toml
emits = { fire_safety = { amount = 10.0, radius = 12, by_road = true } }
dispatch = { meshes = ["kenney/car-kit/firetruck"], every = 15 }
```

- **`by_road`:** `radius` is a travel time in street tiles: a highway tile
  counts 0.4, a dirt road 2. Places take the value of the best road tile
  within 2 tiles of them, falling off with travel time plus the last steps
  on foot. A fire station covers what its trucks can drive to; a house just
  as near but on a street the station cannot reach stays uncovered. Level
  crossings count as road, other track does not.
- **`dispatch`:** sends a vehicle now and then (about once every `every`
  ticks) along the streets to a building within its reach. `model` says how
  it moves (`car`, `walker`…), and `meshes` what it looks like.
- **Shipped rules:** fire stations send fire trucks, police stations patrol
  cars and hospitals ambulances. Before 1800 the watch, the guards and the
  bathhouse staff walk.

### Railways

Track is a kind of road (`rail`) that only trains use. A **station** is a
block building; a track tile beside it is its platform.

- **Linked places:** places within 8 tiles of a station on a line can trade
  with places near any other station on that line. That holds even when no
  street joins them. The trip costs the walk to and from the stations, 3
  tiles' worth of boarding, and the ride at speed 4.
- **Which resources ride:** those the road network lists a rail vehicle for
  (`ways = ["rail"]`). If the network lists no vehicles at all, every resource rides.
- **Cars never drive on track.** Road vehicles cross it at **level
  crossings**: a track tile with road on two opposite sides. Road tools leave
  track alone; laying track over a street makes a crossing.
- **Trains** run between stations on the same line, drawn with the rail
  vehicle's `meshes`.
- **Rules:** the modern rules have a `station` unit (workers, power, upkeep)
  and a **Railways** research project that unlocks the `rail` and `station`
  tools. The pre-industrial rules and the older skins disable both.
- **GeoJSON import:** `railway=rail` becomes track and `railway=station`
  becomes a station.

```toml
[[network.vehicle]]
id = "train"
name = "Train"
model = "car"
ways = ["rail"]
carries = ["workers", "shoppers", "goods"]
share = 0                          # trains do not take road trips
meshes = ["kenney/train-kit/train-diesel-a", "kenney/train-kit/train-electric-city-a"]
```

### Ideas and research

Pooled resources build networks that are not on the map. The shipped
rules chain them:

```
cafés, restaurants, business centres ──ideas──▶ pool ──▶ schools ──research points──▶ pool ──▶ projects
(taverns, coaching inns, market squares and shops before 1800; academies)
```

### Tech trees

Research projects live in a **tech tree file**, apart from the rules, so
rule sets can share a tree, swap it, or go without one (then nothing is
locked). A rules file names its tree, which is read from
`<data>/techs/<id>.toml`:

```toml
tech_tree = "modern"                 # rules/glassbox.toml -> techs/modern.toml
```

Inline `[[tech]]` entries in a rules file still work and are merged with
the tree. The layout follows the tech tree of the Stronghold experiment:
branches, tiers, a prerequisite graph, rival "doctrine" choices and bonuses.

```toml
name = "Modern"
description = "Roads, towers, industry and power for a modern city."

[[tech]]
id = "steel_frame"
name = "Steel-frame towers"
description = "Lifts and steel skeletons let buildings rise past ten floors."
category = "Housing"                 # branch, groups the research window
tier = 2                             # depth, for display and order only
cost = { research = 4000 }           # pooled resources only
requires = ["motorways"]             # projects to finish first (optional)
unlocks = ["tower_2", "tower_3", "mall_3", "highway"]   # unit ids and tool ids
bonus = { goods = 0.2 }              # 20% more goods once done (optional)
excludes = ["service_economy"]       # rivals ruled out for good (optional)
```

- **Gates:** `requires` and the cost decide when a project opens. `tier` and
  `category` only arrange the window, so a deep project opens as soon as
  its requirements are done.
- **What is locked:** a unit or tool that any project names stays locked
  until one of the projects naming it is done.
- **Locked units:** a plot never develops into a locked unit, and buildings
  never level up into one.
- **Locked tools:** they are greyed out in the tool bar, and their tooltip
  names the project they wait for.
- **Services:** a service tool is locked when the unit it builds is.
- **Rival projects:** finishing a project rules out every project it
  `excludes`, and every project that excludes it, for good. This makes a
  city choose a doctrine: a service economy or industrial policy, a free
  market or a guild charter.
- **Bonuses:** a finished project's `bonus` adds that share to the
  production of the resource (and of its kinds) in every building.
- **Choosing a project:** pick one in the 🔬 Research window, which groups
  projects by branch and tier and shows what each one rules out. A project
  draws on the pools each tick until it is paid for, and a progress bar
  shows how far along it is.
- **Without a research window** (the classic interface, Android), the city
  works through the projects in the order the tree lists them.
- **Demo towns** start with everything researched; of rival projects they
  take the first one listed.

| Tree | Branches and projects |
|---|---|
| `modern` (GlassBox) | Transport: Motorways (highway). Economy: Service economy (+10% taxes, +30% ideas) or Industrial policy (+20% goods, +10% fuel); Computing (+25% research). Housing: Steel-frame towers (High-rise, Slab tower, Skyscraper). Industry: Aeronautics (Aeronautics works). Power: Nuclear fission (nuclear plant) |
| `pre-industrial` | Streets: Paving (cobbled streets). Farming: Crop rotation (+20% food and fodder), Horse breeding (Stables). Charter: Free market (Market square, +15% taxes) or Guild charter (Craft hall, +20% goods). Housing: Masonry (Courtyard house, Insula block). Civic: Bathing & medicine (Bathhouse), Printing (+30% ideas, +15% research). Trade: Banking (Merchants' exchange) |

A school makes about 2 research points a tick, some 2,400 a year, so a
project takes from a few months to a few years.

### Taxes and policies

Taxes and policies live in a **policy file**, apart from the rules like
tech trees. A rules file names it, and it is read from
`<data>/policies/<id>.toml` (inline `[[tax]]` and `[[policy]]` merge with it):

```toml
policy_set = "modern"                # rules/glassbox.toml -> policies/modern.toml
```

A **tax** scales a city resource (taxes) made by the units of some zones.
Units make what the rules write at the default rate, and in proportion at
other rates. Each point above the default adds `demand` to those zones and
`satisfaction` to the units paying; below the default, the opposite.

```toml
[[tax]]
id = "property"
name = "Property tax"
resource = "taxes"                   # a city resource
zones = ["residential"]              # empty: every unit making it
rate = 9                             # default, percent
range = [0, 20]                      # what the city may set
demand = -0.4                        # zone demand per point above the default
satisfaction = -0.01                 # per point above the default
```

A **policy** is an axis of stances; the first is the default. A stance can
wait for a research project, the way the Stronghold experiment's stances
wait for a doctrine (the free market opens Festivals, the guild charter
Long hours):

```toml
[[policy]]
id = "energy"
name = "Energy"

[[policy.stance]]
id = "balanced"
name = "Balanced"

[[policy.stance]]
id = "atomic_age"
name = "Atomic age"
requires = "fission"                 # research project (optional)
bonus = { power = 0.3 }              # production share, as for research
demand = { residential = -1.0 }      # per zone id
satisfaction = -0.02                 # for every unit
cost = 0.5                           # treasury per tick; negative earns
```

Set rates and stances in the 📜 **Policies** window. Without it (classic
interface, Android) every tax stays at its default and every policy at its
first stance. Saves keep both.

| File | Taxes | Policies |
|---|---|---|
| `modern` | Property, sales, business (9 %, 0-20 %) | Transport (car city, walkable centres), Energy (efficiency drive, atomic age), Education (free schooling, night classes), Growth (open city, strict zoning) |
| `pre-industrial` | Hearth tax (5 %), market tolls (8 %), guild dues (6 %) | Labour (festivals, long hours), Provision (feasts, thrift), Gates (open gates, town watch) |

## Playing

* The zone tools show a **Low / Medium / High** density switch.
* The **City** panel shows zone demand and, per resource, what was delivered
  against what was wanted. Shortages are orange.
* **Inspect** (`/`) a building to see its name and description, its zone,
  density and level ("level 2 of 3"), its size in tiles and floors, its
  satisfaction, needs, production and layer requirements. Inspect a road for
  its kind, speed, capacity, lanes, cost and traffic.
* **View > Data layer** draws pollution, land value or a service's coverage over the map.
* **Resource network** (`V`, **View > Resource network**, or **Network** in the
  City panel) lays the GlassBox network over the map, on the networks
  that carry it. Each link (a sender to a receiver) is followed along its
  own network and summed per stretch:
  * people, cars, lorries and buses along the streets (every kind of road),
    joining each building at its nearest street tile;
  * trains along the track between stations, and boats over the water
    between harbours (the walk to the stop is drawn straight);
  * power along power lines, streets and the buildings it runs through
    (whatever the grid network lists, `through_buildings` included), drawn
    higher, without travelling dots;
  * a border gate is one more node: its trips start or end on its edge tile.

  Each resource has its own lane in its colour (`colour` in `[[resource]]`),
  on the right of the way it travels. Lanes are as thick as the flow,
  smoothed over recent ticks, with dots streaming along them. Buildings that
  lack a shown resource are tinted red by how much they lack. Hover a
  stretch to read the resource, the way and the amount per tick.
  * The panel toggles each resource. Workers, shoppers and goods are on by
    default; power is off. It hides stretches under an amount and switches the
    shortage tint. **Only the inspected building's links** isolates one
    building.
  * Per resource, it lists links, amount per tick, mean trip length, the share
    served and the unmet amount. Two lists, **Busiest links** and **Biggest
    shortages**, jump to a building when clicked.
  * **Inspect** (`/`) a building to list its incoming and outgoing links,
    with links to the buildings at the other end.
* **GlassBox graphs** (`G`, **View > GlassBox graphs** or the button in the City panel)
  plot every series the engine samples each tick, for the last 2000 ticks:
  demand; produced, delivered, unmet and undelivered resources; buildings and
  average satisfaction per zone; layer mean and maximum; logged metrics; funds;
  and growth churn (buildings built, abandoned, levelled up or down).
  * Tick series on and off in the list (with a filter), or pick a preset.
  * Zoom with the wheel, pan by dragging, double-click to reset. Hover to read
    values, and click legend entries to hide lines.
  * Choose the time range, **Normalise** each series to its maximum to compare
    shapes, and smooth with a moving average.
  * The table gives last, min, max, mean, standard deviation and trend per 100
    ticks for each shown series. **Reversals** counts how often a series changed
    direction, and is orange when it oscillates.
  * **Export CSV** writes the shown series (all of them when none is shown)
    next to the save file.

## GlassBox rule scripts

Behaviour can also be written in the GlassBox rule language (`unitRule`,
`mapRule`, `zoneRule`, `agentRule`), including the Extended GlassBox metrics,
in `.gbx` scripts listed under `scripts`. See [GLASSBOX.md](GLASSBOX.md).

## File format

```toml
id = "my_rules"
name = "My rules"
population = "residents"           # resource counted as population
scripts = ["my_rules.gbx"]         # GlassBox rule scripts (optional)
tech_tree = "my_tree"              # techs/my_tree.toml (optional)
policy_set = "my_policies"         # policies/my_policies.toml (optional)

[growth]                           # level_up, level_down, chance, max_new, road_access

[[resource]]                       # id, name, transport, network, max_trip, colour, uses, trail
  [[resource.subtype]]             # id, name, share, max_trip, colour, uses, trail
[[network]]                        # id, name, structures, through_buildings
[[network.vehicle]]                # id, name, model, ways, speed, carries, share, direct, meshes
[[layer]]                          # id, name, diffusion, decay, colour, max
[[zone]]                           # id, name, structure, densities, demand, base_demand
[services]                         # tool id = unit id
[[unit]]                           # id, name, description, graphic, produce, needs, sends, emits, requires, build_cost
[[tech]]                           # id, name, description, category, tier, cost, requires, unlocks, bonus, excludes
[[tax]]                            # id, name, description, resource, zones, rate, range, demand, satisfaction
[[policy]]                         # id, name, description
  [[policy.stance]]                # id, name, description, requires, bonus, demand, satisfaction, cost
```

A tech tree file holds `name`, `description` and `[[tech]]` entries; a
policy file holds `name`, `description`, `[[tax]]` and `[[policy]]`. Both
merge with any inline entries of the rules file before it is validated.

Top-level keys also include `description`, `years` (the ages the rules
suit) and `disabled_tools` (tool ids hidden with these rules).

`graphic` is an OpenCity model code (see `assets/graphism.conf`). The model's
footprint is the unit's size, and skins restyle it like any other building.
Rules files are validated on load, and errors name the faulty reference.
