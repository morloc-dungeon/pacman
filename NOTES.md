# Notes

Design friction from building this, and the reasoning behind the conventions it
settled on. Bugs are in `FINDINGS.md`; this file is about the things that were
awkward rather than wrong. Read it before copying the pattern.

## A language-specific frontend cannot depend on the interface alone

`lib/tui` is written in Rust, so its `.rs` file must name `PacCommands` and
`PacState`. Those names come from the `record Rust => ...` mappings, which live
in `lib/pacman-rust` next to the engine. So the frontend imports the engine
module although it calls nothing in it, and the "the frontend names no
implementation" claim holds at the level of functions but not imports.

The clean split is a third module holding only the Rust representation -- the
`type Rust =>` and `record Rust =>` lines plus the struct definitions -- with
the engine and the frontend both importing that. It was not done here because
a module needs at least one sourced symbol to pull its `.rs` file in
(`source Rust from "f.rs" ()` is a parse error), so a representation-only module
needs a token function to exist at all. Worth doing if this pattern is used
again.

## The stdlib's shared test suites are not language-independent

`root/test`, `maybe/test` and the rest are described as language-independent and
reused by each implementation module, but they `source Py` their own harness.
Running `root-rust`'s suite therefore starts a Python pool in what is otherwise
a pure Rust program.

This demo splits the harness the same way as everything else: the signatures for
`testEqual` / `printMsg` / `printResult` are declared in the shared test module
and implemented in `lib/harness-rust`. Adding a `harness-py` would be four
lines. The stdlib would benefit from the same treatment.

## Test output should be on two streams

The stdlib harness prints per-test lines and the verdict to stdout, which makes
a golden-style `diff` of the result impossible -- the transcript is in the way.
Here the per-test lines go to stderr and only the verdict reaches stdout, so
`make test` can diff `test/exp.txt` and a failure still explains itself on the
terminal.

## A pool is a background job

The single least obvious thing about writing a TUI in morloc. The nexus spawns
each pool with `setpgid(0, 0)`, so the pool is in its own process group and the
terminal's foreground group belongs to the shell. All three standard fds are the
terminal and `/dev/tty` opens, but a raw-mode read from a background process
group raises `SIGTTIN` and stops the process.

`lib/tui/tui.rs` claims the foreground group on entry and restores it in a
`Drop` guard, ignoring `SIGTTOU` around both calls because taking the terminal
from a background group would otherwise stop the caller. The guard is sound
because a morloc Rust pool is built with `panic = "unwind"`.

`morloc-workspace/tooling/meco/t/ttyprobe` measured this and is the diagnostic
to re-run if terminal behaviour ever looks wrong.

## Effects can only be performed where a schema exists

`@save` and `@load` need a compiler-assigned `Schema`. `rustmorloc::save` is
public but takes one, and hand-written Rust has no way to obtain a schema for a
type by name. So a sourced Rust function cannot save a value, and persistence
has to happen at the morloc level.

That turned out to be the right shape anyway -- it is what forces the frontend
to be free of file IO -- but it is a constraint, not a choice, and it is why
`tuiRun` returns `(Bool, GameState)` instead of writing the file itself.

## Rust source files share one namespace

Every sourced `.rs` is `include!`d at the pool crate root alongside root-rust's
`core.rs`. Two consequences, both borrowed from `tooling/meco/CLAUDE.md`:

- Prefix everything: `pac_*` for functions, `Pac*` for types.
- No top-level `use`; the same `use` in two sourced files is `E0252`.

The second rule makes ratatui code unreadable, so `lib/tui/tui.rs` puts the
implementation in an inner `mod` with ordinary `use` statements and keeps only
thin `pub fn` wrappers at the crate root. Module-scoped imports cannot collide.
This works and is worth adopting wherever a sourced file needs more than a
couple of external names.

## Small gaps met along the way

- There is no `maybe-rust`, so `isNull` and `require` are unavailable to a
  Rust-only program. The demo does its optional handling in Rust instead.
- There is no `text-rust`, so `unlines` and friends are unavailable too; the
  test output uses `-f jsonl` rather than a `@render` that joins lines.
- The stdlib has no effectful `mapE_`. This demo declares `eachE` for itself.
- A single-command program's nexus options must precede `@`
  (`./pacman-tui-test -f jsonl @ 60 40`), and the command name is only accepted
  when no nexus option comes first. A two-command program takes the ordinary
  `./prog -f jsonl cmd args`.

## Glyphs are a frontend concern

`Frame.rows` is a tile map, not a picture: `#` wall, `.` dot, `o` energizer,
`<^v>` for Pac-Man, a letter per ghost. The interface says what is on each
tile and the frontend decides how it looks, so `lib/tui` draws walls as full
blocks, dots as middle dots and ghosts as a syllabics glyph, without the
engine knowing.
A web frontend would pick different glyphs from the same rows, and the test
suite keeps diffing plain ASCII.

The same separation buys the aspect ratio. A terminal cell is about twice as
tall as it is wide, so a one-column tile makes the board look stretched; the
frontend draws each tile two columns wide. That is a rendering decision, and it
touches nothing else.

`CONVENTIONS.md` allows Unicode in text that exists to be looked at, provided
meaning survives a terminal that cannot draw it. Here it does: a wall drawn as
a replacement box still reads as a wall, and every actor is also distinguished
by colour. The glyphs are written as `\u{..}` escapes, so the source file stays
ASCII while the screen does not.
