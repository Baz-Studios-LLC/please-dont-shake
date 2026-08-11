# Please Don't Shake

A 3D ant farm that asks you not to shake it.

An ambient sandbox in the mould of *Mountain* — one screen, no menus inside the world,
something you leave running rather than sit down to play. The difference is what's
underneath: a genuine ant colony simulation, accurate enough that the colony's behaviour
*is* the storytelling. There is no text inside the tank.

The tunnels are the accumulated history. Every chamber and abandoned dead-end was dug by
the ants themselves through stigmergy — nothing is procedural, nothing is placed by us. So
shaking isn't about killing ants. It's about erasing the record of their work.

See [DESIGN.md](DESIGN.md) for the full design and the locked decisions.

## Running it

Double-click `Please Don't Shake.command`, or:

```bash
cargo run --release
```

| input | |
|---|---|
| click | tap the glass |
| click and drag | shake the tank |
| right-drag | dig by hand (debug — the ants' job) |
| shift + right-drag | fill sand back in |
| F12 | screenshot |

## Where it's up to

**M1 — the toy.** Done. Sand, glass, tap, shake, no ants.

The sand is a cellular grid where one variable does the work: each cell scores how well
it's held in place, and falls when that score drops below a threshold that rises with
local **agitation**. Calm, and vertical walls stand indefinitely. Shaken, and only solidly
buried grains hold — the mass survives, the architecture doesn't.

Verified: six seconds of calm produce a byte-identical frame, and sand is conserved
exactly through a violent shake with grains mid-flight.

**M2 — the colony.** In progress. Ants exist, walk, and dig emergent tunnels; pheromone
fields and a navigation flood are in; tap and shake now land on the colony's alarm
response. Digging and hauling work end to end and mass stays exact, but spoil logistics
are still inefficient, so the nest grows more slowly than it should.

**M3 — the arc.** Not started. Demography through to senescence, persistence across days.

**M4 — the shelf.** Not started. Steam plumbing, achievements, species unlocks.

## Verification

The sim is verified by scripted runs rather than by eye, because emergent behaviour can't
be judged from a screenshot. Each takes screenshots and reports what actually happened.

```bash
# The colony: dig for 100s, then tap it, then shake it.
cargo run --release -- --capture --out /tmp/shots

# The original M1 sand test, with no colony to disturb the numbers.
cargo run --release -- --capture --sand-only --out /tmp/shots

# One frame of the title screen.
cargo run --release -- --capture --title-shot --out /tmp/shots
```

Runs render to an offscreen texture rather than grabbing the window, so a locked or
sleeping screen doesn't silently produce black frames.

The number that matters most is total sand. It must stay exactly constant across all three
places a grain can hide — in the grid, mid-flight as a particle, or in an ant's mandibles.
A farm that leaked even a few grains per shake would quietly empty itself over the days
this game is meant to run for.

## Tech

Rust + Bevy 0.19. macOS/Windows desktop, mouse only. Input is deliberately a thin adapter
that only writes agitation and alarm — see DESIGN.md for why that matters.
