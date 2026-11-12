// Copyright (C) 2026 Mickaël LANOË
// SPDX-License-Identifier: GPL-3.0-or-later
//
// Locations shared by the test suites.
//
// This project depends on the `boring` repository (never the reverse):
//   * the `boring` compiler binary that transpiles the interpreter, and
//   * the shared corpus `tests/cases/` (read-only here).
//
// Environment overrides:
//   BORING_DIR  path of the boring repository   (default: ../boring)
//   BORING_BIN  path of the boring executable   (default: $BORING_DIR/target/release/boring,
//               else $BORING_DIR/target/debug/boring)
//
// There is deliberately NO fallback to `boring` on PATH: an installed binary can be
// older than the checkout the corpus comes from and would give misleading failures.
#![allow(dead_code)]

use std::path::PathBuf;

pub fn project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Sources of the Boring-written interpreter (and where the generated Rust projects go).
pub fn interpreter_dir() -> PathBuf {
    project_dir().join("interpreter")
}

pub fn boring_dir() -> PathBuf {
    let dir = match std::env::var_os("BORING_DIR") {
        Some(d) => PathBuf::from(d),
        None => project_dir().join("..").join("boring"),
    };
    dir.canonicalize()
        .unwrap_or_else(|e| panic!("boring repository not found at {} ({}); set BORING_DIR", dir.display(), e))
}

pub fn boring_bin() -> PathBuf {
    if let Some(p) = std::env::var_os("BORING_BIN") {
        let p = PathBuf::from(p);
        assert!(p.is_file(), "BORING_BIN={} is not a file", p.display());
        return p;
    }
    let name = format!("boring{}", std::env::consts::EXE_SUFFIX);
    for profile in ["release", "debug"] {
        let p = boring_dir().join("target").join(profile).join(&name);
        if p.is_file() {
            return p;
        }
    }
    panic!(
        "no boring binary under {}/target/{{release,debug}}: run `cargo build --release` there, or set BORING_BIN",
        boring_dir().display()
    );
}

/// The shared test corpus, owned by the boring repository.
pub fn cases_dir() -> PathBuf {
    let d = boring_dir().join("tests").join("cases");
    assert!(d.is_dir(), "corpus not found at {}", d.display());
    d
}

/// Fixtures that only this interpreter uses (`cases/` in this project).
pub fn own_cases_dir() -> PathBuf {
    project_dir().join("cases")
}

/// Resolves a case path: this project's own `cases/` first, then the boring corpus.
pub fn case_path(rel: &str) -> PathBuf {
    let own = own_cases_dir().join(rel);
    if own.exists() { own } else { cases_dir().join(rel) }
}
