# boring-self-interpreter

An interpreter for [Boring](https://github.com/mlanoe/boring) **written in Boring**: lexer, parser and tree-walking evaluator, about 17,000 lines of `.br` and no hand-written Rust.

It is a showcase for the language. A non-trivial program that the `boring` transpiler must compile to Rust correctly, in four configurations, and that is then judged by the same corpus the compiler itself is tested with.

```
$ echo 'print "hello from a Boring program, run by a Boring interpreter"' \
    | interpreter/main_rust/target/debug/main
```

## Requirements

This project **depends on [`boring`](https://github.com/mlanoe/boring)**; `boring` does not depend on it.

| Needed from `boring` | Default | Override |
|---|---|---|
| repository checkout | `../boring` | `BORING_DIR` |
| `boring` executable | `$BORING_DIR/target/release/boring`, else `target/debug/boring` | `BORING_BIN` |
| test corpus (`tests/cases/*.br` + `.expected`), read-only | `$BORING_DIR/tests/cases` | follows `BORING_DIR` |

```bash
git clone https://github.com/mlanoe/boring.git
git clone https://github.com/mlanoe/boring-self-interpreter.git
(cd boring && cargo build --release)
```

There is deliberately no fallback to a `boring` on `PATH`: an installed binary can be older than the checkout the corpus comes from, and the failures would be misleading. A Rust toolchain is required (see `rust-toolchain.toml`).

## Build and run

The interpreter is a Boring project (`interpreter/boring.toml`). Build it with the compiler from the `boring` checkout:

```bash
cd interpreter
../../boring/target/release/boring build      # -> main_rust/  (Cargo project)
cargo build --manifest-path main_rust/Cargo.toml
echo 'print 1 + 2' | main_rust/target/debug/main
```

It reads the program on stdin, or takes a file argument, and accepts the same launch parameters as `boring run`. `use boring.<module>` is resolved against the `stdlib/` of the directory it runs from (the tests run it from the `boring` checkout).

### Four variants

The same sources are transpiled four times, so the interpreter also exercises both ownership models of the transpiler:

| variant | command | generated project |
|---|---|---|
| strict + multi-thread | `boring build` | `interpreter/main_rust` |
| strict + single-thread | `boring build --threading single` | `interpreter/main_rust_single` |
| managed + multi-thread | `boring build --mode managed` | `interpreter/main_rust_managed` |
| managed + single-thread | `boring build --mode managed --threading single` | `interpreter/main_rust_managed_single` |

The generated Rust projects take about 1 GB. Remove them with `rm -rf interpreter/*_rust*`.

## Tests

```bash
cargo test --test interpreter                              # each .br file transpiles, seconds
cargo test --test interpreter_build -- --test-threads=1    # 4x `boring build` + `cargo build`, slow
cargo test --test interpreter_functional                   # needs the 4 binaries from the previous step
```

The functional suite takes its cases from an explicit list in `tests/interpreter_functional.rs`. Each case runs against all four binaries and its stdout is compared with the `.expected` file. A case the interpreter cannot handle yet stays out of the list (commented, with the reason) rather than failing the suite. Cases come from the `boring` corpus, plus the fixtures that only this interpreter uses, in `cases/`.

Status against the `boring` checkout of 2026-11-12: 10/10 transpile smoke tests, 4/4 variant builds, 186/186 functional tests (each case on 4 binaries).

### Parity

`tools/interpreter_parity.py` runs the Rust interpreter and these binaries over the fixtures registered in `boring`'s `tests/transpile.rs` and compares each output with the `.expected` file. On 2026-11-15, with a 6 s timeout, over 374 simple fixtures:

| | ok | error | different output | timeout |
|---|---:|---:|---:|---:|
| Rust interpreter (`boring run`) | 373 | 1 | 0 | 0 |
| This interpreter (strict + multi) | 374 | 0 | 0 | 0 |

Every registered fixture passes (the Rust interpreter still fails `misc_scenarios_10`). Notable pieces: raw `std.net` sockets live in a `Sockets` actor, since `TcpListener`/`TcpStream` are not `Clone` (needs the `boring` fix `fa9fdb9f`); reflection exchanges values through the built-in `FieldValue` enum, with `type let`/`type var`/`type set`; `{x:?}` of an optional prints `Some(v)`/`None` from the static type of the hole (declared `let` types, struct fields, built-in optional calls), since optionals are transparent at run time.

The numbers measure this corpus, not a percentage of the language. [`docs/interpreter-parity.md`](docs/interpreter-parity.md) (in French, written before this work) has the architecture and a historical gap analysis. `tools/bench_interpreters.sh` compares timings.

## Layout

```
interpreter/       the interpreter, a Boring project
  main.br            entry point
  tokens.br lexer.br                         lexing
  ast.br parser_core.br parser_exprstmt.br   parsing
  value.br eval.br exec.br methods.br        evaluation, built-in methods
  stdlib.br fixedpoint.br                    `use` resolution, stdlib, fixed-point numbers
cases/             fixtures used only by this interpreter
tests/             the three suites above, shared helpers in tests/common
tools/             parity tool, benchmark
docs/              parity audit
```

## Why it is a useful regression guard for `boring`

The interpreter exercises actors, dicts, trait objects, enums with payloads, closures, `var`/`mut` parameters and large recursive types. Several transpiler bugs were found because this program would not compile. Since the compiler repository no longer runs this suite, a transpiler change is checked here, against the current `boring` build. The CI workflow does that on Linux and macOS.

## Contributing

Bug reports, ideas and pull requests are welcome. Contributions fall under the `boring` project's [Contributor License Agreement](https://github.com/mlanoe/boring/blob/main/CLA.md); add your name to its `CLA-signatories.md` as described there before your first pull request is merged. A bug that comes from the transpiler rather than from this interpreter belongs in the `boring` repository, ideally with a minimal reproduction in its `tests/cases`.

## License

GPL-3.0, the same as `boring`. See [`LICENSE`](LICENSE). Source files carry `SPDX-License-Identifier: GPL-3.0-or-later`.
