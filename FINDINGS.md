# Findings

Bugs found while building this demo, and the ones found later while reading its
own list of open items. Eleven of the fourteen are fixed. The three open
ones are all in `data` types, which arrived after the rest of this tree was
written. One marker remains: `FINDINGS #12`, on the line that keeps the
engine's enum out of the compiler's hands.

Numbers are permanent and are never reused.

---

## 1. A generated thunk moved every non-Copy local it touched

*Fixed: compiler `6338b8e7`.*

Every effect thunk in a Rust pool was built as a `move` closure, so a value it
merely read was consumed and could not be read again. Writing a file and then
using the path, or naming one value in both arms of a `@catch`, would not
compile.

The Rust member was mirroring the C++ member, which captures by copy for a real
reason: a thunk that is a manifold's return value outlives the frame and must
own what it names. But a copy leaves the original intact and a move does not. A
thunk now captures by value only where the frame's signature lets a closure
escape, which is exactly the manifold return, and by reference everywhere else.
The predicate is read from the same type that produces the signature, so the
capture mode and the return type cannot disagree.

Golden: `rust-thunk-capture`.

---

## 2. A conditional with effectful arms did not compile

*Fixed: compiler `65ae257e` and `68e23a32`.*

There is no conditional in morloc but the guard, so this was "you cannot write
`if` around an effect" in a Rust pool. Only a recursive guard worked, because the
tail-recursion lowering builds one differently.

A deferred effect is carried as a nullary thunk, and two arms producing thunks
produce two different thunk types that no single binding can name. Both
positions a conditional can occupy needed a rule, and the second only surfaced
when this demo's save decision was rewritten as an ordinary guard:

- forced on the spot, the force distributes over the arms and the conditional
  becomes eager, yielding the value type the arms agree on;
- returned rather than used, the suspension commutes outwards into one thunk
  whose body is the eager conditional.

The narrower rule these replace recognised only arms already wrapped as thunks
and gave up on the rest.

Golden: `rust-effect-conditional`.

---

## 3. An optional argument was required on the command line

*Fixed: compiler `6c54f939`.*

A program's three published views disagreed about its own interface: the JSON
help and the MCP tool shapes both reported an optional argument as not required,
while the parser demanded it. Omitting one now means null, which for a string
argument is the only way to express null at all -- a bare `null` on the command
line is the four-letter word.

Two things came out of pushing on it. An absent argument is dispatched without
the source and format vocabulary, which describes how to read a token and would
otherwise have sent an argument declared as a file path looking for a file named
`null`. And a required positional may no longer follow an optional one, since an
omitted argument can only be the last.

Goldens: `optional-json`, `optional-positional-order`.

---

## 4. The manual described local imports backwards

*Fixed: docs `4d55bd3`, report `0072` closed as not-a-bug.*

The manual said three times that a dot-prefixed import resolves against the
directory of the file that writes it. It resolves against the project root. The
worked example was the compiler's own regression test for the feature with the
wrong rule attached, resolving to a path that cannot exist.

The same block listed the two candidate paths in the wrong order. And the rule
readers confuse this one with -- a `source` path resolves against the file that
names it -- was documented nowhere, so it is now stated alongside.

---

## 5. A thunk moved a value another thunk was still reading

*Fixed: compiler `65ae257e`.*

`@catch (f v) v` did not compile: one arm reads the list while the other yields
it, and yielding took it away from the reader.

A thunk's result leaves the thunk by value, which makes it an owned sink like a
container element, and every other owned sink already copies a value it does not
own. A thunk's body was not passed through that adaptation at all.

Golden: `rust-thunk-capture`, the `armShare` case.

---

## 6. A dead node gave three printers a contradictory thunk emitter

*Fixed: compiler `65ae257e`.*

An instruction in the intermediate language had no producer anywhere and three
printers implementing it, two of which contradicted the live emitter for their
own language -- one claiming a capture by reference where the real one captures
by copy, the other a second and by then divergent copy of the thunk emitter.

Not a runtime defect, but an active trap: it cost time on finding 1, and anyone
fixing findings 2 or 5 would have been sent to the wrong place. Deleted.

---

## 7. A setter gave its values to the wrong fields

*Fixed: compiler `9c826eea`.*

Silent data corruption in four languages. A setter naming more than one field
walked the record's declared fields and handed each the next unused value, so
the names the author wrote chose only which fields were touched, never which
value each received. Written in declaration order the two orderings agree, which
is why every test had passed; written any other way the values land shuffled,
and when the fields share a type nothing downstream can notice.

**My first diagnosis of this was wrong.** I recorded it as depending on the
record being user-mapped. It does not: the axis is whether the setter is
evaluated in a language pool or in the nexus, whose evaluator walks the pattern
and was always right. Records and tuples both, in Rust, C++, Python and R,
because they share one loop.

Values are now attached to the paths they were written at before the rebuild
starts. That also settled a second silent failure the fix would otherwise have
left: two writes beneath one field reached that field as a single name and only
the first was honoured.

Goldens: `pattern-setters` (Python, R, C++), `rust-patterns`,
`setter-overlapping-paths`.

---

## 8. A setter's receiver was read from the wrong end of the argument list

*Fixed: compiler, uncommitted.*

A pattern application puts its receiver at one end for a getter and the other
for a setter: a getter is `pat bounds.. receiver`, a setter is
`pat receiver values..`. Codegen's dispatcher read the last argument in both
cases, so on a setter it took the last value written and made every
receiver-keyed decision about that instead.

With a `PatternAccessible` instance on the value's type, the whole setter was
replaced by a call to the user's `__extract_pattern__` walker, applied to the
value; the receiver did not appear in the generated code at all. The same
argument fed the IFile check, which would have routed a setter to the file
walker had a set value ever been file-typed.

A setter now takes the structural path unconditionally. Both walkers the
dispatcher can route to only read -- the IFile walker reads a file, and
`__extract_pattern__` extracts -- and a setter rebuilds its receiver, which
neither can express.

Golden: `setter-receiver-position`.

---

## 9. A selector naming both a field and a slot crashed the compiler

*Fixed: compiler, uncommitted.*

`.(.a, .0)` raised a Haskell `error` with a call stack and no source location.
A group's entries all read the same value, and that value is either a record or
a tuple, so the group is a source error -- it now says so, at the caret, naming
both the field and the slot it found.

Unit tests: `pattern selector shapes`.

---

## 10. A record was rebuilt in one field order and read back in another

*Fixed: compiler, uncommitted.*

Silent data corruption, and not confined to setters.

A record's field order is its layout on the wire, so every node carrying the
record's type has to agree about it. A pattern selector's structural type does
not: it is built from the keys the selector names, sorted. Two paths let that
sorted order become the layout of a value laid out by something else.

A check that ends in synth-and-subtype annotates the node with the expected
type, and between two record types with the same keys it took the expected one
outright. So `.(.zed, .alpha) (.1 (0, r))` relabelled the record with the outer
selector's sorted keys while its bytes stayed in the order the tuple wrote
them, and the two fields swapped. No setter is involved in that one.

The other path is the setter's. A setter is desugared into a lambda so it can
also be written unapplied, as in `map .(.x = 1) xs`. Applied directly, the
wrapper is synthesised before its argument, so the setter's own rule -- which
reads the receiver's field types before checking anything against them -- met a
bare parameter where the receiver should have been. The rebuilt record's type
then kept only the fields the selector named. Where the widths disagreed the
runtime refused it as `Expected setter return and input sizes to be the same`;
where they agreed, the fields swapped and nothing said so.

The fix reduces the redex before either direction of the typechecker looks at
it, derives a setter's result type from the receiver it actually checked rather
than from the selector's structural type, and re-lays a *structural* expected
record into the order the synthesised type uses. A declared record is never
re-laid-out: its order is the one its per-language forms are written against.

Goldens: `pattern-record-field-order`, `pattern-setters-crosspool`, and the
in-memory column of `ifile-record-patterns`.

---

## 11. A setter could not write a plain value into an optional field

*Fixed: compiler, uncommitted.*

`.(.a = "new") r`, where `a :: ?Str`, was rejected with "Cannot compare types
?Str and Str". `Null` worked, and so did an annotated `("new" :: ?Str)`, which
is the shape of a widening that never happened.

Same root as finding 10: with the receiver hidden behind the desugar's lambda,
there was no field type to check the value against, so it froze at what it
synthesises to alone. Reducing the redex gives the value the field's type to
widen into. A setter under a signature seats the expected type into the value
slots first, which is what lets an integer literal reach a fixed-width field.

Unit tests: `pattern selector shapes`.

---

## 12. An unmapped `data` type cannot reach a Rust pool

*Open.*

A `data` type with no `data Rust => T = "..."` mapping is generated by the pool
that uses it -- the enum definition and its voidstar impls both. The definition
comes out right. Every use of a constructor does not: the constructor's
concrete type is resolved to `Vec<$1>`, an unsubstituted native-type template,
so the pool emits

    let n1: Vec<Color> = morloc_replicate((3 as i64), &(Vec<$1>::Red));

and cargo answers `error: expected expression, found $`. Five lines reproduce
it:

```morloc
module main (go)
import root-rust
data Color = Red | Green | Blue
go :: [Color]
go = replicate 3 Red
```

Any position that puts a constructor into a pool does it -- an argument to a
sourced function, an argument to a morloc intrinsic, `x == Red`. Constructors
that stay in the nexus are fine, which is what hides it: a bare constructor
export, a list literal of them, and a `|`-pattern over one are all evaluated
nexus-side and never reach a pool at all. Adding the mapping fixes it, and the
same program is correct in a Python or an R pool, so it is the Rust member's
resolution of an unmapped `data` type and nothing wider.

A second, smaller thing rides along: `printRustEnum` and `printRustStruct` emit
their definitions without `pub`, so a sourced `pub fn` that names one draws
`type Color is more private than the item pac_key_of` from rustc. Only a
warning, and moot while the first problem stands.

*Worked around here.* `lib/pacman-rust` carries `data Rust => Command =
"PacCommand"` and `engine.rs` writes the `#[repr(u8)]` enum out by hand, with
its nine discriminants restated. That restatement is what the mapping costs:
the declaration order in `lib/pacman/main.loc` is the wire contract, and
nothing checks the Rust side against it.

---

## 13. An unmapped `data` type is never defined in a C++ pool

*Open.*

The same shape as finding 12 and a different cause. The Rust member emits a
definition for a `data` type the pool owns; the C++ member emits none, although
`cppTypeOf` says one is "generated for this pool". A use site is rendered
anyway, so g++ reports

    pool.cpp:1055:9: error: 'Color' was not declared in this scope

with no indication of what the author should have written. C++ already requires
an explicit mapping for records, and that path says so in the compiler's own
voice -- "Recursive record without an explicit C++ concrete type mapping is not
yet supported. Add `record Cpp => <Name> = "<struct-name>"`". An enum should
either get the definition or get that sentence.

Not worked around here: this demo has no C++ pool.

---

## 14. In an R pool, an enum value never equals its own constructor

*Open. Silently wrong answers.*

`x == Red` returns `FALSE` for every `x`, including `Red`.

An enum has two R forms and they are not the same object. A value off the wire
is a factor -- integer codes plus a `levels` attribute, built by
`attach_factor_levels` in `rmorloc.c`. A constructor literal in generated code
is a character string, because R sets `ldEnumLitByName: true` in its
`lang.yaml`. `morloc_eq` strips attributes and compares `typeof` before
anything else; a factor unclasses to `integer` and the literal is `character`,
neither is numeric, so it returns `FALSE` and stops.

```morloc
module main (isRed)
import root-r
data Color = Red | Green | Blue
source R from "h.R" ("mono" as mono)   -- mono <- function(x) x
mono :: Color -> Color
isRed :: Color -> Bool
isRed x = (mono x) == Red
```

    ./prog isRed Red     -> false

The same program answers correctly in a Python pool and in a Rust one.
Equality was deferred deliberately when `data` landed, but it is deferred by
being unspecified rather than by being rejected: `==` typechecks over an enum,
dispatches to each backend's native comparison, and three of them agree while
R disagrees without saying so.

The split is wider than `==`. A sourced R function sees a different R type
depending on where its argument came from:

    describe <- function(x) paste(class(x), as.character(x))

    describe x        , x from the command line  ->  "factor Green"
    describe Green    , the literal              ->  "character Green"

So an R function that reads `levels(x)`, or dispatches on `is.factor`, is
correct for one and wrong for the other. The encoder hides it in the simple
case: `rmorloc.c` accepts "a factor or a constructor name" on the way out, so a
literal that is only passed through still serializes to the right value.

Either the literal should be emitted as `factor("Green", levels = ...)`, so
that one morloc type is one R type, or `==` over a `data` type should be
refused until it is specified. It should not keep answering.

---

## Still open, found along the way

Not part of the fourteen, not fixed, and not caused by any of this work.
Recorded here because they were found here.

- **A closed set is advertised to a machine and not to a person.** For an
  argument of a `data` type, `--json-help` carries `"enum": ["Red","Green",
  "Blue"]` and the runtime rejects a wrong value by name -- `'Purple' is not a
  constructor of this type; expected one of Red, Green, Blue`. The terminal
  help for the same argument says only `1:  type: Color`. The set is known at
  both places; only one of them prints it.

- **A Rust pool panics at teardown.** Any Rust program, including a two-line
  one, prints `cannot access a Thread Local Storage value during or after
  destruction` followed by `panic in a function that cannot unwind`. The result
  is correct and the exit status is zero, which is why no golden catches it:
  they diff stdout, and `rust-basic` writes fourteen panic lines to `obs.err`
  while passing. Predates this work.

- **Four defects found while reading and left alone**, each independent of the
  fixes above: an option's own null default still travels through the file and
  format vocabulary; `--json-help` calls every record argument required while
  the MCP view does not; an unrolled record behind an optional type silently
  loses its directives; and a thunk-valued conditional arriving at a
  serialization sink is never forced, because the forcing helper matches only a
  bare thunk.

---

## Withdrawn

- **An eta-expanded record setter is wrong in the R pool.** Recorded here as
  `map .(.x = v) rs` giving the wrong answer in R and the right one in Python
  and C++. It does not reproduce. Twenty-five shapes -- captured and literal
  values, one field and several, declaration order and reverse, nested paths,
  single-field records, receivers built by an R source function, a map inside a
  map -- agree across all three languages and are right in all three. Finding 7
  landed between the observation and this check and shares the machinery, which
  is the likeliest explanation. The shape is now pinned by
  `pattern-setters-eta`, which runs every one of those in Python, R and C++, so
  a regression would be caught rather than re-discovered.
