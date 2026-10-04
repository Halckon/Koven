"""CI event/path policy must never turn an unexecuted required gate green."""

import importlib.util
import re
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "check_ci_results", Path(__file__).resolve().parents[1] / "check_ci_results.py"
)
CI = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CI)


def needs(docs_changed="false", rust_changed="false", editors_changed="false", **results):
    jobs = {name: {"result": results.get(name, "skipped")}
            for name in ("docs", "editors", "fmt", "clippy", "test")}
    jobs["dependencies"] = {"result": results.get("dependencies", "success")}
    jobs["rust-size"] = {"result": results.get("rust-size", "success")}
    jobs["changes"] = {"result": results.get("changes", "success"),
                       "outputs": {"docs": docs_changed, "rust": rust_changed, "editors": editors_changed}}
    return jobs


def stage_target_occurrences(stage, target):
    """Preserve each exact --test token, including repeats on the same command."""
    commands = stage.replace("\\\n", " ").splitlines()
    pattern = rf"(?:^|\s)--test\s+{re.escape(target)}(?=\s|$)"
    return [line for line in commands if line.startswith("run cargo test ")
            for _ in re.findall(pattern, line)]


class CheckCiResultsTests(unittest.TestCase):
    def test_docs_only_pr_can_skip_rust(self):
        self.assertEqual([], CI.check_results(
            needs("true", "false", docs="success"), "pull_request", "refs/pull/1/merge"))

    def test_rust_pr_requires_both_matrix_jobs(self):
        jobs = needs("false", "true", fmt="success", clippy="success", test="success")
        self.assertEqual([], CI.check_results(jobs, "pull_request", "refs/pull/1/merge"))
        for name in ("fmt", "clippy", "test"):
            for result in ("skipped", "failure", "cancelled"):
                with self.subTest(name=name, result=result):
                    altered = {**jobs, name: {"result": result}}
                    self.assertTrue(CI.check_results(altered, "pull_request", "refs/pull/1/merge"))

    def test_main_and_manual_dispatch_require_all_gates(self):
        for event, ref in (("push", "refs/heads/main"), ("workflow_dispatch", "refs/heads/fix/test")):
            jobs = needs("false", "false", **dict.fromkeys(("docs", "editors", "fmt", "clippy", "test"), "success"))
            self.assertEqual([], CI.check_results(jobs, event, ref))
            self.assertTrue(CI.check_results(needs(), event, ref))

    def test_editor_change_requires_actual_job_on_push_and_pr(self):
        for event in ("push", "pull_request"):
            jobs = needs(editors_changed="true", editors="success")
            self.assertEqual([], CI.check_results(jobs, event, "refs/heads/feature/editor"))
            for result in ("skipped", "failure", "cancelled", None):
                with self.subTest(event=event, result=result):
                    altered = {**jobs, "editors": {"result": result}}
                    self.assertTrue(CI.check_results(altered, event, "refs/heads/feature/editor"))

    def test_editor_and_rust_changes_require_both_independent_gates(self):
        jobs = needs(rust_changed="true", editors_changed="true", editors="success",
                     fmt="success", clippy="success", test="success")
        self.assertEqual([], CI.check_results(jobs, "pull_request", "refs/pull/1/merge"))
        for name in ("editors", "test"):
            with self.subTest(name=name):
                altered = {**jobs, name: {"result": "skipped"}}
                self.assertTrue(CI.check_results(altered, "pull_request", "refs/pull/1/merge"))

    def test_editor_filter_and_cli_are_connected_to_required_summary(self):
        root = Path(__file__).resolve().parents[2]
        workflow = (root / ".github/workflows/ci.yml").read_text()
        filters = workflow.split("            editors:\n", 1)[1].split("            frontend:\n", 1)[0]
        self.assertIn("- 'editors/**'", filters)
        job = workflow.split("  editors:\n", 1)[1].split("  fmt:\n", 1)[0]
        self.assertIn("needs.changes.outputs.editors == 'true'", job)
        self.assertIn("npm ci --prefix editors/tree-sitter", job)
        self.assertIn("git diff --exit-code -- editors/tree-sitter/src", job)
        self.assertIn("npm test --prefix editors/tree-sitter", job)
        package = (root / "editors/tree-sitter/package.json").read_text()
        self.assertIn('"tree-sitter-cli": "0.26.12"', package)
        self.assertIn("tree-sitter test && python3 test/test_contract.py -v", package)

    def test_feature_push_preserves_existing_cost_policy(self):
        jobs = needs("false", "true", fmt="success")
        self.assertEqual([], CI.check_results(jobs, "push", "refs/heads/feature/test"))

    def test_failed_or_missing_change_detection_cannot_pass(self):
        for result in ("failure", "cancelled", "skipped", None):
            self.assertTrue(CI.check_results(needs(changes=result), "pull_request", "refs/pull/1/merge"))
        self.assertTrue(CI.check_results({}, "pull_request", "refs/pull/1/merge"))

    def test_workflow_filters_all_guide_litmus_inputs_and_gate_scripts(self):
        root = Path(__file__).resolve().parents[2]
        workflow = (root / ".github/workflows/ci.yml").read_text()
        filters = workflow.split("            rust:\n", 1)[1].split("            frontend:\n", 1)[0]
        source = root / "crates/lang-frontend/tests/guide_litmus.rs"
        inputs = re.findall(r'include_str!\("([^"]+)"\)', source.read_text())
        self.assertTrue(inputs, "Litmus input discovery must not silently match zero files")
        for name in inputs:
            path = (source.parent / name).resolve().relative_to(root).as_posix()
            self.assertIn(f"- '{path}'", filters)
        for path in ("scripts/check_stage_integration.sh", "scripts/check_guide_litmus.sh",
                     "scripts/install_ci_llvm.sh", "scripts/check_ci_results.py",
                     "scripts/tests/test_check_ci_results.py", ".github/actions/setup-llvm/**"):
            self.assertIn(f"- '{path}'", filters)

    def test_workflow_preserves_both_matrices_and_required_summary(self):
        root = Path(__file__).resolve().parents[2]
        workflow = (root / ".github/workflows/ci.yml").read_text()
        self.assertEqual(2, workflow.count("os: [macos-14, ubuntu-24.04]"))
        self.assertEqual(2, workflow.count("fail-fast: false"))
        self.assertIn("needs: [changes, rust-size, dependencies, docs, editors, fmt, clippy, test]", workflow)
        self.assertIn("bash scripts/check_integration.sh", workflow)
        core = (root / "scripts/check_core.sh").read_text()
        for package in ("lang-codegen", "lang-cli", "lang-lsp", "lang-std"):
            self.assertIn(f"cargo test --locked -p {package}", core)

    def test_ownership_iteration_runs_unfiltered_in_existing_test_matrix(self):
        root = Path(__file__).resolve().parents[2]
        workflow = (root / ".github/workflows/ci.yml").read_text()
        job = workflow.split("  test:\n", 1)[1].split("  ci-passed:\n", 1)[0]
        self.assertIn("os: [macos-14, ubuntu-24.04]", job)
        self.assertIn("run: bash scripts/check_integration.sh", job)
        composition = (root / "scripts/check_integration.sh").read_text()
        self.assertEqual(1, composition.count(
            "cargo test --locked -p lang-frontend --test ownership_iteration\n"))

    def test_ownership_iteration_does_not_expand_frontend_to_all_integrations(self):
        root = Path(__file__).resolve().parents[2]
        for path in ("check_core.sh", "check_integration.sh", "check_stage_integration.sh"):
            commands = (root / "scripts" / path).read_text()
            self.assertNotIn("--workspace", commands)
            self.assertNotIn("-p lang-frontend --tests", commands)

    def test_owned_unit_view_contracts_run_in_stage_integration(self):
        root = Path(__file__).resolve().parents[2]
        stage = (root / "scripts/check_stage_integration.sh").read_text()
        commands = stage.replace("\\\n", " ").splitlines()
        for target in ("owned_compilation_unit_view", "owned_unit_view_compile_contracts"):
            selected = [line for line in commands if f"--test {target}" in line]
            self.assertEqual(1, len(selected), f"{target} must execute exactly once")
            self.assertTrue(selected[0].startswith(
                "run cargo test --locked -p lang-frontend --no-fail-fast "))
            self.assertNotIn(" -- ", selected[0], "contract targets must not be filtered")
            self.assertTrue((root / f"crates/lang-frontend/tests/{target}.rs").is_file())

    def test_const_owned_unit_contracts_run_in_stage_integration(self):
        root = Path(__file__).resolve().parents[2]
        stage = (root / "scripts/check_stage_integration.sh").read_text()
        for target in ("const_owned_compilation_unit_view", "const_owned_unit_view_compile_contracts",
                       "multifile_constant_ownership"):
            selected = stage_target_occurrences(stage, target)
            self.assertEqual(1, len(selected), f"{target} must execute exactly once")
            self.assertTrue(selected[0].startswith(
                "run cargo test --locked -p lang-frontend --no-fail-fast "))
            self.assertNotIn(" -- ", selected[0], "contract targets must not be filtered")
            self.assertTrue((root / f"crates/lang-frontend/tests/{target}.rs").is_file())

    def test_stage_target_count_detects_same_command_duplicates_and_exact_tokens(self):
        target = "const_owned_compilation_unit_view"
        command = f"run cargo test --locked -p lang-frontend --no-fail-fast --test {target}"
        self.assertEqual(1, len(stage_target_occurrences(command, target)))
        self.assertEqual(2, len(stage_target_occurrences(
            f"{command} --test {target}", target)))
        self.assertEqual(2, len(stage_target_occurrences(f"{command}\n{command}", target)))
        self.assertEqual([], stage_target_occurrences(f"{command}_extra", target))
        self.assertEqual([], stage_target_occurrences(f"# {command}", target))
        self.assertEqual([], stage_target_occurrences("run cargo test --locked", target))

    def test_unit_name_snapshot_contracts_run_in_stage_integration(self):
        root = Path(__file__).resolve().parents[2]
        stage = (root / "scripts/check_stage_integration.sh").read_text()
        commands = stage.replace("\\\n", " ").splitlines()
        for target in ("unit_name_snapshot", "unit_name_snapshot_compile_contracts"):
            selected = [line for line in commands
                        if re.search(rf"--test {target}(?:\s|$)", line)]
            self.assertEqual(1, len(selected), f"{target} must execute exactly once")
            self.assertTrue(selected[0].startswith(
                "run cargo test --locked -p lang-frontend --no-fail-fast "))
            self.assertNotIn(" -- ", selected[0], "contract targets must not be filtered")
            self.assertTrue((root / f"crates/lang-frontend/tests/{target}.rs").is_file())

    def test_basic_unit_ownership_contracts_run_in_stage_integration(self):
        root = Path(__file__).resolve().parents[2]
        stage = (root / "scripts/check_stage_integration.sh").read_text()
        commands = stage.replace("\\\n", " ").splitlines()
        for target in ("basic_unit_ownership", "basic_unit_ownership_compile_contracts"):
            selected = [line for line in commands
                        if re.search(rf"--test {target}(?:\s|$)", line)]
            self.assertEqual(1, len(selected), f"{target} must execute exactly once")
            self.assertTrue(selected[0].startswith(
                "run cargo test --locked -p lang-frontend --no-fail-fast "))
            self.assertNotIn(" -- ", selected[0], "contract targets must not be filtered")
            self.assertTrue((root / f"crates/lang-frontend/tests/{target}.rs").is_file())

    def test_single_file_analysis_contracts_run_in_stage_integration(self):
        root = Path(__file__).resolve().parents[2]
        stage = (root / "scripts/check_stage_integration.sh").read_text()
        commands = stage.replace("\\\n", " ").splitlines()
        for target in ("single_file_analysis", "single_file_analysis_compile_contracts"):
            selected = [line for line in commands
                        if re.search(rf"--test {target}(?:\s|$)", line)]
            self.assertEqual(1, len(selected), f"{target} must execute exactly once")
            self.assertTrue(selected[0].startswith(
                "run cargo test --locked -p lang-frontend --no-fail-fast "))
            self.assertNotIn(" -- ", selected[0], "contract targets must not be filtered")
            self.assertTrue((root / f"crates/lang-frontend/tests/{target}.rs").is_file())

    def test_rust_size_guard_is_always_required(self):
        for event, ref in (("pull_request", "refs/pull/1/merge"),
                           ("push", "refs/heads/feature/test"),
                           ("push", "refs/heads/main"),
                           ("workflow_dispatch", "refs/heads/fix/test")):
            for result in ("skipped", "failure", "cancelled", None):
                jobs = needs("true", "true", docs="success", fmt="success",
                             clippy="success", test="success", **{"rust-size": result})
                with self.subTest(event=event, ref=ref, result=result):
                    self.assertTrue(CI.check_results(jobs, event, ref))

    def test_workflow_runs_size_guard_without_path_filter(self):
        root = Path(__file__).resolve().parents[2]
        workflow = (root / ".github/workflows/ci.yml").read_text()
        job = workflow.split("  rust-size:\n", 1)[1].split("  docs:\n", 1)[0]
        self.assertNotIn("    if:", job)
        self.assertIn("fetch-depth: 0", job)
        self.assertIn("github.event.pull_request.head.sha || github.sha", job)
        self.assertIn("github.event.pull_request.base.sha", job)
        self.assertIn("github.event.before", job)
        self.assertIn('python3 scripts/check_rust_sizes.py --base "$base"', job)
        self.assertIn("-p test_check_rust_sizes.py", job)

    def test_invalid_or_missing_filter_outputs_cannot_pass(self):
        for value in ("", "unknown", None):
            self.assertTrue(CI.check_results(needs(rust_changed=value), "pull_request", "refs/pull/1/merge"))


if __name__ == "__main__":
    unittest.main()
