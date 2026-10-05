#!/usr/bin/env python3
"""SPEC-0269: Real frontend checker mutation verification (G4).

Disables one L0131 reporting point in crates/lang-frontend on I1 path, verifies
red test, restores source, checks diff/hash clean, and verifies green test.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

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


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n")


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
        # Create detached worktree from HEAD to ensure the main workspace is untouched
        add_res = subprocess.run(["git", "worktree", "add", str(temp_worktree), "HEAD", "--detach"],
                                 cwd=root_dir, capture_output=True)
        if add_res.returncode != 0:
            raise Failure("checker-mutation", "tool_or_harness_failure", "git-worktree-add-failed",
                          add_res.stderr.decode(errors="replace"))

        target_path = temp_worktree / CHECKER_FILE
        if not target_path.is_file():
            raise Failure("checker-mutation", "tool_or_harness_failure", "missing-checker-file")

        original_content = target_path.read_text(encoding="utf-8")
        original_sha = sha256(original_content)
        mutated_content = mutate_checker(original_content)
        patch_diff = f"--- {CHECKER_FILE}\n+++ {CHECKER_FILE} (mutated)\n@@ disable L0131 @@\n- self.diagnostics.push(diagnostic);\n+ // MUTANT_CHECKER_DISABLED: self.diagnostics.push(diagnostic);\n"
        patch_sha = sha256(patch_diff)

        i1_case = next(c for c in model.cases() if c["shape"] == "I1")
        rendered = model.render(i1_case)

        case_tmp = artifacts_dir / "i1-case"
        case_tmp.mkdir(parents=True, exist_ok=True)
        (case_tmp / "case.ko").write_text(rendered["source"], encoding="utf-8")
        write_json(case_tmp / "expected-diagnostics.json", rendered["expected_diagnostics"])

        evidence = dict(
            target_file=CHECKER_FILE,
            original_sha256=original_sha,
            patch_sha256=patch_sha,
            i1_case_id=i1_case["id"],
            isolated_worktree=str(temp_worktree),
        )

        try:
            # Phase 1: Apply mutant in isolated worktree
            target_path.write_text(mutated_content, encoding="utf-8")
            build_red = subprocess.run(
                ["cargo", "test", "--manifest-path", str(temp_worktree / "Cargo.toml"), "-p", "lang-codegen", "--lib",
                 "--no-run", "--message-format=json"],
                cwd=temp_worktree, capture_output=True, timeout=900,
            )
            if build_red.returncode != 0:
                raise Failure("checker-mutation", "tool_or_harness_failure", "mutant-cargo-build-failed",
                              build_red.stderr.decode(errors="replace"))
            test_bin = extract_test_binary(build_red.stdout.decode())
            if test_bin is None or not test_bin.is_file():
                raise Failure("checker-mutation", "tool_or_harness_failure", "missing-test-binary")

            # Clean prior export outputs
            for name in ("stages.tsv", "diagnostics.tsv"):
                if (case_tmp / name).exists():
                    (case_tmp / name).unlink()

            res_red = run_frontend_case(test_bin, case_tmp, cwd=temp_worktree)
            if res_red.returncode != 0:
                raise Failure("checker-mutation", "tool_or_harness_failure", "red-test-execution-failed",
                              res_red.stderr.decode(errors="replace"))

            stages_tsv = case_tmp / "stages.tsv"
            diag_tsv = case_tmp / "diagnostics.tsv"
            if not stages_tsv.is_file():
                raise Failure("checker-mutation", "tool_or_harness_failure", "missing-stages-file")
            if not diag_tsv.is_file():
                raise Failure("checker-mutation", "tool_or_harness_failure", "missing-diagnostics-file")

            stages = [line.split("\t") for line in stages_tsv.read_text().splitlines()]
            if not any(len(s) >= 2 and s[0] == "ownership" for s in stages):
                raise Failure("checker-mutation", "tool_or_harness_failure", "ownership-stage-not-recorded")

            diags = diag_tsv.read_text().splitlines()
            # In red phase, L0131 must NOT be reported
            has_l0131 = any("L0131" in line for line in diags)
            if has_l0131:
                raise Failure("checker-mutation", "unexpected_acceptance", "mutant-not-killed-still-reported-L0131")
            evidence["mutant_killed"] = True
            evidence["red_diagnostics_count"] = len(diags)

        finally:
            # Phase 2: Restore source in isolated worktree
            target_path.write_text(original_content, encoding="utf-8")
            diff_proc = subprocess.run(["git", "diff", "--", str(target_path)], cwd=temp_worktree, capture_output=True)
            if diff_proc.stdout.strip():
                raise Failure("checker-mutation", "tool_or_harness_failure", "dirty-git-diff-after-restore")
            evidence["restored_clean"] = True

        # Phase 3: Green verification in isolated worktree
        build_green = subprocess.run(
            ["cargo", "test", "--manifest-path", str(temp_worktree / "Cargo.toml"), "-p", "lang-codegen", "--lib",
             "--no-run", "--message-format=json"],
            cwd=temp_worktree, capture_output=True, timeout=900,
        )
        if build_green.returncode != 0:
            raise Failure("checker-mutation", "tool_or_harness_failure", "green-cargo-build-failed")
        green_bin = extract_test_binary(build_green.stdout.decode())
        if green_bin is None or not green_bin.is_file():
            raise Failure("checker-mutation", "tool_or_harness_failure", "missing-green-test-binary")

        for name in ("stages.tsv", "diagnostics.tsv"):
            if (case_tmp / name).exists():
                (case_tmp / name).unlink()

        res_green = run_frontend_case(green_bin, case_tmp, cwd=temp_worktree)
        if res_green.returncode != 0:
            raise Failure("checker-mutation", "tool_or_harness_failure", "green-test-execution-failed",
                          res_green.stderr.decode(errors="replace"))
        if not (case_tmp / "diagnostics.tsv").is_file():
            raise Failure("checker-mutation", "tool_or_harness_failure", "missing-green-diagnostics-file")

        checks.check_diagnostics(case_tmp, rendered["expected_diagnostics"])
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
