// Copyright (C) 2026 Mickaël LANOË
// SPDX-License-Identifier: GPL-3.0-or-later
//
// Functional test suite for the Boring-in-Boring interpreter.
//
// Each test runs the `.br` case against all 4 transpiled binaries:
//   strict+multi, strict+single, managed+multi, managed+single.
//
// Prerequisite: run `cargo test --test interpreter_build` at least once to
// compile all four interpreter binaries before running these tests.
//
// Run with:
//   cargo test --test interpreter_functional

use std::io::{Read, Write as IoWrite};
mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// Fail with a diagnostic instead of hanging the test runner on an actor deadlock.
fn wait_for_interpreter(mut child: std::process::Child) -> std::process::Output {
    // Drain both pipes while polling so a verbose guest cannot fill a pipe and
    // turn an otherwise successful execution into an artificial timeout.
    let mut stdout = child.stdout.take().expect("missing stdout pipe");
    let mut stderr = child.stderr.take().expect("missing stderr pipe");
    let out_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).expect("cannot read stdout");
        bytes
    });
    let err_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).expect("cannot read stderr");
        bytes
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().expect("cannot poll interpreter") {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait().expect("cannot reap interpreter");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let output = std::process::Output {
        status,
        stdout: out_reader.join().expect("stdout reader panicked"),
        stderr: err_reader.join().expect("stderr reader panicked"),
    };
    assert!(!timed_out, "interpreter exceeded 60s; stderr: {}", String::from_utf8_lossy(&output.stderr));
    output
}

const MODES: &[(&str, &str, &str)] = &[
    ("strict",  "multi",  "main_rust"),
    ("strict",  "single", "main_rust_single"),
    ("managed", "multi",  "main_rust_managed"),
    ("managed", "single", "main_rust_managed_single"),
];

fn find_bin_in(rust_dir: &str) -> PathBuf {
    let base = common::interpreter_dir().join(rust_dir).join("target");
    let name = format!("main{}", std::env::consts::EXE_SUFFIX);
    // Scan one level deep for a target-triple subdirectory (e.g. x86_64-pc-windows-msvc).
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("debug").join(&name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    base.join("debug").join(&name)
}

fn run_case_with_bin(name: &str, bin: &Path, label: &str) {
    assert!(
        bin.exists(),
        "[{}@{}] binary not found at {} — run `cargo test --test interpreter_build` first",
        name, label, bin.display()
    );

    let br_file = common::case_path(&format!("{}.br", name));
    let expected_file = common::case_path(&format!("{}.expected", name));

    let source = std::fs::read(&br_file)
        .unwrap_or_else(|_| panic!("cannot read {}", br_file.display()));

    let mut child = Command::new(bin)
        .current_dir(common::boring_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("[{}@{}] failed to spawn: {}", name, label, e));

    child.stdin.take().unwrap().write_all(&source).unwrap();

    let out = wait_for_interpreter(child);

    assert!(
        out.status.success(),
        "[{}@{}] interpreter exited with error:\n{}",
        name, label,
        String::from_utf8_lossy(&out.stderr)
    );

    let actual = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let expected = std::fs::read_to_string(&expected_file)
        .unwrap_or_else(|_| panic!("missing expected file: {}", expected_file.display()))
        .replace("\r\n", "\n");

    assert_eq!(
        actual.trim_end(),
        expected.trim_end(),
        "[{}@{}] output mismatch\n--- expected ---\n{}\n--- actual ---\n{}",
        name, label,
        expected.trim_end(),
        actual.trim_end(),
    );
}

fn run_case(name: &str) {
    for (mode, threading, rust_dir) in MODES {
        let label = format!("{}+{}", mode, threading);
        let bin = find_bin_in(rust_dir);
        run_case_with_bin(name, &bin, &label);
    }
}


macro_rules! itest {
    ($name:ident) => {
        #[test]
        fn $name() {
            run_case(stringify!($name));
        }
    };
}

itest!(basics);
itest!(match_inline_guard_alt);
itest!(strings);
// `.slice()` negative-index clamping — deliberately NOT added to the
// `strings`/`collections` fixtures above: those are shared verbatim with
// `tests/transpile.rs` (the main compiler's own transpile+build+run suite),
// whose native `.slice()` builtin doesn't clamp negative indices at all (a
// separate, out-of-scope gap) and would fail to even compile on a negative
// literal. Dedicated cases keep this self-hosted-interpreter-only fix
// (boring/interpreter/methods.br) isolated from that other suite.
itest!(string_slice_clamp);
itest!(array_slice_clamp);
itest!(control_flow);
itest!(match_stmt);
itest!(functions);
itest!(closures);
itest!(throwing_closures);
itest!(method_vs_fn_throws);
itest!(spawn_method_vs_fn_throws);
itest!(spawn_throws_caught_at_handle);
itest!(spawn_throws_join);
itest!(spawn_throws_wait);
itest!(spawn_throws_try_catch);
itest!(error_builtin_display);
itest!(spawn_throws_inline);
itest!(fn_field_struct);
itest!(fn_field_bare_call);
itest!(fn_field_optional_call);
itest!(fn_handle_pass_and_str_arg);
itest!(structs);
itest!(collections);
itest!(error_handling);
itest!(tasks);
itest!(channels);
itest!(protocols);
itest!(optionals);
itest!(enums);
itest!(streams);
itest!(stream_methods);
itest!(newtypes);
itest!(guard);
itest!(with_stmt);
itest!(generics);
itest!(operators);
itest!(method_overloading);
itest!(free_fn_overloading);
itest!(macros);
itest!(defer);
itest!(do_block);
itest!(tuples);
itest!(tuple_methods);
itest!(tuple_map);
itest!(inline_loops);
itest!(format);
itest!(loops);
itest!(traits);
itest!(trait_downcast);
itest!(trait_downcast_build);
itest!(dyn_return_borrowed_field_shared);
itest!(numeric);
itest!(float_width_cross_eq);
itest!(int_width_cross_assert_eq);
itest!(assert_eq_string_literal);
itest!(string_assign_alias_no_move);
itest!(scalar_catch);
itest!(modules);
itest!(boring_stdlib_collections);
itest!(boring_stdlib_identity);
itest!(ownership);
itest!(let_pattern);
itest!(environment_builtin);
itest!(result_compat);
itest!(multi_catch);
itest!(multi_variant_catch_dispatch);
itest!(implicit_self);
itest!(shadowing);
itest!(struct_spread);
itest!(default_rest);
itest!(tuple_string);
itest!(array_pop_remove);
itest!(closure_break);
itest!(pattern_some);
itest!(string_len_chars);
itest!(mixed_modulo);
itest!(range_unary);
itest!(closure_colon);
itest!(for_destructure);
itest!(numeric_separators);
itest!(fn_shorthand);
itest!(camel_to_snake);
itest!(lazy);
itest!(array_comprehension);
itest!(callable_struct);
itest!(fixed_array);
itest!(labeled_array);
itest!(collections2);
itest!(triple_string);
itest!(raw_apostrophe_string);
itest!(raw_single_quote_string);
itest!(dollar_string);
itest!(pipe);
itest!(supertraits);
itest!(type_cast);
itest!(inline_match);
itest!(ref_identity);
itest!(qualifiers_actor);
itest!(join_handle);
itest!(join_returns_string);
itest!(select);
itest!(task_timeout);
itest!(error_match);
itest!(try_else_block);
itest!(nil_assign);
itest!(transpiler_coerce);

// ─── Real, on-disk `use` module resolution ─────────────────────────────────
//
// Every case above is piped in over stdin as a single in-memory "file" with
// no real path (see run_case_with_bin) — there's no entry-file directory for
// `use`'s relative sibling-file resolution (exec_use in
// boring/interpreter/stdlib.br) to resolve against. These cases instead
// invoke the compiled interpreter binary with a real file argument, from a
// fixture directory under tests/cases/ that has actual sibling `.br` files.

fn run_file_case_with_bin(dir: &str, entry: &str, bin: &Path, label: &str) -> std::process::Output {
    assert!(
        bin.exists(),
        "[{}@{}] binary not found at {} — run `cargo test --test interpreter_build` first",
        dir, label, bin.display()
    );
    let entry_path = common::case_path(&format!("{}/{}", dir, entry));
    let child = Command::new(bin)
        .current_dir(common::boring_dir())
        .arg(&entry_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("[{}@{}] failed to spawn: {}", dir, label, e));
    wait_for_interpreter(child)
}

/// Runs `dir/entry` against all 4 interpreter binaries and checks stdout
/// against `dir/expected_name`. Mirrors `run_case_with_bin`, but via a real
/// file argument instead of stdin.
fn run_file_case_ok(dir: &str, entry: &str, expected_name: &str) {
    for (mode, threading, rust_dir) in MODES {
        let label = format!("{}+{}", mode, threading);
        let bin = find_bin_in(rust_dir);
        let out = run_file_case_with_bin(dir, entry, &bin, &label);

        assert!(
            out.status.success(),
            "[{}@{}] interpreter exited with error:\n{}",
            dir, label,
            String::from_utf8_lossy(&out.stderr)
        );

        let actual = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        let expected_file = common::case_path(&format!("{}/{}", dir, expected_name));
        let expected = std::fs::read_to_string(&expected_file)
            .unwrap_or_else(|_| panic!("missing expected file: {}", expected_file.display()))
            .replace("\r\n", "\n");

        assert_eq!(
            actual.trim_end(),
            expected.trim_end(),
            "[{}@{}] output mismatch\n--- expected ---\n{}\n--- actual ---\n{}",
            dir, label,
            expected.trim_end(),
            actual.trim_end(),
        );
    }
}

/// Runs `dir/entry` against all 4 interpreter binaries and asserts it fails
/// fast (nonzero exit) with `expected_stderr_substr` somewhere in stderr.
fn run_file_case_err(dir: &str, entry: &str, expected_stderr_substr: &str) {
    for (mode, threading, rust_dir) in MODES {
        let label = format!("{}+{}", mode, threading);
        let bin = find_bin_in(rust_dir);
        let out = run_file_case_with_bin(dir, entry, &bin, &label);

        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success(),
            "[{}@{}] expected a failure, but the interpreter exited successfully (stdout:\n{})",
            dir, label,
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(
            stderr.contains(expected_stderr_substr),
            "[{}@{}] stderr did not contain {:?}:\n{}",
            dir, label, expected_stderr_substr, stderr
        );
    }
}

#[test]
fn use_modules() {
    run_file_case_ok("use_modules", "main.br", "main.expected");
}

// Regression test for `parse_hex_str`/`parse_bin_str`/`parse_oct_str`
// (boring/interpreter/lexer.br) accumulating a hex/octal/binary literal
// straight into an `int` (isize, 64-bit) with no overflow check at all — a
// 17+ hex digit literal (e.g. `0xFFFFFFFFFFFFFFFF`, u64::MAX) silently
// wrapped in release or panicked on the raw `acc * 16 + digit` overflow in
// debug. Must now fail lexing fast with a clear error naming the literal,
// same as the main compiler's own lexer already does for this class of
// literal (src/lexer/mod.rs's `LexError::IntegerOverflow`).
#[test]
fn numeric_literal_overflow() {
    run_file_case_err("numeric_literal_overflow", "main.br", "too large");
}

// Regression test for the self-hosted parser's recursion-depth guard
// (`boring/interpreter/parser_core.br`'s `parser_depth`/`parser_depth_inc`/
// `parser_depth_dec`, and `parser_exprstmt.br`'s `parse_block`/`parse_or`/
// `parse_unary`/`parser_core.br`'s `parse_pattern`). Before this fix, the
// depth counter was a dead stub (`parser_depth` always returned `0`,
// `_inc`/`_dec` were no-ops) — the sole guard site that used it
// (`parse_not`, for `not not not …` chains) never actually tripped, and
// every other recursive path (nested blocks, parens, patterns) had no guard
// at all. A deeply nested program used to crash this compiled interpreter
// with a native stack overflow (SIGABRT) instead of a clean parse error.
//
// Generates the source programmatically (500 levels of nested `if true:`
// blocks) rather than committing a huge static fixture file — same approach
// as the Rust-hosted parser's own analogous regression tests
// (`src/parser/tests_recursion_depth.rs`).
#[test]
fn deeply_nested_blocks_produce_a_clean_error_not_a_crash() {
    let mut src = String::new();
    const N: usize = 500;
    for i in 0..N {
        src.push_str(&"    ".repeat(i));
        src.push_str("if true:\n");
    }
    src.push_str(&"    ".repeat(N));
    src.push_str("print 1\n");

    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("deep_nested_blocks_generated.br");
    std::fs::write(&tmp, &src).expect("failed to write generated fixture");

    for (mode, threading, rust_dir) in MODES {
        let label = format!("{}+{}", mode, threading);
        let bin = find_bin_in(rust_dir);
        assert!(
            bin.exists(),
            "[deep_nested_blocks@{}] binary not found at {} — run `cargo test --test interpreter_build` first",
            label, bin.display()
        );

        let child = Command::new(&bin)
            .current_dir(common::boring_dir())
            .arg(&tmp)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("[deep_nested_blocks@{}] failed to spawn: {}", label, e));
        let out = wait_for_interpreter(child);

        // The exact failure mode we're guarding against is a native crash
        // (SIGABRT from a stack overflow) rather than a normal nonzero exit —
        // `ExitStatus::code()` is `None` on Unix when the process was killed
        // by a signal, which is exactly the crash this test must rule out.
        assert!(
            out.status.code().is_some(),
            "[deep_nested_blocks@{}] interpreter crashed (terminated by signal, status: {}) \
             instead of returning a clean parse error — stderr:\n{}",
            label, out.status, String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            !out.status.success(),
            "[deep_nested_blocks@{}] expected a clean parse-error failure, but the interpreter \
             exited successfully (stdout:\n{})",
            label, String::from_utf8_lossy(&out.stdout)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("nested too deeply"),
            "[deep_nested_blocks@{}] expected a 'nested too deeply' parse error, got:\n{}",
            label, stderr
        );
    }

    let _ = std::fs::remove_file(&tmp);
}

// Interpreter parity with the transpiler numerical fixtures.
itest!(pow_method_int_unaffected);
itest!(pow_method_int_exponent_var);
itest!(pow_method_float_width);
itest!(ord_chr);

itest!(numeric_method_parity);

// Optional dictionary lookup and associated-function parity.
itest!(dict_index_optional_return);
itest!(if_let_dict_index_no_else);
itest!(if_let_expr_nested);
itest!(dict_index_nil_context);
itest!(trait_type_level_methods);
itest!(type_def_typed_throws);
itest!(type_method_throws_untyped);
itest!(enum_type_def);
itest!(enum_type_def_throws);
itest!(implicit_self_length_nontail);

#[test]
fn malformed_field_type_reports_an_error_without_deadlocking() {
    run_file_case_err("error_type_diagnostic", "main.br", "expected type, got Plus at line 2");
}

itest!(monomorphize_method);
itest!(monomorphize_method_on_generic_struct);
itest!(monomorphize_optional_method);
itest!(monomorphize_ext_method);
itest!(monomorphize_enum_method);

itest!(builtin_error_enum);
itest!(typed_catch_match_error);
itest!(float32_struct_method_math);
itest!(float32_math_builtins);
itest!(float32_local_var_math);
itest!(float32_array_literal_width_and_cast);
itest!(float16_array_literal_width_and_cast);
itest!(fixed_point_basic);
itest!(fixed_point_globals);
// fixed_point_tuples: blocked until the self-hosted parser handles plain-identifier tuple types `(fixed16_16, int) t`.
// itest!(fixed_point_tuples);
itest!(fixed_point_collections);
itest!(typed_tuple_idents);
itest!(fixed_point_struct_fn);
itest!(fixed_point_members);
itest!(fixed_point_policies);
itest!(fixed_point_wide);
itest!(fixed_point_math);
itest!(float64_array_literal_width_and_cast);
itest!(optional_interp_numeric);
itest!(if_else_cast_numeric);
itest!(nested_numeric_cast);
itest!(unresolved_field_cast_numeric);

itest!(narrowing_cast_if_let);
itest!(guard_let_cast_struct_field);

itest!(conditional_cast_boundaries);

itest!(option_owned_methods);
itest!(pub_top_level_const);

itest!(collection_named_methods);
itest!(array_param_length_only);
itest!(shared_dyn_array_param_returned);
itest!(destructured_slice_by_value_reuse);
itest!(try_prefix_in_cond_clause_noparen);

#[test]
fn enum_variant_shadow() {
    // This fixture imports two sibling modules, so it must run from its real
    // file path rather than through the stdin-only `itest!` harness.
    run_file_case_ok(".", "enum_variant_shadow.br", "enum_variant_shadow.expected");
}
itest!(inline_if_else_next_line_postfix);
itest!(return_if_expression);
itest!(qualifier_group_param);
itest!(shared_return_callsite_no_double_wrap);
itest!(monomorphize_struct);

#[test]
fn monomorphize_cross_file_main() {
    run_file_case_ok(".", "monomorphize_cross_file_main.br", "monomorphize_cross_file_main.expected");
}

// `var` (rebindable out-param) write-back to the caller, free functions and struct methods.
itest!(var_param_free);
itest!(method_var_param);
itest!(var_param_labeled_reorder);
itest!(var_param_labeled_reorder_method);
itest!(mut_collection_param);
itest!(self_mutating_call);
itest!(mut_param_type_method);
itest!(lend_disjoint_receiver_ok);
itest!(lend_and_read_same_local_lock_wrapper);
itest!(guard_let_else_panic_throws);
itest!(array_index_as_cast);
itest!(index_cast_expr);
itest!(int_literal_overflow_cast);
itest!(array_block_flat_transpile);
itest!(array_block_keyed_for);
itest!(array_move_remove_indices);
itest!(array_block_for_transpile);

// Exceptions raised inside expressions evaluated for their value (`loop`, a `match` arm used as
// an expression, a condition, an operand, an assignment's right-hand side) must propagate
// instead of turning into Nil; `return` in a `match` arm must return from the function.
itest!(exception_propagation);

// A throw that nothing catches ends the program with an error and a non-zero exit code
// (it used to be dropped silently, exit code 0).
#[test]
fn uncaught_throw() {
    run_file_case_err("uncaught_throw", "main.br", "unhandled exception in main: boom");
}

#[test]
fn project_deps_and_boring_path() {
    let root = std::env::temp_dir().join(format!("boring_self_deps_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("dep_c/src")).unwrap();
    std::fs::write(root.join("dep_c/src/leaf.br"), "pub def int leaf_val(): 40\n").unwrap();
    std::fs::create_dir_all(root.join("dep_a/src")).unwrap();
    std::fs::write(root.join("dep_a/boring.toml"), "[deps]\ndep_c = \"../dep_c\"\n").unwrap();
    std::fs::write(
        root.join("dep_a/src/math.br"),
        "use dep_c.leaf.leaf_val\npub def int add_leaf(int x): x + leaf_val()\n",
    ).unwrap();
    std::fs::create_dir_all(root.join("dep_b/src")).unwrap();
    std::fs::write(root.join("dep_b/src/greet.br"), "pub def string hi(): \"ok\"\n").unwrap();
    std::fs::create_dir_all(root.join("bpath")).unwrap();
    std::fs::write(root.join("bpath/extra.br"), "pub def int two(): 2\n").unwrap();
    std::fs::create_dir_all(root.join("app/src")).unwrap();
    std::fs::create_dir_all(root.join("app/test")).unwrap();
    std::fs::write(
        root.join("app/boring.toml"),
        "[deps]\ndep_a = \"../dep_a\"\ndep_b = { path = \"../dep_b\" }\n",
    ).unwrap();
    std::fs::write(root.join("app/src/local_mod.br"), "pub def int local_v(): 100\n").unwrap();
    std::fs::write(
        root.join("app/test/run.br"),
        "use local_mod.*\nuse dep_a.math.add_leaf\nuse dep_b.greet.*\nuse extra.two\ndef void main():\n    print \"{local_v() + add_leaf(0) + two()} {hi()}\"\n",
    ).unwrap();

    for (mode, threading, rust_dir) in MODES {
        let label = format!("{}+{}", mode, threading);
        let bin = find_bin_in(rust_dir);
        let out = wait_for_interpreter(
            Command::new(&bin)
                .arg(root.join("app/test/run.br"))
                .env("BORING_PATH", root.join("bpath"))
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("cannot spawn interpreter"),
        );
        assert!(
            out.status.success(),
            "[project_deps_and_boring_path@{}] failed:\n{}",
            label,
            String::from_utf8_lossy(&out.stderr)
        );
        let actual = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        assert_eq!(actual.trim_end(), "142 ok", "[project_deps_and_boring_path@{}]", label);
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn native_ecs_game_loop() {
    let ecs_src = r#"@component
data Position:
    var float x = 0.0
    var float y = 0.0

    @schedule(Update)
    def movement(Velocity vel) where not Frozen:
        self.x += vel.dx * time.delta
        self.y += vel.dy * time.delta

@component
data Velocity:
    float dx = 0.0
    float dy = 0.0

@component
data Frozen:
    pass

@component
data Enemy:
    var int hp = 10

@resource
data Score:
    var int count = 0

@game
data MiniGame:
    string title = "Mini ECS"

@schedule(Startup)
def setup():
    commands.spawn((Position(x = 0.0, y = 0.0), Velocity(dx = 100.0, dy = 50.0)))
    commands.spawn((Position(x = 5.0, y = 5.0), Velocity(dx = 10.0, dy = 10.0), Frozen()))
    commands.spawn((Enemy(hp = 0),))

@schedule(Update)
def check_enemies(mut Score score):
    for entity, Enemy e:
        if e.hp <= 0:
            score.count += 1
            despawn()

@schedule(PostUpdate)
def report(Score score):
    let p = query single (Position) where not Frozen
    let any_enemy = query any where Enemy
    print("moved=({p.x:.1}, {p.y:.1}) score={score.count} any_enemy={any_enemy}")
"#;
    for (mode, threading, rust_dir) in MODES {
        let label = format!("{}+{}", mode, threading);
        let bin = find_bin_in(rust_dir);
        let mut child = Command::new(&bin)
            .current_dir(common::boring_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("cannot spawn interpreter");
        child.stdin.as_mut().unwrap().write_all(ecs_src.as_bytes()).unwrap();
        drop(child.stdin.take());
        let out = wait_for_interpreter(child);
        assert!(
            out.status.success(),
            "[native_ecs_game_loop@{}] failed:\n{}",
            label,
            String::from_utf8_lossy(&out.stderr)
        );
        let actual = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        assert_eq!(actual.trim_end(), "moved=(1.6, 0.8) score=1 any_enemy=false", "[native_ecs_game_loop@{}]", label);
    }
}

#[test]
fn advanced_ecs_features() {
    let adv_src = r#"enum GameState:
    Menu
    Playing

@component
data Item:
    var int val = 0

@resource
data Log:
    var string text = ""

struct Ping as Event:
    int code = 0

@game
data AdvGame:
    string title = "Advanced"

@schedule(Startup)
def init_world():
    commands.spawn((Item(val = 10),))

@schedule(Update)
@after(step_one)
def step_two(mut Log log):
    log.text = "{log.text}2"
    commands.trigger(Ping(code = 42))

@schedule(Update)
def step_one(mut Log log):
    log.text = "{log.text}1"

@schedule(Update)
@state(GameState.Playing)
def only_in_playing(mut Log log):
    log.text = "{log.text}P"

@schedule(Update)
@state(GameState.Menu)
@after(step_two)
def only_in_menu(mut Log log):
    log.text = "{log.text}M"

pub def on_ping(On<Ping> ev, mut Log log):
    log.text = "{log.text}[ping:{ev.event.code}]"

@schedule(PostUpdate)
def check_added(Log log):
    let added_count = query (Item) where added Item
    print("log={log.text} added={added_count.count()}")
"#;
    for (mode, threading, rust_dir) in MODES {
        let label = format!("{}+{}", mode, threading);
        let bin = find_bin_in(rust_dir);
        let mut child = Command::new(&bin)
            .current_dir(common::boring_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("cannot spawn interpreter");
        child.stdin.as_mut().unwrap().write_all(adv_src.as_bytes()).unwrap();
        drop(child.stdin.take());
        let out = wait_for_interpreter(child);
        assert!(
            out.status.success(),
            "[advanced_ecs_features@{}] failed:\n{}",
            label,
            String::from_utf8_lossy(&out.stderr)
        );
        let actual = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        assert_eq!(actual.trim_end(), "log=12[ping:42]M added=1", "[advanced_ecs_features@{}]", label);
    }
}

#[test]
fn stdlib_boring_ecs_direct_import() {
    let direct_src = r#"use boring.ecs.*

struct Pos as Component:
    var float32 x = 0.0
    var float32 y = 0.0

def void main() throws:
    mut world = World()
    let e = world.spawnEmpty()
    let p'shared = Pos(x = 3.0, y = 4.0)
    world.insertComponent(e, "Pos", p)
    let ents = world.queryEntities(["Pos"], [])
    print("entities={ents.count()}")
"#;
    for (mode, threading, rust_dir) in MODES {
        let label = format!("{}+{}", mode, threading);
        let bin = find_bin_in(rust_dir);
        let mut child = Command::new(&bin)
            .current_dir(common::boring_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("cannot spawn interpreter");
        child.stdin.as_mut().unwrap().write_all(direct_src.as_bytes()).unwrap();
        drop(child.stdin.take());
        let out = wait_for_interpreter(child);
        assert!(
            out.status.success(),
            "[stdlib_boring_ecs_direct_import@{}] failed:\n{}",
            label,
            String::from_utf8_lossy(&out.stderr)
        );
        let actual = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        assert_eq!(actual.trim_end(), "entities=1", "[stdlib_boring_ecs_direct_import@{}]", label);
    }
}

#[test]
fn ecs_late_insert_despawn_writeback_and_free_systems() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "late_insert",
            "f=1 newly_buffed=0\nf=2 newly_buffed=1\nf=3 newly_buffed=0",
            r#"@component
data Tracked:
    pass
@component
data Buffed:
    pass
@resource
data SimState:
    var int frame = 0
@game
data LateInsertGame:
    string title = "LI"
@schedule(Startup)
def setup():
    commands.spawn((Tracked(),))
@schedule(Update)
def tick(mut SimState sim):
    sim.frame += 1
@schedule(Update)
@after(tick)
def apply_buff(SimState sim):
    if sim.frame == 2:
        let e = query first (Entity) where Tracked
        commands.entity(e).insert(Buffed())
@schedule(PostUpdate)
def report(SimState sim):
    let newly_buffed = query (Buffed) where added Buffed
    print("f={sim.frame} newly_buffed={newly_buffed.count()}")
"#,
        ),
        (
            "despawn_cmd",
            "f=1 alive=1\nf=2 alive=1\nf=3 alive=1",
            r#"@component
data Tracked:
    pass
@component
data Doomed:
    pass
@resource
data SimState:
    var int frame = 0
@game
data DespawnGame:
    string title = "DE"
@schedule(Startup)
def setup():
    commands.spawn((Tracked(), Doomed()))
    commands.spawn((Tracked(),))
@schedule(Update)
def tick(mut SimState sim):
    sim.frame += 1
@schedule(Update)
@after(tick)
def reap(SimState sim):
    if sim.frame == 1:
        let e = query first (Entity) where Doomed
        commands.entity(e).despawn()
@schedule(PostUpdate)
def report(SimState sim):
    let alive = query (Tracked) where Tracked
    print("f={sim.frame} alive={alive.count()}")
"#,
        ),
        (
            "fetch_writeback",
            "f=1 a=1 b=1 c=0\nf=2 a=2 b=2 c=1\nf=3 a=3 b=3 c=2",
            r#"@component
data TrackedA:
    pass
@component
data TrackedB:
    pass
@component
data TrackedC:
    pass
@component
data CounterA:
    var int n = 0
@component
data CounterB:
    var int n = 0
@component
data CounterC:
    var int n = 0
@resource
data SimState:
    var int frame = 0
@game
data WBGame:
    string title = "WB"
@schedule(Startup)
def setup():
    commands.spawn((TrackedA(), CounterA(n = 0)))
    commands.spawn((TrackedB(), CounterB(n = 0)))
    commands.spawn((TrackedC(), CounterC(n = 0)))
@schedule(Update)
def tick(mut SimState sim):
    sim.frame += 1
@schedule(Update)
@after(tick)
def bump_first(mut SimState sim):
    mut a = query first (mut CounterA) where TrackedA
    a.n += 1
@schedule(Update)
@after(tick)
def bump_any(mut SimState sim):
    mut b = query any (mut CounterB) where TrackedB
    b.n += 1
@schedule(Update)
@after(tick)
def bump_nested(SimState sim):
    if sim.frame >= 2:
        mut c = query first (mut CounterC) where TrackedC
        c.n += 1
@schedule(PostUpdate)
def report(SimState sim):
    let a = query single (CounterA) where TrackedA
    let b = query single (CounterB) where TrackedB
    let c = query single (CounterC) where TrackedC
    print("f={sim.frame} a={a.n} b={b.n} c={c.n}")
"#,
        ),
        (
            "free_per_entity_and_require",
            "changed=2 x=10\nchanged=1 x=13",
            r#"@component
data Transform:
    var int x = 7
@component
@require(Transform)
data Sprite:
    var int w = 10
@component
data Frozen:
    pass
@game
data FreeSysGame:
    string title = "FS"
@schedule(Startup)
def setup(Commands cmds):
    cmds.spawn((Sprite(w = 3),))
    cmds.spawn((Sprite(w = 5), Frozen()))
@schedule(Update)
def step(mut Transform t, Sprite s) where not Frozen:
    t.x += s.w
@schedule(PostUpdate)
def report():
    let changed = query (Transform) where changed Transform
    let unfrozen = query single (Transform) where not Frozen
    print("changed={changed.count()} x={unfrozen.x}")
"#,
        ),
    ];

    for (name, expected, src) in cases {
        let frames = if *name == "free_per_entity_and_require" { "2" } else { "3" };
        for (mode, threading, rust_dir) in MODES {
            let label = format!("{}+{}", mode, threading);
            let bin = find_bin_in(rust_dir);
            let mut child = Command::new(&bin)
                .current_dir(common::boring_dir())
                .env("BORING_ECS_FRAMES", frames)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("cannot spawn interpreter");
            child.stdin.as_mut().unwrap().write_all(src.as_bytes()).unwrap();
            drop(child.stdin.take());
            let out = wait_for_interpreter(child);
            assert!(
                out.status.success(),
                "[ecs:{}@{}] failed:\n{}",
                name,
                label,
                String::from_utf8_lossy(&out.stderr)
            );
            let actual = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
            assert_eq!(actual.trim_end(), *expected, "[ecs:{}@{}]", name, label);
        }
    }
}
