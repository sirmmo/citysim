"""Turns a citysim data export into graphs with city2graph.

    pip install city2graph            # https://city2graph.net
    python city2graph_example.py      # in the export's folder, or pass the folder

Builds four graphs and prints their sizes:
  1. morphology: buildings (as tessellation cells) and street segments
  2. transit:    stops joined by the GTFS routes, with travel times
  3. mobility:   zones joined by the trips the simulation made
  4. proximity:  each building joined to its nearest neighbours
"""

import json
import sys
from pathlib import Path

import geopandas as gpd
import pandas as pd

import city2graph as c2g

folder = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).parent
about = json.loads((folder / "city.json").read_text())
print(f"{about['name'] or 'A city'}, {about['date']}: {about['residents']:.0f} residents")

buildings = gpd.read_file(folder / "buildings.geojson").set_index("id")
segments = gpd.read_file(folder / "segments.geojson").set_index("id")
zones = gpd.read_file(folder / "zones.geojson")

# The files are in longitude and latitude; graphs want metres.
metres = buildings.estimate_utm_crs()
buildings, segments, zones = buildings.to_crs(metres), segments.to_crs(metres), zones.to_crs(metres)
streets = segments[segments["class"].isin(["street", "highway", "dirt_road", "paved_street", "tram"])]

# 1. Morphology: which plots touch, which streets meet, which plot faces which street.
nodes, edges = c2g.morphological_graph(buildings, streets)
print("morphology")
for kind, table in nodes.items():
    print(f"  {len(table):6} {kind} nodes")
for (a, relation, b), table in edges.items():
    print(f"  {len(table):6} {a} -{relation}-> {b}")

# 2. Transit: the routes on a day of the timetable's year.
if (folder / "gtfs.zip").exists():
    gtfs = c2g.load_gtfs(folder / "gtfs.zip")
    day = f"{about['gtfs_year']}0601"
    stops, links = c2g.travel_summary_graph(gtfs, calendar_start=day, calendar_end=day)
    print(f"transit\n  {len(stops):6} stops\n  {len(links):6} links, {links['travel_time_sec'].mean():.0f} s on average")

# 3. Mobility: flows between zones (people going to work, when the city has them).
if (folder / "od.csv").exists():
    od = pd.read_csv(folder / "od.csv")
    weight = "workers" if "workers" in od.columns else "flow"
    zone_nodes, flows = c2g.od_matrix_to_graph(
        od, zones, zone_id_col="zone_id", source_col="origin", target_col="destination", weight_cols=[weight]
    )
    print(f"mobility ({weight})\n  {len(zone_nodes):6} zones\n  {len(flows):6} flows, {flows[weight].sum():.0f} in all")

# 4. Proximity: every building and its five nearest.
if len(buildings) > 5:
    centres = buildings.set_geometry(buildings.centroid)
    _, near = c2g.knn_graph(centres, k=5)
    print(f"proximity\n  {len(near):6} links between neighbouring buildings")

# With PyTorch Geometric installed (pip install "city2graph[cpu]"), any of
# these becomes a tensor graph:
#     data = c2g.gdf_to_pyg(nodes, edges)
