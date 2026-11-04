# citysim as an MCP server

`opencity-cli mcp` runs a local [Model Context Protocol](https://modelcontextprotocol.io)
server on stdin and stdout. AI assistants such as Claude Code, Claude Desktop
or any other MCP client can then build a city, run it, read its state and
change its research, taxes and policies.

The server keeps one city open at a time. It runs the same simulation as the
game, without a window.

## Adding it

**Claude Code.** Run this from the folder you unpacked (it holds `opencity-cli` and `assets/`):

```sh
claude mcp add citysim -e OPENCITY_DATA="$PWD/assets" -- "$PWD/opencity-cli" mcp
# or open a saved city at start
claude mcp add citysim -e OPENCITY_DATA="$PWD/assets" -- "$PWD/opencity-cli" mcp --load "$PWD/town.ocsave.json"
```

From a source checkout, build first with `cargo build --release -p opencity-cli` and use
`target/release/opencity-cli`. Check that the server is connected with `claude mcp list`, or
with `/mcp` inside Claude Code.

**Claude Desktop and other clients.** Add an entry to the client's MCP configuration
(`claude_desktop_config.json` for Claude Desktop):

```json
{
  "mcpServers": {
    "citysim": {
      "command": "/path/to/citysim/opencity-cli",
      "args": ["mcp"],
      "env": { "OPENCITY_DATA": "/path/to/citysim/assets" }
    }
  }
}
```

`OPENCITY_DATA` (or `--data-dir`) points at the `assets/` folder. The server needs it for
the rules sets, tech trees, policy files and model sizes.

## Tools

| Tool | What it does |
|---|---|
| `new_city` | Generates a new map, replacing the open city. Arguments: `width`, `length`, `seed`, `relief`, `water`, `shape`, `trees`; `flat` for an empty flat map; `rules` (`glassbox`, `pre-industrial`, or none for classic OpenCity); `demo` for the demo town. |
| `load_city`, `save_city` | Opens or writes a `.ocsave.json`. |
| `city_summary` | Date, funds, population, zones and power. Under GlassBox rules it adds demand, resources (produced, delivered, unmet, pools), research, tax rates and stances. |
| `simulate` | Runs forward by `days`, `months` and/or `years` (30 days by default, 50 years at most per call), then returns the summary. |
| `list_tools` | The building tools these rules offer, and the research a locked tool waits for. |
| `apply_tool` | Uses a tool from `from` to `to` (tiles as `[w, l]`). Zones and roads span the area. Plants and services go at `from` and turn to face the street. `density` sets the zone density (0 to 2). Costs money, like in the game. |
| `inspect_tile` | What is on a tile: structure, level, status, size and facing. Under GlassBox rules it adds the building, what it makes and needs, its satisfaction, and the map layers there. |
| `turn_building` | Turns a plant or service a quarter clockwise. |
| `research` | Lists projects with their state (done, in progress, available, locked, ruled out) and progress. `project` starts one; an empty string stops it. |
| `policies` | Lists taxes and policies. `taxes` (`{"property": 12}`) and `stances` (`{"growth": "open_city"}`) change them. |
| `render_map` | Writes a top-down PNG of the city. |
| `export_geojson` | Writes the city as GeoJSON. |

Tool ids for `apply_tool` include `residential`, `commercial`, `industrial`, `road`, `highway`,
`paved_street`, `dirt_road`, `power_line`, `coal_plant`, `nuclear_plant`, `park`, `trees`,
`fire_dept`, `police_dept`, `hospital`, `school` and `bulldozer`. `list_tools` gives the exact
set for the open city.

Tiles are `[w, l]`, counted from the north-west corner. Generated maps carry trees and slopes:
clear them with `bulldozer`, or start with `flat: true`.

## An example session

> Make a flat 48×48 modern city. Lay a street grid with residential, commercial and industrial
> zones and a coal plant, run it for two years, and tell me what holds it back. Then try a 12 %
> property tax with the open city policy, and compare.

The assistant calls these tools in turn: `new_city` (`flat`, `rules: "glassbox"`), several
`apply_tool` calls, `simulate` (`years: 2`), `city_summary`, `policies`, `simulate` again,
and `city_summary`. The summaries show what changed.

## Region server

`opencity-cli region-server` is a separate service. It is not part of MCP:
it hosts a region whose towns are played by different people. See
[PLAYING.md](PLAYING.md#playing-a-region-with-others). Its HTTP routes are
`GET /region`, `POST /claim {slot, player}` (which returns a token) and
`PUT /city {token, summary}`.

## Protocol

- **Transport:** newline-delimited JSON-RPC 2.0 over stdio. Logs go to stderr only.
- **Methods:** `initialize` (the server answers with the client's protocol version), `ping`, `tools/list` and `tools/call`.
- **Tool results:** returned as JSON text. A failure (a tool refused, an unknown argument, no city open) comes back as a tool result with `isError: true`, so the assistant can read why and try again.
