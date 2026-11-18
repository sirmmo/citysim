# Changelog

Release notes in full are on the [releases page](https://github.com/sirmmo/citysim/releases).

## Unreleased: Cities for OpenTactics
- **Run a city for an OpenTactics game:** a link from OpenTactics (`play/#holding=…`) founds a city named after the one there, on land shaped by its neighbours: sea, mountains and forest along the edges they lie on.
- **No racing ahead of the war:** each OpenTactics day allows 3 months of city time; then the city waits, saying *Waiting for day N+1 in OpenTactics*, and you can still build. A newer link carries on from the city's own save.
- **Report to OpenTactics**, in the new OpenTactics window: a link back with the population and the taxes, copied to the clipboard. The city then pays 50% of its income there, plus 100% for every $400 of taxes a month.
- On the command line: `--holding LINK`. See [PLAYING.md](docs/PLAYING.md#a-city-for-opentactics).

## 0.2.79: Events and decisions
- **Things happen, and ask for a decision** (GlassBox rules): the city's staff want a raise, residents want a festival, a factory asks for a tax break, a developer wants a park, the paper has a scandal. Each choice says what it does: money, money month by month, points of approval. Some are gambles.
- **A question waits three months**, then its last choice is taken. *Decide* in the city panel reopens it. Approval from decisions fades by a tenth a month.
- **Events from outside:** an event marked `external` never comes up by itself. A game this city is part of sets it off with `--event ID`; so do the MCP tool `events` and `opencity-cli simulate --event ID`.
- **The government has fallen** (`government_falls`; before 1800 `realm_falls`): the mayor's office, city hall and neighbourhood offices close at once and stand as relics, and the same tools now build a Council chair's office, a Provisional council and Citizens' committees.
- **Events are rules:** `[[event]]` with `[[event.choice]]`, and effects that can retire units (`obsoletes`) and change what a service tool builds (`services`). See [RULES.md](docs/RULES.md#events).
- Disasters menu: *Events come up by themselves* and *Make happen*. They are off under the classic interface, on Android and in `opencity-cli simulate` (unless `--events`).

## 0.2.78: Government buildings
- **Three offices of the city's own**, in the Services row (GlassBox rules):
  - **Mayor's office** (one to a city, $2,500): 3 points of approval.
  - **City hall** (one to a city, $6,000): running the city costs a quarter less.
  - **Neighbourhood office** (any number, $800): half a point of approval and 2% off the cost of running the city, each.
- They need a street, staff and power, and give what they give by how well they work. Before 1800 they are the Magistrate's house, the Town hall and the Ward house.
- GeoJSON: `amenity=townhall` imports as a city hall.
- For rule writers: `limit`, `approval` and `administration` on a unit. See [RULES.md](docs/RULES.md#tech-trees).

## 0.2.77: Obsolete buildings
- **Research can retire buildings.** *Nuclear fusion* makes coal power plants obsolete, and the new project *Circular economy* (after Computing, 5,000 research points) does the same to landfills.
- **A year to adapt:** no more can be built from the day the project is done, and the ones standing work for one more year. Inspecting one shows the months left.
- **Then they are relics:** an *Old power station* raises land value, gives ideas and takes visitors; a *Capped landfill* no longer smells. Keep a relic, or pull it down from its window: the power station pays $1,500 of scrap.
- A city that picks its own research (classic interface, Android) keeps its buildings running.
- For rule writers: `obsoletes` on a tech, `relic` and `salvage` on a unit. See [RULES.md](docs/RULES.md#tech-trees).

## 0.2.76: Open data for city2graph
- **A city as open data:** **File > Export open data…** writes the city the way a real one publishes itself, in one zip: building footprints with residents and jobs, street segments from junction to junction, land use, census-like zones, the trips between zones and between buildings, and the bus, tram and metro routes as a GTFS feed.
- **Ready for [city2graph](https://city2graph.net):** the zip holds a script that loads it all and builds four graphs (morphology, transit, mobility, proximity). Checked with city2graph 1.1.0.
- On the command line: `opencity-cli export-data city.ocsave.json -o folder`. For AI assistants: the MCP tool `export_data`.
- Coordinates are longitude and latitude; a city that was not imported from a map lies on the equator with tiles of 16 metres. Timetable speeds and hours are set by the export. See [DATA.md](docs/DATA.md).

## 0.2.75: A city panel for small screens
- **The parts of the city panel fold away:** click a title (Zones and power, Demand, People, Resources, Metrics, Last 20 years, About the city) to fold or open it.
- **The panel scrolls** when it is taller than the window.
- On a screen under 760 pixels tall, the parts from the resources down start folded.

## 0.2.74: Draw the routes yourself
- **An opt-in mode for those who like to run the buses:** View > Draw bus, tram and metro routes yourself. People then ride only between stops that a route joins; without it everything works as before.
- **The Routes window:** start a bus, tram or metro route, click its stops on the map in order, and give it vehicles. More vehicles are a shorter wait and a bigger bill ($15 a bus, $25 a tram, $50 a metro train each month).
- **People change** between routes at a stop they share, and wait again.
- **Each route shows its wait and its riders**, so you can see which ones earn their vehicles. Routes can be circular.
- **Nothing breaks when you switch it on:** the city's own routes are written down first, and *Fill the gaps* adds routes for stops that have none.
- Routes are drawn on the map in their colours while the window is open; buses and trams on the map follow them.
- For agents: the MCP tool `routes` does the same.

## 0.2.73: The browser game installs as an app
- **Install it:** at <https://ingmmo.com/citysim/play/> the game installs as an app, with its own icon and window. In Chrome and Edge use File > Install as an app… or the button in the address bar; on an iPhone or iPad, Share > Add to Home Screen.
- **It plays offline.** The first visit over https keeps the whole game in the browser (about 21 MB); after that it starts without a network, installed or in a tab.
- **Updates:** a new version downloads while you play and a notice in the corner offers to restart with it. Closing the game does the same.
- The browser is asked not to clear the saves when the disk runs short.
- Over plain `http://` nothing changes: no install, no offline play. Saves made there stay there (the two addresses have separate storage), and only the http page can join an `http://` region server on another machine.
- For those who host the web build: it has five new files (`manifest.webmanifest`, `sw.js`, three icons) and must be served over https to install.

## 0.2.72: Turning turbines and rules for the vote
- **Wind turbine blades turn**, faster in rough weather and not at all in a calm. With the weather off the map (View > Day and night, weather) they turn at an even pace.
- **Rules decide when a town turns on its mayor:** a `[mayor]` table sets the approval line, the months under it, the smallest town that votes and the length of a term. The modern rules keep 30%, 12 months, 500 people and 4 years.
- **Before 1800** it takes two years under 25%, in a town of 200 people or more, and terms last 5 years.
- Checked: tourists nobody takes in one town of a region drive on to the neighbour's attractions.

## 0.2.71: Fields and mines follow the land
- **Before 1800 the land decides what fields and mines give.** Two new data layers are read off the terrain: *Fertile ground* (flat land, best by rivers, lakes and the sea) and *Stone and ore* (high and steep ground).
- **Grain fields, farms and farmsteads** make half as much again on the best ground by the water, as before on ordinary plain land, and three quarters on slopes.
- **Quarries, mines and stone works** make half as much again in the hills, and three quarters in the plain (they made the same anywhere before).
- The inspector shows what the land gives a building: "makes Food 5.2 of 4.0".
- For rules authors: a layer takes `ground = "fertile"` or `"ore"`, and a unit `yields = { layer, full }`. The modern rules do not use them yet.

## 0.2.70: A tutorial
- **Tutorial: your first streets**, first in File > New scenario…: eleven steps from an empty valley to a town of 800 people, each with a hint in the Goals window saying what to click and why. Steps are checked as you do them, with no deadlines.
- For scenario authors: `ordered = true` makes the goals steps taken in order, and each goal takes a `hint`.
- The `buildings` goal metric counts only plots with a building on them under GlassBox rules (zoned empty plots counted before).

## 0.2.69: Ordinances
Five new policies in the modern rules, each with two alternatives to the default:
- **Policing:** community policing (each station brings crime down 40% more) or a night curfew (twice as hard, with fewer shoppers and unhappier people).
- **Industry and air:** a clean air act (35% less smoke from works and plants, 20% less dirty water, 5% fewer goods) or a free enterprise zone (more industry, more goods, 35% more smoke).
- **Road pricing:** a congestion charge (earns money, a fifth less fumes and noise from traffic, fewer shoppers) or free parking (more shoppers, more fumes and noise).
- **Rubbish:** recycling (30% less to collect) or pay per bag (half, and it earns, but people like it less).
- **Public health:** vaccination campaigns (epidemics half as likely) or a smoking ban (fewer fires, a few less epidemics).
- For rules authors: a stance takes `emits`, a share on what buildings leave on a map layer.

## 0.2.68: Terms of office and promises
- **Terms of office**, an opt-in mode (View > Terms of office and promises): an election every four years. Half the vote keeps you in office; with less you are out, as after a vote of no confidence.
- **Your share of the vote** is your approval in percent, plus 6 for each campaign promise kept, less 10 for each broken, less 3 for each term already served (up to three).
- **Campaign promises:** in the first year of a term, pick up to two from what the city offers: more people, a complaint halved, fewer than 5% out of work, money in the treasury, or getting out of debt. They are judged on election day.
- **An Election window** (in the City panel) shows the date, how the vote would go today and how each promise stands. The news window warns when an election looks lost.
- Scenario goals gain the metrics `debt`, `jobless`, `homeless` and `complaint:<name>`.
- The MCP server's `mayor` tool takes `terms` and `promise`.

## 0.2.67: Complaints, news and advice
- **Complaints reach the mayor.** Approval now loses points for what residents live with at home: crime and radiation (up to 20 each), smoke from works and plants (12), dirty air from traffic (10), dirty ground water (10), noise (8), and no fire station, police or hospital (6 each) or school (5) within reach. Each counts by the share of residents living with it. A town that meets every need but has no services now sits near 75%, not 98%.
- **News and advice**, a new window behind the 📰 News button in the City panel. The top lists what needs you now: a coming vote, debt, shortages, people out of work or without a home, complaints shared by many (with a Show button to the home that has it worst and a line on what helps), a nuclear plant on a cut budget, a zone in high demand.
- **The news is kept:** the last 80 things the game told you, with their dates, saved with the city.
- The MCP server has a `news` tool with the advice, the complaints and the news.
- For rules authors: `[[complaint]]` (a layer above or below a value, a name, points, advice). Both rule sets have a list.
- Cities saved earlier lose approval gradually if they lack services; the vote still needs 12 months under 30%.

## 0.2.66: Jobs, homes and crime
- **Crime**, a new data layer (modern rules). It rises where people with no job live, and everywhere while the family is in town. Shops and the better homes lose half their satisfaction where it is above 6, and land loses value above 4.
- **Police stations bring crime down** along the streets their cars can reach, most of all close by.
- **Out of work**, in the City panel: the share of workers with no job to go to.
- **Without a home**, in the City panel: people whose home was abandoned, burnt, evacuated, pulled down or made smaller. They move into new homes as room appears, and about half of those still waiting leave town within three months. They cost up to 20 points of approval.
- Graphs have `City/Jobless %` and `City/Homeless`; the MCP server's `city_summary` has `jobless` and `homeless`.
- For rules authors: resources take `idle` (what a unit made and found no taker for leaves on map layers), and `[people]` sets the jobless and homeless counts. Emissions along roads can now be negative.
- Cities saved earlier get the crime layer empty and count the homeless from now on.

## 0.2.65: Approval and elections
- **Approval rating**, in the menu bar: the share of the city behind its mayor. Hover it for what makes it up: how well homes, shops and works get what they need, less 10 points for debt, up to 40 for people leaving, up to 25 for recent disasters and up to 30 while the family is in town. It moves a fifth of the way there each month.
- **Vote of no confidence:** in a city that has had 500 people or more, approval under 30% for 12 months in a row gets you voted out. The council warns you when it starts and as the vote comes near; a month back above 30% takes two months off the count.
- **Voted out:** the city stands still and nothing can be built until you choose to run again (a new term from 50%), to stay on as mayor for life, or to start a new city.
- **View > Elections** switches the votes off for a city; the rating is still shown.
- Scenario goals can use the `approval` metric, the graphs have `City/Approval %`, and the MCP server has a `mayor` tool. The command line's `simulate` runs without votes unless given `--elections`.
- A new cheat, `Landslide`.
- Cities saved earlier start at 60% approval.

## 0.2.64: Tourism
- **Tourists**, a new resource apart from the town's shoppers. They arrive at the airport (80 a tick), the railway station (20) and the harbour (12), and travel by coach and taxi to the places that want them.
- **Resort hotel** (4 × 3, $6,000), **Observation tower** (2 × 2, $3,500) and **Amusement park** (6 × 5, $12,000), in the Amenities row after the research project **Tourism** (2,200 points). They live on visitors: the hotel and the park pay well and cost no upkeep when the tourists come, and all but close when fewer than half of those they want arrive. The tower raises land value over a wide area; the park is noisy.
- **A Tourism policy:** an advertising campaign brings a third more visitors through each gateway, a festival season half as many again, with busier shops and residents a little less happy.
- **In a region**, tourists nobody takes in one town go on by road to the neighbours, so a town with something to show can live on its neighbour's airport.
- Stations, harbours and airports in cities saved earlier bring tourists too; nothing wants them until you build for them.
- Map imports build a resort hotel from `tourism=hotel`, an observation tower from `man_made=tower` with `tower:type=observation` and an amusement park from `tourism=theme_park`; exports write them back.

## 0.2.63: Clean water
- **Water pollution**, a new data layer: dirty water in the ground. It comes from sewage nobody treats (it runs into the ground under the building that made it), from landfills, and from the slaughterhouse, the tank farm, the refinery and the steelworks. It seeps to the tiles around and clears within a few months once its source is gone.
- **What it does:** a pumping station on dirty ground (above 8) gives half its water, land above 5 loses value, and family houses, apartment blocks, high-rises and slab towers above 10 are half as content.
- **Waterworks** (3 × 3, $2,600): filters what it draws, so it gives clean water from any ground, and twice a pumping station's. It waits for **Water purification** (1,800 research points).
- **Sewage outfall** (1 × 1, $300): lets the sewage of its mains out untreated. No staff, no power, next to no upkeep, and dirty ground water for some ten tiles around: a cheap start, well away from pumps and homes.
- **Reed bed** (2 × 2, $400): a planted pond that cleans the ground water around it.
- The water treatment plant is now called **Sewage treatment plant**, which is what it does.
- **In a city saved earlier**, a landfill or heavy industry next to a pumping station now costs water. The demo town's landfill has moved away from its pumps; towns already built from it keep the old layout.
- **In the rules:** `spill` on a resource (what an unmet need leaves on a map layer), and emissions with a negative amount clean a layer (see RULES.md).
- Map imports build a waterworks from `man_made=water_works` (a pumping station from `man_made=pumping_station`), an outfall from `man_made=outfall` and a reed bed from `natural=wetland` with `wetland=reedbed`.

## 0.2.62: Nuclear accidents
- **A nuclear plant can have an accident**, and how it is run decides how likely that is. With its staff, its upkeep and its full budget a plant is safe for about a hundred years; at half its funding, or short of workers, it is one accident in ten years. The **Atomic age** energy policy doubles the risk. Until now a plant on half its budget made the same power for half the money; that saving has a price.
- **What an accident does:** the plant is lost, most homes, shops and factories within seven tiles are evacuated at once, and the land is contaminated. A new **Radiation** data layer shows it; where it is above 4 nothing is built and nobody stays. It fades by half every seven months: the edge of the zone comes back within months, the middle after more than two years.
- Fusion plants have no such risk. The Disasters menu can set an accident off by hand, and switching chance disasters off stops this one too.
- **In the rules:** `source` and `fallout` on a `[[disaster]]`, `uninhabitable` on a `[[layer]]`, `disasters` on a policy stance (see RULES.md).

## 0.2.61: Electric cars
- **A Vehicles policy** in the Policies window, after the new research project **Electric vehicles** (3,500 points). One car in four is electric to begin with; **Electric car grants** make it six in ten and **Combustion ban** all of them. Electric cars leave no fumes along their routes and charge from the grid at home, so the air and the noise maps clear along the busy roads, the fuel industry sells less, and the town needs more power. The ban costs two points of everyone's satisfaction.
- **In the rules:** a policy stance can set the shares of a resource's kinds (`kinds = { "car.electric" = 0.6, "car.gasoline" = 0.4 }`) and scale what traffic leaves on a map layer (`traffic = { "Noise.level" = -0.3 }`). See RULES.md.

## 0.2.60: Wind turbines, and sun that sets
- **Wind turbine** (1 × 1, $600): 25 units of clean power on average from a single tile, with no staff. It makes more in rough weather, nearly twice in a storm, and less on a fine day; it hums, so keep it off the best streets. It waits for **Wind power** (1,500 research points).
- **Solar farms follow the sun.** Nothing at night and three to four times their rated 120 at noon; more over summer's long days, less in winter and under clouds. A town on solar power alone now goes dark at night: keep another plant or enough turbines on the grid. With *Day and night* off in the View menu there is no hour, and the farms make the day's average.
- The clock in the top bar tells the sun and the wind of the moment when you hover it, and a plant's inspector shows what it makes right now next to its rated power.
- **In the rules:** `follows = "sun"` or `follows = "wind"` on a `[[unit]]` (see RULES.md).
- Map imports build wind turbines from `generator:source=wind`; exports write them back.

## 0.2.59: Right click in the browser
- **Fixed: after a right click in the browser the map stayed tied to the mouse.** The browser's own menu opened over the game and took the release of the button, so the game went on turning the view as if it were held. The menu no longer opens over the map, and a button let go outside the map, over another part of the page or while the window loses focus is let go in the game too.

## 0.2.58: Solar and fusion power, research by the region, a campus
- **Solar farm** (4 × 4, $1,800): clean power with hardly any staff or upkeep, and little of it for the land it takes: 120 units, where a coal plant on fewer tiles makes 400. It waits for **Photovoltaics** (2,500 research points).
- **Fusion power plant** (7 × 6, $40,000): 6,000 units of power, five nuclear plants' worth, with no smoke. It waits for **Nuclear fusion**, the biggest project in the tree: 120,000 research points, after Nuclear fission and Computing.
- **Region-wide research.** Nuclear fusion is a project of the whole region: every town can pay into it, what the others have paid counts, and once one town finishes it they all have it. The Research window marks such projects and shows the share the other towns have paid. A town on its own pays the lot. In the rules: `regional = true` on a `[[tech]]`.
- **The university is a campus** of 7 × 5 tiles, with faculty wings, a library, a laboratory and a residence tower around a quad. Universities built before keep their 5 × 4 building and work the same.
- Map imports build solar farms from `plant:source=solar` and fusion plants from `plant:method=fusion`; exports write them back.

## 0.2.57: Amenities, fires that burn, mixed streets
- **Amenities**, a new row in the sidebar: **Museum**, **Archaeology museum**, **Modern art museum**, **Opera house**, **Stadium** and **Shopping mall**. They take visitors (the shoppers of the homes around, who then have somewhere to go) and raise land value; the museums and the opera add ideas for the schools, the stadium and the mall pay taxes and draw crowds and cars. The plain museum needs no research; the others come with **Cultural institutions** and **Mass entertainment**. Before 1800: an **Arena** and a **Theatre**, after *Spectacles*.
- **Fires burn, and spread.** A building that catches fire is no longer gone at once: it burns for some twenty ticks, with flames, smoke and a flame mark over the roof. Until then fire coverage may put it out and save it, and the fire may pass to the buildings next door. With no fire station in reach what burns is lost and a block can go; under good coverage most fires are out in a few ticks. A message counts the buildings lost and saved when the last fire is out.
- **Floods and epidemics show on the map** too: a wave mark on flooded buildings, a red cross on those whose people are ill, for as long as it lasts. The inspector says what a building suffers from.
- **Mixed streets.** In the modern skin every level of shop, office and factory now picks, per plot, one of several buildings of its size and a tint, and factories a chimney or a tank: a row of shops is no longer one building repeated.
- **In the rules:** `lasts`, `spread` and `relief` on a `[[disaster]]` (see RULES.md). A fire with no `lasts` destroys at once, as before.
- Map imports build museums from `tourism=museum` (archaeology and art museums by their `museum` tag), an opera house from `amenity=theatre`, a stadium from `leisure=stadium` and a mall from `shop=mall`; exports write them back.
- The MCP `disaster` tool reports whether the buildings hit are burning; `research` takes `all: true` to finish every project, for trying things out.

## 0.2.56: The browser version loads its own data
- **Fixed: new buildings as empty 1 × 1 lots in the browser.** After an update the browser could pair the new game with the data files it had kept from the version before: the high school and university then had no model, no size and no rules. The game now asks for the data of its own build, and the page for the game of its own build.
- A building whose model is missing from the data files still takes a plot of its proper size.

## 0.2.55: Empty lots, a clearer underground, safer saves
- **Fixed: 0.2.54 loaded older cities with the wrong buildings.** Saves number each building's unit by its place in the rules, and 0.2.54 added the high school and university in the middle of the list: stations, stops, water works and landfills of a city saved earlier loaded as something else. The new units now stand at the end, and saves name their units, so a city is renumbered when the rules gain, lose or reorder units (also after editing rules).
- **Empty lots pass power and water.** Zoned land that is not built on yet conducts like a building: a strip of zone carries power and water to the houses behind it.
- **Fire, police and health cover reach the back rows.** Cover spreads from the street across zoned land a tile at a time, built on or not, where it used to stop two tiles from the road (plots develop three deep).
- **Underground view:** metro stations glow orange under a beacon, so they are not taken for bus or tram stops, which stay dim. Pumping stations, water towers and treatment plants glow blue, cyan and green. With a metro tool in hand the water mains fade, and with a water tool the tunnels do.

## 0.2.54: High schools and universities
- **High school** (4 × 3, $3,000): 7 research points a tick from 3 ideas, where a school makes 2 from 1, and education over a wider area. Unlocked by the new **Secondary education** project.
- **University** (5 × 4, $8,000): 24 research points a tick from 8 ideas, with a staff of 40, and education for the whole town. Unlocked by **Higher education**, after Secondary education. It needs a town with cafés and restaurants enough to feed it ideas.
- Both are in the Services menu with their own icons; the modern skin composes them from the Kenney city kits, the other skins get a generated school block and a campus with a clock tower.
- **Before 1800:** the **Studium**, a university of masters and scholars (6 research points to the academy's 1.5), after the *Studium generale* project. No high school in those rules.
- Map imports build a university from `amenity=university` and `college`; exports write them back.

## 0.2.53: Sensors and rush hours
- **Sensors.** Inspect any tile and leave a sensor there (GlassBox rules). At a crossing it counts the trips driving past, in all and by what they carry (workers, shoppers, goods, cars…), along real road routes; on a building, what arrives and what leaves; anywhere, pollution, noise and the other maps. The inspector shows its readings, the GlassBox graphs plot them, and **Save CSV** writes one row per tick with the hour of day. **View > Sensors** lists them; they are saved with the city.
- **Rush hours.** With day and night on, traffic follows the clock: people leave home around 8 and come back around 18, shoppers go out late in the morning and after work, deliveries run by day, rubbish is collected early, and the nights are quiet. The traffic roads show, traffic pollution and noise, and the vehicles on the map rise and fall with it. Over a day the traffic adds up to what it was, and how far people get still goes by the day's average: the economy runs every tick.
- **Round the clock.** Hospitals, police and fire stations, and one shop or factory in twenty, work day and night in three shifts: their staff travel at 6, 14 and 22, not with the rush. The inspector marks them.
- **In the rules:** `tide = { out = [8], back = [18], spread = 1.5 }` on a resource, `round_the_clock` on a unit (a flag) and on a zone (a share), and `[shifts]`. The pre-industrial rules have a day of their own: out at first light, a morning market, and a watch in two shifts.
- `opencity-cli simulate --sensor NAME@W,L --sensor-csv DIR` records sensors in a headless run (`--no-tides` for traffic without a clock); the MCP server has a `sensors` tool.
- Vehicles shown on the map are drawn with their own dice: what is shown no longer changes what happens.

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
