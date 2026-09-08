# Findings

Bugs found while building this demo. All seven are fixed. Nothing in this tree
works around a compiler defect any more -- every `FINDINGS #n` marker is gone
and the code that carried one is written the way it wanted to be written, which
is the check that the fixes actually landed.

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

## Still open, found along the way

Not part of the seven, not fixed, and not caused by any of this work. Recorded
here because they were found here.

- **A Rust pool panics at teardown.** Any Rust program, including a two-line
  one, prints `cannot access a Thread Local Storage value during or after
  destruction` followed by `panic in a function that cannot unwind`. The result
  is correct and the exit status is zero, which is why no golden catches it:
  they diff stdout, and `rust-basic` writes fourteen panic lines to `obs.err`
  while passing. Predates this work.

- **An eta-expanded record setter is wrong in the R pool.** `map .(.x = v) rs`
  gives the wrong answer in R and the right one in Python and C++. Single-field
  and in declaration order, so it is not finding 7; the coverage of that shape
  lives in the Rust golden for this reason.

- **Six defects found while reading and left alone**, each independent of the
  fixes above: an option's own null default still travels through the file and
  format vocabulary; `--json-help` calls every record argument required while
  the MCP view does not; an unrolled record behind an optional type silently
  loses its directives; a setter's receiver is taken from the wrong end of the
  argument list when a file-handle walk or a user pattern instance is in play; a
  selector mixing a field name and a tuple index crashes with an unsourced
  internal error; and a thunk-valued conditional arriving at a serialization
  sink is never forced, because the forcing helper matches only a bare thunk.
