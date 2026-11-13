# GeoJSON import and export

`opencity-geo` builds a city from map data, so a city can come from
OpenFantasyMap (OFM), OpenHistoryMap or OpenHistoricalMap (OHM), or plain OSM.

## Input

* GeoJSON: `FeatureCollection`, `Feature`, any geometry including `Multi*`
  and `GeometryCollection`. Tags may be flat in `properties` or nested in
  `properties.tags`, as osmtogeojson writes them.
* Overpass / OSM JSON queried with `out geom;`. Closed ways become polygons
  unless they are linear features. Multipolygon relations use their closed
  outer and inner members.

## Coordinates

* **WGS84** longitude/latitude. A local equirectangular projection is used
  at 16 m per tile by default.
* **Planar** map units, for worlds that make no Earth assumption (OFM). The
  data is fitted to about 96 tiles unless `--units-per-tile` is given.

`--crs auto` (the default) honours the GeoJSON `crs` member. Without one, it
assumes WGS84 when the coordinates fit within two degrees.
The city never exceeds `--max-size` tiles a side. The scale is coarsened to fit.

## Rules

Feature properties are mapped to buildings by a TOML rules file. The
built-in file, [`default_rules.toml`](../crates/opencity-geo/src/default_rules.toml),
covers OSM/OHM tags. The first matching rule wins. A rule matches when every
listed key is present with one of the listed values (`*` = any).

```toml
[[rule]]
when = { kind = ["market", "tavern"] }
build = "commercial"
```

Build kinds: `residential`, `commercial`, `industrial`, `road`,
`power_line`, `coal_plant`, `nuclear_plant`, `park`, `trees`, `water`,
`fire_station`, `police`, `hospital`, `school`, `high_school`, `university`, `ignore`.
`build = "from:<property>"` reads the kind from a property, which is how
exported files re-import losslessly.

The same file names the properties used for elevation (`ele`), for history
(`start_date`, `end_date`) and for floors (`building:levels`). Floors set
the initial zone level.

## History

`--date 1850` (or `1850-06`, `1850-06-12`, `-0300` for 301 BCE) keeps the
features whose `start_date <= date < end_date`. A missing bound is open.

## Terrain

* Elevation comes from features carrying `ele`: points, or every vertex of
  contour lines. It is interpolated by inverse distance weighting and turned
  into steps of `--metres-per-step` metres (8 by default), from 0 to 10.
* Water polygons and rivers are lowered below sea level.

## Placement order

Water, roads, power lines, power plants, public services, then zones from
the smallest to the largest polygon. Buildings therefore come before the
land use around them. Parks and trees come last. Multi-tile blocks (plants,
services) are centred on their feature and nudged up to 4 tiles to find
room. The import report lists, per kind, how many tiles were placed or
refused. Tiles are refused when occupied, sloped or under water.

## Export

`export-geojson` writes every structure with OSM-style tags plus
`opencity:build` and `opencity:level`. It also writes water tiles and an
`ele` point per terrain corner unless `--no-terrain` is given. Coordinates
use the georeference recorded at import. Cities that were not imported get a
planar grid with one unit per tile.

## Fetching OpenHistoricalMap data

```sh
docker run --rm -u 0 -v "$PWD/data":/out curlimages/curl:8.11.1 -sS -o /out/area.json \
  --data-urlencode 'data=[out:json][timeout:150];(way["highway"](S,W,N,E);way["building"](S,W,N,E);way["landuse"](S,W,N,E);nwr["amenity"](S,W,N,E););out geom;' \
  https://overpass-api.openhistoricalmap.org/api/interpreter
```
