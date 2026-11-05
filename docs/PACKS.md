# Packs: one scenario as a game of its own

A **pack** seals a [scenario](PLAYING.md#scenarios) with a skin and a rules
set that the player cannot change. Use one to ship a single campaign, such
as "Bologna 1600" or "Rail Baron", as a game that opens straight into it.

## Playing a pack

```sh
opencity --pack bologna-1600      # assets/packs/bologna-1600.toml
```

or put a `pack.toml` in the data folder (`assets/pack.toml`). The game then
always opens into that pack, without any flag. While a pack runs:

- The window takes the pack's name. A **Game** menu replaces File, with
  *Restart*, *Save*, *Back to the last save* and *Save and quit*.
- There are no new cities, regions, imports or other scenarios, and no
  Skin menu.
- The cheat box (Ctrl+Shift+C) and the Disasters menu are gone, unless
  the pack allows them.
- The game is saved on quit to its own file (`<pack id>.ocsave.json`
  beside the usual `city.ocsave.json`), and the next launch carries on
  from there.

Everything else (tools, research, taxes and policies, budget, goals) works
as in a free game, under the pack's rules.

## Writing a pack

```toml
# assets/packs/my-pack.toml
name = "My campaign"                # window title and menu name
description = "One line for the menu."
scenario = "my-town"                # assets/scenarios/my-town.toml
skin = "renaissance"                # replaces the scenario's; "none" for OpenCity's look
rules = "pre-industrial"            # replaces the scenario's
cheats = false                      # the cheat box (default false)
disaster_menu = false               # chance switch and set-offs (default false)
```

Only `name` and `scenario` are required. The skin and rules default to the
scenario's own. Unknown keys are refused, so a typo is reported rather than
ignored. In `assets/pack.toml` you can add `id = "my-pack"` to name the
save file (by default it is `pack.ocsave.json`).

The tests load every shipped pack and build its city, so a pack that
names a missing scenario, skin or rules set fails the build.

## Shipping a pack

`scripts/make-pack.sh` turns any release into a single-pack game. It
downloads and builds nothing:

```sh
gh release download v0.2.27 -R sirmmo/citysim -p 'citysim-v0.2.27-x86_64-pc-windows-msvc.zip'
scripts/make-pack.sh bologna-1600 citysim-v0.2.27-x86_64-pc-windows-msvc.zip
# -> dist/packs/bologna-1600-citysim-v0.2.27-x86_64-pc-windows-msvc.zip
```

The result is the same game with `assets/pack.toml` and a `PACK.txt` for
the player. Deleting `assets/pack.toml` brings the full game back: a pack
is a way to present the game, not a security lock.

Android reads `pack.toml` from inside the APK too: copy it to `assets/`
before `scripts/android.sh` to build a sealed APK.

## Shipped packs

| Pack | Scenario | Skin | Rules | Disasters menu |
|---|---|---|---|---|
| `rail-baron` | Rail baron | Modern (Kenney) | GlassBox | no |
| `bologna-1600` | Bologna, 1600 | Renaissance | Pre-industrial | yes |
