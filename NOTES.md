# Working notes

Continuity notes, written to survive a context compaction. [DESIGN.md](DESIGN.md) holds
the design and the locked decisions; this holds the state of play and the traps.

## The three repos

| repo | what | state |
|---|---|---|
| `Baz-Studios-LLC/please-dont-shake` | this game | public, `main`, tag `v0.1.0` |
| `Baz-Studios-LLC/Ordo` | UI kit — owns the radial menu widget | pinned by rev in Cargo.toml |
| `Baz-Studios-LLC/baz-studios-launcher` | distribution | catalog row + art added, v0.1.21 |

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

**M2 (the colony) works end to end but isn't finished.** Ants dig from flat sand, haul
spoil, and mass stays exact across all three places a grain can hide — grid, particle,
mandibles. What's still wrong is the *rate*: net excavated volume grows more slowly than
the excavation count implies, because some spoil still gets redeposited.

Measured at 100s with 22 ants, on flat sand:

| | digs | net excavated | efficiency |
|---|---|---|---|
| spoil held in place by an agitation blast | 231 | 87 | 38% |
| loose spoil, no clearance rule | 231 | 87 | 38% |
| loose spoil + mouth clearance + outbound hauling | 176 | 118 | **67%** |

The remaining third is still worth chasing, but the shape is right now: there is a shaft
with a crater rim around it, which is what a *Lasius* entrance looks like.

**M3 (demography, persistence) and M4 (Steam) are untouched.**

## Verify with the harness, not by eye

Emergent behaviour can't be judged from a screenshot, and guessing at it wasted several
rounds. Every claim about the colony should come from here:

```bash
cargo test --release                                          # the sand model and the splash curve
cargo run --release -- --capture --out /tmp/shots              # colony: stock, dig, tap, shake
cargo run --release -- --capture --sand-only --out /tmp/shots  # the M1 sand test, no colony
cargo run --release -- --capture --title-shot --out /tmp/shots  # one frame of the title screen
cargo run --release -- --capture --splash-shot --out /tmp/shots # three frames across the studio mark
```

The output directory has to exist — the harness won't create it, and a missing one shows
up only as `Cannot save screenshot` buried in the log while every number still prints
happily.

Runs render to an **offscreen texture**, not the window, so a locked or sleeping screen
can't silently produce black frames — that cost an hour once. The two UI shots are the
exception and have to opt out of it in *both* directions: they grab the window, and main
must also skip `setup_offscreen_target` for them. Miss the second half and the only camera
is pointed at the texture, Bevy UI draws to whichever camera is on the window, and the
screenshot is a convincing sheet of black.

The number that matters most is total sand. It must stay exactly constant.

## Traps already paid for

Things that cost real time and will look like new bugs if forgotten.

- **`Single<T>` silently skips its system** when the query doesn't match exactly one
  entity. In an `OnEnter` system, which fires once, that means the thing never gets built
  and nothing errors. This ate the whole title screen once.
- **Ordo's buttons are `bevy_ui_widgets::Button`** and carry no `Interaction` component.
  Listen for the `Activate` event. A query for `Interaction` matches nothing while the
  button still lights up on hover, so it looks perfectly wired and does nothing.
- **Duplicate components in one bundle is a hard panic.** `radial()` already carries
  `Radial`; adding a second to seed state crashed on open.
- **Bevy only compiles in Vorbis by default.** `wav`, `mp3` and `flac` are opt-in features.
  A perfectly valid WAV panicked the audio system and left a window with nothing in it.
  Ship OGG.
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

**Releases are Brett's call, never a tidy-up step.** A tag reaches every installed
launcher, so tagging is publishing. Build, test and commit freely; stop at the tag and say
it's ready. When he asks:

```bash
git tag v0.1.1 && git push origin v0.1.1
```

`v0.1.0` is out, with all three platform assets attached.

## Not yet done

- Spoil-hauling rate (the open M2 task above) — 67% and worth pushing further.
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
