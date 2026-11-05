# Playing citysim

A guide to building and running a city. For the file formats behind the
game see [RULES.md](RULES.md); for the documentation index see
[README.md](README.md).

## Starting a city

- **File > New city…** opens the new city dialog:
  - **Map:** width, length, relief, water, shape, trees and seed (🎲 picks a random one).
  - **Era:** Antiquity, Middle Ages, Renaissance, Industrial age or Modern, plus a culture for antiquity.
  - **Rules:** **Classic** is OpenCity's original growth rules. **GlassBox** is the data-driven engine. Cities before 1800 default to the pre-industrial GlassBox rules.
- **File > Import GeoJSON…** builds a city from OpenHistoryMap, OpenHistoricalMap or OpenFantasyMap data (see [GEOJSON.md](GEOJSON.md)).
- **From the command line:** `--demo` starts a demo town and `--load FILE` opens a saved one.

## Controls

| Input | Action |
|---|---|
| Left drag | Use the selected tool. Zones and roads cover the dragged area. |
| Right drag, Q / E, PageUp / PageDown | Rotate the view |
| Middle drag, arrows, WASD | Pan (hold Shift or Alt to go faster) |
| Wheel, + / −, Insert / Delete | Zoom |
| Home / End | Tilt |
| R, C, I | Residential, commercial, industrial zone |
| P | The street of the age: asphalt, else paved, else dirt |
| L, T | Power line, trees |
| X | Bulldozer |
| U, J | Raise, lower terrain |
| / | Inspect a tile |
| N | Drop the tool |
| O | Turn the inspected plant or service a quarter |
| G | GlassBox graphs |
| V | Resource network on the map |
| Space, F | Pause, fast forward |
| Esc | Drop the tool, or open the menu (new, load, save, quit) |

**On Android and other touch screens:**
- With no tool selected, drag with one finger to pan.
- Two fingers pan, pinch zooms and twist rotates.
- The menu is the button under the speed control.

## Building

- **Zones** grow when they have a road nearby and power (in the modern rules), and are close to the other two zone types.
  - Under GlassBox rules, pick a **density** (low, medium, high) before zoning. Denser zones cost more and grow bigger buildings.
- **Roads** come in four kinds that connect to each other. Each era offers its own.

  | Road | Cost | Speed | Capacity | Notes |
  |---|---|---|---|---|
  | Street | $5 | 1 | 1 | Modern asphalt |
  | Highway | $25 | 2.5 | 3 | Buildings cannot open onto it |
  | Paved street | $3 | 0.8 | 0.7 | Cobbles of older towns |
  | Dirt road | $1 | 0.5 | 0.4 | Cheap, slow, narrow |

  | Railway | $20 | 4 | 4 | Trains only; roads cross it at level crossings |

  Dragging a road over another kind rebuilds it, but roads leave track alone.
- **Railways:** lay track with the Railway tool, then put a **Railway
  station** next to it, near a street. Places within 8 tiles of a station
  reach places near every other station on the same line, even in another
  town no road joins. Trains run between the stations. In the modern rules
  rail waits for the **Railways** research project; there is none before 1800.
- **Plants and services** (power plants, fire stations, police, hospitals, schools, wells…):
  - **Placing:** a new one turns to face the street beside it, and the preview shows the footprint it will take.
  - **Turning:** inspect it and press **⟳ Turn**, or **O**. Long buildings swap their footprint when turned.
- **Harbours and canals:** a **Harbour** goes on dry land with water within
  2 tiles, past the sloping shore. Places near harbours on the same lake,
  river, coast or canal trade with each other, carried by ships (rowing and
  sailing boats before 1800, at a **Quay**). The **Canal** tool digs empty
  land below the water line to join waters or bring them to a town.
- **Disasters:** fires, floods and epidemics (great fires and plague
  before 1800) strike now and then. Fire coverage protects against fire and
  health coverage against sickness; floods hit low ground by the water. A
  notice says where it struck and how many buildings it hit, and fire trucks
  head there. The **Disasters** menu switches them off or sets one off by hand.
- **Services cover what they can reach by road.** Fire, police and health
  coverage spreads along the streets from the building, so a station on a
  street cut off from a neighbourhood does not protect it. Fire trucks,
  police cars and ambulances (the watch and guards on foot before 1800) go
  out to the buildings they cover.
- **Locked tools** are greyed out until research unlocks them; their tooltip names the project.

## The city panel

The right-hand panel shows:
- zone sizes and power;
- demand for each zone (GlassBox), which comes from what the city lacks: jobs left open, shoppers and goods with nowhere to go;
- what each resource delivered against what was wanted (orange when short), with taxes, upkeep and pools such as ideas;
- metrics from the rule scripts and the last 20 years of population and funds.

Click any building with the inspect tool (/) to see:
- its zone, density and level, size and floors;
- its satisfaction;
- what it needs and makes, where its supplies come from and what it lacks;
- the map layers on its tile.

## Scenarios

**File > New scenario…** starts a game with a map, rules, money and goals.
The 🎯 **Goals** window (in the City panel) shows how each goal is going.
`--scenario ID` starts one from the command line. A **pack** goes further:
it seals a scenario with its skin and rules, as a game of its own
(`--pack ID`; see [PACKS.md](PACKS.md)).

| Scenario | Start | Goals |
|---|---|---|
| First town | A lakeside valley, modern rules, $30,000 | 2,000 people within 6 years, research motorways within 8, never in debt |
| Rail baron | A long hilly map, $60,000 | Research railways within 4 years, 3 stations within 8, 4,000 people within 12, never in debt |
| Bologna, 1600 | The centre of Bologna from OpenHistoricalMap in 1600, pre-industrial rules, Renaissance dress | Printing within 15 years, 200 food a day within 5, never in debt |

**How goals work:**
- A goal with a deadline must be reached within its years. Once reached, it
  stays reached.
- An "all game long" goal must hold every month.
- You win when every goal is reached and every all-game goal holds. You lose
  when a deadline passes or an all-game goal breaks.

**Writing your own:** a scenario is a TOML file in `assets/scenarios/`,
listed in `index.toml`.

```toml
id = "my-town"
name = "My town"
description = "What the player is asked to do."

[start]                      # a generated map, or geojson = "…" with date = "…"
width = 64
length = 64
seed = 7
relief = "hill"              # plain, hill, mountain
water = "coast"              # dry, lake, coast
rules = "glassbox"           # or pre-industrial; none for classic
skin = "industrial"          # optional
fund = 20000
research_all = false         # demo = true lays out the demo town

[[goal]]
name = "5,000 people"
metric = "population"        # population, fund, buildings, research:<id>,
at_least = 5000              # unit:<id>, structure:<kind>, produced:<res>, unmet:<res>
within_years = 10            # or always = true; at_most for upper bounds
```

## Regions

A region is a grid of neighbouring cities, played one at a time, that trade
across their borders.

- **File > New region…** picks a name, the number of cities across and down,
  the map size and the rules, then founds the first town. **File > Open
  region…** opens one; regions live next to your saves in `regions/`.
  `--region FOLDER` opens or creates one from the command line.
- **The 🗺 Region window** shows the grid. Click a town to play it, which
  saves the open one, or found a new one in an empty slot.
- **Border crossings:** neighbours trade through a road, track or power line
  that reaches their shared edge, on both sides. The window shows each edge:
  ✔ trading, ✖ a crossing is missing, · no neighbour.
- **Monthly trade:** each month the open town buys what it lacks from
  neighbours with a surplus and sells what it has left over: workers
  commuting, shoppers, goods, fuel and power (food before 1800). Imports fill
  unmet needs and cost money; exports earn it. The window lists the deals.
- **Waiting towns:** neighbours keep the summary they had when you last left
  them, until you play them again.

### Playing a region with others

One player hosts a region server; each player plays one town of it in their
own game.

```sh
# the host (any machine the players can reach)
opencity-cli region-server --dir my-region --listen 0.0.0.0:8077 --name Emilia --across 3 --down 3
```

- **Joining:** each player picks **File > Join region server…** and enters
  the server's address, a name and a free slot. You can also start the game
  with `--join http://host:8077 --player ada --slot 1,0`.
- **Your town** saves on your own machine. Each month your game fetches the
  other towns' summaries, agrees trade with the neighbours, takes in their
  ideas, and sends your town's summary back.
- **The Region window** shows who plays which town. Online you play only
  the town you claimed.
- **No security:** the server keeps `region.json` and the players' tokens in
  its folder. It has no passwords or encryption, so play with people you trust,
  on a network you trust.

### Shared culture

Pooled resources marked `shared` spread across the region. **Ideas** do in
both rule sets: every month each town takes in a tenth of the ideas the other
towns of the region make, so a town of thinkers helps its neighbours'
research. This happens in regions played alone and online.

## Research

Under GlassBox rules, ideas and research unlock buildings and tools:

1. People meeting in cafés, shops and malls (taverns, inns and market squares before 1800) have **ideas**, which go into a city-wide pool.
2. Schools (academies) turn ideas into **research points**.
3. In the **🔬 Research** window, pick a project. It draws on the pools each tick until it is paid for.

The window groups projects by branch and sorts them by tier, and shows for each one:
- ✔ done, ▶ in progress, ○ available, 🔒 waiting for another project, ✖ ruled out;
- its cost, what it unlocks and its production bonuses.

Some projects are **rivals**: finishing one rules out the others for good.
- Modern: a **service economy** or **industrial policy**.
- Before 1800: a **free market** (market squares) or a **guild charter** (craft halls).

Without the window (classic interface, Android), the city researches projects in order by itself. Demo towns start with everything researched.

## Taxes and policies

The **📜 Policies** window sets:
- **Tax rates.** Each tax has a default rate and a range.
  - Above the default, the zones paying it bring in more, but they want to grow less and their buildings are less satisfied.
  - Below the default, they grow faster for less income.
  - The window shows the effect on income, demand and satisfaction.
- **A stance for each policy.** Every policy has a default stance and alternatives with trade-offs: production bonuses, more or less demand, happier or unhappier people, and a running cost or income.
  - Some stances wait for research. Atomic age needs fission, Festivals the free market, Long hours the guild charter.

| Rules | Taxes | Policies |
|---|---|---|
| Modern | Property, sales, business (9 %) | Transport, Energy, Education, Growth |
| Pre-industrial | Hearth tax, market tolls, guild dues | Labour, Provision, Gates |

## Seeing the city

- **View > Data layer:** pollution, land value, air quality, noise, water, and fire, police, health and education coverage, drawn over the map.
- **🔀 Network (V):** arcs from each sender to each receiver, coloured by resource. You can:
  - filter by resource and hide small flows;
  - list the busiest links and the biggest shortages;
  - click a building to see only its flows.
- **📈 Graphs (G):** every series the engine samples (demand, flows, satisfaction, layers, metrics, funds, growth). Zoom, smooth, normalise, compare and export CSV.
- **Vehicles:** cars, taxis and trucks in modern towns (Kenney car kit); people, handcarts and horse carts before 1800. What travels where follows the rules: no cars in the Middle Ages, no carts on the highway.
- **View > Skin:** re-dresses the city for an era or a world (see [SKINS.md](SKINS.md)).

## Day and night, seasons and weather

- **Day and night:** the light turns once a game month, through dawn, noon,
  a warm dusk and a blue night. (A game day passes in under two seconds, so a
  daily cycle would only flicker.) The top bar shows the hour, the season and
  the weather.
- **Seasons:** the ground greens in spring, turns gold in autumn and goes
  white in winter.
- **Weather:** it changes weekly with the season's odds: clear, cloudy, rain
  and storms, and snow in winter. Rain and snow fall around you.
- **Effects on disasters:** storms make fires likelier, as does dry summer.
  Rain and snow damp them down. Rain and storms swell floods, and so does
  spring. Sickness spreads more in winter.
- **Switching it off:** **View > Day and night, weather** turns the light,
  ground colours and falling rain and snow off. The weather's effect on
  disasters stays.

## Budget

The **💰 Budget** window, in the City panel, shows where the money goes.

- **Last month and this month so far, by line:**
  - taxes, per tax;
  - upkeep, per kind of service;
  - policies, trade sales and purchases;
  - loans, and the family (if called);
  - under classic rules, maintenance and the yearly taxes.
- **Service funding:** a slider per kind of service from 0 to 150%. It
  scales both what the service costs and how strongly it covers the city,
  so half-funded fire stations cost half and protect less.
- **Loans:** borrow $10,000, $50,000 or $200,000 at 6% a year, repaid monthly
  over five years, with three loans at most.

The MCP server has a `budget` tool with the same lines, funding and loans.

## Cheats

**Ctrl+Shift+C** opens the cheat box, as in SimCity 3000.

- **`CallCousinVinnie`** wires **$1,000,000** to the city. But Cousin Vinnie
  has family, and they move in:
  - **Protection money:** every month they collect up to $20,000, scaled by
    their hold on the city.
  - **Crime:** police coverage falls everywhere while they are in town.
  - **Arson:** now and then a block burns ("Somebody didn't pay").
  - **Driving them out:** each police station loosens their hold month by
    month, and with enough stations the family packs up and leaves. Calling
    Vinnie again tightens their grip.

The MCP server has a `cheat` tool too.

## Saving and sharing

- **Saving:** **File > Save** writes a `.ocsave.json`. Saves keep the rules, research, tax rates, stances, building facings and vehicles.
- **Exporting:** **File > Export GeoJSON…** writes the city back in the coordinates it was imported from.
- **Headless:** `opencity-cli` creates, simulates, renders, imports and exports cities without a window. `opencity-cli simulate --metrics out.csv` exports every series.
