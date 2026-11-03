# GlassBox rule language

citysim's rules can be written in the rule language of **GlassBox**, the
simulation engine Maxis built for SimCity (2013), extended with the metrics
and agent rules of the **Extended GlassBox** model for urban digital twins.
Scripts (`.gbx`) are listed in a rules file and compiled into it when the
game or the CLI loads it:

```toml
# assets/rules/glassbox.toml
scripts = ["glassbox.gbx"]
```

The shipped [`assets/rules/glassbox.gbx`](../assets/rules/glassbox.gbx) adds
metrics, traffic pollution, land value and a development rule to the default
rules. The TOML file still declares the units (their models), the zones and
densities, and the grid resources. Scripts describe behaviour.

## References

* Andrew Willmott, *Inside GlassBox*, GDC 2012 —
  <https://www.andrewwillmott.com/talks/inside-glassbox>; examples of the rule
  syntax are reported in *GDC 2012: Breaking down SimCity's GlassBox engine*,
  Game Developer — <https://gamedeveloper.com/design/gdc-2012-breaking-down-em-simcity-em-s-glassbox-engine>.
* Andrea Roberta Costagliola, Marco Montanari, Paolo Bellavista,
  *Interconnecting Urban Networks: a Novel Approach to Digital Twins through
  GlassBox Adaptation*, University of Bologna —
  <https://cris.unibo.it/bitstream/11585/1033948/2/134951.pdf>. It adds
  **metrics** (values valid for a timeframe), `log -timestamp=…`, `appliesTo …
  via … using …` and `agentRule` with `area`.

The original GlassBox grammar was never published in full. This
implementation follows the published examples. Statements marked *citysim*
below are additions.

## Rules

Each rule is a block from `<kind> <name>` to `end`. `#` and `//` start comments.

### `unitRule` (and `rule`): what a unit does

```text
unitRule mustardFactory
    appliesTo factory_1                    # unit id; defaults to the rule name
    rate 10                                # runs every 10 ticks
    global Simoleans in 1                  # paid from the treasury
    local YellowMustard in 6               # needed by the unit
    local BottleOfMustard out 1            # produced
    agent Goods out 2                      # produced and sent by agents
    map Pollution out 5 radius 3           # emitted into a map
    map LandValue atLeast 2                # condition on the map at the unit
    log -timestamp=Time Shipments 1        # metric
    successEvent effect smokePuff          # accepted, ignored
end
```

| Statement | Meaning | Source |
|---|---|---|
| `appliesTo <unit> [via <agent>] [using <network>]` | Unit the rule belongs to; `using` sets the network for its agents | Extended |
| `options -sendTo <x> -via <y> -using <network>` | Same network option, Maxis form | Maxis |
| `rate <n>` | Runs every *n* ticks. Amounts are averaged per tick (amount / *n*). | Maxis |
| `local <R> in\|out <n>` | Needs / produces resource *R* | Maxis |
| `global <R> in\|out <n>` | Paid from / into the treasury (*R* becomes a `city` resource) | Maxis |
| `agent <R> in\|out <n>` | Received / produced and sent by agents on the network (*R* becomes an `agents` resource) | Maxis, Extended |
| `<R> in\|out <n>` | Unscoped resource: local | Maxis |
| `map <M> out\|in <n> [radius <r>]` | Emits into / takes from map *M* around the unit | Maxis (`radius`: citysim) |
| `map <M> atLeast\|atMost\|greater\|less\|is <n>`, `test map …` | Condition on *M* at the unit. Each failed condition halves satisfaction. | Maxis (`atMost`, `less`: citysim) |
| `log [-timestamp=Time] <metric> <n>` | Adds *n* to a city metric every tick, scaled by satisfaction | Extended |
| `successEvent …`, `onFail …` | Presentation; ignored | Maxis |

### `mapRule`: what happens on every tile

```text
mapRule growGrass
    rate 200
    map Soil atLeast 20          # conditions
    map Water in 10              # taken from the tile, if available
    map Grass out 5              # added to the tile
end
```

### `zoneRule`: development

```text
zoneRule developHouses
    appliesTo residential        # zone id (any zone when omitted)
    timeTrigger Day 2            # or rate: every 2 ticks
    sample random -count 3       # plots tried each time
    test global Builders greater 5
    test map Forest is 0
    createUnit -id house_1
end
```

`test global <X>` reads last tick's production of resource *X*, or else the
metric *X*. Zone rules run alongside the demand-driven growth of the rules
file. Set `base_demand` to 0 and leave a density's demand terms empty to let
zone rules alone decide what gets built.

### `agentRule`: what moving agents do

```text
agentRule carPollution
    condition resource=Car                         # agents carrying Car
    repeatAfter 10                                 # every 10 tiles travelled
    area -timestamp=Time -map=AirQuality pm10 10 5 # map.measure, amount, radius
    area -timestamp=Time -map=NoiseMap noise 8 10
end
```

`-map=AirQuality pm10` writes the map `AirQuality.pm10`.

## Declarations

Resources and maps a script uses but the rules file does not declare are
created automatically:
* An `agent` resource travels by agents on the `using` network (default `road`).
  Network names are normalised, so `RoadNetwork` and `Road` both mean `road`.
* A `global` resource goes through the treasury.
* Other resources stay local.
* New maps get default diffusion, decay and colour. Declare them in the TOML
  file to tune them, as the shipped rules do for `AirQuality.pm10` and `Noise.level`.

## Metrics

Metrics are kept per tick with their timestamp (the tick number), along with
every other engine series (last 2000 ticks):
* **Game:** the City panel shows the latest values, and **GlassBox graphs** (`G`)
  plots and analyses them (see [RULES.md](RULES.md#playing)).
* **CLI:** export all series (metrics as `Metric/<name>` columns) to compare a simulation with measured data, the validation
  loop the Extended GlassBox paper proposes:

```sh
opencity-cli simulate --demo --game-rules glassbox --years 5 --metrics metrics.csv
```

## Differences from GlassBox

* Agents deliver within the tick they are sent. Senders are chosen by
  straight-line distance, and visible vehicles follow real road paths.
* `{type}` placeholders expand over the kinds declared in the rules file
  (`[[resource.subtype]]`, see [RULES.md](RULES.md#the-model)), one line per kind.
  Kinds are declared in TOML, not in the script.
* Rules with `rate` are averaged per tick instead of firing in bursts.
