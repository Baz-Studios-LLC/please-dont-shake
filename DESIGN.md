# Please Don't Shake

A 3D ant farm. You feed it, you tap it, and you are asked not to shake it.

---

## What it is

An ambient sandbox in the mould of David OReilly's *Mountain* — $0.99, one screen, no
menus inside the world, something you leave running rather than sit down to play. The
difference is what's underneath: a genuine ant colony simulation, accurate enough that
the colony's behaviour *is* the storytelling.

The tank is a side-view formicarium — a thin slab of sand between glass — filling the
vast majority of the screen. The camera never moves.

### The loop

> shake → chaos → **watch them patiently rebuild**

The rebuild is the payoff. It's what keeps a destructive verb inside a cozy game.

### Why the title has teeth

The tunnels are the accumulated history. Every chamber, branch and abandoned dead-end
was authored by the ants themselves through stigmergy — nothing is procedural, nothing
is placed by us. At hour 40 the farm looks nothing like hour 1, and it is uniquely the
player's.

So shaking is not about killing ants. **It is about erasing the record of their work.**
That's the whole emotional mechanism. The player is a vandal, not a murderer.

---

## Locked decisions

| | |
|---|---|
| **Tone** | Cozy ambient sandbox. No fail state, no meters, no UI panels. |
| **Reference** | *Mountain*. $0.99, impulse buy, lives in a window. |
| **Time** | Real time over days. Colony clock ≈ **1 real hour : 1 colony day**. Sand and ant motion stay real-time; only biology compresses. |
| **Offline** | Paused when closed. You never return to a disaster you couldn't prevent. |
| **Ending** | Natural lifespan. The queen's sperm reserve runs out, the colony winds down over ~40–60 hours. |
| **Text** | **None inside the tank, ever.** Behaviour is the only channel. |
| **Grimness** | Full biological accuracy, presented plainly. No music sting, no camera push. It's just what happens. |
| **Scale** | Hundreds of ants (200–500) at peak. Architected SoA so thousands is a config change. |
| **Art** | Chunky stylised. Everything procedural, no art assets. |
| **Species** | *Lasius niger* first, shipped complete. Others unlock via achievement, as free updates. |

### The three verbs

| Verb | Input | Effect |
|---|---|---|
| **Feed** | drop food | A forager finds it, recruits, and it propagates through the colony by trophallaxis. |
| **Tap** | click | Local impulse at one point on the glass. Ants nearby freeze, then investigate. Brief local alarm pheromone. A few grains trickle. No structural damage. |
| **Shake** | click-drag | Global. Cohesion collapses, tunnels cave, alarm floods the grid, the colony goes into genuine defensive frenzy and then chemically calms over minutes. |

Tap and shake are one continuous gesture, deliberately. **The game asks you not to
shake. It says nothing about tapping.** So you tap. And tapping is a little bit fun. The
entire psychological content of the game lives on the slope between those two verbs.

Tapping is also biologically legible: ants sense substrate vibration through subgenual
organs in their legs, and several species stridulate to alarm each other. You are
speaking to them in a channel they genuinely have. They respond, correctly, by treating
you as a predator.

### The voice

The tank never speaks. Everything outside the tank can.

Steam achievements are therefore the *only* voice, and they carry the dry humour the
title promises — free, external, optional. "Thank You" for reaching natural senescence
without ever shaking. Something considerably less warm for the opposite. Real
biological milestones for the rest: first eclosion, first midden, a tunnel reaching the
tank floor, alates produced.

---

## The simulation

### Sand — a cohesion-driven cellular grid

The grid (256 × 160 cells) is ground truth. Cells are `Air`, `Sand`, `Food`, `Stone`,
`Glass`; `Water` is reserved and switched on in M2.

A single scalar does the heavy lifting. Each sand cell computes a **stability** score
from its neighbours, weighted so support from below counts most:

```
stability = 3·below + 2·(below-left + below-right) + 1·(left + right)   →  0..9
```

A cell falls when `stability < required`, and `required` rises with **agitation**:

- **Agitation 0** — `required` is 1. A cell with any neighbour at all holds. Vertical
  walls stand, tunnel ceilings hang, architecture persists indefinitely.
- **Agitation 1** — `required` is ~7. A cell needs to be solidly buried to stay put.
  Overhangs fail, piles slump, tunnels cave in.

That's the entire chaos mechanic in one variable. Agitation is a coarse field, not a
global, so a tap disturbs one region while a shake floods all of them.

One refinement the first build forced. `stability` is an integer, so with a uniform
threshold every cell scoring the same number fails at the same instant — a tap either
did nothing at all or dropped an entire ceiling in one tick, with no middle ground.
Each cell now carries a small fixed **cohesion jitter**, so grains differ in how well
they've packed, exactly as real sand does. Ceilings shed a *fraction* of their grains
as agitation climbs and the survivors settle into an arch. This is what makes the
gradient from tap to shake continuous rather than binary.

Determinism matters (the farm has to serialize across days), so randomness is a cheap
positional hash of `(x, y, tick)` rather than an RNG. The scan runs bottom-row-first so
each cell moves at most one step per tick, with the x-direction alternating per tick to
kill directional bias.

### Idling at near-zero CPU

Non-negotiable, because the game lives in a window for hours. The grid is divided into
chunks that **sleep** when nothing in them moved and nothing crossed their border.
Settled sand costs nothing. Cheap to build in now, miserable to retrofit — hence M1.

### Ants — what "real" means

Not scheduled for M1, but the grid is built to carry it.

- **Temporal polyethism.** Labour divides by *age*, not assignment. Young workers nurse
  brood deep inside; middle-aged ones dig and maintain; the oldest forage at the
  surface, because foraging is the dangerous job and you spend the ants who are nearly
  dead anyway. Falls out of one `age` float. The ants nearest the glass are the old ones.
- **Trophallaxis.** Food is not a stockpile. A forager fills its crop, walks home, and
  feeds nestmates mouth-to-mouth, who feed nestmates. Nutrition diffuses through a
  social network, so one dropped seed visibly ripples through the colony over an hour.
- **Four pheromone fields**, sharing the sand grid's coordinates:
  - *Trail* — recruitment, evaporating, so routes to spent food fade on their own.
  - *Alarm* — released on injury or disturbance. This is what the shake lands on.
  - *Queen* — suppresses worker reproduction and signals she lives. When she dies it
    fades and the colony *knows*. The ending is chemical, not scripted.
  - *Necromone* — oleic acid on corpses triggers undertaking; the dead are carried to a
    midden.
- **Stigmergy in digging.** Ants dig where digging is already happening and drop spoil
  where spoil already is. Architecture is emergent from local rules — this is the single
  most important thing to get right.
- **Functional nest structure.** Brood chambers at the right depth for temperature and
  humidity, refuse at the periphery, queen deep. Nurses relocate larvae vertically as
  surface conditions change across the day.
- **Colony demography.** Claustral founding (the queen sealed in, metabolising her own
  flight muscles to raise the first brood) → ergonomic growth → reproductive. Alates
  hatch, swarm, and die against the glass, because there is nowhere to go.
- **Most ants are idle most of the time.** Real colonies keep a large inactive reserve.
  If every ant is busy it reads as a factory, not a colony.

---

## Milestones

**M1 — The Toy** *(built)*
Sand, glass, tap, shake. No ants. Ends with something you can put your finger on.

Verified by `--capture`, a scripted dig → wait → tap → shake run. Cells moved is
cumulative against the nest as first carved, so each row includes the ones above it:

| stage | cells moved vs dug | sand total |
|---|---|---|
| carved, settled | 32 | 22596 |
| …then left alone 6 seconds | 32 — **no change** | 22596 |
| one tap on the glass | 210 | 22596 |
| moderate shake | 1424 | 22596 |
| violent shake, settled | 4706 | 22596 |

Two rows carry the milestone. Six seconds of calm produced a **byte-identical frame** —
architecture persists indefinitely, which is what lets the farm accumulate a history at
all. And sand is conserved *exactly* at every stage, including mid-shake with grains in
flight; a farm leaking even a few grains per shake would quietly empty itself over the
days this game is meant to run for.

The same run in debug and release agrees to the cell through the tap and moderate shake,
which is the check that shake damage doesn't scale with frame rate.

1. Bevy 0.19 project, fixed camera, chunky lighting on a glass tank
2. `SandGrid` resource, layered strata, `FixedUpdate` for determinism
3. Falling-sand CA with cohesion as the master variable
4. Grid → chunked mesh, dirty-rebuild only
5. Glass pane — transparency, specular
6. Tap and drag-to-shake: tank spring, agitation field, collapse
7. Loose grain particles, ejecting and reintegrating
8. A debug brush, so you can dig a cathedral by hand and then bring it down

**M2 — The Colony.** Ants, pheromones, stigmergic digging, brood, trophallaxis. Water.

**M3 — The Arc.** Full demography through to senescence. Persistence across days.

**M4 — The Shelf.** Steam plumbing, achievements, species unlocks.

---

## Tech

Rust + Bevy 0.19. Target: macOS/Windows desktop, mouse only.

### Keep input a thin adapter

Nothing in the simulation may know what a mouse is. Input's entire job is to write two
things: `agitation` on the sand grid, and `Alarm` pheromone. That's the whole contract.

The reason is a possible iPad build, where this game arguably belongs more than on
desktop: the accelerometer means you'd shake *the actual object in your hands*, and tap
would be a finger on the actual glass — two genuinely distinct physical acts rather than
one mouse gesture doing double duty. A 4:3 screen also frames a side-view formicarium
better than 16:9 does, and "paused when closed" stops being a design choice and becomes
simply how the platform works.

Bevy runs on iOS but it's a far less trodden path than desktop, so this is not a
commitment — it's a door held open for the price of one architectural rule.
