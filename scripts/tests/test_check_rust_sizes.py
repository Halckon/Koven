"""Black-box policy tests for the incremental Rust physical-line size gate."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "check_rust_sizes.py"
POLICY = "scripts/rust-size-policy.json"


def exception(max_lines: int = 1100) -> dict:
    return {
        "max_lines": max_lines,
        "reason": "Keep one coherent parser responsibility during migration",
        "owner": "frontend maintainers",
        "split_plan": "Extract diagnostic rendering into its own module",
        "review": "Review at the next parser maintenance change",
    }


def generated() -> dict:
    return {
        "inputs": "grammar/source.txt",
        "generator": "scripts/generate_parser.py",
        "version": "1.0",
    }


class RustSizePolicyTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory(prefix="rust-size-policy-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.git("init", "--quiet")
        self.git("config", "user.name", "Rust size policy test")
        self.git("config", "user.email", "rust-size-tests@example.invalid")
        self.git("config", "core.autocrlf", "false")
        self.git("commit", "--quiet", "--allow-empty", "-m", "Initial fixture")
        self.base = self.git("rev-parse", "HEAD").strip()
        self.write_policy()

    def git(self, *args: str) -> str:
        result = subprocess.run(
            ["git", *args], cwd=self.root, text=True, capture_output=True, check=True
        )
        return result.stdout

    def write(self, path: str, contents: bytes) -> Path:
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(contents)
        return target

    def rust(self, path: str, lines: int, *, newline: bytes = b"\n",
             final_newline: bool = True, prefix: str = "line") -> Path:
        contents = newline.join(f"// {prefix} {number}".encode() for number in range(lines))
        if lines and final_newline:
            contents += newline
        return self.write(path, contents)

    def write_policy(self, *, baseline: dict | None = None,
                     exceptions: dict | None = None,
                     generated_files: dict | None = None) -> dict:
        policy = {
            "version": 1,
            "baseline": baseline or {},
            "exceptions": exceptions or {},
            "generated": generated_files or {},
        }
        self.write(POLICY, json.dumps(policy, indent=2).encode())
        return policy

    def commit(self) -> str:
        self.git("add", "--all")
        self.git("commit", "--quiet", "--allow-empty", "-m", "Fixture base")
        self.base = self.git("rev-parse", "HEAD").strip()
        return self.base

    def existing(self, lines: int = 1200, *, budget: int | None = None) -> None:
        self.rust("src/existing.rs", lines)
        self.write_policy(baseline={"src/existing.rs": budget or lines})
        self.commit()

    def check(self, *, base: str | None = None) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--base", base or self.base,
             "--root", str(self.root)],
            cwd=self.root, text=True, capture_output=True,
        )

    def assert_passes(self) -> None:
        result = self.check()
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)

    def assert_fails(self, *details: str, base: str | None = None) -> None:
        result = self.check(base=base)
        output = result.stdout + result.stderr
        self.assertNotEqual(0, result.returncode, output)
        for detail in details:
            self.assertIn(detail, output)

    def test_new_file_at_1000_lines_passes(self) -> None:
        self.rust("src/new.rs", 1000)
        self.assert_passes()

    def test_new_file_at_1001_lines_fails(self) -> None:
        self.rust("src/new.rs", 1001)
        self.assert_fails("src/new.rs", "1001", "1000")

    def test_empty_file_passes(self) -> None:
        self.rust("src/empty.rs", 0)
        self.assert_passes()

    def test_physical_lines_count_crlf_and_missing_final_newline(self) -> None:
        for newline in (b"\n", b"\r\n"):
            for final_newline in (False, True):
                for lines in (1000, 1001):
                    with self.subTest(newline=newline, final_newline=final_newline, lines=lines):
                        self.rust("src/new.rs", lines, newline=newline,
                                  final_newline=final_newline)
                        if lines == 1000:
                            self.assert_passes()
                        else:
                            self.assert_fails("src/new.rs", "1001")

    def test_blank_lines_count_toward_the_limit(self) -> None:
        self.write("src/blank.rs", b"\n" * 1001)
        self.assert_fails("src/blank.rs", "1001")

    def test_non_lf_control_characters_do_not_create_physical_lines(self) -> None:
        self.write("src/controls.rs", b"// text\v\f\rtext\n" * 1000)
        self.assert_passes()

    def test_initial_baseline_preserves_all_49_existing_large_files(self) -> None:
        baseline = {}
        for number in range(49):
            path = f"src/existing_{number:02}.rs"
            baseline[path] = 1001 + number
            self.rust(path, baseline[path])
        (self.root / POLICY).unlink()
        self.commit()
        self.write_policy(baseline=baseline)
        self.assert_passes()

    def test_initial_baseline_must_include_every_large_base_file(self) -> None:
        self.rust("src/old.rs", 1200)
        (self.root / POLICY).unlink()
        self.commit()
        self.write_policy()
        self.assert_fails("src/old.rs")

    def test_initial_baseline_must_use_exact_base_sizes(self) -> None:
        self.rust("src/old.rs", 1200)
        (self.root / POLICY).unlink()
        self.commit()
        for budget in (1199, 1201):
            with self.subTest(budget=budget):
                self.write_policy(baseline={"src/old.rs": budget})
                self.assert_fails("src/old.rs")

    def test_initial_baseline_cannot_grandfather_a_new_file(self) -> None:
        self.rust("src/new.rs", 1001)
        self.write_policy(baseline={"src/new.rs": 1001})
        self.assert_fails("src/new.rs")

    def test_existing_file_may_stay_same_size_or_shrink(self) -> None:
        self.existing()
        for lines in (1200, 1100, 1000, 900):
            with self.subTest(lines=lines):
                self.rust("src/existing.rs", lines)
                self.assert_passes()

    def test_existing_file_cannot_grow_above_base_size(self) -> None:
        self.existing()
        self.rust("src/existing.rs", 1201)
        self.assert_fails("src/existing.rs", "1201", "1200")

    def test_actual_base_size_tightens_an_older_larger_budget(self) -> None:
        self.existing(lines=1100, budget=1200)
        self.rust("src/existing.rs", 1101)
        self.assert_fails("src/existing.rs", "1101", "1100")

    def test_shrink_below_limit_does_not_preserve_future_growth_allowance(self) -> None:
        self.existing()
        self.rust("src/existing.rs", 900)
        self.commit()
        self.rust("src/existing.rs", 1001)
        self.assert_fails("src/existing.rs", "1001")

    def test_head_baseline_may_lower_but_not_raise_existing_budget(self) -> None:
        self.existing()
        self.rust("src/existing.rs", 1100)
        self.write_policy(baseline={"src/existing.rs": 1100})
        self.assert_passes()
        self.write_policy(baseline={"src/existing.rs": 1201})
        self.assert_fails("src/existing.rs")

    def test_baseline_cannot_be_increased_even_with_an_exception(self) -> None:
        self.existing()
        self.write_policy(baseline={"src/existing.rs": 1300},
                          exceptions={"src/existing.rs": exception(1300)})
        self.assert_fails("src/existing.rs")

    def test_subsequent_baseline_cannot_add_a_new_budget(self) -> None:
        self.commit()
        self.rust("src/new.rs", 1001)
        self.write_policy(baseline={"src/new.rs": 1001})
        self.assert_fails("src/new.rs")

    def test_resolved_baseline_entry_may_be_removed(self) -> None:
        self.existing()
        self.rust("src/existing.rs", 1000)
        self.write_policy()
        self.assert_passes()

    def test_deleted_file_and_its_baseline_entry_may_be_removed(self) -> None:
        self.existing()
        (self.root / "src/existing.rs").unlink()
        self.write_policy()
        self.assert_passes()

    def test_complete_exception_allows_new_file_up_to_exact_maximum(self) -> None:
        self.rust("src/new.rs", 1100)
        self.write_policy(exceptions={"src/new.rs": exception()})
        self.assert_passes()
        self.rust("src/new.rs", 1101)
        self.assert_fails("src/new.rs", "1101", "1100")

    def test_complete_exception_allows_existing_file_growth(self) -> None:
        self.existing()
        self.rust("src/existing.rs", 1250)
        self.write_policy(baseline={"src/existing.rs": 1200},
                          exceptions={"src/existing.rs": exception(1250)})
        self.assert_passes()

    def test_exception_requires_each_nonempty_metadata_field(self) -> None:
        self.rust("src/new.rs", 1001)
        for field in ("reason", "owner", "split_plan", "review"):
            for value in (None, "", "  ", 7):
                with self.subTest(field=field, value=value):
                    entry = exception()
                    if value is None:
                        del entry[field]
                    else:
                        entry[field] = value
                    self.write_policy(exceptions={"src/new.rs": entry})
                    self.assert_fails("src/new.rs", field)

    def test_invalid_exception_diagnostic_is_independent_of_hash_seed(self) -> None:
        self.rust("src/new.rs", 1001)
        entry = exception()
        for field in ("reason", "owner", "split_plan", "review"):
            entry[field] = ""
        self.write_policy(exceptions={"src/new.rs": entry})
        outputs = []
        for seed in range(1, 6):
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--base", self.base,
                 "--root", str(self.root)],
                cwd=self.root, text=True, capture_output=True,
                env={**os.environ, "PYTHONHASHSEED": str(seed)},
            )
            output = result.stdout + result.stderr
            self.assertNotEqual(0, result.returncode, f"seed={seed}: {output}")
            outputs.append(output)
        self.assertEqual([outputs[0]] * len(outputs), outputs)

    def test_budget_fields_require_integers_greater_than_1000(self) -> None:
        self.rust("src/new.rs", 1001)
        for value in (True, 1000, 0, -1, 1100.0, "1100"):
            for section in ("baseline", "exceptions"):
                with self.subTest(value=value, section=section):
                    if section == "baseline":
                        self.write_policy(baseline={"src/new.rs": value})
                    else:
                        entry = exception()
                        entry["max_lines"] = value
                        self.write_policy(exceptions={"src/new.rs": entry})
                    self.assert_fails("src/new.rs")

    def test_rename_preserves_baseline_when_size_does_not_grow(self) -> None:
        self.existing()
        self.git("mv", "src/existing.rs", "src/renamed.rs")
        self.write_policy(baseline={"src/renamed.rs": 1200})
        self.assert_passes()

    def test_rename_still_compares_growth_to_old_base_path(self) -> None:
        self.existing()
        self.git("mv", "src/existing.rs", "src/renamed.rs")
        self.rust("src/renamed.rs", 1201)
        self.write_policy(baseline={"src/renamed.rs": 1200})
        self.assert_fails("src/renamed.rs", "1201", "1200")

    def test_rename_cannot_increase_the_original_baseline_budget(self) -> None:
        self.existing()
        self.git("mv", "src/existing.rs", "src/renamed.rs")
        self.write_policy(baseline={"src/renamed.rs": 1201})
        self.assert_fails("src/renamed.rs")

    def test_copy_is_new_file_and_cannot_inherit_source_baseline(self) -> None:
        self.existing()
        shutil.copyfile(self.root / "src/existing.rs", self.root / "src/copied.rs")
        self.git("add", "src/copied.rs")
        self.assert_fails("src/copied.rs")
        self.write_policy(baseline={"src/existing.rs": 1200, "src/copied.rs": 1200})
        self.assert_fails("src/copied.rs")

    def test_low_similarity_move_cannot_inherit_source_baseline(self) -> None:
        self.existing()
        self.git("mv", "src/existing.rs", "src/rewritten.rs")
        self.rust("src/rewritten.rs", 1200, prefix="entirely different responsibility")
        self.git("add", "src/rewritten.rs")
        self.write_policy(baseline={"src/rewritten.rs": 1200})
        self.assert_fails("src/rewritten.rs")

    def test_generated_file_requires_explicit_path_and_complete_metadata(self) -> None:
        self.rust("src/generated.rs", 1400)
        self.write_policy(generated_files={"src/generated.rs": generated()})
        self.assert_passes()
        for field in ("inputs", "generator", "version"):
            for value in (None, "", "  ", 7):
                with self.subTest(field=field, value=value):
                    entry = generated()
                    if value is None:
                        del entry[field]
                    else:
                        entry[field] = value
                    self.write_policy(generated_files={"src/generated.rs": entry})
                    self.assert_fails("src/generated.rs", field)

    def test_initial_baseline_excludes_explicitly_generated_file(self) -> None:
        self.rust("src/generated.rs", 1400)
        (self.root / POLICY).unlink()
        self.commit()
        self.write_policy(generated_files={"src/generated.rs": generated()})
        self.assert_passes()

    def test_generated_directory_name_does_not_exempt_files(self) -> None:
        self.rust("generated/parser.rs", 1001)
        self.assert_fails("generated/parser.rs")

    def test_generated_paths_cannot_also_have_baselines_or_exceptions(self) -> None:
        self.rust("src/generated.rs", 1400)
        for section in ("baseline", "exceptions"):
            with self.subTest(section=section):
                kwargs = {"generated_files": {"src/generated.rs": generated()}}
                entry = 1400 if section == "baseline" else exception(1400)
                kwargs[section] = {"src/generated.rs": entry}
                self.write_policy(**kwargs)
                self.assert_fails("src/generated.rs")

    def test_initial_policy_rejects_invalid_or_unmatched_paths(self) -> None:
        self.rust("src/new.rs", 1000)
        self.write("src/notes.txt", b"notes\n")
        for path in ("src/*.rs", "src/missing.rs", "src/notes.txt", "../outside.rs"):
            for section in ("baseline", "exceptions", "generated"):
                with self.subTest(path=path, section=section):
                    kwargs = {"baseline": {}, "exceptions": {}, "generated_files": {}}
                    key = "generated_files" if section == "generated" else section
                    entry = {"baseline": 1200, "exceptions": exception(),
                             "generated": generated()}[section]
                    kwargs[key] = {path: entry}
                    self.write_policy(**kwargs)
                    self.assert_fails()

    def test_deleted_file_may_keep_historical_baseline_but_cannot_reuse_it(self) -> None:
        self.existing()
        (self.root / "src/existing.rs").unlink()
        self.assert_passes()
        self.commit()
        self.rust("src/existing.rs", 1001)
        self.assert_fails("src/existing.rs")

    def test_tracked_ignored_file_is_checked_but_ignored_untracked_is_not(self) -> None:
        self.write(".gitignore", b"ignored/\n")
        self.rust("ignored/tracked.rs", 1000)
        self.git("add", "--force", "ignored/tracked.rs")
        self.commit()
        self.rust("ignored/untracked.rs", 1500)
        self.assert_passes()
        self.rust("ignored/tracked.rs", 1001)
        self.assert_fails("ignored/tracked.rs")

    def test_nonignored_untracked_file_is_checked(self) -> None:
        self.commit()
        self.rust("src/untracked.rs", 1001)
        self.assert_fails("src/untracked.rs", "1001")

    def test_non_rust_files_are_not_counted(self) -> None:
        self.write("notes.txt", b"line\n" * 2000)
        self.assert_passes()

    def test_paths_with_spaces_are_checked(self) -> None:
        self.rust("src/a file.rs", 1001)
        self.assert_fails("src/a file.rs")

    def test_invalid_base_fails_instead_of_skipping_the_gate(self) -> None:
        self.assert_fails(base="refs/heads/nonexistent-policy-test-base")

    def test_diverged_target_is_compared_at_shared_merge_base(self) -> None:
        self.existing()
        shared = self.base
        self.git("checkout", "--quiet", "-b", "target")
        self.rust("src/existing.rs", 1100)
        self.rust("src/target_only.rs", 1000)
        target = self.commit()
        self.git("checkout", "--quiet", "-b", "feature", shared)
        self.rust("src/feature_only.rs", 1000)
        self.commit()
        result = self.check(base=target)
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)

    def test_unrelated_base_fails_instead_of_using_empty_history(self) -> None:
        original = self.base
        self.git("checkout", "--quiet", "--orphan", "unrelated")
        self.commit()
        unrelated = self.base
        self.git("checkout", "--quiet", "--detach", original)
        self.write_policy()
        self.assert_fails(base=unrelated)

    def test_base_argument_is_required(self) -> None:
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(self.root)],
            cwd=self.root, text=True, capture_output=True,
        )
        self.assertNotEqual(0, result.returncode)
        self.assertIn("--base", result.stdout + result.stderr)

    def test_root_defaults_to_script_repository_from_a_subdirectory(self) -> None:
        fixture_script = self.root / "scripts/check_rust_sizes.py"
        shutil.copyfile(SCRIPT, fixture_script)
        self.rust("src/new.rs", 1000)
        result = subprocess.run(
            [sys.executable, str(fixture_script), "--base", self.base],
            cwd=self.root / "src", text=True, capture_output=True,
        )
        self.assertEqual(0, result.returncode, result.stdout + result.stderr)

    def test_missing_or_malformed_policy_fails(self) -> None:
        for contents in (None, b"{not json", b"[]", b"{}"):
            with self.subTest(contents=contents):
                if contents is None:
                    (self.root / POLICY).unlink()
                else:
                    self.write(POLICY, contents)
                self.assert_fails()

    def test_policy_schema_rejects_invalid_version_and_section_types(self) -> None:
        for field, values in (("version", (True, 2, "1")),
                              ("baseline", ([], None)),
                              ("exceptions", ([], None)),
                              ("generated", ([], None))):
            for value in values:
                with self.subTest(field=field, value=value):
                    policy = self.write_policy()
                    policy[field] = value
                    self.write(POLICY, json.dumps(policy).encode())
                    self.assert_fails()

    def test_malformed_base_policy_cannot_be_reinitialized(self) -> None:
        self.write(POLICY, b"{not valid json")
        self.commit()
        self.write_policy()
        self.assert_fails()


if __name__ == "__main__":
    unittest.main()
