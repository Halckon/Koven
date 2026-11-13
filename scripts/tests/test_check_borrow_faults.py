"""Exercise gate parsing and fake subprocesses only; never run Rust fault tests."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "borrow_fault_gate", Path(__file__).resolve().parents[1] / "check_borrow_faults.py")
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)

FE = "ownership_checking::loop_no_carry::tests::fault"
CG = "ssa::refine::fresh_loop_receipt::ci_tests::fault"
MANIFEST = {"lang-frontend": [FE], "lang-codegen": [CG]}
ENV = {"CI": "true", "GITHUB_ACTIONS": "true"}


def listing(names):
    return "\n".join(f"{name}: test" for name in names) + f"\n\n{len(names)} tests, 0 benchmarks\n"


def passed(name):
    return (f"\nrunning 1 test\ntest {name} ... ok\n\n"
            "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 35 filtered out; finished in 0.01s\n")


class FakeRunner:
    def __init__(self, overrides=None):
        self.calls = []
        self.overrides = overrides or {}

    def __call__(self, args, **kwargs):
        self.calls.append(args)
        package = args[args.index("-p") + 1]
        name = MANIFEST[package][0]
        output = (listing([name] + (["llvm::debug_tests::debugserver"] if package == "lang-codegen" else []))
                  if "--list" in args else passed(name))
        output = self.overrides.get(len(self.calls), output)
        return subprocess.CompletedProcess(args, 0, output, "compiler output")


class BorrowFaultGateTests(unittest.TestCase):
    def test_legacy_scalar_entry_is_in_full_frontend_inventory(self):
        manifest = GATE.load_manifest(GATE.ROOT / "scripts/borrow_fault_tests.json")
        target = ("ownership_checking::compilation_unit::dataflow::drop_planner::"
                  "owner_transport::loops::counter_segment::entry::tests::"
                  "ci_only_bridge_scalar_entry_requires_actual_ordered_completion")
        self.assertIn(target, manifest["lang-frontend"])

    def test_legacy_scalar_workflow_does_not_execute_twice(self):
        workflow = (GATE.ROOT / ".github/workflows/ci.yml").read_text()
        commands = [line.strip() for line in workflow.splitlines()
                    if "run: bash scripts/check_bridge_scalar_entry_ci.sh" in line]
        self.assertEqual(commands, [
            "run: bash scripts/check_bridge_scalar_entry_ci.sh --list-only"])
        self.assertIn("run: python3 scripts/check_borrow_faults.py", workflow)

    def test_local_execution_refused_before_any_subprocess(self):
        for env in ({}, {"CI": "true"}, {"GITHUB_ACTIONS": "true"},
                    {"CI": "True", "GITHUB_ACTIONS": "true"}):
            fake = FakeRunner()
            with self.assertRaises(ValueError):
                GATE.run(MANIFEST, env=env, runner=fake)
            self.assertEqual(fake.calls, [])

    def test_list_only_never_executes_faults(self):
        fake = FakeRunner()
        self.assertEqual(2, GATE.run(MANIFEST, list_only=True, env={}, runner=fake))
        self.assertEqual(2, len(fake.calls))
        self.assertTrue(all(args[-3:] == ["--", "--list", "--ignored"] for args in fake.calls))
        self.assertIn("loan-observation-test-support", fake.calls[1])
        self.assertNotIn("--features", fake.calls[0])

    def test_ci_executes_each_exact_fqn_after_all_discovery(self):
        fake = FakeRunner()
        self.assertEqual(2, GATE.run(MANIFEST, env=ENV, runner=fake))
        self.assertEqual(4, len(fake.calls))
        for args, name in zip(fake.calls[2:], (FE, CG)):
            self.assertEqual(["--", name, "--exact", "--ignored", "--format", "pretty"], args[-6:])
            self.assertNotIn("debugserver", " ".join(args))

    def test_each_success_logs_exact_execution_with_flush(self):
        with patch("builtins.print") as output:
            GATE.run(MANIFEST, env=ENV, runner=FakeRunner())
        self.assertEqual(2, output.call_count)
        output.assert_any_call(f"lang-frontend/{FE} executed=1 passed=1 ignored=0", flush=True)
        output.assert_any_call(f"lang-codegen/{CG} executed=1 passed=1 ignored=0", flush=True)
        with patch("builtins.print") as output:
            with self.assertRaises(ValueError):
                GATE.run(MANIFEST, env=ENV, runner=FakeRunner({3: passed(FE).replace("1 passed", "0 passed")}))
        output.assert_not_called()

    def test_scope_does_not_hide_debugserver_named_fault(self):
        name = CG + "_debugserver"
        self.assertTrue(GATE.in_scope("lang-codegen", name))
        fake = FakeRunner({2: listing([CG, name, "llvm::debug_tests::debugserver"])})
        with self.assertRaisesRegex(ValueError, "unlisted"):
            GATE.run(MANIFEST, env=ENV, runner=fake)
        self.assertEqual(2, len(fake.calls))

    def test_missing_extra_empty_discovery_fail_before_execution(self):
        for names in ([], [FE, FE + "_new"], [FE + "_renamed"]):
            fake = FakeRunner({1: listing(names)})
            with self.assertRaises(ValueError):
                GATE.run(MANIFEST, env=ENV, runner=fake)
            self.assertEqual(1, len(fake.calls))
        fake = FakeRunner({2: listing([CG, CG + "_new"])})
        with self.assertRaises(ValueError):
            GATE.run(MANIFEST, env=ENV, runner=fake)
        self.assertEqual(2, len(fake.calls))

    def test_malformed_listing_and_duplicate_rejected(self):
        for output in ("", listing([FE]).replace("1 tests", "2 tests"),
                       listing([FE, FE]), listing([FE]) + listing([CG]),
                       listing([FE]).replace("0 benchmarks", "1 benchmarks")):
            with self.assertRaises(ValueError):
                GATE.parse_list(output)

    def test_execution_zero_ignored_wrong_name_multi_and_failed_rejected(self):
        good = passed(FE)
        for bad in (good.replace("running 1 test", "running 0 tests"),
                    good.replace("1 passed", "0 passed"), good.replace("0 ignored", "1 ignored"),
                    good.replace(FE, CG), good + good, good.replace("0 failed", "1 failed"),
                    good.replace("result: ok", "result: FAILED"), good.replace("... ok", "... ignored")):
            fake = FakeRunner({3: bad})
            with self.assertRaises(ValueError):
                GATE.run(MANIFEST, env=ENV, runner=fake)
            self.assertEqual(3, len(fake.calls))

    def test_nonzero_process_fails_even_with_passing_stdout(self):
        def fail(args, **kwargs):
            return subprocess.CompletedProcess(args, 1, listing([FE]), "compile error")
        with self.assertRaisesRegex(ValueError, "command failed"):
            GATE.run(MANIFEST, list_only=True, runner=fail)

    def test_manifest_strict_scope_packages_duplicates_and_nonzero(self):
        invalid = [{}, {"lang-frontend": [FE]}, {**MANIFEST, "lang-codegen": []},
                   {**MANIFEST, "lang-codegen": [CG, CG]},
                   {**MANIFEST, "lang-codegen": ["llvm::debug_tests::debugserver"]},
                   {**MANIFEST, "lang-codegen": ["--ignored"]},
                   {**MANIFEST, "lang-codegen": [12]}]
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "manifest.json"
            path.write_text(json.dumps(MANIFEST))
            self.assertEqual(MANIFEST, GATE.load_manifest(path))
            for data in invalid:
                path.write_text(json.dumps(data))
                with self.assertRaises(ValueError):
                    GATE.load_manifest(path)
            path.write_text('{"lang-frontend": [], "lang-frontend": []}')
            with self.assertRaisesRegex(ValueError, "duplicate manifest package"):
                GATE.load_manifest(path)


if __name__ == "__main__":
    unittest.main()
