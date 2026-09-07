# Findings

Bugs and friction found while building this demo. Each entry says what it
blocked and, where a workaround is in the tree, how to undo it once the bug is
fixed. Search the source for `FINDINGS #n` to find every workaround site.

---

## 1. Rust pool: a generated thunk moves every non-Copy local it touches

- Status: open
- Component: compiler (Rust member)
- morloc: 0.102.1, cargo 1.98.0
- Severity: blocking. Any Rust morloc program that writes a file and then keeps
  using the values it wrote hits this.

### Observed

`RustPrinter.hs:92` (`printExpr (IDoBlock e) = "move || { " ...`) and
`Rust.hs:1564-1565` (`lcMakeDoBlock`) emit every generated thunk as a `move`
closure. The thunks are immediately invoked -- `(move || { .. })()` -- or handed
straight to `rustmorloc::mlc_catch`, so nothing escapes the frame and the `move`
buys nothing. But `move` captures *every* referenced variable by value, so a
non-Copy local the thunk only borrows is moved out and every later use of it is
`error[E0382]: borrow of moved value`.

Nine lines reproduce it:

```morloc
module main (f)

import root-rust

f :: Str -> <IO, Err> U64
f p = do
  @save 0 p [1, 2, 3]
  xs <- @load p :: <IO, Err> [Int]
  size xs
```

```
error[E0382]: borrow of moved value: `n0`
82 |     let n1363: () = (move || {
   |                      ------- value moved into closure here
84 |         rustmorloc::save_voidstar(&(n3), schema(1), (0 as i64), &(n0))
   |                                                                  ---- variable moved due to use in closure
85 |     })();
86 |     let n2: Vec<i64> = rustmorloc::load::<Vec<i64>>(schema(1), &(n0));
   |                                                                ^^^^^ value borrowed here after move
```

The generated code already clones where it needs an owned value
(`&(n1.clone())` inside the closure), so the outer `move` is the only thing
taking ownership.

### Scope (measured)

- Both `@save` statements and both `@catch` arms are affected. `@catch (@load p) dflt`
  twice over the same `p` fails the same way.
- It is not confined to the top dispatch manifold: the tail-recursion lowering
  hits it too, on the `let mut n2: String` the loop owns.
- Isolating the `@save` in its own morloc function does **not** help -- morloc
  inlines it back into the caller's manifold.
- Effectful *sourced* calls are fine: they become their own `fn` with no
  captures.

The practical rule today: **a non-Copy value may be touched by at most one
generated thunk in a manifold, and never used afterwards.**

### Guess (unverified)

Dropping `move` should be both sufficient and safe. Rust 2021 infers captures
per variable from use, so a thunk that only borrows would borrow, and a thunk
that returns a captured value by value would still capture it by value and infer
`FnOnce`. Nothing here outlives the frame.

### Workaround in this demo

The game saves on exit rather than mid-play: the TUI returns `(True, state)`
when the player asks to save, and `main.loc` writes the file as the last
statement of `run`, with nothing using `state` afterwards. Mid-game save
requires re-entering the TUI after the write, which needs the state after the
`@save` thunk and does not compile.

*Undo:* `lib/tui/tui.rs` and `main.loc`, marked `FINDINGS #1`. Restore the
`playLoop` / `resume` recursion described in the plan.

---

## 2. Rust pool: a non-recursive guard with effectful arms emits unforced closures

- Status: open
- Component: compiler (Rust member)
- morloc: 0.102.1, cargo 1.98.0
- Severity: blocking. There is no other conditional in the language, so this is
  "you cannot write `if` around an effect" in a Rust pool.

### Observed

A guard whose arms are effectful lowers each arm to a closure and then never
forces it, so the `if` yields a closure where the manifold expects the value:

```morloc
module main (f)

import root-rust

source Rust from "own.rs" ("rp_emit" as emit)
emit :: Int -> <IO> Int

f :: Bool -> <IO> Int
f b
  ? b = emit 1
  : emit 2
```

```
error[E0308]: mismatched types
   --> src/main.rs:329:9
328 |        let helper0: () = if (n26) {
    |  ________________________-
329 | |/         move || {
...
334 | ||         }
    | ||_________^ expected `()`, found closure
```

### Scope (measured)

- Fails with `<IO> Int` and with `<IO> ()`; with two sourced calls, with an
  intrinsic (`@save`) in one arm, and with a `do` block in one arm. The arms do
  not have to differ in purity.
- A **pure** guard returning `()` builds fine.
- A **recursive** guard with effectful arms builds fine -- the tail-recursion
  lowering forces the arms. The golden `recursion-loop-io-rust` still passes,
  so this is a gap in the plain conditional lowering, not a regression in that
  path.

This is broader than the note recorded in `tooling/meco/CLAUDE.md`, which
describes it as a guard "whose arms mix a pure value and an effectful one". Two
effectful arms fail just as reliably; what matters is whether the enclosing
function recurses.

### Workaround in this demo

There is no morloc-level conditional that survives, so the branch moves into
Rust and is carried as data: a pure function returns a list of the games that
should be written (empty or one), and an effectful action is applied to each.
See `toSave` and `eachE` in `lib/pacman/main.loc`, marked `FINDINGS #2`.

*Undo:* replace `eachE (toSave r) saveOne` in `main.loc` with the guard, and
delete `toSave` / `eachE`.

---

## 3. An optional argument `?T` is still required on the command line

- Status: open
- Component: nexus (CLI)
- morloc: 0.102.1
- Severity: moderate. `?T` is the obvious way to spell an optional argument and
  it does not work; the failure is at the CLI, not in the type.

### Observed

```morloc
module main (f, g)

import root-rust

source Rust from "own.rs" ("ro_show" as f, "ro_two" as g)
f :: ?Str -> Str
g :: Int -> Int
```

```
$ ./ro f
error: the following required arguments were not provided:
  <arg0>
$ ./ro f hello
"got hello"
```

`--help` lists the argument under `Positional arguments:` with `type: ?Str`, so
the optionality is known and simply not acted on. Passing the literal `null`
works, so the wire side is fine.

Two smaller things in the same output: the error names `<arg0>` rather than the
argument's `@metavar`, and a single-command program prints
`Usage: <prog> <nexus_options> @ <command_options>` rather than naming its one
command and its flags.

### Workaround in this demo

The save file is a `Str` behind a type alias carrying `@arg -f/--file` and
`@default "pacman.sav"`, which produces a genuine optional flag. This is nicer
than the `?Str` positional would have been, so it is not marked as a workaround
in the source -- but `?T` was the first thing tried.

---

## Note: these were found against a modified compiler

The compiler working tree at `morloc-workspace/compiler/morloc` has uncommitted
changes while this demo was built (another session is working on
`CodeGenerator/Reduce.hs`), and the installed `morloc 0.102.1` was built from
them. One visible symptom is a `MECOTRACE ...` dump on stderr from a
`Debug.Trace.trace` at `Reduce.hs:150`, tens of kilobytes per build, on any
program with an `EvalN` over a record.

That trace is someone's live debugging and is not reported here as a defect.
It does mean every reproduction above should be re-run against a clean build
before being treated as settled -- particularly finding 2, since the
in-flight edit is to the force-cancellation logic that decides whether a guard
arm gets forced.

---

## 4. The manual describes local import resolution backwards

- Status: open
- Component: docs
- Where: `docs/morloc-project.github.io/src/content/features-modules.asc:100`
  and `:179-188`

### Expected (what the manual says)

> The dot prefix tells the compiler to look for the module relative to the
> directory of the importing file, not in the system library.

and, with a worked example:

> Local modules can also import other local modules. The path is always
> relative to the importing file. For example, if `bar/baz/main.loc` needs to
> import a sibling at `bif/biz/`, it writes `import .bif.biz (mul)`. This
> resolves relative to `bar/baz/`, looking for `bar/baz/bif/biz/main.loc`.

### Observed

A dotted import resolves relative to the **project root** -- the directory of
the entry file passed to `morloc make` -- not the importing file.
`Frontend/API.hs:95` sets `stateProjectRoot` from the entry file's directory and
`Module.hs:376` joins every dotted import onto it.

The manual's own example is the counter-example. The golden test that exercises
it, `test-suite/golden-tests/local-import-cousin-py`, puts `bif/biz/` at the
project root, not at `bar/baz/bif/biz/`, and passes:

```
local-import-cousin-py/
  main.loc
  bar/baz/main.loc      <- contains `import .bif.biz (mul)`
  bif/biz/main.loc      <- resolved here, from the ROOT
```

This demo is a second witness: `lib/tui/main.loc` imports its sibling as
`import .lib.pacman`, not `import .pacman`.

### Impact

Anyone laying out a multi-module project from the manual will write imports
that do not resolve, and the error will look like a missing module. The rule to
document is: `source` paths are relative to the file that names them, dotted
imports are relative to the project root.
