# Parity of the transpiler and the interpreters

`boring` has two ways to run a program: the transpiler (`boring build`, the functional reference)
and a tree-walk interpreter (`boring run`). This project adds a third, an interpreter written in
Boring itself. This document says what is measured, how, what the architecture looks like, and
which gaps remain. Last update: 2026-11-15.

## What is measured

`tools/interpreter_parity.py` runs each program with its real path and compares stdout with the
`.expected` file the transpiler suite uses. Two corpora, both read from the `boring` checkout:

| corpus | selection | rust (`boring run`) | this interpreter |
|---|---|---:|---:|
| registered | the 379 simple `transpile_test!` fixtures of `tests/transpile.rs` that have `.br` + `.expected` | 378 ok | 379 ok |
| unregistered (`--unregistered`) | the 86 other `tests/cases/*.br` with an `.expected` file | 86 ok | 84 ok, 2 timeout |

The numbers measure these corpora, not a percentage of the language. A timeout is a case to
diagnose, not proof of an infinite loop. Project tests, negative tests, GPU targets and calls to
external Rust libraries are not covered by this selection. The generated Rust is compiled and run
only with `--transpile`; otherwise the `.expected` file is the oracle.

The JSON written by `--output` records the `boring` commit under test (`+dirty` when its tree has
local changes), because the result depends on it.

## Architecture

The interpreter in `interpreter/` is a chain written entirely in Boring: lexer, parser and
evaluator. It does not use the Rust front end to parse the guest program. The checker, the
transpiler and the GPU back ends stay in Rust.

The same sources are transpiled four times (strict or managed memory, multi or single thread), so
the interpreter also checks that the transpiler's output is right in all four models. That does not
prove that the four engines cover the same language as the transpiler.

Choices that stay simpler than the generated Rust:

- `Value.Dict` and `Value.Set` are arrays with linear search (O(n)); set operations that look up
  each element can be O(n·m). A hash table would need a `Hash`/`Eq` contract for every recursive
  and dynamic `Value`.
- Channels are arrays with removal at the head (O(n)), not a `VecDeque`.
- Relative file imports work; `[deps]` and `BORING_PATH` do not, and `use boring.<module>` is
  resolved against the `stdlib/` of the directory the interpreter runs from.

### Mechanisms worth knowing

- **Sockets.** `TcpListener` and `TcpStream` are not `Clone`, so they cannot sit in `Interpreter`'s
  state (it is copied through its actor handle). They live in a `Sockets` struct behind an actor,
  keyed by an `id`; the guest object only carries that `id`. `read`/`read_exact` refill the
  caller's buffer in place. This needs the `boring` fix `fa9fdb9f` (non-`Clone` external types in
  collections). I/O errors only say which call failed: `try?` drops the message.
- **Reflection.** `introspect()` returns `Field`/`Method` objects; values go through the built-in
  `FieldValue` enum (`Int`, `Float`, `Bool`, `Str`, `Other`, ...), registered at start-up.
  `Method.call` works on type-level methods; `type let`, `type var` and `type set` are parsed, kept
  in `type_var_store` (key `Owner::name`) and written through their setter, which cannot re-enter
  itself.
- **`{x:?}` of an optional.** Optionals are transparent at run time (the value or `nil`), so
  `Some(v)`/`None` comes from the static type of the hole: a declared `let` type, a struct field,
  an element of a typed collection, a built-in optional call (`first`, `last`, `find`, `indexOf`,
  `parseInt`, ...), or a function's return type. Declared types are remembered per call depth and
  name (`declared_types`), only for this purpose.
- **By-value structs and `'observed`.** An unqualified, non-`mut` struct parameter is stored by
  copy; `'observed` values live in an `__Observed` wrapper that notifies subscribers.
- **`view`.** `boring` lowers views in Rust; the interpreter emulates the runtime methods
  (`viewKind`, `expand`, modifiers, `env`/`state`) natively in `methods.br`.
- **A parser pitfall of this codebase.** Reading two fields of `interp` (an actor handle) in one
  expression locks it twice and deadlocks; read each into its own `let` first.

## Known gaps (unregistered corpus)

Both fixtures below pass with `boring run`; 84 of the 86 unregistered ones pass with this interpreter.

| case | what is missing |
|---|---|
| `kernel_aux_method_return_type_first`, `kernel_sim_many_threads_big_field` | Performance, not semantics: each builds a 30 000-element array with `for i in 0..<30000: b.push(..)`. Here `push` copies the whole array (the environment holds values, not references), so the loop alone costs about 450 million value copies, minutes in a debug build. The kernel part (see below) is not the cost: a 256-thread launch with an empty body is as slow. Fixing it needs an in-place `push`, which the environment's by-value storage does not allow without redesign |

The 21 gaps closed since the first audit, by family:

- **Parsing**: `Fn(args)` closure types, `{{` before `$"..."`, `[..<n]` / `[..=n]` allocation, a function-typed parameter keeps a function type.
- **Storage by value**: a bare struct variable stored into a dict, field, constructor argument or collection is copied unless the destination slot is typed `'actor`/`'shared` or the binding is; a by-value parameter is always copied; `.clone()` of a `'actor` binding aliases.
- **Numeric coercion**: a write to a field of the receiver takes the field's numeric type; a setter body writes its field instead of re-entering the setter.
- **Array blocks**: a function with a trailing array or closure parameter, implicit `Identifiable` keys, static `Sited` site numbering.
- **Views**: seeded `newState()`, the parser knows `boring.ui`'s `bind` fields, a state handed to a child's `bind` passes its `Slot`, callbacks write back into the mounted state and report `touch`/`escalate` to their owner's `DirtySink`.
- **Kernels**: `kernel K:` runs thread by thread (`k(block = .., grid = ..)`, `.thread`/`.block`/`.grid`, top-level `sync` splits the body into phases a whole block finishes in turn). A kernel array that is never written is kept in 256-element chunks.

## Reproduce

This project depends on `boring` (`../boring`, or `BORING_DIR`), never the reverse.

```sh
tools/check.sh              # build boring, build the interpreter, parity (both corpora), the 3 suites
```

or step by step:

```sh
(cd ../boring && cargo build --release)
(cd interpreter && ../../boring/target/release/boring build \
   && cargo build --manifest-path main_rust/Cargo.toml)
python3 tools/interpreter_parity.py --timeout 6 --output /tmp/parity.json
python3 tools/interpreter_parity.py --unregistered --timeout 6 --output /tmp/parity-unregistered.json
cargo test --test interpreter
cargo test --test interpreter_build -- --test-threads=1
cargo test --test interpreter_functional
```

Add `--transpile` to also compile and run the generated Rust of each selected case (strict + multi),
`--all-modes` to check the four binaries. The tool exits with code 1 while any case differs; it
keeps no exclusion list, so a gap never disappears from the report. Raise `--timeout` to tell
slowness from a hang.
