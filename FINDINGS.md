# Findings

Bugs and friction found while building this demo. Each entry says what it
blocked and, where a workaround is in the tree, how to undo it once the bug is
fixed. Search the source for `FINDINGS #n` to find every workaround site.

---

## 1. Rust pool: a generated thunk moves every non-Copy local it touches

- Status: FIXED, compiler 6338b8e7 ("rust: capture an effect thunk by value only
  when it escapes"). A thunk now captures by value only where the frame's
  signature lets a closure leave it; everywhere else it borrows. Golden
  `rust-thunk-capture`. The workaround below is still in this tree and can now
  be undone.
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

### Cause (found while fixing finding 1, not yet fixed)

`rustMakeIf` (`Members/Rust.hs:1699`) annotates the conditional's result
binding with `rustTypeOf (typeFof origExpr)`, and `rustTypeOf` erases the effect
row -- an `<IO> Int` renders as `i64`. But a deferred effect is represented as a
nullary thunk, so the arms are `impl Fn() -> i64`, and the binding is declared
with the value type the thunk would produce rather than the thunk's own type:

```
78 | unsafe fn m1355() -> impl Fn() -> i64 {
   |                      ---------------- the found opaque type
99 |         n2
   |         ^^ expected `i64`, found opaque type
```

The sibling `rustMakeLet` already handles this: `isFunctionTypeF` reports true
for an `EffectF` type and the let omits its annotation, letting Rust infer the
closure. `rustMakeIf` consults nothing. A recursive function escapes because the
tail-recursion lowering builds the conditional differently.

This is a **different defect from finding 1**, which is about how a thunk
captures. Both are consequences of representing a deferred effect as a Rust
closure without one discipline covering every place such a value is bound, but
they are separate fixes and neither implies the other.

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

---

## 5. Rust pool: a thunk moves a value another thunk is still borrowing

- Status: open
- Component: compiler (Rust member)
- morloc: 0.102.1
- Found while fixing finding 1; it is a **different** defect and finding 1's fix
  does not touch it.

### Observed

When one `@catch` arm borrows a value and the other yields it, the second
closure moves what the first still holds:

```morloc
module main (f)

import root-rust

source Rust from "own.rs" ("rt_nonempty" as nonEmpty)
nonEmpty :: [Int] -> <Err> [Int]

f :: [Int] -> <IO, Err> [Int]
f v = @catch (nonEmpty v) v
```

```rust
pub fn rt_nonempty(xs: &Vec<i64>) -> Vec<i64> {
    if xs.is_empty() { rustmorloc::morloc_throw("rt_nonempty: empty"); }
    xs.clone()
}
```

```
error[E0505]: cannot move out of `n15` because it is borrowed
248 |     let n15: Vec<i64> = rustmorloc::get_value::<Vec<i64>>(s15, schema(1));
249 |     let n17 = m1631(&(n15));
    |                     ------ borrow of `n15` occurs here
250 |     return rustmorloc::put_value(&(rustmorloc::mlc_catch(n17, || { n15 })), schema(1));
    |                                    ---------------------      ^^   --- move occurs due to use in closure
```

### Guess (unverified)

The Rust member already classifies a value used at more than one point as
*shared* (`varUseCountOps` / `sharedIndicesSM` in `Members/Rust.hs`), and a
shared non-Copy value is meant to be borrowed at reference sinks and cloned at
owned sinks, never moved. The thunk's result position looks like it is not
treated as an owned sink, so the clone is not inserted. Either the use count
does not see through the thunk body, or the body's tail is not run through the
ownership adaptation.

### Not covered by a test yet

Deliberately left out of the `rust-thunk-capture` golden, which covers how a
thunk *captures*; this is about what it *yields*. It needs its own test
alongside the fix.

---

## 6. A dead IR node gives three printers a second, contradictory thunk emitter

- Status: open
- Component: compiler
- Found while fixing finding 1.

### Observed

`IExpr(IDoBlock)` (`Grammars/Translator/Imperative.hs:144`) has **no producer
anywhere in the compiler**. Every effect thunk is emitted by `lcMakeDoBlock`
instead. Three printers nonetheless implement `IDoBlock`, and two of them
contradict the live emitter for their own member:

| site | dead `IDoBlock` says | live `lcMakeDoBlock` says |
|---|---|---|
| `Members/CppPrinter.hs:96` | `[&](){...}` capture by reference | `[=](){...}` capture by copy (`Members/Cpp.hs:886`) |
| `Members/RustPrinter.hs:92` | `move \|\| { ... }` | now conditional (`Members/Rust.hs`) |
| `Grammars/Translator/Generic.hs:1085` | template-driven | template-driven |

### Impact

It is an active trap rather than mere clutter. Reading `CppPrinter.hs:96` says
C++ captures by reference, which is false and is the opposite of the safety
property the live code depends on -- it cost time on exactly this bug. And
`RustPrinter.hs:92` is a second, independent `move` emitter, so the next person
fixing thunk capture in the Rust member can change it, observe no effect, and
conclude the fix does not work.

### Fix

Delete the constructor and its three printer cases. Left out of the capture-mode
fix deliberately: removing a shared IR constructor is a separate change from
correcting one member's capture semantics.
