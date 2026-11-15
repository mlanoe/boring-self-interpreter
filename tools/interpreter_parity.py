#!/usr/bin/env python3
"""Compare interpreters with the stdout fixtures registered in tests/transpile.rs.

Build prerequisites (this project depends on the `boring` repository, never the reverse):
    cargo build --release        # in $BORING_DIR (default ../boring)
    cargo test --test interpreter_build    # in this project

The default checks committed expectations, not freshly compiled Rust. Use
--transpile to also emit, compile and run the reference for each selected case.
Only simple top-level transpile_test! fixtures are selected; project tests,
negative tests and GPU targets need their dedicated suites.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

HERE = Path(__file__).resolve().parents[1]
# The boring repository: compiler binary, tests/transpile.rs (fixture registry) and tests/cases.
ROOT = Path(os.environ.get("BORING_DIR", HERE.parent / "boring")).resolve()
MODES = ("main_rust", "main_rust_single", "main_rust_managed", "main_rust_managed_single")


def run(command, timeout, expected, env=None):
    # Files bound Python's memory usage when a broken guest prints in a loop.
    with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
        try:
            result = subprocess.run(command, cwd=ROOT, stdout=stdout, stderr=stderr,
                                    timeout=timeout, env=env)
        except subprocess.TimeoutExpired:
            return {"status": "timeout", "detail": f"exceeded {timeout}s"}
        stdout.seek(0)
        actual = stdout.read().decode("utf-8", errors="replace").replace("\r\n", "\n")
        stderr.seek(0)
        error = stderr.read(4000).decode("utf-8", errors="replace")
    if result.returncode:
        return {"status": "error", "code": result.returncode, "detail": error}
    if expected is not None and actual.rstrip() != expected.rstrip():
        return {"status": "output", "detail": actual[:4000]}
    return {"status": "ok"}


def boring_revision():
    """The commit of the `boring` checkout under test, `+dirty` when its tree has local changes."""
    try:
        rev = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT, capture_output=True,
                             text=True, check=True).stdout.strip()
        dirty = subprocess.run(["git", "status", "--porcelain", "--untracked-files=no"], cwd=ROOT,
                               capture_output=True, text=True, check=True).stdout.strip()
        return rev + ("+dirty" if dirty else "")
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cases", nargs="*", help="registered fixture names (default: all)")
    parser.add_argument("--timeout", type=float, default=10, help="seconds per guest")
    parser.add_argument("--build-timeout", type=float, default=180)
    parser.add_argument("--all-modes", action="store_true", help="check all four Boring binaries")
    parser.add_argument("--transpile", action="store_true", help="also compile/run strict+multi Rust")
    parser.add_argument("--unregistered", action="store_true",
                        help="check the tests/cases fixtures with an .expected file that transpile.rs does not register")
    parser.add_argument("--output", type=Path, help="write detailed JSON results")
    args = parser.parse_args()
    if args.timeout <= 0 or args.build_timeout <= 0:
        parser.error("timeouts must be positive")
    names = list(dict.fromkeys(re.findall(r"^transpile_test!\((\w+)",
                                         (ROOT / "tests/transpile.rs").read_text(), re.M)))
    names = [n for n in names if (ROOT / f"tests/cases/{n}.br").is_file()
             and (ROOT / f"tests/cases/{n}.expected").is_file()]
    if args.unregistered:
        registered = set(names)
        names = sorted(p.stem for p in (ROOT / "tests/cases").glob("*.br")
                       if p.stem not in registered and p.with_suffix(".expected").is_file())
    elif args.cases:
        unknown = set(args.cases) - set(names)
        if unknown:
            parser.error(f"unregistered fixtures: {', '.join(sorted(unknown))}")
        names = list(dict.fromkeys(args.cases))
    exe = ".exe" if os.name == "nt" else ""
    native = Path(os.environ["BORING_BIN"]) if os.environ.get("BORING_BIN") else next(
        (p for p in (ROOT / f"target/{prof}/boring{exe}" for prof in ("release", "debug")) if p.is_file()),
        ROOT / f"target/release/boring{exe}")
    bins = {"rust": native}
    for mode in MODES if args.all_modes else MODES[:1]:
        target = HERE / "interpreter" / mode / "target"
        # Newest wins: interpreter_build recreates a stale <triple>/debug/main
        # that must not shadow a fresher plain `cargo build` in debug/main.
        candidates = [p for p in [target / f"debug/main{exe}", *target.glob(f"*/debug/main{exe}")]
                      if p.is_file()]
        bins[mode] = (max(candidates, key=lambda p: p.stat().st_mtime) if candidates
                      else target / f"debug/main{exe}")
    for binary in bins.values():
        if not binary.is_file():
            parser.error(f"missing {binary}; run the build prerequisites")
    for label, binary in bins.items():
        print(f"using {label}: {binary}", flush=True)
    boring_rev = boring_revision()
    print(f"boring: {boring_rev}", flush=True)
    rows = []
    for name in names:
        fixture = ROOT / f"tests/cases/{name}.br"
        expected = fixture.with_suffix(".expected").read_text().replace("\r\n", "\n")
        row = {"case": name}
        for label, binary in bins.items():
            row[label] = run([str(binary), str(fixture)], args.timeout, expected)
        if args.transpile:
            # Keep emitted projects in tests/cases so sibling imports resolve just
            # as they do in the repository's reference integration suite.
            with tempfile.TemporaryDirectory(prefix=".parity-", dir=fixture.parent) as temp:
                project = Path(temp) / "rust"
                result = run([str(native), "build", str(fixture), "--output-dir", str(project)],
                             args.build_timeout, None)
                if result["status"] == "ok":
                    env = dict(os.environ, CARGO_TARGET_DIR=str(ROOT / "target/parity-reference"))
                    result = run(["cargo", "build", "--quiet", "--manifest-path",
                                  str(project / "Cargo.toml")], args.build_timeout, None, env)
                    if result["status"] == "ok":
                        # Generated package names follow fixture names; obtain the
                        # actual bin name from Cargo instead of guessing it.
                        metadata = subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1",
                                                   "--manifest-path", str(project / "Cargo.toml")],
                                                  cwd=ROOT, capture_output=True, text=True,
                                                  timeout=args.build_timeout, env=env, check=True)
                        package = json.loads(metadata.stdout)["packages"][0]
                        bin_name = next(t["name"] for t in package["targets"] if "bin" in t["kind"])
                        result = run([str(ROOT / "target/parity-reference/debug" / (bin_name + exe))],
                                     args.timeout, expected)
                row["transpiler"] = result
        rows.append(row)
        print(name + ": " + ", ".join(f"{k}={v['status']}" for k, v in row.items() if k != "case"), flush=True)
    labels = list(bins) + (["transpiler"] if args.transpile else [])
    summary = {label: {status: sum(r[label]["status"] == status for r in rows)
                       for status in ("ok", "error", "output", "timeout")} for label in labels}
    print(json.dumps(summary, indent=2))
    if args.output:
        args.output.write_text(json.dumps({"timeout": args.timeout, "boring": boring_rev, "summary": summary, "cases": rows},
                                         ensure_ascii=False, indent=2) + "\n")
    return int(any(r[label]["status"] != "ok" for r in rows for label in labels))


if __name__ == "__main__":
    raise SystemExit(main())
