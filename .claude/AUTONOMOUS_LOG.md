# Autonomous Development Log

## Active Goal

Improve the coupled ant-colony and sand simulation until it is internally correct, biologically
and physically believable at the game's chosen level of abstraction, and visibly alive over
long-running play, while preserving every locked decision in `DESIGN.md`.

**Phase — spoil logistics and mature-colony congestion: satisfied.** See below for the evidence
and for the one criterion that is out of reach without a run of hours.

## Result

Measured with `--congestion` at 110 ants at the honest 24x, five runs of 25 to 45 minutes.

| | baseline | now |
|---|---|---|
| dropped inside the nest | 66%, climbing | 32%, flat |
| excavated / dug | 30% | **72%** |
| dropped while sealed in | some | **0** |
| excavated (25 min) | — | 40-42, still climbing |
| room cells | 4 | 7 |
| sand drift, every stage | +0 | +0 |

Three faults, each a consequence of labour moving onto the colony clock while the rules around
it stayed where they were. All three are written up in NOTES.md under "the hauling rate":

1. **The colony was paralysed** — the wait between bites only elapsed while an ant was already
   facing the sand, so 92 of 110 stood pressing at the floor. It is elapsed time now.
2. **The bite landed before the walk** — a digger always stands on sand, so it bit underfoot the
   instant its wait elapsed and the `Dig` gradient never entered the decision. `at_local_max`
   makes it walk to the face first.
3. **A dead end is a reason to dig** — which released the ~40 diggers the previous rule stranded,
   and raised excavation 24% for the same biting.

Also fixed on the way: `settle` could destroy a grain silently, and the queen's founding spoil was
aimed at the roof of the tank where it vanished once the top filled. Mass is the one invariant this
game promises absolutely, and it now has four tests of its own.

## Criteria, one by one

- Drift exactly zero in every stage — **yes**, every run.
- Excavation progresses at mature size rather than flatlining — **yes**, efficiency 30% to 72%.
- Excavated material reaches the mound — **yes**, `excavated == mound` throughout.
- Spoil inside and falling back materially reduced — **yes**, 66% to 32%, sealed drops eliminated.
- Holds near a hundred ants — **yes**, every measurement is at 110.
- Brood care, queen, alarm recovery, digging, hauling do not regress — **alarm, digging and
  hauling yes** (colony capture: drift +0 through tap and shake, 11 dislodged, 10 still alarmed
  when settled, 7 back to digging). **Brood and founding are unverified**: the queen needs about
  two hours at 24x to reach laying depth, and no run here was that long.
- Emergent and stigmergic, no prescribed layout or central routing — **yes**, the whole change is
  one gradient read as a destination instead of a mood.
- Loose and packed retain their behaviour — **yes**, M1 sand identical: 32 cells changed after
  digging, still 32 six seconds later.
- Tap local, shake agitation-driven, neither frame-rate dependent — **yes**, and both now have
  tests naming the criterion.
- No arbitrary zero-stall target — respected. Of 65 stalled ants, 38 stand on worked ground and 21
  of the rest are nurses on the queen: a crowd at the face and a huddle round her.

## Not done, and why

**Nest architecture.** `room` moved 4 to 7, which is the right direction, but 45 minutes buys ~75
bites and galleries need hundreds. Whether the colony builds chambers is a question for a run of
hours. Nothing in this repo has done one.

**`dig_memory_scale` is verified in mechanism, not in outcome.** A unit test pins what it claims —
the `Dig` memory outlasts the gap between bites, and the reach is unchanged — but whether it
improves nests needs the same long run. One function and one call site if it does not.

## Needs Brett

- **"Grow while closed" versus the labour clock.** The catch-up advances laying, ageing, eclosion
  and death but never digging, because nothing simulates sand offline. That was survivable when
  digging was 1,300x faster; now a returning colony finds a nest that cannot catch up. Cap it,
  grant excavation too, or accept and document.
- **Three stale claims in DESIGN.md's locked table.** Settings says "nothing behind it yet" and
  there is a six-control window; Scale claims 200-500 ants and "architected SoA" when the
  equilibrium is ~100 by construction and ants are a per-entity struct; Feed sits in the verb
  table with no "not yet" marker while Food and Water are dimmed and unsimulated.

## Deferred

- Idle render-loop CPU and GPU cost. The crowding field that swept 40,960 cells at 15Hz for a
  value nothing read is now behind `CROWDING_BRAKE`, off — but the render loop itself still does
  not sleep, and the product law says the game must be cheap to leave running.
- The crowding brake itself: built, calibrated from measured readings, one bool from live, and
  parked until a run of hours can judge it.
- M3 demography beyond brood and worker ageing. Steam, achievements, species, packaging.
- Mouse feel needs Brett's hands.
