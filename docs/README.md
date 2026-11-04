# citysim documentation

| Document | What it covers |
|---|---|
| [PLAYING.md](PLAYING.md) | Starting a city, controls, building, research, taxes and policies, overlays, graphs, saving |
| [RULES.md](RULES.md) | The GlassBox rules files: resources, networks, vehicles, layers, zones, units, services, growth, tech trees, taxes and policies |
| [GLASSBOX.md](GLASSBOX.md) | The GlassBox rule language (`.gbx`): `unitRule`, `mapRule`, `zoneRule`, `agentRule`, metrics |
| [SKINS.md](SKINS.md) | Era and world skins: colours, names, tools, icons, models; the 0 A.D. cultures |
| [GEOJSON.md](GEOJSON.md) | Building cities from OpenHistoryMap, OpenHistoricalMap and OpenFantasyMap data, and exporting them |
| [PORTING.md](PORTING.md) | How OpenCity's C++ maps onto the Rust crates, and what changed |
| [settings.example.toml](settings.example.toml) | User settings |
| [../CHANGELOG.md](../CHANGELOG.md) | What changed in each release |
| [../NOTICE.md](../NOTICE.md) | Origins, credits and third-party art |

## Data files at a glance

```
assets/
  rules/<id>.toml        rules sets (index.toml lists them) and their .gbx scripts
  techs/<id>.toml        tech trees, named by a rules file's tech_tree
  policies/<id>.toml     taxes and policies, named by a rules file's policy_set
  skins/                 era and world skins
  models/kenney/         CC0 Kenney car, train and watercraft models (.obj)
  graphism/, sound/ …    OpenCity's original art and sounds
```

Rules, tech trees and policy files are kept apart, so you can copy one rules set and swap its
tech tree or policies, or share one tree between several rule sets.
