# Porting notes

Source: `svn export https://svn.code.sf.net/p/opencity/code/trunk` (OpenCity
0.0.7dev, about 20k lines of C++ plus 12k lines of vendored TinyXML/TinyXPath).

## Where things went

| Legacy C++ | Rust |
|---|---|
| `enum.h`, `enum/*.h` | `opencity-core::codes` (`StructureCode`, `GraphicCode`, `Tool`, `Direction`) |
| `map.cpp` (`Map`) | `opencity-core::terrain::Terrain` |
| `layer.h`, `buildinglayer.cpp` | `opencity-core::layer::BuildingLayer` |
| `structure/*.cpp` | `layer::Structure` + `layer::TileKind` |
| `simulator/*.cpp` | `opencity-core::sim` (plain functions) |
| `pathfinder.cpp`, `vehicle.cpp`, `movement*.cpp`, `destination.cpp` | `opencity-core::traffic` |
| `city.cpp` (rules, budget, calendar) | `opencity-core::city::City` |
| `city.cpp` (save/load), `persistence.h` | `opencity-core::save` (JSON + legacy importer) |
| `propertymanager*.cpp`, `property.cpp`, `conf.cpp` | `opencity-assets::catalog`, `opencity-core::codes` (costs) |
| `model/ac3d*.cpp`, `model/modelloader.cpp` | `opencity-assets::ac3d` |
| `mapgen/**` | `opencity-mapgen` |
| `renderer.cpp`, `graphicmanager.cpp`, `texture.cpp` | `opencity::render` (macroquad) |
| `gui*.cpp`, `city.cpp` (GUI parts) | `opencity::ui`, `opencity::main` |
| `audiomanager*.cpp` | `opencity::audio` |
| `main.cpp`, `SimpleOpt.h`, `globalvar.*`, `binreloc` | `clap` arguments, `Settings`, `find_data_dir` |
| `tinyxml/`, `tinyxpath/` | `roxmltree` (model metadata only) |
| `config/opencity.xml` | `~/.config/opencity/settings.toml` |

## Structural changes

* **No globals, no threads, no mutex.** The legacy simulators ran in an SDL
  thread sharing raw pointers with the renderer under one mutex. The Rust
  simulation is a set of functions called from `City::update` on a
  simulated-time schedule. With the same seed, a city evolves the same way
  every time, which makes it testable.
* **No pointer graph.** Each tile owns its structure or records the origin
  of the multi-tile building it belongs to.
* **Road and power line pieces** come from a 16-entry connection table
  instead of the 300-line state machine in `PathStructure`.
* **Pathfinding** is Dijkstra over road tiles (cost `1 + traffic` for cars,
  `1` for pickups) instead of re-sorting a vector every step.
* **Rendering** uses vertex buffers through macroquad instead of
  fixed-function OpenGL display lists. Lighting is baked into vertex colours.
  The 3D terrain texture is reduced to a per-height palette sampled from the
  original texture.

## Behaviour that intentionally differs

* Random structure picks are uniform among matching structures. The legacy
  code gave up after 300 random probes, which failed on sparse maps.
* Power plants are found by scanning the map on every power update. The
  legacy list kept stale entries when a rectangle destroy missed a plant's
  first corner.
* When a growing zone swallows a bigger neighbour, the neighbour's leftover
  tiles are re-zoned over its real footprint. The legacy rectangle was offset.
* Pausing stops the simulation as well as the calendar.
* There is an extra fast speed (x4).
* Generated maps use a seeded PCG generator, so a given seed does not
  reproduce the exact C++ `rand()` terrain.

## Not ported (yet)

* The multi-agent system (`mas/`: police, demonstrator and robber agents).
* Networking and the ZeN server (`networking/`, `zen.cpp`). It was experimental.
* The model viewer (`viewer/`) and the autopackage and Windows build scripts.
* Background music. The trunk ships no music files.
* Concave polygons in AC3D models are fan-triangulated. The legacy code had
  its own triangulation module. No shipped model shows artefacts.
