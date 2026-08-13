# Please Don't Shake: Claude Code Instructions

Read `DESIGN.md` before changing behavior and `NOTES.md` before investigating a
system. `DESIGN.md` contains locked product decisions. `NOTES.md` records the
current implementation, measurements, and traps already paid for. Do not treat
either as optional background.

## Product Law

- This is a one-screen ambient ant farm in the spirit of *Mountain*, not a
  conventional session game.
- The colony simulation is the storytelling. There is no text, HUD, meter, or
  explanatory panel inside the tank.
- Time is real time. Do not compress biology or labour to make a short play
  session busier. Capture-mode acceleration is verification machinery only.
- The tunnel network is accumulated history authored by the ants through
  stigmergy. Do not procedurally design the nest for them.
- Shaking destroys architecture, not ants for spectacle. The payoff is watching
  the colony rebuild over days.
- The camera does not move. Input remains a thin adapter that writes agitation
  and alarm; simulation code must not know about a mouse.
- There is one continuously saved farm and no save slots.
- The game must be cheap enough to leave running. Idle CPU and GPU cost are
  product requirements, not optional optimisation.
- Preserve exact sand mass across the grid, loose particles, and ant mandibles.
  Any non-zero sand drift is a correctness failure.

Do not change a locked decision, add a major mechanic, expose simulation state
to the player, or alter the intended emotional experience without Brett's
explicit approval.

## Repository Practice

- Preserve the existing Rust and Bevy 0.19 architecture and local style.
- Do not run whole-repository `cargo fmt`. This repository intentionally uses
  compact literals that rustfmt expands into unrelated churn. Format touched
  code by hand in the surrounding style.
- Ordo stays pinned by Git revision, never by a path dependency.
- Capture mode must never read or write the real farm. Use `PDS_SAVE_DIR` for
  deliberate save experiments.
- Do not commit, push, tag, rewrite history, update dependencies, or modify the
  Ordo repository unless the active goal explicitly requires it.
- Never revert, overwrite, or clean up changes you did not make. Inspect a dirty
  worktree and work with concurrent edits. Stop if they make safe progress
  impossible.
- Keep changes tightly related to the active goal. Record adjacent discoveries
  under `Deferred` rather than implementing them opportunistically.

## Goal-Based Autonomous Work

`CLAUDE.md` defines how to work. The current prompt or `/goal` defines what to
work on. Do not turn a temporary objective into a permanent project rule.

For autonomous goals, use this loop:

1. Read the active goal, its acceptance criteria, relevant code, `DESIGN.md`,
   `NOTES.md`, and `.claude/AUTONOMOUS_LOG.md`.
2. Inspect current behavior and evidence before proposing a cause.
3. Select the highest-value unresolved issue required by the goal.
4. Classify discoveries as `Required`, `Deferred`, `Unrelated`, or
   `Needs Brett`. Implement only `Required` work.
5. Research current primary sources when the problem is unfamiliar, obscure,
   version-sensitive, or has resisted two substantive approaches.
6. Implement the smallest complete solution consistent with the established
   architecture and product law.
7. Validate the relevant invariant, behavior, visual result, or performance
   claim. Compilation alone is not proof.
8. Fix regressions introduced by the change.
9. Review the diff for scope, accidental churn, save compatibility, and
   player-visible consequences.
10. Update the autonomous log with significant evidence and decisions.
11. Reassess the acceptance criteria. Continue only when meaningful required
    work remains.

Do not stop merely because one subtask compiled. Also do not manufacture work
to remain active. Stop when the acceptance criteria are met, remaining ideas
are speculative or outside scope, Brett asks you to stop, or a decision belongs
to Brett.

If a goal arrives without acceptance criteria, infer conservative measurable
criteria from existing tests, instruments, and locked design, then record them
in the log before editing. Ask Brett when completion depends on subjective feel
or a materially different player experience.

## Priorities

Within the active goal, prefer:

1. correctness and invariant violations
2. broken or unfinished behavior
3. player-visible simulation quality
4. biological believability and consistency
5. measured performance problems
6. maintainability required to complete the goal
7. refactoring

Do not replace working architecture for cleanliness, optimise without evidence,
or create abstractions for hypothetical future work.

## Ant-and-Sand Simulation Evidence

The simulation is a coupled ant-and-sand system. Sand shapes ant movement and
nest survival; ants move sand and author the tunnel history. Never improve one
side by making the other less truthful. Validate claims with instrumentation
and repeated release-mode runs, not a screenshot, a single seed, or intuition.

- Start with focused tests, then use `cargo test --release` when appropriate.
- Use `cargo run --release -- --capture --out /tmp/shots` for the colony.
- Use `--sand-only` for sand behavior and the other documented capture modes
  for their corresponding surfaces.
- Always confirm screenshots were actually written when a harness promises
  them.
- Never quote excavation or hauling results without elapsed time, headcount,
  excavated volume, mound volume, inside-nest drops when available, and sand
  drift.
- Compare multiple runs or seeds before treating a noisy emergent result as a
  regression or improvement.
- `excavated == mound` describes healthy material transport. Read both values
  together.
- Zero stalled ants is not a goal. Real colonies retain inactive workers.
  Distinguish appropriate idleness by job from workers failing to progress.
- A living colony keeps meaningful processes progressing over time: excavation,
  hauling, brood care, population turnover, alarm recovery, and rebuilding.
- Test behavior at realistic colony sizes. A rule that works for nineteen ants
  may collapse under congestion at one hundred.
- Preserve save compatibility unless the active goal explicitly authorises a
  migration. Test persistence when changing ants, brood, clocks, or stored state.
- Preserve the two-state sand model: loose sand rolls naturally while packed
  sand holds accumulated tunnels. Do not tune one threshold until it destroys
  the other behavior.
- Calm architecture must remain stable indefinitely. Tap should produce local,
  limited disturbance; shake should cause agitation-dependent structural
  collapse without leaking mass or scaling accidentally with frame rate.
- A spoil mound is part of the simulated landscape. Dropped grains should obey
  the same physical rules as other loose sand, then settle into packed ground;
  do not special-case them into decorative or non-physical geometry.
- When changing ant movement, hauling, digging, grain settling, stability, or
  agitation, validate both colony behavior and the resulting sand structure.

Player experience still matters. Behavior must be legible in the tank without
text, but subjective feel such as drag weight or shake threshold requires Brett's
judgment rather than autonomous tuning by metrics alone.

## External Research

Research before a third substantive attempt at the same difficult problem.
Prefer official Bevy 0.19 documentation and source, official repositories and
issue trackers, papers, domain references, and production-quality open-source
implementations. For ant biology, distinguish documented behavior from a game
design choice. For algorithms, identify the underlying problem and examine
relevant work beyond Rust or Bevy, then translate the useful concept into the
existing architecture rather than copying an engine-specific design.

Record only research that changes an implementation decision. Include the
observed failure, source, applicable finding, rejected alternatives, and the
evidence that the chosen approach solved the original problem.

## Escalate to Brett

Stop and ask when:

- a choice changes a locked design decision or the player's experience
- several valid solutions create meaningfully different visible behavior
- a major architecture or data-format replacement appears necessary
- save compatibility would knowingly break
- a destructive Git operation or dependency change is required
- intended behavior cannot be inferred from design, notes, code, and evidence
- validation reaches a subjective judgment that only Brett can make

Routine implementation details within an approved goal do not need approval.

## Autonomous Log

Maintain `.claude/AUTONOMOUS_LOG.md` during autonomous work. Keep it concise and
evidence-based, not a transcript. Update it after meaningful changes or before
switching to the next issue. The active goal, acceptance criteria, current
measurements, completed work, validation, research decisions, deferred items,
and questions for Brett belong there.

Do not duplicate long-lived project history from `NOTES.md`. When a goal is
complete, reduce the log to its useful result and prepare it for the next goal.
