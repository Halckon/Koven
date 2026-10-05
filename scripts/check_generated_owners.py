#!/usr/bin/env python3
"""SPEC-0269 bounded generated resource programs, independent oracles and replay."""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time

if __package__:
    from . import generated_owners as model, generated_owner_checks as checks, check_native_sanitizers as native, generated_owner_calibration as calibration
else:
    import generated_owners as model
    import generated_owner_checks as checks
    import check_native_sanitizers as native
    import generated_owner_calibration as calibration

Failure = checks.Failure
ROOT = Path(__file__).resolve().parents[1]
EXPORT_TEST = "native_generated_owner_tests::export_generated_owner_case"


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n")


def check_toolchain_version(llvm, clang, *, linux):
    supported = llvm == "21.1.8" if linux else re.fullmatch(r"21\.1\.[0-9]+", llvm) is not None
    clang_match = re.search(r"clang version ([0-9]+\.[0-9]+\.[0-9]+)(?:\s|$)", clang)
    if not supported or clang_match is None or clang_match[1] != llvm:
        raise Failure("setup", "tool_or_harness_failure", "llvm-version")


def test_binary(text):
    matches = set()
    for line in text.splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (item.get("reason") == "compiler-artifact" and item.get("target", {}).get("name") == "lang_codegen"
                and item.get("profile", {}).get("test") and item.get("executable")):
            matches.add(item["executable"])
    if len(matches) != 1:
        raise Failure("build", "tool_or_harness_failure", "exporter-artifact", repr(matches))
    return Path(matches.pop())


def assert_export(text):
    if (len(re.findall(rf"^test {re.escape(EXPORT_TEST)} \.\.\. ok$", text, re.M)) != 1
            or len(re.findall(r"test result: ok\. 1 passed; 0 failed; 0 ignored;", text)) != 1):
        raise Failure("export", "tool_or_harness_failure", "exact-test-hit")


def assert_complete(expected, rows):
    if (not expected or len(rows) != len(expected) or sorted(row["id"] for row in rows) != sorted(expected)
            or any(row["status"] != "pass" for row in rows)):
        raise Failure("batch", "tool_or_harness_failure", "case-inventory")


def prepare_case(case, directory):
    model.validate(case)
    rendered = model.render(case)
    raw = rendered["source"].encode()
    if len(raw) > 8192:
        raise Failure("generate", "resource_limit", "source-bytes")
    directory.mkdir()
    (directory / "case.ko").write_bytes(raw)
    write_json(directory / "case.json", case)
    write_json(directory / "expected-diagnostics.json", rendered["expected_diagnostics"])
    oracle = None if rendered["expected_diagnostics"] else model.evaluate(case)
    write_json(directory / "oracle.json", oracle)
    (directory / "expected.stdout").write_bytes(b"" if oracle is None else oracle["stdout"].encode())
    (directory / "expected-allocations.txt").write_text(f"{0 if oracle is None else oracle['allocations']}\n")
    names = ["case.ko", "case.json", "expected-diagnostics.json", "oracle.json", "expected.stdout", "expected-allocations.txt"]
    if "expected_free_order" in case:
        (directory / "expected-order.txt").write_text(",".join(map(str, case["expected_free_order"])) + "\n")
        names.append("expected-order.txt")
    checks.seal_inputs(directory, names)


def copy_replay(source, directory):
    names = checks.verify_inputs(source)
    required = {"case.ko", "case.json", "expected-diagnostics.json", "oracle.json", "expected.stdout", "expected-allocations.txt"}
    if not required <= set(names):
        raise Failure("replay", "tool_or_harness_failure", "missing-replay-input")
    directory.mkdir()
    for name in (*names, "input-sha256.json"):
        shutil.copyfile(source / name, directory / name)


class Execution:
    def __init__(self, root, binary=None):
        self.root = root
        self.binary = binary
        prefix = os.environ.get("LLVM_SYS_211_PREFIX")
        if not prefix:
            raise Failure("setup", "tool_or_harness_failure", "LLVM_SYS_211_PREFIX")
        self.prefix = Path(prefix)
        self.clang = self.prefix / "bin/clang"
        self.linux = platform.system() == "Linux"
        self.deadline = None
        self.limits = {"output_bytes": 1024*1024, "artifact_bytes": 64*1024*1024, "artifact_root": str(root)}
        if self.linux:
            if platform.machine() != "x86_64":
                raise Failure("setup", "tool_or_harness_failure", "unsupported-linux-architecture")
            self.limits.update(rss_bytes=1024*1024*1024, processes=16)

    def run(self, command, directory, name, *, timeout=30, env=None, must_succeed=False):
        if self.deadline is not None:
            timeout = min(timeout, self.deadline - time.monotonic())
            if timeout <= 0:
                raise Failure("batch", "timeout", "execution-budget")
        try:
            result = native.run(command, directory, name, timeout=timeout, env=env, limits=self.limits)
        except native.ResourceLimit as error:
            raise Failure(name, "resource_limit", "process-budget", str(error)) from error
        except subprocess.TimeoutExpired as error:
            raise Failure(name, "timeout", "process-timeout", str(error)) from error
        except OSError as error:
            raise Failure(name, "tool_or_harness_failure", "process-monitor-or-tool", str(error)) from error
        if must_succeed and result.returncode:
            raise Failure(name, "tool_or_harness_failure", "command-exit", result.stderr.decode(errors="replace"))
        return result

    def record_packages(self):
        # A workspace-local LLVM prefix need not be installed in the system's
        # package database. Exact executable versions and required runtime
        # archive presence/hashes remain mandatory for either packaging mode.
        result = self.run(["dpkg-query", "--show", "llvm-21-dev", "clang-21", "libclang-rt-21-dev"],
                          self.root, "runtime-packages")
        write_json(self.root / "package-provenance.json", dict(
            prefix=str(self.prefix), system_packages_verified=result.returncode == 0,
            status="package-manager-recorded" if result.returncode == 0 else "package-manager-metadata-unavailable",
            package_query_exit=result.returncode))

    def setup(self):
        versions = {}
        for name, command in {
            "llvm": [self.prefix / "bin/llvm-config", "--version"],
            "clang": [self.clang, "--version"], "rust": ["rustc", "--version"],
            "cargo": ["cargo", "--version"], "python": [sys.executable, "--version"],
            "compiler-sha": ["git", "rev-parse", "HEAD"], "compiler-status": ["git", "status", "--porcelain"],
        }.items():
            result = self.run(command, self.root, name, must_succeed=True)
            versions[name] = result.stdout.decode().strip()
        check_toolchain_version(versions["llvm"], versions["clang"], linux=self.linux)
        if os.environ.get("CI") == "true" and versions["compiler-status"]:
            raise Failure("setup", "tool_or_harness_failure", "dirty-ci-checkout")
        diff = self.run(["git", "diff", "HEAD", "--binary"], self.root, "compiler-diff", must_succeed=True)
        (self.root / "compiler.patch").write_bytes(diff.stdout)
        # Untracked implementation files do not appear in git diff; hash them too.
        untracked = self.run(["git", "ls-files", "--others", "--exclude-standard", "-z"],
                             self.root, "compiler-untracked", must_succeed=True)
        files = [value.decode() for value in untracked.stdout.split(b"\0") if value]
        write_json(self.root / "compiler-untracked-sha256.json", {name: checks.sha256(ROOT / name) for name in files})
        if self.linux:
            archives = native.linux_runtime_archives(self.root, self.clang)
            write_json(self.root / "runtime-sha256.json", {str(path): checks.sha256(path) for path in archives})
            self.record_packages()
            self.run([self.prefix / "bin/llvm-symbolizer", "--version"], self.root, "symbolizer", must_succeed=True)
        if self.binary is None:
            built = self.run(["cargo", "test", "--locked", "--offline", "-p", "lang-codegen", "--lib", "--no-run",
                              "--message-format=json"], self.root, "build-exporter", timeout=900, must_succeed=True)
            self.binary = test_binary(built.stdout.decode())
        if not self.binary.is_file():
            raise Failure("setup", "tool_or_harness_failure", "missing-exporter")
        write_json(self.root / "environment.json", dict(versions=versions, system=platform.system(),
                   machine=platform.machine(), lock_sha256=checks.sha256(ROOT / "Cargo.lock"),
                   exporter=str(self.binary), exporter_sha256=checks.sha256(self.binary),
                   source_patch_sha256=checks.sha256(self.root / "compiler.patch"), limits=self.limits,
                   rss_scope="sampled Linux process group" if self.linux else "not monitored on macOS"))

    def export(self, directory):
        result = self.run([self.binary, EXPORT_TEST, "--exact", "--nocapture", "--test-threads=1"],
                          directory, "export", timeout=20, env={"KOVEN_GENERATED_OWNER_CASE": str(directory)})
        if result.returncode:
            raise Failure("export", "ssa_or_codegen_failure", "export-test-failed", result.stderr.decode(errors="replace"),
                          stable_witness=False)
        assert_export(result.stdout.decode())

    def build_run(self, directory, name, llvm, *, counter=False, detector=None):
        executable = directory / name
        args = [self.clang, "-O0", "-g", "-fno-omit-frame-pointer", llvm]
        env = {}
        if counter:
            args.append(directory / "counter.c")
        if detector == "asan":
            args.append("-fsanitize=address")
            env = {"ASAN_OPTIONS": "detect_leaks=0:halt_on_error=1:abort_on_error=0:exitcode=86",
                   "ASAN_SYMBOLIZER_PATH": str(self.prefix / "bin/llvm-symbolizer")}
        elif detector == "lsan":
            native.assert_lsan_entry(Path(llvm).read_text())
            adapter = directory / "lsan-entry.c"
            adapter.write_bytes((ROOT / "scripts/native_lsan_entry.c").read_bytes())
            args += ["-fsanitize=leak", "-pthread", "-Wl,--wrap=main", adapter]
            env = {"LSAN_OPTIONS": f"detect_leaks=1:exitcode=87:external_symbolizer_path={self.prefix / 'bin/llvm-symbolizer'}"}
        self.run([*args, "-o", executable], directory, f"build-{name}", must_succeed=True)
        return self.run([executable], directory, f"run-{name}", timeout=5, env=env)

    def accept_native(self, result, expected, mode):
        if result.returncode:
            for detector, runtime in (("AddressSanitizer", "asan"), ("LeakSanitizer", "lsan")):
                if f"{detector} has encountered a fatal error".encode() in result.stderr:
                    raise Failure("native", "tool_or_harness_failure", f"{runtime}-runtime-unavailable",
                                  result.stderr.decode(errors="replace"), stable_witness=False)
            for detector in ("AddressSanitizer", "LeakSanitizer"):
                failure = checks.sanitizer_failure(result.stderr, detector)
                if failure is not None:
                    raise failure
            if mode == "counter" and any(mark in result.stderr for mark in (b"counter.c", b"counted_free", b"verify_counts")):
                raise Failure("native", "resource_counter_failure", "pointer-ledger", result.stderr.decode(errors="replace"),
                              stable_witness=False)
            raise Failure("native", "process_crash_without_detector", mode, str(result.returncode), stable_witness=False)
        if result.stdout != expected:
            raise checks.output_failure(expected, result.stdout)
        if result.stderr:
            raise Failure("native", "native_output_mismatch", "unexpected-stderr", result.stderr.decode(errors="replace"))

    def case(self, directory):
        checks.verify_inputs(directory)
        self.export(directory)
        expected = json.loads((directory / "expected-diagnostics.json").read_text())
        checks.check_diagnostics(directory, expected)
        if expected:
            if list(directory.glob("*.ll")) or (directory / "counter.c").exists():
                raise Failure("frontend", "tool_or_harness_failure", "invalid-case-emitted-llvm")
            return
        stdout = (directory / "expected.stdout").read_bytes()
        self.accept_native(self.build_run(directory, "counter", directory / "case.counter.ll", counter=True), stdout, "counter")
        if self.linux:
            self.accept_native(self.build_run(directory, "asan", directory / "case.asan.ll", detector="asan"), stdout, "asan")
            self.accept_native(self.build_run(directory, "lsan", directory / "case.raw.ll", detector="lsan"), stdout, "lsan")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", required=True, type=Path)
    parser.add_argument("--replay", type=Path)
    parser.add_argument("--exporter", type=Path, help="reuse a freshly built test binary; its SHA is recorded")
    args = parser.parse_args(argv)
    root = args.artifacts.resolve()
    root.mkdir(parents=True, exist_ok=False)
    rows = []
    first_failure = None
    try:
        executor = Execution(root, args.exporter)
        executor.setup()
        executor.deadline = time.monotonic() + 300
        if args.replay:
            sources = [args.replay.resolve()]
            expected_ids = [json.loads((sources[0] / "case.json").read_text())["id"]]
        else:
            sources = model.cases()
            expected_ids = [case["id"] for case in sources]
        write_json(root / "expected-cases.json", expected_ids)
        for index, source in enumerate(sources):
            directory = root / f"case-{index:02d}"
            if args.replay:
                copy_replay(source, directory)
            else:
                prepare_case(source, directory)
            case = json.loads((directory / "case.json").read_text())
            try:
                executor.case(directory)
            except Failure as failure:
                first_failure = failure.record()
                rows.append(dict(id=case["id"], status="failure", **first_failure))
                write_json(directory / "verdict.json", first_failure)
                # Reduction gets a separate bounded window, preserving the first verdict.
                executor.deadline = time.monotonic() + 120
                reductions = root / "reduction"
                def replay(candidate, attempt):
                    path = reductions / f"attempt-{attempt:02d}"
                    prepare_case(candidate, path)
                    try:
                        executor.case(path)
                    except Failure as observed:
                        write_json(path / "verdict.json", observed.record())
                        return observed
                    return None
                started = time.monotonic()
                try:
                    reductions.mkdir()
                    reduced = checks.minimize(case, failure, model.shrink_candidates, replay,
                                              lambda item: len(model.render(item)["source"].encode()),
                                              deadline=executor.deadline)
                except (OSError, ValueError, AssertionError) as error:
                    # A failed reduction cannot replace the already observed case failure.
                    reduced = dict(original=case, original_failure=failure.record(), minimal=case,
                                   status="minimization_incomplete", attempts=[], confirmation_count=0,
                                   elapsed_seconds=time.monotonic() - started,
                                   reduction_error=dict(stage="minimization", kind="tool_or_harness_failure",
                                                        witness=type(error).__name__, detail=str(error)))
                write_json(root / "minimization.json", reduced)
                raise
            rows.append(dict(id=case["id"], status="pass"))
            write_json(directory / "verdict.json", dict(status="pass"))
            print(f"generated owner {case['id']}: passed", flush=True)
        assert_complete(expected_ids, rows)
        if not args.replay:
            calibration_dir = root / "calibration"
            calibration_dir.mkdir(parents=True, exist_ok=True)
            calibration.verify(executor, calibration_dir)
        write_json(root / "acceptance.json", dict(status="pass", cases=rows, replay=bool(args.replay)))
        return 0
    except (Failure, OSError, ValueError, AssertionError) as error:
        record = error.record() if isinstance(error, Failure) else dict(
            stage="harness", kind="tool_or_harness_failure", witness=type(error).__name__, detail=str(error))
        report = dict(first_failure=first_failure or record, cases=rows)
        if first_failure is not None and record != first_failure:
            report["evidence_failure"] = record
        write_json(root / "failure.json", report)
        print(f"generated owner verification failed: {error}; artifacts: {root}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
