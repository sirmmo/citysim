# Origins and credits

**citysim** is a Rust rewrite of **OpenCity**, a free 3D city simulation
written in C++/SDL/OpenGL by Duong Khang NGUYEN ("neoneurone"), Frédéric
RODRIGO and contributors between 2003 and 2015.

* Original project: <https://sourceforge.net/projects/opencity/> (former home: http://www.opencity.info)
* Source this port starts from: `svn export https://svn.code.sf.net/p/opencity/code/trunk`,
  revision 480 (last changed 2015-08-02), version 0.0.7dev.

## What comes from OpenCity

* **Game design and rules.** Residential, commercial and industrial growth
  conditions and their ranges, power propagation, traffic load, costs, taxes,
  calendar and the map generator are ported from the C++ sources. See
  [docs/PORTING.md](docs/PORTING.md) for the file-by-file mapping and the few
  deliberate behaviour changes.
* **All artwork and sounds.** Everything under `assets/` (AC3D models,
  textures, GUI images, heightmaps, sound effects) is OpenCity's, unchanged.
  Most models and textures are by Frédéric RODRIGO.
* **Save files.** Legacy OpenCity save files can still be imported.

The original authors and contributors are listed in [AUTHORS](AUTHORS) and
[docs/OPENCITY-COPYRIGHT.txt](docs/OPENCITY-COPYRIGHT.txt).

## What is new

The Rust code base: crate layout, deterministic single-threaded simulation,
macroquad renderer, TOML settings, headless CLI, GeoJSON import/export
(OpenFantasyMap, OpenHistoryMap / OpenHistoricalMap) and the Docker-based
build.

## License

OpenCity is GPL version 2 or later, and so is this derivative work
([COPYING](COPYING)). The bundled OpenHistoricalMap sample in `data/` is CC0.
