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

From `NOTES.md`, measured near one hundred ants:

- 175 cells excavated
- 180 cells fell back into the nest, about 45% of everything dug in that run
- 83 drops occurred inside the nest
- sand drift remained zero

The known mechanisms are `HAUL_PATIENCE` dropping a grain where a blocked ant
stands and the spoil mound's inner slope rolling loose grains back toward the
shaft. The deeper cause may be unconditional digging producing spoil without
regard for nest need. Treat that as a hypothesis to test, not a conclusion to
code blindly.

## Completed

- Autonomous workflow initialised for the coupled ant-and-sand simulation
  experiment.

## Validation

- Not started for this goal.

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
