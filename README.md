# pacman

Pac-Man in a terminal. The rules are a pure morloc interface, the terminal
frontend receives that interface as a record of functions, and the two never
mention each other.

The game is the excuse. The subject is a way of structuring a morloc program
that we expect to reuse: **an interactive shell that is handed its logic as
data.**

## The pattern

```
  lib/pacman          the interface: types, the PacmanCommands record, and
                      bare signatures. No implementation, no language mappings.

  lib/pacman/test     the test suite. Imports the interface only, so it runs
                      unchanged against any implementation of it.

  lib/pacman-rust     the engine: Rust representations for the interface's
                      types, and Rust functions satisfying its signatures.

  lib/harness-rust    the test harness, split the same way.

  lib/tui             the frontend: `tuiRun :: PacmanCommands -> GameState ->
                      <IO> (Bool, GameState)`.

  main.loc            the only file that names both an engine and a frontend.
```

The interface's central declaration is a record whose fields are functions:

```morloc
record PacmanCommands where
  newGame :: Int -> GameState
  keyOf   :: Str -> Command
  step    :: Command -> GameState -> GameState
  view    :: GameState -> Frame

commands :: PacmanCommands
commands = {newGame = newGame, keyOf = keyOf, step = step, view = view}
```

`main.loc` hands `commands` to `tuiRun`. That single value is the whole contract
between them, and three properties follow from it:

- **The frontend calls no engine function by name.** Swapping in a `pacman-py`
  is an import change in `main.loc` and `test.loc`; no other file moves.
- **The engine performs no IO.** Every field is pure, so a frontend can render,
  replay, or rewind a game without asking permission -- and the test suite
  drives the whole game as a list of commands.
- **The frontend performs no file IO.** `main.loc` owns the save file. The TUI
  owns the terminal and nothing else.

Because everything crossing that boundary is pure and total, the same engine
drives the interactive game, the test suite, and the off-screen renderer with
no conditional compilation and no test doubles.

## What it plays

The arcade first level: the real 28x31 maze, 240 dots and 4 energizers, four
ghosts with their original targeting (Blinky chases, Pinky aims four tiles
ahead, Inky takes the doubled vector from Blinky, Clyde breaks off within eight
tiles), the scatter/chase schedule with its forced reversals, frightened mode,
the ghost house, and the side tunnel.

Deliberately not included: levels past the first, fruit, the arcade's per-ghost
dot counters for leaving the house (fixed timers instead), and its fractional
speed tables (Pac moves every frame, ghosts three frames in four, frightened
ghosts one in two, eyes every frame). Nothing is random except which way a
frightened ghost turns, and that runs off a seed in the game state, so a game
replays exactly.

## Playing

```
make build
./pacman run                 # resume or start, saving to pacman.sav
./pacman run -f mygame.sav   # use another file
```

Arrows or `hjkl` to move, `s` to save and quit, `q` to quit without saving.
The board is drawn two terminal columns per tile, because a cell is about twice
as tall as it is wide and a one-column tile looks stretched. Which glyph stands
for which tile is a frontend decision -- the engine returns a tile map.

The maze is 31 rows and that is not negotiable, so the window needs 32 rows and
40 columns at the very least. Above that the frontend spends what it has: the
blank line under the board, then the frame, then the key legend (which
compresses onto the status line before it disappears) are given back as the
window shrinks. Below the minimum it says what it needs rather than drawing a
clipped board.
A saved game is the `GameState` record written by `@save`, so it is an ordinary
morloc object -- `pacman-test showSave` reads one back and prints the board.

It must be run from a real terminal. A morloc pool runs in its own process
group, which makes it a background job as far as the terminal is concerned, so
the frontend claims the terminal's foreground process group on the way in and
hands it back on the way out. Without that a raw-mode read takes `SIGTTIN` and
the game would stop rather than start. See `NOTES.md`.

## Testing

```
make test
```

`test.loc` is four lines: it imports the suite from the interface and the two
Rust implementation modules. The suite itself never mentions Rust.

Per-test output goes to stderr and the verdict to stdout, so `make test` diffs
the verdict against `test/exp.txt` while a failure still explains itself. The
test run also writes a save file with one program and reads it back with
another, and renders a frame through ratatui's off-screen backend, so the
drawing code is covered without a terminal.

42 tests cover key mapping, the starting board, movement and walls, the tunnel,
energizers and frightened ghosts, dying, and what a save file is allowed to
contain. Eating a frightened ghost is not covered: frightened ghosts flee, and
no short scripted route catches one.

The interactive path cannot be reached from `make test`, so `test/tty-drive.py`
drives the real game under a pseudo-terminal and prints what it drew:

    python3 test/tty-drive.py hhhhhhhjjjs   # walk west, turn down, save and quit
    python3 test/tty-drive.py q             # start and leave

It is timing-based, so it is a manual check. It is also what caught a saved
game being stored in its `SAVED` end state, which made resuming land on a
finished board.

## Findings

`FINDINGS.md` records the compiler bugs this demo ran into and the workarounds
in the tree; each workaround site is marked `FINDINGS #n`. `NOTES.md` records
the design friction -- the things that were awkward rather than wrong.
