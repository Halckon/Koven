"""Recovery contracts: real shell composition and declared dependency mutations."""

import copy
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("dependencies", ROOT / "scripts/check_workspace_dependencies.py")
DEPS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DEPS)


class DependencyContracts(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.packages = []
        for name in sorted(DEPS.MEMBERS):
            path = Path(self.directory.name) / name
            path.mkdir()
            (path / "Cargo.toml").write_text("[package]\n")
            self.packages.append(dict(id=name, name=name, manifest_path=str(path / "Cargo.toml"), dependencies=[]))
        self.metadata = dict(workspace_members=[p["id"] for p in self.packages], packages=self.packages)
        for source, target in sorted(DEPS.EDGES):
            self.package(source)["dependencies"].append(dict(name=target, path=str(Path(self.directory.name) / target), kind=None, target=None, optional=False, rename=None))

    def package(self, name):
        return next(p for p in self.packages if p["name"] == name)

    def test_valid_and_all_declaration_kinds_and_aliases(self):
        for kind in (None, "dev", "build"):
            for target in (None, 'cfg(target_os = "macos")'):
                with self.subTest(kind=kind, target=target):
                    dep = self.package("lang-codegen")["dependencies"][0]
                    dep.update(kind=kind, target=target, optional=True, rename="alias")
                    self.assertEqual([], DEPS.check_metadata(self.metadata))

    def test_duplicate_reverse_nonmember_and_invalid_paths(self):
        baseline = copy.deepcopy(self.metadata)
        original = self.package("lang-codegen")["dependencies"][0]
        self.package("lang-codegen")["dependencies"].append(copy.deepcopy(original))
        self.assertTrue(DEPS.check_metadata(self.metadata))
        for name, path in (("lang-cli", "lang-cli"), ("external", "."), ("missing", "absent"), ("bad", "\0")):
            metadata = copy.deepcopy(baseline)
            metadata["packages"][0]["dependencies"].append(dict(name=name, path=str(Path(self.directory.name) / path)))
            with self.subTest(path=path):
                self.assertTrue(DEPS.check_metadata(metadata))

    def test_members_and_required_edges_cannot_disappear(self):
        self.metadata["workspace_members"].pop()
        self.assertTrue(DEPS.check_metadata(self.metadata))
        self.metadata["workspace_members"] = [p["id"] for p in self.packages]
        self.package("lang-cli")["dependencies"].pop()
        self.assertTrue(DEPS.check_metadata(self.metadata))

    def test_workflow_actual_job_is_unconditional_and_required(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        match = re.search(r"^  dependencies:\n(.*?)(?=^  [a-z-]+:|\Z)", workflow, re.M | re.S)
        self.assertIsNotNone(match)
        body = "\n".join(line for line in match[1].splitlines() if not line.lstrip().startswith("#"))
        self.assertNotRegex(body, r"\bif:")
        self.assertIn("run: python3 scripts/check_workspace_dependencies.py", body)
        self.assertIn("needs: [changes, rust-size, dependencies, docs, editors, fmt, clippy, test]", workflow)
        self.assertIn("- 'scripts/rust_test_artifact.rs'", workflow)


class CompositionContracts(unittest.TestCase):
    def invoke(self, fail=""):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            log = path / "calls"
            stub = '#!/bin/sh\nprintf "%s %s\\n" "$(basename "$0")" "$*" >> "$CALL_LOG"\ncase "$*" in *"$FAIL_MATCH"*) [ -z "$FAIL_MATCH" ] || exit 37;; esac\n'
            for tool in ("cargo", "python3"):
                executable = path / tool
                executable.write_text(stub)
                executable.chmod(0o755)
            result = subprocess.run(["bash", "scripts/check_integration.sh"], cwd=ROOT,
                                    env={**os.environ, "PATH": f"{path}:{os.environ['PATH']}", "CALL_LOG": str(log), "FAIL_MATCH": fail}, capture_output=True)
            return result.returncode, log.read_text().splitlines()

    def test_selection_is_unique_and_bounded(self):
        status, calls = self.invoke()
        self.assertEqual(0, status)
        cargo = [line for line in calls if line.startswith("cargo ")]
        targets = [target for line in cargo for target in re.findall(r"--test ([a-z0-9_]+)", line)]
        self.assertEqual(len(targets), len(set(targets)))
        self.assertEqual(77, len(targets))
        self.assertEqual(10, len(cargo))
        self.assertTrue(calls[-1].endswith("scripts/check_tutorial.py"))

    def test_each_phase_propagates_failure_and_stops_following_phases(self):
        for fail in ("--lib", "lang-codegen", "ownership_iteration", "numeric_literals", "parser_class_family", "parser_prefix_truncation_matrix", "guide_litmus", "check_tutorial"):
            with self.subTest(fail=fail):
                status, calls = self.invoke(fail)
                self.assertEqual(37, status)
                if fail != "check_tutorial":
                    self.assertFalse(any("check_tutorial.py" in line for line in calls))


if __name__ == "__main__":
    unittest.main()
