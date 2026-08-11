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

### The hand

There is no cursor. There is a hand, taken from Divus Factus — the studio already has one,
and it is the right object for a game whose entire subject is what you do with yours.

It carries the verbs on its face:

| | |
|---|---|
| **A fingertip** | reaches out and presses, and the hand leans in after it. This is a tap. |
| **A whole palm** | fingers splay, the hand flattens onto the glass and plants. This is the grab, and from here the tank goes wherever your arm does. |

Which is a better sign than any label. The game asks you not to shake it, and the
difference between one finger and a flat palm is legible *before* you have done either —
so the moment your hand changes shape is the moment you know you've stopped being innocent.
It changes at exactly the pixel the input path stops calling the gesture a tap; if those
ever disagree the hand is lying.

It is the cursor over the menus too, pressing buttons with the same finger. Nothing about
it reads the mouse: it is told whether it is tapping or dragging, which is the same thing
a finger on an iPad already knows.

### One farm, kept

A farm is only ever destroyed by asking for a new one. Going back to the title screen
doesn't end it: the colony carries on digging behind the menu, and **Continue** walks
back into it. The title screen isn't a place the game stops, it's a place you can see it
from — which is the only reading consistent with a game whose whole subject is
accumulated history. A menu that quietly binned forty hours of tunnels would be doing
the thing the player is asked not to.

That is also why there is no **Load**, and never will be. Loading implies slots, and
slots imply the farm is a document you keep copies of. There is one farm. It is the one
in the tank.

It saves itself. Continuously, on a timer, on leaving play and on quitting — and it is
restored before the title screen is drawn, so closing the app and opening it again puts
you back in front of the farm you left with **Continue** waiting. There is no save button
because there is no decision to make: an ambient game that asked you to remember to save
it would be asking the wrong thing of you. The player should never learn that a file is
involved.

So the menu is three entries, and one of them is conditional:

| | |
|---|---|
| **Continue** | Only when a farm exists. Fades the menu away to reveal it still running. |
| **New Game** | Pours a fresh tank. The only thing in the game that discards a farm. |
| **Settings** | Nothing behind it yet, so it's shown dimmed rather than hidden. |

Continue is *absent* rather than dimmed on a first run, because it isn't an unfinished
feature — it's a statement about the farm, and a greyed-out Continue would be claiming a
farm exists when none does.

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

#### Loose and packed: two states, not one threshold

Cohesion alone cannot also produce an angle of repose, and the attempt to make it do
both wasted real time. The top grain of a one-cell spire scores 3 (something directly
beneath it) while a tunnel ceiling hanging by its sides scores 2 — so *any* threshold
low enough to keep ceilings up is also happy to hold a spire. Poured sand and ant spoil
built spindly towers, and the colony climbed its own chimney.

They aren't the same physics, so the model says so. Every grain is **loose** or
**packed**:

- **Loose** — poured, tipped out by an ant, or just knocked off a wall by a shake. No
  cohesion whatsoever. It falls, and failing that rolls diagonally off whatever it's on,
  until neither diagonal is open. That condition *is* a slope of 45°, so heaps come out
  as cones with no angle-of-repose rule written anywhere. The instant it runs out of
  moves it packs.
- **Packed** — the strata the tank was filled with, and anything that has come to rest.
  The cohesion model above, unchanged. Tunnels persist; a settled spoil mound can be
  dug through like any other sand.

A grain that moves becomes loose again, which is why a shake liquefies the surface and
leaves it at repose rather than in the shape it was.

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

### Brood — what the nurses are for

The colony digs and hauls, and a third of it has nothing to do. Nurses correctly go to the
queen and stay with her, which without brood reads as ants milling in a hole. Brood is the
missing half of "behaviour is the storytelling": it is what makes the colony **grow**, what
gives two thirds of the workers a job, and what the ending is an ending *of*.

**Brood are entities, not a substance.** They are carried, tended and moved individually,
they sit in the air of a chamber rather than in the sand, and the queen makes them one at a
time. A grid layer would fight all four of those.

| stage | becomes | what a nurse does with it |
|---|---|---|
| **Egg** | larva | carries it to the deepest chamber it can reach; keeps the pile together |
| **Larva** | pupa | feeds it — this is where food will matter, and where hunger will bite first |
| **Pupa** | worker | nothing; it is left alone until it ecloses |

The pile is the point. Nurses gathering brood into one heap in the deepest chamber is a
*visible* behaviour nobody authored, and it is the first thing in the game that makes the
nest look inhabited rather than excavated.

#### The timescale problem, and the decision

Real *Lasius niger* takes something like seven or eight weeks from egg to worker. At the
locked clock of **1 real hour : 1 colony day** that is fifty-odd hours — and the locked
lifespan is 40–60 hours. A real colony lives through hundreds of brood cycles; ours would
live through *one*, and the player would watch a single cohort arrive shortly before the
queen's decline.

So the brood cycle compresses, and this is a deliberate departure from the accuracy the rest
of the simulation holds to: **egg to worker in about six colony days**, so a farm left running
overnight shows several cohorts and a population that visibly climbs and then falls. The
alternative — keeping brood honest and stretching the lifespan to weeks of real time — is a
different game, and not the one in the rest of this document.

Recorded here rather than buried in a constant, because it is the one place the simulation
knowingly lies, and anyone who finds the number later deserves to know it was a choice.

#### What it takes

- `Brood { stage, age_days, carried_by }` as a component; the queen lays on a timer.
- Nurses gain two behaviours: fetch a loose egg, and settle it on the pile. The `Queen`
  pheromone already marks where "deep and safe" is.
- Eclosion spawns a worker at `age_days = 0`, which the existing `Job::for_age` picks up with
  no changes — the labour split has been waiting for this.
- Population over time becomes the number worth reporting from the harness, the way mass and
  excavation are now.

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
