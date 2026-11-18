# A city as open data

Written by [citysim](http://ingmmo.com/citysim/). The files look like what
a real city publishes, so the tools made for real cities read them.
`city.json` says which city this is and how many of everything there are.

| File | What |
|---|---|
| `buildings.geojson` | Building footprints. `id`, `class` (residential, commercial, industrial, civic), `kind`, `unit`, `name`, `level`, `residents`, `jobs`, `satisfaction` (0 to 1), `area_m2`, `neighbourhood` (its name, when it lies in one), and OpenStreetMap-style tags. |
| `segments.geojson` | Streets, railway tracks and metro tunnels, one line from junction to junction. `id`, `class` (street, highway, dirt_road, paved_street, rail, tram, metro), `length_m`, `traffic` (the game's load on the road, 0 to 255), OpenStreetMap-style tags. |
| `landuse.geojson` | Parks, wetlands, landfills, empty plots and water. |
| `zones.geojson` | A grid of zones, like census tracts: `zone_id`, `residents`, `jobs`, `buildings`. |
| `od.csv` | Trips between zones in one tick of the simulation (a day's worth): `origin`, `destination`, `flow` (everything), then a column for each purpose (workers, shoppers, goods…). |
| `trips.csv` | The same trips between buildings. Ids are those of `buildings.geojson`; an id that is not there is a gate on the edge of the map. `road_share` is the part that went by road vehicle. |
| `gtfs.zip` | The bus, tram and metro routes as a GTFS feed, with frequencies and shapes. Only when something runs. |
| `city2graph_example.py` | Loads all of it with [city2graph](https://city2graph.net) and builds four graphs. |

Coordinates are longitude and latitude (WGS84). A city that was not
imported from a map lies on the equator at the prime meridian
(`"georeferenced": false` in `city.json`), with tiles of 16 metres.

`od.csv` and `trips.csv` are missing when the simulation has not run a tick
since the city was opened, or when it plays by the classic rules, which
have no trips.

What the timetable is made of: the game knows a route's stops and how many
vehicles it has. Speeds (bus 25 km/h, tram 30, metro 45 between stops),
20 seconds at each stop and the hours of service are set by the export; the
wait follows from the length of the route and its vehicles.
