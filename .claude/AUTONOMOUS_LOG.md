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
