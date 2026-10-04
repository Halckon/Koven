#!/usr/bin/env python3
"""SPEC-0266: bounded checks of actual Koven LLVM, with replayable failure artifacts."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess

ROOT = Path(__file__).resolve().parents[1]
EXPORT_TEST = "native_sanitizer_tests::asan_instruments_generated_user_runtime_and_drop"
CASES = {"clean", "user", "runtime", "drop", "leak"}


def run(command, directory, name, *, timeout=30, env=None):
    """File-backed output avoids pipe limits; every attempt records its exact command."""
    command = [str(argument) for argument in command]
    directory.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    for variable in ("ASAN_OPTIONS", "LSAN_OPTIONS", "UBSAN_OPTIONS", "ASAN_SYMBOLIZER_PATH"):
        environment.pop(variable, None)
    environment.update(env or {})
    (directory / f"{name}.command.json").write_text(json.dumps(
        dict(argv=command, cwd=str(ROOT), timeout_seconds=timeout, env=env or {}), indent=2) + "\n")
    with (directory / f"{name}.stdout").open("wb") as stdout, (directory / f"{name}.stderr").open("wb") as stderr:
        try:
            # Export Cargo owns its Rust test and plain counter compiler. The
            # sanitizer commands run afterwards, each in one fresh process group.
            # Kill that group on timeout, including ordinary child processes.
            with subprocess.Popen(command, cwd=ROOT, stdout=stdout, stderr=stderr,
                                  env=environment, start_new_session=True) as process:
                try:
                    code = process.wait(timeout=timeout)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    process.wait()
                    raise
            result = subprocess.CompletedProcess(command, code)
        except (subprocess.TimeoutExpired, OSError) as error:
            (directory / f"{name}.result.json").write_text(json.dumps(
                dict(error=str(error), timed_out=isinstance(error, subprocess.TimeoutExpired)), indent=2) + "\n")
            raise
    (directory / f"{name}.result.json").write_text(json.dumps(dict(exit=result.returncode, timed_out=False)) + "\n")
    result.stdout = (directory / f"{name}.stdout").read_bytes()
    result.stderr = (directory / f"{name}.stderr").read_bytes()
    return result


def checked(command, directory, name, **kwargs):
    result = run(command, directory, name, **kwargs)
    if result.returncode:
        raise AssertionError(f"{name} failed ({result.returncode}); artifacts: {directory}\n{result.stderr.decode(errors='replace')}")
    return result


def assert_export_ran(text):
    if (len(re.findall(rf"^test {re.escape(EXPORT_TEST)} \.\.\. ok$", text, re.M)) != 1
            or len(re.findall(r"test result: ok\. 1 passed; 0 failed; 0 ignored;", text)) != 1):
        raise AssertionError("the exact sanitizer exporter test must run once, without failures or ignores")


def function_body(text, function):
    bodies = []
    current = None
    for line in text.splitlines():
        if line.startswith("define ") and (f"@{function}(" in line or f'@"{function}"(' in line):
            current = []
        elif current is not None:
            if line == "}":
                bodies.append("\n".join(current))
                current = None
            else:
                current.append(line)
    if len(bodies) != 1:
        raise AssertionError(f"expected one actual definition of {function}, got {len(bodies)}")
    return bodies[0]


def assert_instrumented(text, function, access, enabled):
    body = function_body(text, function)
    actual = bool(re.search(rf"call void @__asan_report_{access}(?:\d+|_n)\(", body))
    if actual != enabled:
        raise AssertionError(f"{function}: expected ASan {access} checks={enabled}, got {actual}")


def assert_detected(result, detector, category):
    marker = re.escape(f"ERROR: {detector}: {category}".encode()) + rb"(?:\s|$)"
    if result.returncode == 0 or not re.search(marker, result.stderr):
        raise AssertionError(f"expected nonzero {detector} {category}; got {result!r}")


def assert_disabled(result):
    if b"AddressSanitizer" in result.stderr or b"LeakSanitizer" in result.stderr:
        raise AssertionError(f"disabled probe unexpectedly reported sanitizer detection: {result!r}")


def assert_clean(result, expected):
    if result.returncode or result.stdout != expected or result.stderr:
        raise AssertionError(f"clean native fixture differs from its independent oracle: {result!r}")


def fixtures(directory):
    rows = [line.split("\t") for line in (directory / "fixtures.tsv").read_text().splitlines()]
    if any(len(row) != 3 for row in rows) or len(rows) != len(CASES) or {row[0] for row in rows} != CASES:
        raise AssertionError("expected exactly the five Koven sanitizer fixtures")
    return {row[0]: (row[1], row[2]) for row in rows}


def check_ir(directory, clang):
    """Both supported hosts check instrumentation; runtime acceptance is Linux-only."""
    version = checked([clang, "--version"], directory, "clang-version")
    if b"clang version 21.1." not in version.stdout:
        raise AssertionError("Koven IR checks require Clang 21.1, matching the emitter")
    rows = fixtures(directory)
    for name, (target, access) in rows.items():
        if name in {"clean", "leak"}:
            continue
        for mode in ("raw", "asan"):
            output = directory / f"{name}.{mode}.instrumented.ll"
            checked([clang, "-O0", "-fsanitize=address", "-S", "-emit-llvm",
                     directory / f"{name}.{mode}.ll", "-o", output], directory, f"ir-{name}-{mode}")
            assert_instrumented(output.read_text(), target, access, mode == "asan")
    print("Koven IR: three actual target functions instrumented; three attribute-off controls verified")


def linux_runtime_archives(directory, clang):
    # The pinned Debian package uses the legacy layout; --print-runtime-dir
    # can report a nonexistent per-target directory in Clang 21.
    resource = checked([clang, "--print-resource-dir"], directory, "resource-directory")
    runtime_dir = Path(resource.stdout.decode().strip()) / "lib/linux"
    archives = [runtime_dir / f"libclang_rt.{name}-x86_64.a" for name in ("asan", "lsan")]
    for path in archives:
        if not path.is_file():
            raise AssertionError(f"pinned LLVM sanitizer archive is missing: {path}")
    return archives


def check_linux(directory):
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise RuntimeError("required dynamic acceptance needs Linux x86_64; other hosts must not pass by skipping")
    if directory.exists():
        raise RuntimeError("artifact directory must be new, so previous results cannot satisfy this run")
    directory.mkdir(parents=True)
    prefix = Path(os.environ["LLVM_SYS_211_PREFIX"])
    clang = prefix / "bin/clang"
    llvm = checked([prefix / "bin/llvm-config", "--version"], directory, "llvm-version")
    if llvm.stdout.strip() != b"21.1.8":
        raise AssertionError("Linux sanitizer contract requires pinned LLVM 21.1.8")
    version = checked([clang, "--version"], directory, "clang-version")
    if b"clang version 21.1.8" not in version.stdout:
        raise AssertionError("Linux sanitizer contract requires pinned Clang 21.1.8")
    checked([prefix / "bin/llvm-symbolizer", "--version"], directory, "symbolizer-version")
    checked(["dpkg-query", "--show", "llvm-21-dev", "clang-21", "libclang-rt-21-dev"], directory, "packages")
    archives = linux_runtime_archives(directory, clang)
    (directory / "runtime-sha256.json").write_text(json.dumps(
        {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in archives}, indent=2) + "\n")
    checked(["git", "rev-parse", "HEAD"], directory, "compiler-sha")
    (directory / "host.json").write_text(json.dumps(dict(system=platform.system(), machine=platform.machine(),
                                                          platform=platform.platform()), indent=2) + "\n")
    fixture_dir = directory / "fixtures"
    exported = checked(["cargo", "test", "--locked", "--offline", "-p", "lang-codegen", "--lib",
                        EXPORT_TEST, "--", "--exact", "--nocapture"], directory, "export-koven", timeout=900,
                       env={"KOVEN_SANITIZER_ARTIFACTS": str(fixture_dir)})
    assert_export_ran(exported.stdout.decode())
    check_ir(fixture_dir, clang)
    rows = fixtures(fixture_dir)
    expected = (fixture_dir / "expected.stdout").read_bytes()
    symbolizer = str(prefix / "bin/llvm-symbolizer")
    asan_env = {"ASAN_OPTIONS": "detect_leaks=0:halt_on_error=1:abort_on_error=0:exitcode=86",
                "ASAN_SYMBOLIZER_PATH": symbolizer}
    summary = []
    for name in ("clean", "user", "runtime", "drop"):
        for mode in ("asan", "raw"):
            executable = directory / f"{name}-{mode}"
            checked([clang, "-O0", "-g", "-fno-omit-frame-pointer", "-fsanitize=address",
                     fixture_dir / f"{name}.{mode}.ll", "-o", executable], directory, f"build-{name}-{mode}")
            result = run([executable], directory, f"run-{name}-{mode}", timeout=10, env=asan_env)
            if name == "clean":
                assert_clean(result, expected)
            elif mode == "asan":
                category = "heap-use-after-free" if name == "drop" else "heap-buffer-overflow"
                assert_detected(result, "AddressSanitizer", category)
                if rows[name][0].encode() not in result.stderr:
                    raise AssertionError(f"report must locate actual target function {rows[name][0]}")
            else:
                # UB without instrumentation need not have a stable exit code/output.
                # It must never be counted as a sanitizer detection.
                assert_disabled(result)
            summary.append(f"ASan {name}/{mode}: expected classification verified")
    for name in ("clean", "leak"):
        executable = directory / f"{name}-lsan"
        checked([clang, "-O0", "-g", "-fno-omit-frame-pointer", "-fsanitize=leak",
                 fixture_dir / f"{name}.raw.ll", "-o", executable], directory, f"build-{name}-lsan")
        for enabled in (True, False):
            options = f"detect_leaks={int(enabled)}:exitcode=87:external_symbolizer_path={symbolizer}"
            result = run([executable], directory, f"run-{name}-lsan-{int(enabled)}", timeout=10,
                         env={"LSAN_OPTIONS": options})
            if name == "leak" and enabled:
                try:
                    assert_detected(result, "LeakSanitizer", "detected memory leaks")
                except AssertionError:
                    # Diagnose conservative roots without changing the original
                    # acceptance result or disabling any root scanning.
                    try:
                        run([executable], directory, "diagnose-leak-lsan", timeout=10,
                            env={"LSAN_OPTIONS": options + ":verbosity=1:log_threads=1:log_pointers=1"})
                    except (OSError, subprocess.TimeoutExpired):
                        pass  # run retained diagnostic failure evidence; keep the acceptance failure.
                    raise
                if (b"Direct leak of 4 byte(s) in 1 object(s)" not in result.stderr
                        or rows["runtime"][0].encode() not in result.stderr):
                    raise AssertionError("leak report must identify the actual four-byte Koven Cell allocation")
            else:
                assert_clean(result, expected)
            summary.append(f"LSan {name}/enabled={enabled}: expected classification verified")
    (directory / "acceptance.txt").write_text("\n".join(summary) + "\nUBSan on Koven LLVM IR: NOT COVERED\n")
    print("\n".join(summary))
    print(f"Linux sanitizer artifacts: {directory}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--ir-checks", type=Path)
    modes.add_argument("--linux", action="store_true")
    parser.add_argument("--clang", type=Path)
    parser.add_argument("--artifacts", type=Path)
    args = parser.parse_args()
    if args.linux:
        if args.artifacts is None:
            parser.error("--linux requires --artifacts")
        check_linux(args.artifacts.resolve())
    else:
        if args.clang is None:
            parser.error("--ir-checks requires --clang")
        check_ir(args.ir_checks.resolve(), args.clang)
