#!/usr/bin/env python3
"""SPEC-0269: Real frontend checker mutation verification (G4).

Disables one L0131 reporting point in crates/lang-frontend on I1 path, verifies
red test, restores source, checks diff/hash clean, and verifies green test.
"""
import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time

if __package__:
    from . import generated_owners as model, generated_owner_checks as checks
else:
    import generated_owners as model
    import generated_owner_checks as checks

Failure = checks.Failure
ROOT = Path(__file__).resolve().parents[1]
CHECKER_FILE = "crates/lang-frontend/src/ownership_checking/checker.rs"
FRONTEND_EXPORT_TEST = "native_generated_owner_tests::export_generated_owner_frontend_case"


def sha256(data):
    if isinstance(data, str):
        data = data.encode("utf-8")
    return hashlib.sha256(data).hexdigest()


def write_bytes(path, content):
    """Publish one complete evidence file; a failed write leaves the old file intact."""
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=".evidence-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as output:
            output.write(content)
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def write_json(path, value):
    write_bytes(path, (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode())


def recorded_run(directory, name, argv, *, cwd, timeout, env=None):
    command = dict(argv=[str(item) for item in argv], cwd=str(cwd), timeout_seconds=timeout)
    if env is not None:
        command["env"] = {"KOVEN_GENERATED_OWNER_CASE": env["KOVEN_GENERATED_OWNER_CASE"]}
    write_json(directory / f"{name}.command.json", command)
    started = time.monotonic()
    result = dict(exit=None, timed_out=False)
    stdout = stderr = b""
    try:
        completed = subprocess.run(argv, cwd=cwd, capture_output=True, timeout=timeout,
                                   **({"env": env} if env is not None else {}))
        result["exit"] = completed.returncode
        stdout, stderr = completed.stdout, completed.stderr
        return completed
    except Exception as error:
        result["error"] = dict(type=type(error).__name__, detail=str(error))
        if isinstance(error, subprocess.TimeoutExpired):
            result["timed_out"] = True
            stdout, stderr = error.stdout or b"", error.stderr or b""
            raise Failure("checker-mutation", "timeout", f"{name}-timeout") from error
        if isinstance(error, OSError):
            raise Failure("checker-mutation", "tool_or_harness_failure", f"{name}-spawn-error", str(error)) from error
        raise
    finally:
        result["elapsed_seconds"] = time.monotonic() - started
        write_bytes(directory / f"{name}.stdout", stdout)
        write_bytes(directory / f"{name}.stderr", stderr)
        write_json(directory / f"{name}.result.json", result)


def assert_frontend_hit(text):
    if (len(re.findall(rf"^test {re.escape(FRONTEND_EXPORT_TEST)} \.\.\. ok$", text, re.M)) != 1
            or len(re.findall(r"test result: ok\. 1 passed; 0 failed; 0 ignored;", text)) != 1):
        raise Failure("checker-mutation", "tool_or_harness_failure", "exact-test-hit")


def mutate_checker(source):
    pattern = r'(fn ensure_place_available[\s\S]*?diagnostic\.add_label\(self\.sources,\s*origin,\s*"value was moved here"\)\?;\s*\n\s*)self\.diagnostics\.push\(diagnostic\);'
    if not re.search(pattern, source):
        raise Failure("checker-mutation", "tool_or_harness_failure", "target-pattern-not-found")
    return re.sub(pattern, r'\1// MUTANT_CHECKER_DISABLED: self.diagnostics.push(diagnostic);', source, count=1)


def run_frontend_case(binary, case_dir, cwd=ROOT):
    env = os.environ.copy()
    env["KOVEN_GENERATED_OWNER_CASE"] = str(case_dir)
    return subprocess.run(
        [str(binary), FRONTEND_EXPORT_TEST, "--exact", "--nocapture"],
        cwd=cwd, env=env, capture_output=True, timeout=30,
    )


def extract_test_binary(cargo_stdout):
    for line in cargo_stdout.splitlines():
        try:
            item = json.loads(line)
            if (item.get("reason") == "compiler-artifact" and item.get("target", {}).get("name") == "lang_codegen"
                    and item.get("profile", {}).get("test") and item.get("executable")):
                return Path(item["executable"])
        except json.JSONDecodeError:
            pass
    return None


def verify_checker_mutant(root_dir, artifacts_dir):
    artifacts_dir.mkdir(parents=True, exist_ok=True)
    temp_worktree = Path(tempfile.mkdtemp(prefix="koven-checker-worktree-"))
    try:
        add_res = recorded_run(artifacts_dir, "worktree-add",
                               ["git", "worktree", "add", str(temp_worktree), "HEAD", "--detach"],
                               cwd=root_dir, timeout=30)
        if add_res.returncode != 0:
            raise Failure("checker-mutation", "tool_or_harness_failure", "git-worktree-add-failed")
        target_path = temp_worktree / CHECKER_FILE
        original = target_path.read_bytes()
        write_bytes(artifacts_dir / "original-source.rs", original)
        mutated = mutate_checker(original.decode("utf-8")).encode("utf-8")
        patch = "".join(difflib.unified_diff(original.decode().splitlines(keepends=True),
                                          mutated.decode().splitlines(keepends=True),
                                          fromfile=CHECKER_FILE, tofile=CHECKER_FILE)).encode()
        write_bytes(artifacts_dir / "mutated-source.rs", mutated)
        write_bytes(artifacts_dir / "mutant.patch", patch)
        i1_case = next(c for c in model.cases() if c["shape"] == "I1")
        rendered = model.render(i1_case)
        evidence = dict(target_file=CHECKER_FILE, original_sha256=sha256(original),
                        mutated_sha256=sha256(mutated), patch_sha256=sha256(patch),
                        i1_case_id=i1_case["id"], isolated_worktree=str(temp_worktree), phases={})
        build_argv = ["cargo", "test", "--manifest-path", str(temp_worktree / "Cargo.toml"),
                      "-p", "lang-codegen", "--lib", "--no-run", "--message-format=json"]

        def phase(name, expected):
            directory = artifacts_dir / name
            directory.mkdir(exist_ok=False)
            case_dir = directory / "case"
            case_dir.mkdir()
            write_bytes(case_dir / "case.ko", rendered["source"].encode())
            write_json(case_dir / "expected-diagnostics.json", expected)
            build = recorded_run(directory, "build", build_argv, cwd=temp_worktree, timeout=900)
            if build.returncode != 0:
                raise Failure("checker-mutation", "tool_or_harness_failure", f"{name}-cargo-build-failed")
            binary = extract_test_binary(build.stdout.decode())
            if binary is None or not binary.is_file():
                raise Failure("checker-mutation", "tool_or_harness_failure", "missing-test-binary")
            write_json(directory / "exporter.json", dict(path=str(binary), sha256=sha256(binary.read_bytes())))
            env = {**os.environ, "KOVEN_GENERATED_OWNER_CASE": str(case_dir)}
            execution = recorded_run(directory, "export", [str(binary), FRONTEND_EXPORT_TEST, "--exact", "--nocapture"],
                                     cwd=temp_worktree, env=env, timeout=30)
            if execution.returncode != 0:
                raise Failure("checker-mutation", "tool_or_harness_failure", f"{name}-test-execution-failed")
            assert_frontend_hit(execution.stdout.decode(errors="replace"))
            checks.check_diagnostics(case_dir, expected)
            evidence["phases"][name] = dict(directory=name, build_exit=build.returncode,
                                           export_exit=execution.returncode, exporter_sha256=sha256(binary.read_bytes()))

        red_error = None
        try:
            target_path.write_bytes(mutated)
            phase("red", [])
            evidence["mutant_killed"] = True
            evidence["red_diagnostics_count"] = 0
        except Exception as error:
            red_error = error
            raise
        finally:
            try:
                target_path.write_bytes(original)
                restored = target_path.read_bytes()
                write_bytes(artifacts_dir / "restored-source.rs", restored)
                evidence["restored_sha256"] = sha256(restored)
                diff = recorded_run(artifacts_dir, "restore-diff", ["git", "diff", "--", str(target_path)],
                                    cwd=temp_worktree, timeout=30)
                if diff.returncode != 0 or diff.stdout.strip() or restored != original:
                    raise Failure("checker-mutation", "tool_or_harness_failure", "source-restore-not-clean")
                evidence["restored_clean"] = True
            except Exception as restore_error:
                write_json(artifacts_dir / "restore-failure.json",
                           dict(type=type(restore_error).__name__, detail=str(restore_error)))
                if red_error is None:
                    raise
            finally:
                write_json(artifacts_dir / "checker-progress.json", evidence)
        phase("green", rendered["expected_diagnostics"])
        evidence["green_verified"] = True
        evidence["status"] = "pass"
        write_json(artifacts_dir / "checker-calibration.json", evidence)
        return evidence
    finally:
        subprocess.run(["git", "worktree", "remove", "--force", str(temp_worktree)],
                       cwd=root_dir, capture_output=True)
        if temp_worktree.exists():
            import shutil
            shutil.rmtree(temp_worktree, ignore_errors=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", required=True, type=Path)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args(argv)
    artifacts = args.artifacts.resolve()
    root = args.root.resolve()
    try:
        verify_checker_mutant(root, artifacts)
        print("generated owner checker mutant: red/green cycle verified cleanly", flush=True)
        return 0
    except Failure as failure:
        write_json(artifacts / "checker-failure.json", failure.record())
        print(f"checker mutation verification failed: {failure}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
