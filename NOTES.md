# Working notes

Continuity notes, written to survive a context compaction. [DESIGN.md](DESIGN.md) holds
the design and the locked decisions; this holds the state of play and the traps.

## The three repos

| repo | what | state |
|---|---|---|
| `Baz-Studios-LLC/please-dont-shake` | this game | public, `main`, tag `v0.4.0` |
| `Baz-Studios-LLC/Ordo` | UI kit — owns the radial menu widget | pinned by rev in Cargo.toml |
| `Baz-Studios-LLC/baz-studios-launcher` | distribution | catalog row + art added, v0.1.21 |

The local Ordo clone runs behind: `git pull --ff-only` in it before touching anything, or
you'll branch from a commit the game's pinned rev isn't even descended from.

Ordo is pinned **by rev**, never by path. Changing the widget means: commit in Ordo, push,
copy the new rev into `Cargo.toml`, rebuild. There is no path dependency to shortcut this,
on purpose — see Ordo's README.

## Where the milestone actually is

**M1 (sand) is done and verified.** Six seconds of calm produce a byte-identical frame;
sand is conserved exactly through a violent shake.

**Sand is now a two-state model** — loose or packed — and this is the thing to understand
before touching the automaton. Loose sand has no cohesion and rolls to 45°; packed sand
has all of it and holds tunnels. See DESIGN.md for why one threshold can't do both. Five
unit tests in `src/sand.rs` hold the two halves apart; run them before believing anything
about a pile.

**M2 (the colony) works.** Ants dig from flat sand, haul spoil, and mass stays exact across
all three places a grain can hide — grid, particle, mandibles.

The hauling rate **was** the problem, and it is measured and largely fixed. Every congestion
figure this file quoted before 2026-08-20 came off runs at a colony day a *second* — biology at
86,400x with ants digging and walking at real pace — and described a farm that never existed. The
real numbers, from `--congestion` at 110 ants at the honest 24x:

| | baseline | fixed |
|---|---|---|
| dropped inside the nest | 66%, climbing | 32%, flat |
| excavated / dug | 30% | **72%** |
| dropped while sealed in | some | **0** |
| drift | +0 | +0 |

Three faults, in the order they were found, all of them consequences of labour moving onto the
colony clock without the rules around it following:

**The colony was paralysed.** `dig_cooldown` only decremented while an ant was already in a
digging posture — "time spent trying". At 0.45s a bite that is invisible; at 30,000s it is a trap.
A digger faces the sand, cannot walk because every step is into solid ground, cannot dig because
the wait has not elapsed, and the wait only elapses while it keeps facing the sand. 92 of 110 ants
stuck, climbing. The wait is elapsed time now and runs whatever the ant is doing, and only an ant
whose wait *has* elapsed adopts the downward heading.

**The bite landed before the walk.** A digger is always standing on sand and `step` bites what it
walks into, so the instant its wait elapsed it bit the ground under its feet and the `Dig` gradient
never entered the decision. Forty columns of shallow scrapes, no shaft, no entrance the nav flood
could recognise — and with no entrance there is no *outside*, so haulers timed out and dropped
where they stood. `Pheromones::at_local_max` requires an ant to be at least as marked as everything
touching it before biting: walk toward a stronger mark, bite at the peak, and unmarked ground is
trivially a maximum so a colony on flat sand can still start a hole.

**A dead end is a reason to dig.** That concentration rule then stranded ~40 diggers pressing at
faces they could not reach. An ant that has failed every direction it can walk has nowhere better
to be, so a boxed-in ant bites instead of only turning — which is also how a tunnel advances.
Excavation rose 24% for the same biting.

**How to read the stall numbers.** `of the stalled, N are on worked ground` is the line that
matters. At 110 ants: 65 stalled, 38 of them on worked ground and 21 of the rest nurses on the
queen — a crowd at the face and a huddle round the queen, which is what a colony looks like.
Chasing this number to zero is explicitly wrong; DESIGN.md wants a large inactive reserve.

**Still unmeasured: nest architecture.** `room` (cells with air on all four sides) went 4 to 7,
which is the right direction, but 45 minutes buys ~75 bites and galleries need hundreds. That is a
run of hours, and nothing in this repo has done one yet.

**Always read excavated and mound together, and never conclude anything from a single run** —
the harness drives on real frame times, so variance is large.

The population settles at **about a hundred** and stays there, which is not a cap anybody
wrote: `LAY_INTERVAL` allows 2.86 eggs a day and a worker lives 35 days, so the colony
converges on 2.86 × 35 ≈ 100. DESIGN.md asks for 200–500 at peak, so the lever when that
matters is the laying interval, not the lifespan.

**Persistence is done**, brood included. One farm, saved automatically — no button, no slots.
Written on leaving play, on quitting, and every 30s in between; restored at startup before the
title screen, which is what makes Continue seamless. See [src/save.rs](src/save.rs) for what
isn't saved and why (the pheromone fields, deliberately). The colony also keeps living while
the app is shut — see the clock section below.

**The rest of M3 (demography) and M4 (Steam) are untouched.**

## Verify with the harness, not by eye

Emergent behaviour can't be judged from a screenshot, and guessing at it wasted several
rounds. Every claim about the colony should come from here:

```bash
cargo test --release                                          # the sand model and the splash curve
cargo run --release -- --capture --out /tmp/shots              # colony: stock, dig, tap, shake
cargo run --release -- --capture --sand-only --out /tmp/shots  # the M1 sand test, no colony
cargo run --release -- --capture --title-shot --out /tmp/shots   # the title screen, both states, and the fade
cargo run --release -- --capture --splash-shot --out /tmp/shots  # three frames across the studio mark
cargo run --release -- --capture --wheel-shot --out /tmp/shots    # the radial menu, unlit and lit
cargo run --release -- --capture --hand-shot --out /tmp/shots     # the hand's three poses
cargo run --release -- --capture --settings-shot --out /tmp/shots # the settings window, tab by tab

# Spoil logistics and locomotion near a hundred ants, at the honest 24x. The only instrument
# that reaches that regime: kits are tipped in through the game's own stocking path until the
# tank is full, then it prints the whole spoil ledger every minute.
cargo run --release -- --capture --congestion --ants 100 --minutes 25 --out /tmp/cong
```

**Copy the binary out before a timed run**, and do not compile while one is in flight. These runs
are paced by real time and `Time::<Virtual>` clamps how much one frame may deliver, so a machine
busy with a release build falls behind and the ledger measures the compiler. A background build
and a foreground `cargo check` also deadlock on the target-dir lock, which held one baseline at
zero progress for several minutes. `cp target/release/please_dont_shake /tmp/pds-run` and run
that.

**A worker's first bite is up to a whole `DIG_INTERVAL` away**, so the first twenty minutes of a
24x run legitimately read `dug 0`. Discount the warm-up or compare only the later lines.

Anything that photographs UI has to keep the camera on the window *and* skip the offscreen
target — see `ui_shot` in main.rs. The hand and settings runs also fake their input at the
same seam a touchscreen would use, because an unattended run has no cursor at all.

The harness makes its own output directory now. It did not, and a missing one showed up only as
`Cannot save screenshot` buried in the log while every number printed happily — a run that looks
completely successful and has no pictures in it. Cost a whole run before it was fixed.

**Capture mode never reads or writes the farm on disk.** Loading one would put a colony
into a measurement meant to start from bare strata, and saving one would replace somebody's
forty hours of tunnels with a test fixture. Not touching the file at all is the only
version with no way to get it wrong. `PDS_SAVE_DIR` relocates the save if you need to poke
at one; the real farm lives in `~/Library/Application Support/Please Don't Shake/`.

Runs render to an **offscreen texture**, not the window, so a locked or sleeping screen
can't silently produce black frames — that cost an hour once. The two UI shots are the
exception and have to opt out of it in *both* directions: they grab the window, and main
must also skip `setup_offscreen_target` for them. Miss the second half and the only camera
is pointed at the texture, Bevy UI draws to whichever camera is on the window, and the
screenshot is a convincing sheet of black.

The number that matters most is total sand. It must stay exactly constant — `drift +0` at
every stage of every run, including mid-shake. It is the only assertion here that has ever
caught a leak nobody suspected, and it has now done it twice.

## Traps already paid for

- **Do not run `cargo fmt` on this repo.** It is not rustfmt-clean and the disagreement is a
  style one: the code writes compact struct literals like `Cell { mat: Substance::Sand, shade }`
  and rustfmt explodes each onto five lines. A whole-repo format is a 350-line diff across
  fourteen files that touches nothing anybody changed, and it buries whatever the commit was
  actually about — it did exactly that to a two-file sound fix. A `rustfmt.toml` does not rescue
  it either; `struct_lit_width` and `fn_call_width` were tried and all fourteen files still
  differ. Format the file you are editing by hand, in the style around you.


Things that cost real time and will look like new bugs if forgotten.

- **`Single<T>` silently skips its system** when the query doesn't match exactly one
  entity. In an `OnEnter` system, which fires once, that means the thing never gets built
  and nothing errors. This ate the whole title screen once — and then, when the hand added a
  *second* camera, every `Single<&Camera>` in the codebase stopped matching and the mouse
  quietly went dead. Ask for `With<TankCamera>`; never for "the camera".
- **A stacked overlay camera must not tonemap.** The hand draws on layer 1 through a second
  camera with `order: 1` and no clear, over an image the tank camera has already run through
  its tonemapping curve. Running the curve again turns the *whole window* black — and only
  once the overlay has something to draw, so an empty layer looks fine and the bug appears
  the moment the feature works. `Tonemapping::None` on the overlay. Divus Factus has the
  same note about HDR having to match between stacked cameras; treat both as one rule.
- **Two cameras make UI attachment ambiguous.** Say `IsDefaultUiCamera` on the tank camera,
  or the menu may attach to the hand's overlay — which draws only layer 1, so the menu
  simply isn't there.
- **Ordo's buttons are `bevy_ui_widgets::Button`** and carry no `Interaction` component.
  Listen for the `Activate` event. A query for `Interaction` matches nothing while the
  button still lights up on hover, so it looks perfectly wired and does nothing.
- **Duplicate components in one bundle is a hard panic.** `radial()` already carries
  `Radial`; adding a second to seed state crashed on open. The settings window then walked
  into it three more times in one sitting — `backdrop()` brings its own `Layer`, and both
  `button()` and `card()` bring their own `Node`. **Never add a `Node` beside an Ordo
  bundle.** Size it in an `Added<..>` dressing pass, the way `dress_menu` and
  `size_settings_ui` do.
- **A hidden pane still holds its space.** `Visibility::Hidden` stops a thing being drawn and
  leaves it in the layout, so a tabbed window reserved room for every pane at once and the
  open one sat in a column of gaps. Ordo's tabs use `Display::None` now; remember it for
  anything else that hides.
- **Anything that despawns an ant has to put its grain back.** A worker that died of old age
  while hauling deleted the grain in its mandibles: 25344 sand cells down to 25330 over 229
  deaths, a tenth of a percent, invisible by eye and permanent. Sand lives in three places —
  the grid, a falling particle, an ant's mandibles — and *every* exit from the third one has to
  return it. `crate::grains::settle` is the way to do it; it searches upward for air, so it
  can't overwrite. The same rule will apply to predation, to the midden, and to anything else
  that ever removes an ant.
- **A frozen counter says a system stopped, not why it stopped.** Systems that `return` early
  on a missing resource or a failed `single()` log nothing at all, and any stats they own then
  read as a frozen simulation. Print a census of the world before theorising — see the
  two-queens section for what the alternative costs.
- **Bevy only compiles in Vorbis by default.** `wav`, `mp3` and `flac` are opt-in features.
  A perfectly valid WAV panicked the audio system and left a window with nothing in it.
  Ship OGG.
- **Never write a colour Ordo paints.** The repaint pass owns `BackgroundColor`,
  `BorderColor` and `TextColor` for anything carrying `Fill`, `Edge` or `Ink`, and it wins
  — a colour written in `OnEnter` is overwritten in the same frame's `Update`. Both menus
  dimmed their disabled entries this way and *neither had ever worked*; every label sat at
  the same 236 from the day it was written. Change the **role** (`Ink(Role::InkDim)`) or
  the **`Opacity`**, never the colour. Ordo's own docs call this out; it's still the
  easiest mistake in the codebase to make, because writing the colour looks correct.
- **A generic font family needs `system_font_discovery`.** Without it text renders as
  *nothing*. The theme names no fonts on purpose; Bevy's embedded default stands. A font
  named by **path** is safe, which is how the studio line gets its font.
- **Bevy's embedded font is `FiraMono-subset.ttf` — ASCII and nothing else.** 95 glyphs,
  U+0020..U+007E. No `©`, no `®`, no em dash, no curly quotes; they all draw as
  missing-glyph boxes. The game ships `assets/fonts/FiraMono-Medium.ttf` (the same
  typeface, unsubsetted, 1349 glyphs, OFL 1.1 with its licence beside it) for the one line
  that needs a `©`. Point the theme's `display`/`body` at it if the rest of the UI ever
  wants real punctuation.
- **Agitation is added per rendered frame and decays per fixed tick.** Anything feeding it
  must be a per-second rate or the same gesture is twice as destructive at 120fps. This
  turned a moderate shake into a total collapse the first time it met a release build.
- **Don't try to get an angle of repose out of the cohesion threshold.** It cannot be
  done, and two rounds were spent trying. A spire's top grain scores 3, a tunnel ceiling
  scores 2, so any threshold that drops the spire drops every ceiling in the farm. The
  loose/packed split is the answer; agitating locally wherever new sand lands is what it
  replaced, and that was worse than it looked — `agitate` with a 4-cell radius actually
  floods a whole 16×16 chunk, so every grain an ant put down was caving in the shaft it
  came out of, *and* its strength depended on where in the chunk the grain happened to
  land (a grain arriving near a chunk edge got a falloff of exactly zero).
- **Digging rules collide in both directions.** Ban above-ground digging entirely and a
  colony on flat sand can never get underground, because the only way down is to dig.
  Allow it freely and they shuffle the same topsoil forever. Downward-only threads it.
- **On flat sand the working face is a valid dump site.** Spoil must be carried
  `MIN_HAUL_DISTANCE` from where it was dug, or 845 excavations produce a farm with no
  tunnel in it.
- **Distance from where it was dug is not distance from the hole.** An ant that digs at
  depth 20 has already travelled `MIN_HAUL_DISTANCE` by the time it climbs out, so it
  drops its grain on the lip — and loose sand rolls, so down it goes. Spoil needs
  `MOUND_CLEARANCE` columns between it and the nearest entrance, which is what
  `NavField::nearest_mouth` is for. An entrance is a column sunk `MOUTH_DIP` below the
  ground on **both** sides; one side would also match the flank of a spoil mound, and
  calling that an entrance leaves nowhere to dump at all.
- **The launcher bakes its catalog at compile time** and versions from
  `src-tauri/tauri.conf.json` + the crate, *not* from the git tag. Tagging alone rebuilds
  the old version under a new tag, and every installed launcher correctly ignores it.

## Release, and what the launcher needs

The launcher downloads unauthenticated, so the repo must stay public, and it matches a
**version-less asset suffix** — `*-macos-aarch64.app.tar.gz`, `*-windows-x86_64.zip`. The
tag carries the version; don't put it back in those filenames.

Assets are loaded from disk at runtime, so every package carries `assets/` **beside the
executable** (`asset_root()` in main.rs looks there first). A binary shipped alone launches
into a tank with no sand, no music and no theme. The macOS half mirrors Divus Factus's
packaging script, including its ad-hoc `codesign` and its space-free bundle name.

**Only the patch number moves.** v0.4.1, v0.4.2, v0.4.3, and on. Never the minor, whatever
the change is — v0.4.0 carried the labour-rate rework and the whole sound system, and the next
release after it is v0.4.1. Three places have to agree: `version` in `Cargo.toml`, the CHANGELOG
heading, and the tag, since the tag is what the launcher reads.

**Releases are Brett's call, never a tidy-up step.** A tag reaches every installed
launcher, so tagging is publishing. Build, test and commit freely; stop at the tag and say
it's ready. When he asks:

```bash
git tag v0.1.1 && git push origin v0.1.1
```

`v0.1.0` is out, with all three platform assets attached.

## The clock is real time

`ColonyClock::days_per_second` is `1/86400` — a colony day takes a day. It was an hour per
day; Brett asked for real time and that is now the locked decision in DESIGN.md.

Consequence, because it is easy to mistake for a bug: **you will not see a life stage in a
sitting.** Egg to worker is six real days. A farm shows you that the pile is bigger than it
was, not that anything hatched. Every scripted run overrides this — see
`CAPTURE_DAYS_PER_SECOND` — and that override is the only reason a two-minute test can say
anything at all about brood.

### Labour runs on the colony's clock, and the fast-forward scales both

The clock decision was only half done for a long time: biology ran at a day a day while an ant
still bit sand every 0.45 seconds. That is 1,300× too fast — 1.2mm cells, a real *Lasius*
formicarium nest of ~7,000 cells over two months, so 115 cells a day for the colony and under three
a day for one digger. `DIG_INTERVAL` is 30,000 seconds now, and `ColonyClock::labour_scale` multiplies
it by however many times faster than real time the clock is set to.

**It also means every colony number published before 2026-08-12 is fiction.** They came off runs at
a colony day a *second*, where biology ran 86,400× and ants dug at walking pace, so the founding
workers died of old age in thirty-five seconds and any rule needing work-before-biology failed by
construction. Three measurements of the crowding brake were lost to it before Brett asked the
question that explained it: "shouldn't the digging speed up too?"

**The speed table has two entries and the ceiling is 24×, derived not chosen.** The test is how far
an ant walks between bites, since that is what spreads digging into galleries instead of hollowing
out where it stands: real diggers manage ~28,000 cells walked per cell dug, real time here gives
420,000, a day an hour gives 17,500, a day a minute gives 292 and a day a second gives 4.9. The last
two are blob-makers and are gone. A test in `devcapture` pins the ceiling so nothing creeps back.

Two things stay off the labour clock, deliberately: escaping a burial (`ESCAPE_INTERVAL`, an animal
in trouble, not construction) and everything the player does.

**The 125-second capture can no longer see the colony** — three hundredths of a colony day, one
cell dug. It is a sand and locomotion instrument now: mass conservation, stalling, hauling,
collapse and rebuild. Brood, founding and nest shape need long runs at 24× — six real hours for one
brood cycle. Nobody has run one yet, and until somebody does, no claim about nest shape in this
repo is evidence.

### Testing it: `[` and `]`

Six real days a brood cycle is untestable by eye, so the colony's calendar has a speed control —
`devcapture::SPEEDS`, four named rates from real time to a colony day a second. `[` slower, `]`
faster, announced in the terminal; `--speed <multiplier>` starts fast (1 is real time, 86400 is a
day a second). Every run starts at real time and nothing remembers the speed, because a testing
tool that *could* be left switched on eventually is.

**Only biology moves.** Not a limitation to be fixed later: a brood cycle in a minute is 8,640×
real time, and the sand is a 60 Hz automaton over a quarter of a million cells — 8,640× would be
half a million sweeps a second. The sand and the ants are always real; what compresses is the
calendar. If you need faster *sand*, that is `Time::<Virtual>::relative_speed` and it tops out
somewhere single-digit.

The fastest rate in the table is the one the capture harness has always used, so every scripted
run is a hundred and twenty-five days of evidence for it.

**Do not "fix" this by shortening the stage constants in `src/brood.rs`.** That advice used to
be here and it was wrong: DESIGN.md now says plainly that the real-time clock is what makes this
*Mountain* with ants in it rather than a game you sit in front of, and compressing biology to
fit a session is exactly the move that breaks it. A farm that feels slow in a sitting is the
farm working. If it feels *dead*, the answer is that a glance has to be worth taking — the tank
always looking like something, and the away catch-up making a return worth it — not a faster
clock.

### And the farm lives while the app is shut

Which is what makes the above liveable, and is Brett's call as a setting — Gameplay ▸ *Grow
while closed*, on by default. `src/away.rs`. The save records `saved_at`; `load_farm` turns the
gap into colony-days through the *clock's own rate*, so this follows the clock rather than
assuming a day per day; `catch_up_while_away` spends it.

The mechanism is the part worth protecting: it runs **the game's own systems** in a loop with
`ColonyStep` set to a tenth of a day instead of a sixtieth of a second. `lay_eggs`, `age_ants`,
`age_brood`, `age_out` — the same four, in the same order the fixed schedule runs them, minus
everything that involves moving. The alternative was a closed-form "resolve N days" function,
which is a second copy of the rules that only runs at startup, and therefore the copy nobody
notices has drifted. This is why `ColonyStep` exists at all and why nothing biological reads
`Time` directly any more.

Two things had to change to make stepping honest, and both were latent bugs at any rate:
`lay_eggs` now *subtracts* the interval instead of zeroing its clock, and `age_brood` carries a
stage's overshoot into the next stage instead of resetting to zero. At a sixtieth of a second
the discarded remainder was invisible; at a tenth of a day it was most of the step, and the
queen would have laid at whatever rate the catch-up happened to step at.

Nothing digs while you're away, and that is a design statement rather than a shortcut — see
DESIGN.md's Offline row. Sand needs two hundred ants reading a pheromone field sixty times a
second, and guessing where they would have dug would be inventing a farm rather than continuing
one.

### Colony-days are `f64`, and that is not a preference

Setting the rate to real time is not the same as the clock working. At `1/86400` days per
second the sim adds `1.9e-7` days a tick, and a single-precision float holding an age of four
days *cannot represent a step that small* — `age += 1.9e-7` rounds straight back to `age`.
Measured, with `age_days` as `f32`: ages ran 24% fast between one and four days, then froze
dead at four. No worker ever reached `NURSE_UNTIL`, so every ant was a nurse forever, nothing
dug, and nobody died of old age. The clock said real time and the colony was a photograph.

Anything that accumulates colony-days is `f64` — `Ant::age_days`, `Brood::age_days`,
`LayClock`, `ColonyClock::days_per_second`, the stage and lifespan constants. It was invisible
under the old hour-per-day rate, because that increment was 24× larger and stayed above the
single-precision step. `a_worker_ages_a_day_in_a_day_at_any_age` in `src/ants.rs` guards it,
and it starts the sum at thirty days rather than zero, which is the entire point of the test.

## Reading the stall metric, and why zero is the wrong target

`went nowhere in 4s` in the capture report is the honest stuck-detector: has this worker moved a
cell and a half in four seconds? It replaced `stuck now`, which counts an ant whose eight
candidate steps were all refused on a single tick — true, cheap, and blind to the failure that
actually happens, where an ant has a legal step every tick and paces between two cells forever.
`stuck now` read **zero** on a run where 45 of 100 ants were going nowhere.

**Do not drive this number to zero.** DESIGN.md: *most ants are idle most of the time; if every
ant is busy it reads as a factory, not a colony.* Real *Lasius* keeps a large inactive reserve, so
a third of the workforce standing about is the farm being right. That is why the report breaks it
down by job — a nurse that has reached the brood is *supposed* to sit on it; a stalled digger is
not.

The number that says the colony is alive is **excavation still climbing**, with hauling non-zero.
When it froze at 98 cells across two reports forty seconds apart, that was the bug. Read the two
together or you will chase the wrong one.

## The queen goes down the shaft now

She had no movement code at all — the branch deposited `Ph::Queen` and `continue`d, under a comment
claiming she "sits deep". Nothing got her deep. She was poured out of the tube, fell onto the sand,
and stayed on the surface for the colony's whole life; and because she is the `Queen` pheromone
source, the nurses gathered up there and `lay_eggs` put the brood pile out in the open where the
first shake scatters it. The nest's centre was outside the nest.

`settle_the_queen` is two states and no plan. If anything adjacent is deeper she walks that way,
favouring down; if nothing is, she potters within `QUEEN_LEEWAY` steps of where she stands. Deeper
is `NavField::deepen`, the mirror of `descend` — the flood is walking distance from open sky, so
climbing it is walking into the burrow, and the deepest reachable cell is the innermost chamber.
Nothing stores where the chamber *is*; there is no such variable and there should not be. The
ants' diggings say.

She never digs, so she can only occupy what the workers have opened. The queen getting deep is
therefore something the colony achieves for her, and on a farm nobody has dug yet she waits on the
surface, which is correct rather than broken.

Two traps, both now covered by tests in `pheromones.rs`: `UNREACHABLE` is `u16::MAX`, so an inward
step that compares distances without excluding it calls every wall the deepest place in the tank
and walks her into one; and a `QUEEN_LEEWAY` of zero freezes her, because at a local maximum every
neighbour is strictly shallower and an equal-or-deeper rule permits nothing at all.

## The two queens, and what it cost to find them

Solved, and worth reading before trusting any old colony measurement. The capture run used to
end with the colony gone and the brood counters frozen at identical numbers:

```
04-nest-100s: 2 ants | jobs: 0 nurses, 0 diggers, 0 surface
              brood 6 eggs, 7 larvae, 4 pupae | laid 73 | eclosed 73 | died 93
```

The diagnosis in these notes was **wrong** in an instructive way. It read "frozen counters mean
`tend_brood` bailed on `queen.single()`, so the queen is being *lost*" — and then went looking
for something that despawns her. Nothing does. The truth is the opposite: there were **two**
queens. `single()` fails on two as surely as on none.

The second one came from the harness. The colony run stocked the farm by pushing a placement
into the queue at 0.2s, and then at 30s opened the radial menu and committed wedge zero to
prove the menu still worked — and wedge zero is the **ant kit**. A second kit is a second
queen. From 32s on, every colony run was measuring a farm that had silently stopped laying and
stopped tending, which is why it looked like a collapse.

`died 93` was never wrong either: two kits is twenty poured workers plus seventy-three eclosed,
and ninety-three of them died. The over-count theory about deferred `Commands` was chasing a
number that added up all along.

Three things came out of it, all of them keepers:

- **The harness stocks the farm through the menu, once.** One kit, through the path a player
  actually uses, at 0.1s. Better coverage than the two halves it replaced.
- **`lay_eggs` and `tend_brood` take the *first* queen, not the only one.** Design says one
  queen per farm, but code that detonates silently if it ever gets two is a trap, and this one
  cost a full round of investigation. Nothing is logged when a system quietly returns early.
- **The report counts the world, not the counters** — `live: N queens, M brood (K held)`. The
  brood numbers in `BroodStats` are written by `tend_brood`, so when it bailed, the report
  printed its last known values forever. A stale report is indistinguishable from a frozen
  simulation, and that is what made this take so long.

The lesson for next time: a frozen counter is evidence that a *system stopped running*, not
evidence about *why*. Print the census before theorising.

## Brood, as built

Egg to larva to pupa to worker in six colony days, in `src/brood.rs`. Designed in DESIGN.md
under "Brood — what the nurses are for", including why six.

It works: measured 10 ants to 98, with 76 laid and 76 eclosed. Nurses gather stray brood to
the queen from two rules and nothing tells them to make a pile.

**The colony clock is why nothing appears to happen in a normal session.** At the locked day
per real hour, one stage takes hours. The scripted runs set `days_per_second = 1.0`, which is
the only way a two-minute test can say anything about a six-day life stage — see
`CAPTURE_DAYS_PER_SECOND`.

Workers now die at `WORKER_LIFESPAN`. Without it the age model was a ramp: at speed, sixty
seconds produced a hundred ants of which none dug and eighty-five patrolled, because
everybody was old. Population should be a curve.

The brood **is** saved now, and so is the moment the file was written — which is what the away
catch-up spends. `held_by` is deliberately not saved: an `Entity` means nothing in the next
process, so brood in a nurse's mandibles comes back set down and is picked up again within a
second of play. Stage goes to disk as a small integer, so renaming a variant can't invalidate
every save on disk.

## Not yet done

- Spoil-hauling rate (the open M2 task above) — 9% returned, and worth pushing further.
- CHANGELOG.md still has `## v0.1.0` at the top, and the release workflow turns whatever
  is top into the release notes. Everything since then needs a section written before the
  next tag, or the next release ships v0.1.0's notes.
- Idle CPU: the *sim* sleeps when settled, the render loop doesn't. An always-running
  ambient game needs both.
- Nobody has played this with a hand on a mouse for long. Drag weight, the tap/shake
  threshold and the hold-to-open delay are all feel, and all unverified by feel.
- The 31MB source WAV is still in git history; removing it needs a force-push.
- iPad: input is deliberately a thin adapter that only writes agitation and alarm, so a
  touch/accelerometer front end replaces that adapter and nothing else. Keep it that way.
