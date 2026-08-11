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

**M2 (the colony) works end to end but isn't finished.** Ants dig from flat sand, haul
spoil, and mass stays exact across all three places a grain can hide — grid, particle,
mandibles. What's still wrong is the *rate*: net excavated volume grows far slower than
the excavation count implies, because spoil still ends up redeposited more than it should.
That's the open task. Every number about it should be re-measured, since the colony
dropped from 120 workers to 11.

**M3 (demography, persistence) and M4 (Steam) are untouched.**

## Verify with the harness, not by eye

Emergent behaviour can't be judged from a screenshot, and guessing at it wasted several
rounds. Every claim about the colony should come from here:

```bash
cargo run --release -- --capture --out /tmp/shots            # colony: stock, dig, tap, shake
cargo run --release -- --capture --sand-only --out /tmp/shots # the M1 sand test, no colony
cargo run --release -- --capture --title-shot --out /tmp/shots # one frame of the title screen
```

Runs render to an **offscreen texture**, not the window, so a locked or sleeping screen
can't silently produce black frames — that cost an hour once. The exception is
`--title-shot`, which must grab the window because Bevy UI attaches to the camera drawing
to it; an offscreen grab shows the farm but never the menu.

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
  *nothing*. The theme names no fonts on purpose; Bevy's embedded default stands.
- **Agitation is added per rendered frame and decays per fixed tick.** Anything feeding it
  must be a per-second rate or the same gesture is twice as destructive at 120fps. This
  turned a moderate shake into a total collapse the first time it met a release build.
- **The cohesion model happily supports a one-cell-wide column** — a grain with something
  under it scores 3 against a threshold of 1.2. Correct for packed strata, wrong for
  anything freshly arrived, so *every* place new sand appears agitates locally to find its
  angle of repose. Miss it and you get spires: the colony built a chimney out of its own
  spoil and climbed it.
- **Digging rules collide in both directions.** Ban above-ground digging entirely and a
  colony on flat sand can never get underground, because the only way down is to dig.
  Allow it freely and they shuffle the same topsoil forever. Downward-only threads it.
- **On flat sand the working face is a valid dump site.** Spoil must be carried
  `MIN_HAUL_DISTANCE` from where it was dug, or 845 excavations produce a farm with no
  tunnel in it.
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

Release a build with:

```bash
git tag v0.1.1 && git push origin v0.1.1
```

## Not yet done

- Spoil-hauling rate (the open M2 task above).
- Idle CPU: the *sim* sleeps when settled, the render loop doesn't. An always-running
  ambient game needs both.
- Nobody has played this with a hand on a mouse for long. Drag weight, the tap/shake
  threshold and the hold-to-open delay are all feel, and all unverified by feel.
- The 31MB source WAV is still in git history; removing it needs a force-push.
- iPad: input is deliberately a thin adapter that only writes agitation and alarm, so a
  touch/accelerometer front end replaces that adapter and nothing else. Keep it that way.
