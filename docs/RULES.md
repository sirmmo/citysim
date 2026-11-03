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
```

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
  City panel) draws who sends what to whom on the map. Each link (a sender
  building to a receiver) is an arc in its resource's colour (`colour` in
  `[[resource]]`). Arcs are as thick as the flow, smoothed over recent ticks,
  with dots travelling towards the receiver. Power links run from plants to
  the consumers of their grid. Buildings that lack a shown resource are
  tinted red by how much they lack. Hover an arc to read the resource, both
  ends, the amount per tick and the distance.
  * The panel toggles each resource. Workers, shoppers and goods are on by
    default; power is off. It hides links under an amount and switches the
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

[growth]                           # level_up, level_down, chance, max_new, road_access

[[resource]]                       # id, name, transport, network, max_trip, colour, uses, trail
  [[resource.subtype]]             # id, name, share, max_trip, colour, uses, trail
[[network]]                        # id, name, structures, through_buildings
[[network.vehicle]]                # id, name, model, ways, speed, carries, share, direct
[[layer]]                          # id, name, diffusion, decay, colour, max
[[zone]]                           # id, name, structure, densities, demand, base_demand
[services]                         # tool id = unit id
[[unit]]                           # id, name, description, graphic, produce, needs, sends, emits, requires, build_cost
```

Top-level keys also include `description`, `years` (the ages the rules
suit) and `disabled_tools` (tool ids hidden with these rules).

`graphic` is an OpenCity model code (see `assets/graphism.conf`). The model's
footprint is the unit's size, and skins restyle it like any other building.
Rules files are validated on load, and errors name the faulty reference.
