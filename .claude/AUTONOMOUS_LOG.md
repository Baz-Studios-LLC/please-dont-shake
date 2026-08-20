# Autonomous Development Log

## Active Goal

Improve the coupled ant-colony and sand simulation until it is internally
correct, biologically and physically believable at the game's chosen level of
abstraction, and visibly alive over long-running play, while preserving every
locked decision in `DESIGN.md`.

Current phase: improve spoil logistics and congestion without increasing the
colony's locked real-time excavation pace merely to make short sessions busier.

## Acceptance Criteria for the Current Phase

- Sand drift remains exactly zero in every capture stage.
- Excavation continues to progress at mature colony size rather than flatlining.
- Excavated material reaches the outside mound reliably; read excavated and
  mound volume together.
- Spoil dropped inside the nest and spoil falling back into it are materially
  reduced across repeated release-mode captures.
- Improvements hold near one hundred ants, not only during the small founding
  colony.
- Brood care, queen behavior, alarm recovery, digging, and hauling do not regress.
- The solution remains emergent and stigmergic. It must not prescribe a nest
  layout, assign individual routes centrally, or violate the colony clock.
- Loose and packed sand retain their distinct behavior: natural slopes above
  ground and indefinitely stable calm tunnels below it.
- Tap remains local, shake remains agitation-driven and structurally meaningful,
  and neither response accidentally depends on frame rate.
- Spoil improvements arise from the coupled hauling and sand-settling behavior,
  not by making carried grains ignore the physical sand model.

Do not invent an arbitrary zero-stall target. Appropriate ant idleness is part
of the design.

## Baseline Evidence

**The quoted baseline is not usable and has to be re-measured.** The numbers below came off
capture runs at a colony day a *second*, and that speed was removed on 2026-08-12 because it is
not faithful: biology ran 86,400x while ants dug and walked at real-time pace, so a digger took
4.9 cells of walking per cell dug where a real one takes ~28,000. A colony that hollows out
where it stands has different logistics from one that tunnels, and congestion is precisely a
logistics measurement. Treat these as describing a farm that never existed:

- 175 cells excavated
- 180 cells fell back into the nest, about 45% of everything dug in that run
- 83 drops occurred inside the nest
- sand drift remained zero

### The instrument problem, and the fixture built for it

The 125-second capture cannot reach the regime this phase is about. At the fastest honest speed
(a colony day an hour, the derived ceiling in `SPEEDS`) it digs **one cell** with eleven ants in
the tank. Growing a colony to a hundred at that speed would take ~35 hours, since the population
converges on laying rate x lifespan over many brood cycles.

So `--congestion` was added: it tips in ant kits through the game's own stocking path until the
tank holds `--ants`, then runs at 24x for `--minutes`, printing the full spoil ledger every
minute. Nothing in it spawns an ant directly or touches the colony clock beyond the honest
ceiling, so the fixture cannot disagree with what the radial menu does.

Two limitations, recorded because a fixture that hides them is worse than none: the colony is
**seeded rather than grown**, so it skips the demographic ramp; and the nest is therefore younger
than the colony working it. What it measures faithfully is the question actually asked — of the
grains this colony digs, where do they end up.

The known mechanisms are `HAUL_PATIENCE` dropping a grain where a blocked ant
stands and the spoil mound's inner slope rolling loose grains back toward the
shaft. The deeper cause may be unconditional digging producing spoil without
regard for nest need. Treat that as a hypothesis to test, not a conclusion to
code blindly.

## Completed

- Autonomous workflow initialised for the coupled ant-and-sand simulation
  experiment.
- `--congestion` fixture: a long run at the honest 24x that reaches a hundred ants and prints
  the spoil ledger. Built before any change, because the phase's acceptance criteria ask for
  evidence near a hundred ants and no existing instrument could produce it.

## Validation

- Baseline run in progress: 100 ants, 60 minutes at 24x, from a *snapshot* of the release
  binary rather than `target/release` directly.

### Two harness lessons from setting the baseline up

**Never compile while a timed run is in flight.** The congestion run is paced by real time, and
`Time::<Virtual>` clamps how much time one frame may deliver, so a machine busy with a release
build falls behind: the sim ticks fewer times per real second and the colony digs less per
reported minute. The ledger would be measuring the compiler. Copy the binary out
(`scratchpad/bin/`) and run *that*, so later builds cannot disturb it either — a background
`cargo build` and a foreground `cargo check` also deadlock on the same target-dir lock, which
silently held the first baseline attempt at zero progress for several minutes.

**Digging has a long warm-up now.** A worker's initial `dig_cooldown` is a random fraction of
`DIG_INTERVAL`, which de-synchronised first bites nicely when the interval was 0.45s and now
spreads them over a full 30,000-second interval — twenty minutes of a 24x run. The first minute
of any congestion run therefore reads `dug 0`, which is correct rather than broken. Discount the
warm-up when reading a ledger, or compare only the final lines.

## Findings this cycle

### The colony was largely paralysed, and it was the labour change that did it

Measured at 110 ants with the congestion fixture — and note that **stalling is a locomotion
measurement, so it needs scale rather than colony-days**: a six-minute run at a hundred ants is
an honest instrument for it even though the same run says almost nothing about excavation.

| | before | after |
|---|---|---|
| stuck 12s+ at 300s | 92 of 110 | 27 |
| of which diggers | 66 | 9 |
| trend over the run | climbing | flat |
| dug by 360s | 6 | 14 |
| drift | +0 | +0 |

`dig_cooldown` only decremented while an ant was *in a digging posture*, which reads as "time
spent trying". With a bite every 0.45s that was invisible; with a bite every 30,000s it is a
trap. A digger turns to face the sand, cannot walk because every step is into solid ground,
cannot dig because the wait has not elapsed, and the wait only elapses while it keeps facing the
sand — so it stands pressing its face into the floor for ten real minutes. The stuck count
climbed at exactly the rate ants adopted the pose.

Two changes, together: the wait is elapsed time since the last bite and runs whatever the ant is
doing, and only an ant whose wait *has* elapsed adopts the downward face-seeking heading. The
rest walk the nest. Wall-following falls out of the existing deflection ladder, so "wander"
underground already means "patrol the galleries".

### A wrong inference, recorded because the shape of the mistake matters

The same run showed `heap 1x14, nest 14 open, 0 room` — fourteen one-cell surface scrapes and no
shaft — and I read that as stigmergy being broken by the labour rate, on the arithmetic that the
`Dig` field forgets in 100s while bites are 30,000s apart. That arithmetic is sound and the field
now runs on the colony clock because of it (`dig_memory_scale`, with both diffusion and
evaporation scaled so the reach survives).

But the *evidence* did not support the conclusion. In eight minutes the whole colony made
**fifteen bites**. Fourteen scrapes is simply what fifteen bites looks like; it says nothing
about whether work attracts work. Seeing a shaft form needs hundreds of bites, which is an hour
at 24x at the very least.

So `dig_memory_scale` is **verified in mechanism and unverified in outcome**: a unit test asserts
the memory now outlasts the gap between bites and that the reach is unchanged, which is exactly
the claim it makes. Whether it produces better nests is a long-run question. If a long run shows
no improvement, revert it — it is one function and one call site.

### The real baseline, at last: 110 ants, 45 minutes, honest speed

| t | dug | dropped inside | excavated | nest | drift |
|---|---|---|---|---|---|
| 540s | 21 | 9% | 19 | 19 open, 0 room | +0 |
| 1140s | 60 | 43% | 32 | 31 open, 2 room | +0 |
| 1740s | 84 | 52% | 34 | 33 open, 3 room | +0 |
| 2340s | 128 | 63% | 40 | 39 open, 4 room | +0 |
| 2700s | 147 | **65%** | **44** | 43 open, 4 room | +0 |

**The phase's problem is real and it is worse than the discarded figure claimed.** Two thirds of
everything dug goes back into the nest, the fraction climbs monotonically, and excavation
flatlines: between 1140s and 2700s the colony dug 87 more grains and the nest grew by twelve
cells. Mass stays exact throughout, which is the invariant doing its job — nothing is lost, it is
just being carried in circles.

### The ring hypothesis is dead, and the histogram killed it

Outside drops by distance from the mouth: `<7:0 | 7-9:7 | 10-14:5 | 15-24:7 | 25+:30`. They do
*not* pile on the minimum acceptable radius, so there is no inward-sloping ridge feeding spoil
back down the shaft. Good hypothesis, wrong.

**Read that histogram carefully, though**: a column with no detected entrance returns
`GRID_W` from `mouth_clearance`, which lands in the `25+` bucket. So thirty of those forty-nine
drops are really "no entrance exists anywhere near me" rather than "far from the entrance". That
is the actual finding, and it points somewhere else entirely.

### What is really wrong: the bite happens before the walk

The farm never sinks a shaft. Forty columns of one-to-four-cell scrapes, `heap 4x40`, no entrance
the nav flood can even recognise — and with no entrance there is no *outside*, so haulers wander
until `HAUL_PATIENCE` expires and put the grain down where they stand. The congestion is a
symptom; the disease is that digging never concentrates.

It never concentrates because a digger is always standing on sand, and `step` bites whatever it
walks into. The moment its wait elapsed, with a downward bias in its heading, it bit the ground
under its feet — wherever that happened to be. The `Dig` gradient had nothing to do with it. Work
cannot attract work when the bite lands before the walk does.

Under test now: `Pheromones::at_local_max`, requiring an ant to be at least as marked as
everything touching it before it may bite. An ant with a stronger mark beside it walks there
instead; an ant at the face is at the peak and bites; an ant on unmarked ground is trivially at a
maximum, so a colony on flat sand can still start a hole. The same gradient, read as a
destination rather than a mood — no routing, no prescribed layout.

Also instrumented, so the next reading names its own mechanism: inside drops split into
`sealed in` versus `out of patience`.

### Result: biting at the face halves the congestion

Two 45-minute runs at 110 ants, same fixture, same speed.

| | baseline | `at_local_max` |
|---|---|---|
| dug | 145 | 74 |
| dropped inside | 66% | **31%** |
| excavated | 44 | 46 |
| excavated / dug | 30% | **62%** |
| room cells | 4 | **9** |
| drift | +0 | +0 |

Half the biting for slightly more nest: what vanished is the wasted work. Efficiency doubled, the
inside-drop fraction halved, and the count of cells with air on all four sides — the closest thing
to a chamber this project can measure — went from four to nine.

Worth stating: excavation is now gated on *reaching* a face, so the bite rate fell from 193 cells
per colony-day to 99. The design's target is about 115, so the honest reading is that the baseline
was digging at nearly double the intended rate and throwing two thirds of it back.

### What is left, with its mechanism named

`inside drops by cause: 2 sealed in, 46 out of patience`. Sealing is solved; patience is not. And
the chain is probably locomotion rather than dump sites: a hauler that cannot move will always
time out and drop where it stands, and 55 of 110 ants were stuck for 12s+ at the end of that run.
Fix the stalling and this should follow.

**Do not read the stall numbers as a regression yet.** Across the two runs they fluctuated
between 29 and 58 stuck with no clear trend, which is exactly the noise this project's notes warn
about — never conclude from a single run. A repeat is running.

### A dead end is a reason to dig, and it bought back the yield

`at_local_max` halved the congestion and stranded about forty diggers pressing toward faces they
could not reach — reproducibly, 58 to 63 stuck across two runs. So a boxed-in ant now bites
instead of only turning: it has failed every direction it can walk, so it has by definition
nowhere better to be and the concentration rule has nothing left to say. Biting through is also
how a tunnel advances at all.

Like-for-like at 1500s, 110 ants:

| | `at_local_max` | + dead end |
|---|---|---|
| dug | 56 | 58 |
| **excavated** | 34 | **42** |
| room | 5 | 7 |
| stuck 12s+ | 63 | 55 |
| diggers stuck | 42 | 34 |
| drift | +0 | +0 |

The same amount of biting for a quarter more nest. Efficiency is now 42/58 = 72%, against 30% for
the original baseline.

**Where the phase stands against its criteria.** Drift exactly zero in every stage of every run.
Excavated equals mound throughout. Spoil dropped inside went 66% and climbing to 44% and flat, so
the runaway is stopped. All of it measured at 110 ants. Nothing routed, no layout prescribed — the
whole change is one gradient read as a destination instead of a mood.

**Not resolved: 34 diggers still stall.** Two candidate explanations and the metric cannot
separate them. Either they are milling *at* the working face, which real colonies do and which the
design's own "most ants are idle" line would bless, or they are still failing to act. What would
distinguish them is a reading of how far a stalled ant is from the nearest `Dig` peak — stuck at
the face is a crowd, stuck in open sand is a fault. That is the next instrument, not the next
guess.

### Still open from this cycle

The stall count spiked to 85 (58 stuck, 52 of them diggers) partway through the 45-minute run
before settling back to 39/29. The paralysis fix moved the steady state a long way but there is
still a transient that wants explaining.

## Research

- None yet.

## Deferred

- Idle render-loop CPU and GPU cost: important product work, outside the current
  ant-logistics phase.
- M3 demography beyond existing brood and worker ageing.
- Steam, achievements, species, packaging, and unrelated interface polish.
- Mouse feel requires Brett's hands-on judgment.

## Needs Brett

- None currently.
