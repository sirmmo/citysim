# A city as open data

citysim writes a city the way a real city publishes itself: building
footprints, street segments, land use, census-like zones, the trips between
them and a transit timetable. The files open in QGIS and load in
[city2graph](https://city2graph.net), which turns them into graphs for
network analysis and graph neural networks.

## Getting the files

- **In the game:** **File > Export open data…** writes one `.zip` (in the browser it is downloaded).
- **On the command line:** `opencity-cli export-data city.ocsave.json -o folder` (or `-o city-data.zip`).
- **From an AI assistant:** the MCP tool `export_data` (see [MCP.md](MCP.md)).

## What is written

| File | What |
|---|---|
| `city.json` | Which city this is: name, date, rules, residents, metres per tile, how many of everything. |
| `buildings.geojson` | Building footprints. `id`, `class` (residential, commercial, industrial, civic), `kind`, `unit`, `name`, `level`, `residents`, `jobs`, `satisfaction` (0 to 1), `area_m2`, `neighbourhood` (its name, when it lies in one), and OpenStreetMap-style tags. |
| `segments.geojson` | Streets, railway tracks and metro tunnels, one line from junction to junction. `id`, `class` (street, highway, dirt_road, paved_street, rail, tram, metro), `length_m`, `traffic` (the game's load on the road, 0 to 255), OpenStreetMap-style tags. |
| `landuse.geojson` | Parks, wetlands, landfills, empty plots and water. |
| `zones.geojson` | A grid of zones 8 tiles wide, like census tracts: `zone_id`, `residents`, `jobs`, `buildings`. |
| `od.csv` | Trips between zones in one tick of the simulation, which exchanges a day's worth: `origin`, `destination`, `flow` (everything), then a column for each purpose (workers, shoppers, goods, tourists…). |
| `trips.csv` | The same trips between buildings: `origin`, `destination` (ids of `buildings.geojson`; an id that is not there is a gate on the edge of the map), their zones, `purpose`, `flow`, `road_share` (the part that went by road vehicle). |
| `gtfs.zip` | The bus, tram and metro routes as a GTFS feed: stops, routes, a trip each way, stop times, frequencies through the day and shapes along the streets. |
| `city2graph_example.py` | Loads all of it with city2graph. |
| `README.md` | These notes. |

Coordinates are longitude and latitude (WGS84). A city imported from a map
([GEOJSON.md](GEOJSON.md)) keeps its place. Any other city lies on the
equator at the prime meridian, with tiles of 16 metres; `city.json` says
`"georeferenced": false`.

## What is real and what is made up

- **From the simulation:** the buildings and who lives and works in them, the streets and their load, the trips of the last tick, the routes with their stops and vehicles.
- **Set by the export:** footprints stand back 12% of a tile from the edge of their plot, so that buildings do not touch. Vehicles run at 25 km/h (bus), 30 (tram) and 45 (metro) between stops and stand 20 seconds at each. Service runs from 05:30 to midnight; the wait at rush hour is the round trip divided by the route's vehicles (2 minutes at least), one and a half times that by day, twice in the evening.
- **Routes:** the ones you drew ([PLAYING.md](PLAYING.md#drawing-the-routes-yourself)), or, while routes run themselves, one along each street, track or tunnel that has two stops or more.
- **Left out:** trains and boats, trips to other cities of a region, and what flows through wires and pipes.

`od.csv` and `trips.csv` are written under GlassBox rules once the city has
run a tick. On the command line the export runs one tick itself. The
timetable's year is the city's, or 2000 when the city's year is before 1900.

## With city2graph

```sh
pip install city2graph
python city2graph_example.py          # in the folder of the export
```

For the demo town this prints:

```
morphology
     354 place nodes
     114 movement nodes
     650 place -touched_to-> place
     268 movement -connected_to-> movement
     876 place -faced_to-> movement
transit
      10 stops
       7 links, 30 s on average
mobility (workers)
      20 zones
      27 flows, 159 in all
proximity
     983 links between neighbouring buildings
```

The script builds four graphs:

1. **Morphology** (`morphological_graph`): plots that touch, streets that meet, and which plot faces which street.
2. **Transit** (`load_gtfs`, `travel_summary_graph`): stops joined by the routes, with travel times and frequencies.
3. **Mobility** (`od_matrix_to_graph`): zones joined by the trips between them.
4. **Proximity** (`knn_graph`): each building and its nearest neighbours.

With PyTorch Geometric installed, `c2g.gdf_to_pyg(nodes, edges)` makes any
of them a tensor graph. Checked with city2graph 1.1.0 on Python 3.12.
