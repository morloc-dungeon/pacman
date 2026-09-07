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

## Levels are plugins

The map for a level is one function:

```morloc
makeMaze :: Int -> [Str]
```

Level 1 is the arcade board; every level after it is generated from its number,
so the same number always deals the same map. Replacing that one signature is
the whole of writing a new level designer -- hand-drawn boards from a file, a
different generator, a shape that spells your name. Nothing else moves: not the
rules, not the frontend, not a line of the test suite.

What keeps that safe is a second function that says what a map has to be:

```morloc
mazeProblems :: [Str] -> [Str]
```

It returns everything wrong with a map and nothing when it is fit to play --
right size, symmetric, walled in, a ghost house with a door the ghosts can get
out of, a tunnel, somewhere for Pac-Man to stand, and every pellet reachable
from where he starts. The suite checks the generator against *that*, not against
a picture of one board, so a replacement generator is judged by the same rule
the current one is. It has already earned its keep: it caught a generated map
that put a dot on Pac-Man's starting tile but not on its mirror, which no
eyeball would have noticed.

How hard a level plays is its own function, of the same shape:

```morloc
levelDifficulty :: Int -> Difficulty
```

It returns everything that changes with the level number and nothing that does
not -- how often a ghost drops a frame, how long an energizer frightens, how
long the ghosts wait in the house, how the scatter and chase phases divide up.
Swap it and you have retuned the whole game without touching the rules, and
`difficultyProblems` guards it the way `mazeProblems` guards a map. Level one
returns the arcade's opening pace exactly, which is how the suite can tell that
introducing a curve changed nothing about the board everyone already knows.

The two are deliberately independent, and both are independent of the rules. A
map does not know how fast the ghosts are, a difficulty does not know what the
maze looks like, and neither knows what happens when Pac-Man meets a ghost.
Replace one and you have a plugin; replace them all and you have a different
game on the same engine.

## What it plays

The arcade first level: the real 28x31 maze, 240 dots and 4 energizers, four
ghosts with their original targeting (Blinky chases, Pinky aims four tiles
ahead, Inky takes the doubled vector from Blinky, Clyde breaks off within eight
tiles), the scatter/chase schedule with its forced reversals, frightened mode,
the ghost house, and the side tunnel.

Levels after the first keep those rules and change the pace: the ghosts drop
fewer frames, the house empties sooner, the scatter breaks shorten, and an
energizer frightens for less until, from the thirteenth board, it only scores.

Deliberately not included: fruit, the arcade's per-ghost dot counters for
leaving the house (fixed timers instead), and its fractional speed tables (a
ghost either takes a frame or does not). Nothing is random except which way a
frightened ghost turns, and that runs off a seed in the game state, so a game
replays exactly.

## Playing

```
make build
./pacman run                 # resume or start, saving to pacman.sav
./pacman run -f mygame.sav   # use another file
```

Arrows or `hjkl` to move, `s` to save and quit, `q` to quit without saving.
Clearing a board shows a tally and offers `c` to go on to the next level or `q`
to stop; running out of lives shows the same tally with only `q`. Leaving by `q`
or `s` skips it; you already know how it went. Score and lives carry across
levels.
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
