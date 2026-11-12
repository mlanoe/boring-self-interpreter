// Copyright (C) 2026 Mickaël LANOË
// SPDX-License-Identifier: GPL-3.0-or-later
//
// Transpilation smoke tests for the boring interpreter source files.
//
// Each test runs `boring build --emit-rust <file.br>` and asserts that the
// transpiler exits cleanly (exit code 0, no error output).  The generated
// Rust is discarded — these tests only guard against transpiler regressions
// on the interpreter's own source.
//
// Run with:
//   cargo test --test interpreter

mod common;

use std::process::Command;

fn transpile_ok(name: &str) {
    let bin = common::boring_bin();
    let br_file = common::interpreter_dir().join(format!("{}.br", name));

    let out = Command::new(&bin)
        .arg("build")
        .arg("--emit-rust")
        .arg(&br_file)
        .output()
        .unwrap_or_else(|e| panic!("[{}] failed to invoke boring: {}", name, e));

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "[{}] transpilation failed:\n{}",
        name,
        stderr
    );
}

macro_rules! interpreter_test {
    ($name:ident) => {
        #[test]
        fn $name() {
            transpile_ok(stringify!($name));
        }
    };
}

interpreter_test!(ast);
interpreter_test!(tokens);
interpreter_test!(value);
interpreter_test!(lexer);
interpreter_test!(parser_core);
interpreter_test!(parser_exprstmt);
interpreter_test!(methods);
interpreter_test!(stdlib);
interpreter_test!(eval);
interpreter_test!(exec);
interpreter_test!(json);
