"""Contracts for SPEC-0495 reversible E2 two-source Unit shadow pilot and A/B harness."""

from __future__ import annotations

import copy
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
MANIFEST_PATH = ROOT / "scripts/e2_two_source_unit_manifest.json"
REPLAY_MANIFEST_PATH = ROOT / "scripts/e2_replay_manifest.json"
PILOT_PATH = ROOT / "scripts/e2_two_source_shadow_pilot.py"
SHADOW_WORKFLOW_PATH = ROOT / ".github/workflows/ci-e2-shadow-pilot.yml"


def load_pilot_module():
    spec = importlib.util.spec_from_file_location("e2_two_source_shadow_pilot", PILOT_PATH)
    if spec is None or spec.loader is None:
        raise ImportError(f"cannot load {PILOT_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class ManifestAndParityContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.pilot = load_pilot_module()
        cls.manifest = cls.pilot.load_manifest(MANIFEST_PATH)
        env = dict(os.environ)
        env.pop("CI", None)
        env.pop("GITHUB_ACTIONS", None)
        cls.captured_calls = cls.pilot.capture_shell_script_list_calls(
            ROOT / cls.manifest["source_script"], ROOT, env=env
        )

    def test_manifest_has_exact_5_configs_and_179_entries(self):
        self.assertEqual(self.manifest["schema"], "koven.e2-two-source-shadow-manifest.v1")
        self.assertEqual(self.manifest["cargo_jobs"], 1)
        configs = self.manifest["configs"]
        self.assertEqual(len(configs), 5)
        self.assertEqual([len(c["entries"]) for c in configs], [81, 7, 81, 7, 3])
        self.assertEqual(sum(len(c["entries"]) for c in configs), 179)
        ignored = [
            (c["config_id"], e["test_name"])
            for c in configs
            for e in c["entries"]
            if e["ignored"]
        ]
        self.assertEqual(len(ignored), 4)
        for _, name in ignored:
            self.assertTrue(name.endswith("_commit_failure_is_atomic"), name)

    def test_manifest_matches_real_shell_script_without_faking_ci(self):
        report = self.pilot.verify_manifest_against_shell_script(
            self.manifest, ROOT, observed_calls=self.captured_calls
        )
        self.assertEqual(report["verified_entries"], 179)
        self.assertEqual(report["verified_configs"], 5)

    def test_manifest_drift_is_rejected_with_config_and_test_identity(self):
        def mutate_features(m):
            m["configs"][2]["cargo_features_arg"] = "two-source-unit"
            m["configs"][2]["expected_features"] = ["two-source-unit"]

        def drop_entry(m):
            m["configs"][4]["entries"].pop()
            m["total_entries"] -= 1

        for label, mutate in [
            ("rename_test", lambda m: m["configs"][0]["entries"][3].update(test_name="ssa::wrong::name")),
            ("flip_ignored", lambda m: m["configs"][0]["entries"][0].update(ignored=True)),
            ("change_features", mutate_features),
            ("drop_entry", drop_entry),
        ]:
            with self.subTest(case=label):
                broken = copy.deepcopy(self.manifest)
                mutate(broken)
                with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                    self.pilot.verify_manifest_against_shell_script(
                        broken, ROOT, observed_calls=self.captured_calls
                    )
                self.assertEqual(ctx.exception.kind, "manifest_drift")
                self.assertTrue(ctx.exception.config_id or ctx.exception.detail)


class CiGateAndSecretSafetyContracts(unittest.TestCase):
    def setUp(self):
        self.pilot = load_pilot_module()
        self.manifest = self.pilot.load_manifest(MANIFEST_PATH)

    def test_non_list_only_refuses_outside_real_github_actions(self):
        with tempfile.TemporaryDirectory() as tmp:
            for env in [
                {},
                {"CI": "true"},
                {"GITHUB_ACTIONS": "true"},
                {"CI": "false", "GITHUB_ACTIONS": "true"},
            ]:
                with self.subTest(env=env):
                    with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                        self.pilot.run_pilot(
                            manifest=self.manifest,
                            root=ROOT,
                            mode="candidate",
                            list_only=False,
                            artifacts_dir=Path(tmp) / f"out-{len(env)}",
                            env=env,
                        )
                    self.assertEqual(ctx.exception.kind, "ci_required")

    def test_probe_snapshot_never_records_secrets_and_detects_env_mismatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            fake_bin = tmp_path / "fake-test-bin"
            fake_bin.write_text("#!/bin/sh\necho 't: test'\necho ''\necho '1 test, 0 benchmarks'\n")
            fake_bin.chmod(0o755)
            fake_clang = tmp_path / "clang"
            fake_clang.write_text("#!/bin/sh\necho 'clang version 21.1.8'\n")
            fake_clang.chmod(0o755)
            fake_llvm_cfg = tmp_path / "llvm-config"
            fake_llvm_cfg.write_text("#!/bin/sh\necho '21.1.8'\n")
            fake_llvm_cfg.chmod(0o755)

            dirty_env = {
                "PATH": f"{tmp}:{os.environ.get('PATH', '')}",
                "CARGO_MANIFEST_DIR": str(ROOT / "crates/lang-codegen"),
                "CARGO_PKG_NAME": "lang-codegen",
                "CARGO_TARGET_DIR": str(tmp_path / "target"),
                "LLVM_SYS_211_PREFIX": str(tmp_path),
                "LD_LIBRARY_PATH": f"{tmp_path}/deps:/opt/llvm/lib",
                "GH_TOKEN": "ghp_TOP_SECRET_VALUE_999",
                "GITHUB_TOKEN": "ghs_ANOTHER_SECRET_888",
                "AWS_SECRET_ACCESS_KEY": "AKIA_SUPER_SECRET_777",
                "CUSTOM_API_SECRET": "DONT_PRINT_ME_666",
            }
            snap = self.pilot.build_probe_snapshot(
                executable=fake_bin,
                cwd=ROOT / "crates/lang-codegen",
                env=dirty_env,
                inspect_dylibs=False,
            )
            serialized = json.dumps(snap)
            for forbidden in (
                "GH_TOKEN",
                "GITHUB_TOKEN",
                "AWS_SECRET_ACCESS_KEY",
                "CUSTOM_API_SECRET",
                "ghp_TOP_SECRET_VALUE_999",
                "ghs_ANOTHER_SECRET_888",
                "AKIA_SUPER_SECRET_777",
                "DONT_PRINT_ME_666",
            ):
                self.assertNotIn(forbidden, serialized)

            # Equal snapshots pass
            self.pilot.verify_probe_equivalence(
                snap, copy.deepcopy(snap), host="Linux", config_id="cfg1"
            )

            # Any difference in cwd, allowlisted Cargo env, dylib search path, or clang fails closed
            for field_case, mutator in [
                ("cwd", lambda s: s.update(cwd=str(ROOT))),
                ("cargo_env", lambda s: s["allowlisted_env"].update(CARGO_PKG_NAME="lang-frontend")),
                ("dylib_path", lambda s: s["dylib_search_paths"].update(LD_LIBRARY_PATH=["/wrong"])),
                ("clang_version", lambda s: s["toolchain"].update(llvm_clang_version="clang version 19.0.0")),
            ]:
                with self.subTest(mismatch=field_case):
                    altered = copy.deepcopy(snap)
                    mutator(altered)
                    with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                        self.pilot.verify_probe_equivalence(
                            snap, altered, host="Linux", config_id="cfg1"
                        )
                    self.assertEqual(ctx.exception.kind, "environment_mismatch")
                    self.assertEqual(ctx.exception.config_id, "cfg1")


class ArtifactLockContracts(unittest.TestCase):
    def setUp(self):
        self.pilot = load_pilot_module()

    def test_unique_artifact_lock_rejects_zero_multiple_wrong_feature_or_non_test(self):
        with tempfile.TemporaryDirectory() as tmp:
            exe = Path(tmp) / "lang_codegen-abc123"
            # Write native Mach-O 64-bit magic or ELF magic matching current platform
            magic = b"\xcf\xfa\xed\xfe" if sys.platform == "darwin" else b"\x7fELF"
            exe.write_bytes(magic + b"\x00" * 64)
            exe.chmod(0o755)
            host_triple = "aarch64-apple-darwin" if sys.platform == "darwin" else "x86_64-unknown-linux-gnu"
            pkg_id = "path+file:///workspace/crates/lang-codegen#0.1.0"
            manifest_path = str((ROOT / "crates/lang-codegen/Cargo.toml").resolve())

            valid_artifact = {
                "reason": "compiler-artifact",
                "package_id": pkg_id,
                "manifest_path": manifest_path,
                "target": {"name": "lang_codegen", "kind": ["lib"]},
                "profile": {"test": True, "debuginfo": 1, "opt_level": "0"},
                "features": ["two-source-unit"],
                "executable": str(exe),
            }
            finished_ok = {"reason": "build-finished", "success": True}

            spec = {
                "config_id": "cfg1",
                "host_triple": host_triple,
                "package_id": pkg_id,
                "manifest_path": manifest_path,
                "target_name": "lang_codegen",
                "target_kind": ["lib"],
                "expected_features": ["two-source-unit"],
                "expected_debuginfo": 1,
            }

            locked = self.pilot.select_unique_test_artifact(
                [json.dumps(valid_artifact), json.dumps(finished_ok)], spec
            )
            self.assertEqual(Path(locked["executable"]), exe.resolve())

            # Failure cases
            bad_cases = [
                ("zero_match", [finished_ok]),
                ("duplicate_match", [valid_artifact, valid_artifact, finished_ok]),
                ("null_executable", [{**valid_artifact, "executable": None}, finished_ok]),
                ("wrong_features", [{**valid_artifact, "features": ["two-source-unit", "loan-observation-test-support"]}, finished_ok]),
                ("non_test_profile", [{**valid_artifact, "profile": {"test": False, "debuginfo": 1, "opt_level": "0"}}, finished_ok]),
                ("wrong_debuginfo", [{**valid_artifact, "profile": {"test": True, "debuginfo": 2, "opt_level": "0"}}, finished_ok]),
                ("wrong_package_id", [{**valid_artifact, "package_id": "path+file:///other#0.1.0"}, finished_ok]),
                ("build_failed", [valid_artifact, {"reason": "build-finished", "success": False}]),
            ]
            for name, records in bad_cases:
                with self.subTest(case=name):
                    lines = [json.dumps(r) for r in records]
                    with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                        self.pilot.select_unique_test_artifact(lines, spec)
                    self.assertEqual(ctx.exception.kind, "artifact_lock_failed")


class AntiSkipExpectedFailureAndComparisonContracts(unittest.TestCase):
    def setUp(self):
        self.pilot = load_pilot_module()

    def _make_synthetic_harness(self, tmp_path: Path, fault_mode: str = "none"):
        """Create fake rustc/cargo/llvm tools + test binaries to exercise full baseline & candidate flows."""
        bin_dir = tmp_path / "bin"
        bin_dir.mkdir()
        target_a = tmp_path / "target-a"
        target_b = tmp_path / "target-b"
        call_log = tmp_path / "cargo-calls.jsonl"

        host_triple = "aarch64-apple-darwin" if sys.platform == "darwin" else "x86_64-unknown-linux-gnu"

        # Fake test binary script (also has valid shebang; arch check allows #! scripts in unit tests only when configured or we emit python/sh)
        test_exe = tmp_path / "compiled_test_harness"
        test_exe.write_text(
            "#!/usr/bin/env python3\n"
            "import os, sys\n"
            "args = sys.argv[1:]\n"
            "fault = os.environ.get('FAKE_FAULT_MODE', 'none')\n"
            "# Filter name is first non-flag argument\n"
            "names = [a for a in args if not a.startswith('--')]\n"
            "name = names[0] if names else ''\n"
            "is_list = '--list' in args\n"
            "is_ignored = '--ignored' in args\n"
            "if is_list:\n"
            "    if fault == 'zero_list' and name.endswith('alpha'):\n"
            "        print('0 tests, 0 benchmarks')\n"
            "        sys.exit(0)\n"
            "    if fault == 'multi_list' and name.endswith('alpha'):\n"
            "        print(f'{name}: test\\n{name}_extra: test\\n\\n2 tests, 0 benchmarks')\n"
            "        sys.exit(0)\n"
            "    print(f'{name}: test\\n\\n1 test, 0 benchmarks')\n"
            "    sys.exit(0)\n"
            "# Execution mode\n"
            "if fault == 'exit_zero_no_run' and name.endswith('alpha'):\n"
            "    print('running 0 tests\\n\\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out; finished in 0.00s')\n"
            "    sys.exit(0)\n"
            "if fault == 'ignored_not_run' and name.endswith('atomic'):\n"
            "    print(f'running 1 test\\ntest {name} ... ignored\\n\\ntest result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 4 filtered out; finished in 0.00s')\n"
            "    sys.exit(0)\n"
            "if name.endswith('expected_red'):\n"
            "    if fault == 'expected_fail_passes':\n"
            "        print(f'running 1 test\\ntest {name} ... ok\\n\\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s')\n"
            "        sys.exit(0)\n"
            "    if fault == 'wrong_witness':\n"
            "        print(f'running 1 test\\ntest {name} ... FAILED\\n\\nfailures:\\n    different panic message\\n\\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s')\n"
            "        sys.exit(101)\n"
            "    if fault == 'ab_candidate_pass_divergence' and os.environ.get('PILOT_MODE_TAG') == 'candidate':\n"
            "        print(f'running 1 test\\ntest {name} ... ok\\n\\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s')\n"
            "        sys.exit(0)\n"
            "    print(f'running 1 test\\ntest {name} ... FAILED\\n\\nfailures:\\n    expected sentinel failure E2-999\\n\\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s')\n"
            "    sys.exit(101)\n"
            "assert not name.endswith('atomic') or is_ignored\n"
            "print(f'running 1 test\\ntest {name} ... ok\\n\\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s')\n"
            "sys.exit(0)\n",
            encoding="utf-8",
        )
        test_exe.chmod(0o755)

        rustc_stub = bin_dir / "rustc"
        rustc_stub.write_text(
            f"#!/bin/sh\necho 'rustc 1.96.0'\necho 'release: 1.96.0'\necho 'host: {host_triple}'\n",
            encoding="utf-8",
        )
        rustc_stub.chmod(0o755)

        llvm_cfg = bin_dir / "llvm-config"
        llvm_cfg.write_text("#!/bin/sh\necho '21.1.8'\n", encoding="utf-8")
        llvm_cfg.chmod(0o755)

        clang_stub = bin_dir / "clang"
        clang_stub.write_text("#!/bin/sh\necho 'clang version 21.1.8'\n", encoding="utf-8")
        clang_stub.chmod(0o755)

        pkg_codegen = f"path+file://{(ROOT / 'crates/lang-codegen').resolve()}#0.1.0"
        manifest_codegen = str((ROOT / "crates/lang-codegen/Cargo.toml").resolve())

        cargo_stub = bin_dir / "cargo"
        cargo_stub.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, subprocess, sys\n"
            "argv = sys.argv[1:]\n"
            "with open(os.environ['CALL_LOG'], 'a', encoding='utf-8') as f:\n"
            "    f.write(json.dumps(argv) + '\\n')\n"
            "if argv[0] == 'metadata':\n"
            f"    print(json.dumps({{'packages': [{{'name': 'lang-codegen', 'id': {pkg_codegen!r}, 'manifest_path': {manifest_codegen!r}, 'version': '0.1.0', 'authors': [], 'description': '', 'homepage': None, 'repository': None, 'license': 'MIT OR Apache-2.0', 'license_file': None, 'rust_version': '1.96.0', 'readme': None}}]}}))\n"
            "    sys.exit(0)\n"
            # Drop -j1 assertion in stub when KOVEN_ALLOW_DEFAULT_JOBS=1 so we can test default 4-core candidate mode
            "assert argv[0] == 'test' and (os.environ.get('KOVEN_ALLOW_DEFAULT_JOBS') == '1' or '-j1' in argv), f'missing -j1 in {argv}'\n"
            "feat = sorted(argv[argv.index('--features') + 1].split())\n"
            "target_dir = os.environ['CARGO_TARGET_DIR']\n"
            "deps_dir = target_dir + '/debug/deps'\n"
            "os.makedirs(deps_dir, exist_ok=True)\n"
            "import hashlib as _hl\n"
            "feat_tag = 'shared' if os.environ.get('FAKE_COLLIDE_FEATURE_ARTIFACTS') == '1' else _hl.sha256((' '.join(feat)).encode()).hexdigest()[:12]\n"
            "exe = f'{deps_dir}/compiled_test_harness-{feat_tag}'\n"
            "if not os.path.exists(exe):\n"
            f"    import shutil as _sh; _sh.copyfile({str(test_exe)!r}, exe)\n"
            "    if feat_tag != 'shared':\n"
            "        with open(exe, 'a', encoding='utf-8') as _ef:\n"
            "            _ef.write(f'# features={feat_tag}\\n')\n"
            "    os.chmod(exe, 0o755)\n"
            "if '--no-run' in argv and '--message-format=json' in argv:\n"
            "    rec = {\n"
            "        'reason': 'compiler-artifact',\n"
            f"        'package_id': {pkg_codegen!r},\n"
            f"        'manifest_path': {manifest_codegen!r},\n"
            "        'target': {'name': 'lang_codegen', 'kind': ['lib']},\n"
            "        'profile': {'test': True, 'debuginfo': int(os.environ.get('CARGO_PROFILE_TEST_DEBUG', '2')), 'opt_level': '0'},\n"
            "        'features': sorted(feat),\n"
            "        'executable': exe,\n"
            "    }\n"
            "    print(json.dumps(rec))\n"
            "    print(json.dumps({'reason': 'build-finished', 'success': True}))\n"
            "    sys.exit(0)\n"
            "env = dict(os.environ)\n"
            f"env['CARGO'] = {str(cargo_stub)!r}\n"
            f"env['CARGO_MANIFEST_DIR'] = {str((ROOT / 'crates/lang-codegen').resolve())!r}\n"
            f"env['CARGO_MANIFEST_PATH'] = {manifest_codegen!r}\n"
            "env['CARGO_PKG_NAME'] = 'lang-codegen'\n"
            "env['CARGO_PKG_VERSION'] = '0.1.0'\n"
            "env['CARGO_PKG_VERSION_MAJOR'] = '0'\n"
            "env['CARGO_PKG_VERSION_MINOR'] = '1'\n"
            "env['CARGO_PKG_VERSION_PATCH'] = '0'\n"
            "env['CARGO_PKG_VERSION_PRE'] = ''\n"
            "env['CARGO_PKG_AUTHORS'] = ''\n"
            "env['CARGO_PKG_DESCRIPTION'] = ''\n"
            "env['CARGO_PKG_HOMEPAGE'] = ''\n"
            "env['CARGO_PKG_REPOSITORY'] = ''\n"
            "env['CARGO_PKG_LICENSE'] = 'MIT OR Apache-2.0'\n"
            "env['CARGO_PKG_LICENSE_FILE'] = ''\n"
            "env['CARGO_PKG_RUST_VERSION'] = '1.96.0'\n"
            "env['CARGO_PKG_README'] = ''\n"
            "dylib_key = 'DYLD_FALLBACK_LIBRARY_PATH' if sys.platform == 'darwin' else 'LD_LIBRARY_PATH'\n"
            "existing = env.get(dylib_key, '')\n"
            "injected = f'{target_dir}/debug/deps:{target_dir}/debug'\n"
            "env[dylib_key] = f'{injected}:{existing}' if existing else injected\n"
            "sep = argv.index('--')\n"
            "post = argv[sep + 1:]\n"
            "# Check if --config target.<triple>.runner=... was passed\n"
            "runner = None\n"
            "for i, tok in enumerate(argv):\n"
            "    if tok == '--config' and '.runner=' in argv[i + 1]:\n"
            "        runner = json.loads(argv[i + 1].split('=', 1)[1])\n"
            "filter_tok = argv[sep - 1] if argv[sep - 2] != '--features' and not argv[sep - 1].startswith('-') else None\n"
            "cmd = (runner + [exe]) if runner else [exe]\n"
            "if filter_tok:\n"
            "    cmd.append(filter_tok)\n"
            "cmd.extend(post)\n"
            f"res = subprocess.run(cmd, cwd={str((ROOT / 'crates/lang-codegen').resolve())!r}, env=env)\n"
            "sys.exit(res.returncode)\n",
            encoding="utf-8",
        )
        cargo_stub.chmod(0o755)

        mini_manifest = {
            "schema": "koven.e2-two-source-shadow-manifest.v1",
            "source_script": "scripts/check_e2_two_source_unit_ci.sh",
            "cargo_jobs": 1,
            "total_configs": 1,
            "total_entries": 3,
            "configs": [
                {
                    "config_id": "cfg1-synthetic",
                    "package": "lang-codegen",
                    "target_name": "lang_codegen",
                    "target_kind": ["lib"],
                    "cargo_target_args": ["--lib"],
                    "cargo_features_arg": "two-source-unit",
                    "expected_features": ["two-source-unit"],
                    "entries": [
                        {
                            "global_index": 0,
                            "test_name": "ssa::two_source_unit::tests::alpha",
                            "exact": True,
                            "ignored": False,
                            "ci_only": True,
                            "expected_outcome": "pass",
                            "expected_failure_substrings": [],
                        },
                        {
                            "global_index": 1,
                            "test_name": "ssa::two_source_unit::tests::atomic",
                            "exact": True,
                            "ignored": True,
                            "ci_only": True,
                            "expected_outcome": "pass",
                            "expected_failure_substrings": [],
                        },
                        {
                            "global_index": 2,
                            "test_name": "ssa::two_source_unit::tests::expected_red",
                            "exact": True,
                            "ignored": False,
                            "ci_only": True,
                            "expected_outcome": "expected_failure",
                            "expected_failure_substrings": ["expected sentinel failure E2-999"],
                        },
                    ],
                }
            ],
        }

        base_env = {
            "PATH": f"{bin_dir}:{os.environ.get('PATH', '')}",
            "CI": "true",
            "GITHUB_ACTIONS": "true",
            "LLVM_SYS_211_PREFIX": str(bin_dir),
            "CARGO_PROFILE_DEV_DEBUG": "1",
            "CARGO_PROFILE_TEST_DEBUG": "1",
            "CALL_LOG": str(call_log),
            "FAKE_FAULT_MODE": fault_mode,
        }
        return mini_manifest, base_env, target_a, target_b, call_log

    def test_baseline_and_candidate_match_including_expected_failure_and_preserve_j1(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            manifest, base_env, target_a, target_b, call_log = self._make_synthetic_harness(tmp_path)

            env_a = {**base_env, "CARGO_TARGET_DIR": str(target_a), "PILOT_MODE_TAG": "baseline"}
            ledger_a = self.pilot.run_pilot(
                manifest=manifest,
                root=ROOT,
                mode="baseline",
                list_only=False,
                artifacts_dir=tmp_path / "artifacts-a",
                env=env_a,
                verify_shell_parity=False,
                allow_script_test_binary=True,
                inspect_dylibs=False,
            )

            call_log.write_text("", encoding="utf-8")
            env_b = {**base_env, "CARGO_TARGET_DIR": str(target_b), "PILOT_MODE_TAG": "candidate"}
            ledger_b = self.pilot.run_pilot(
                manifest=manifest,
                root=ROOT,
                mode="candidate",
                list_only=False,
                artifacts_dir=tmp_path / "artifacts-b",
                env=env_b,
                verify_shell_parity=False,
                allow_script_test_binary=True,
                inspect_dylibs=False,
            )

            # Candidate only called cargo metadata + 1 build + 1 runner probe (never 6 cargo test calls!)
            candidate_cargo_calls = [json.loads(l) for l in call_log.read_text().splitlines()]
            test_calls = [c for c in candidate_cargo_calls if c[0] == "test"]
            self.assertEqual(len(test_calls), 2)  # 1 --no-run build + 1 runner env probe
            for c in test_calls:
                self.assertIn("-j1", c)

            comparison = self.pilot.compare_ledgers(ledger_a, ledger_b)
            self.assertTrue(comparison["equivalent"])
            self.assertEqual(comparison["compared_entries"], 3)
            self.assertEqual(comparison["classifications"]["pass"], 2)
            self.assertEqual(comparison["classifications"]["expected_failure_verified"], 1)

            # Mutating candidate classification or exit code must fail A/B comparison with exact test identity
            diverged = copy.deepcopy(ledger_b)
            diverged["entries"][2]["exit_code"] = 0
            diverged["entries"][2]["classification"] = "pass"
            with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                self.pilot.compare_ledgers(ledger_a, diverged)
            self.assertEqual(ctx.exception.kind, "ab_mismatch")
            self.assertEqual(ctx.exception.config_id, "cfg1-synthetic")
            self.assertEqual(ctx.exception.test_name, "ssa::two_source_unit::tests::expected_red")

    def test_candidate_default_jobs_omits_j1_while_baseline_keeps_j1(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            manifest, base_env, target_a, target_b, call_log = self._make_synthetic_harness(tmp_path)
            base_env["KOVEN_ALLOW_DEFAULT_JOBS"] = "1"

            env_a = {**base_env, "CARGO_TARGET_DIR": str(target_a), "PILOT_MODE_TAG": "baseline"}
            ledger_a = self.pilot.run_pilot(
                manifest=manifest,
                root=ROOT,
                mode="baseline",
                list_only=False,
                artifacts_dir=tmp_path / "artifacts-a",
                env=env_a,
                verify_shell_parity=False,
                allow_script_test_binary=True,
                inspect_dylibs=False,
                candidate_jobs="default",
            )
            baseline_calls = [json.loads(l) for l in call_log.read_text().splitlines() if json.loads(l)[0] == "test"]
            self.assertTrue(all("-j1" in c for c in baseline_calls))
            self.assertEqual(ledger_a["build_flags"]["cargo_jobs"], "-j1")

            call_log.write_text("", encoding="utf-8")
            env_b = {**base_env, "CARGO_TARGET_DIR": str(target_b), "PILOT_MODE_TAG": "candidate"}
            ledger_b = self.pilot.run_pilot(
                manifest=manifest,
                root=ROOT,
                mode="candidate",
                list_only=False,
                artifacts_dir=tmp_path / "artifacts-b",
                env=env_b,
                verify_shell_parity=False,
                allow_script_test_binary=True,
                inspect_dylibs=False,
                candidate_jobs="default",
            )
            candidate_calls = [json.loads(l) for l in call_log.read_text().splitlines() if json.loads(l)[0] == "test"]
            self.assertEqual(len(candidate_calls), 2)
            self.assertTrue(all("-j1" not in c for c in candidate_calls))
            self.assertEqual(ledger_b["build_flags"]["cargo_jobs"], "default")
            self.assertGreater(ledger_b["host_resources"]["cpu_count"], 0)
            self.assertGreater(ledger_b["host_resources"]["total_ram_bytes"], 0)

            comp = self.pilot.compare_ledgers(ledger_a, ledger_b)
            self.assertTrue(comp["equivalent"])

    def test_anti_skip_and_expected_failure_faults_fail_closed_with_exact_identity(self):
        cases = [
            ("zero_list", "list_mismatch", "ssa::two_source_unit::tests::alpha"),
            ("multi_list", "list_mismatch", "ssa::two_source_unit::tests::alpha"),
            ("exit_zero_no_run", "missed_execution", "ssa::two_source_unit::tests::alpha"),
            ("ignored_not_run", "missed_execution", "ssa::two_source_unit::tests::atomic"),
            ("expected_fail_passes", "unexpected_pass", "ssa::two_source_unit::tests::expected_red"),
            ("wrong_witness", "wrong_failure_evidence", "ssa::two_source_unit::tests::expected_red"),
        ]
        for fault_mode, expected_kind, expected_test in cases:
            for arm in ("baseline", "candidate"):
                with self.subTest(fault=fault_mode, arm=arm):
                    with tempfile.TemporaryDirectory() as tmp:
                        tmp_path = Path(tmp)
                        manifest, base_env, target_a, _, _ = self._make_synthetic_harness(
                            tmp_path, fault_mode=fault_mode
                        )
                        env = {**base_env, "CARGO_TARGET_DIR": str(target_a), "PILOT_MODE_TAG": arm}
                        with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                            self.pilot.run_pilot(
                                manifest=manifest,
                                root=ROOT,
                                mode=arm,
                                list_only=False,
                                artifacts_dir=tmp_path / f"out-{arm}",
                                env=env,
                                verify_shell_parity=False,
                                allow_script_test_binary=True,
                                inspect_dylibs=False,
                            )
                        self.assertEqual(ctx.exception.kind, expected_kind)
                        self.assertEqual(ctx.exception.config_id, "cfg1-synthetic")
                        self.assertEqual(ctx.exception.test_name, expected_test)


class ReplayManifestAndSeparateArtifactLockContracts(unittest.TestCase):
    def setUp(self):
        self.pilot = load_pilot_module()

    def test_replay_manifest_has_exact_2_configs_and_14_entries_matching_shell_script(self):
        self.assertTrue(REPLAY_MANIFEST_PATH.is_file(), "missing scripts/e2_replay_manifest.json")
        manifest = self.pilot.load_manifest(REPLAY_MANIFEST_PATH)
        self.assertEqual(manifest["suite_id"], "e2-replay")
        self.assertEqual(manifest["source_script"], "scripts/check_e2_replay_ci.sh")
        self.assertEqual(manifest["formal_ci_step_name"], "Experimental E2 replay faults (CI only)")
        self.assertEqual(manifest["total_configs"], 2)
        self.assertEqual(manifest["total_entries"], 14)
        self.assertEqual([len(c["entries"]) for c in manifest["configs"]], [7, 7])
        self.assertEqual(
            [c["expected_features"] for c in manifest["configs"]],
            [
                ["e2-two-source-test-support"],
                ["e2-two-source-test-support", "loan-observation-test-support"],
            ],
        )
        self.assertTrue(all(e["exact"] and e["ignored"] for c in manifest["configs"] for e in c["entries"]))

        env = dict(os.environ)
        env.pop("CI", None)
        env.pop("GITHUB_ACTIONS", None)
        report = self.pilot.verify_manifest_against_shell_script(manifest, ROOT, env=env)
        self.assertEqual(report["verified_configs"], 2)
        self.assertEqual(report["verified_entries"], 14)

    def test_separate_artifact_lock_rejects_reused_executable_across_distinct_feature_configs(self):
        helper = AntiSkipExpectedFailureAndComparisonContracts()
        helper.setUp()
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            _, base_env, target_a, target_b, call_log = helper._make_synthetic_harness(tmp_path)
            base_env["KOVEN_ALLOW_DEFAULT_JOBS"] = "1"

            two_cfg_manifest = {
                "schema": "koven.e2-two-source-shadow-manifest.v1",
                "suite_id": "e2-replay",
                "source_script": "scripts/check_e2_replay_ci.sh",
                "formal_ci_step_name": "Experimental E2 replay faults (CI only)",
                "cargo_jobs": 1,
                "total_configs": 2,
                "total_entries": 2,
                "configs": [
                    {
                        "config_id": "replay-cfg1",
                        "package": "lang-codegen",
                        "target_name": "lang_codegen",
                        "target_kind": ["lib"],
                        "cargo_target_args": ["--lib"],
                        "cargo_features_arg": "e2-two-source-test-support",
                        "expected_features": ["e2-two-source-test-support"],
                        "entries": [
                            {
                                "global_index": 0,
                                "test_name": "ssa::e2_replay_ci::e2_signature_and_source_roles_reject",
                                "exact": True,
                                "ignored": True,
                                "ci_only": True,
                                "expected_outcome": "pass",
                                "expected_failure_substrings": [],
                            }
                        ],
                    },
                    {
                        "config_id": "replay-cfg2",
                        "package": "lang-codegen",
                        "target_name": "lang_codegen",
                        "target_kind": ["lib"],
                        "cargo_target_args": ["--lib"],
                        "cargo_features_arg": "e2-two-source-test-support loan-observation-test-support",
                        "expected_features": ["e2-two-source-test-support", "loan-observation-test-support"],
                        "entries": [
                            {
                                "global_index": 1,
                                "test_name": "ssa::e2_replay_ci::e2_signature_and_source_roles_reject",
                                "exact": True,
                                "ignored": True,
                                "ci_only": True,
                                "expected_outcome": "pass",
                                "expected_failure_substrings": [],
                            }
                        ],
                    },
                ],
            }

            # 1. Colliding executable across different feature configs MUST fail closed
            colliding_env = {
                **base_env,
                "CARGO_TARGET_DIR": str(target_b),
                "PILOT_MODE_TAG": "candidate",
                "FAKE_COLLIDE_FEATURE_ARTIFACTS": "1",
            }
            with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                self.pilot.run_pilot(
                    manifest=two_cfg_manifest,
                    root=ROOT,
                    mode="candidate",
                    list_only=False,
                    artifacts_dir=tmp_path / "artifacts-collide",
                    env=colliding_env,
                    verify_shell_parity=False,
                    allow_script_test_binary=True,
                    inspect_dylibs=False,
                    candidate_jobs="default",
                )
            self.assertEqual(ctx.exception.kind, "artifact_lock_failed")
            self.assertEqual(ctx.exception.config_id, "replay-cfg2")

            # 2. Distinct feature-hashed executables pass and compare cleanly
            env_a = {**base_env, "CARGO_TARGET_DIR": str(target_a), "PILOT_MODE_TAG": "baseline"}
            ledger_a = self.pilot.run_pilot(
                manifest=two_cfg_manifest,
                root=ROOT,
                mode="baseline",
                list_only=False,
                artifacts_dir=tmp_path / "artifacts-a",
                env=env_a,
                verify_shell_parity=False,
                allow_script_test_binary=True,
                inspect_dylibs=False,
                candidate_jobs="default",
            )
            call_log.write_text("", encoding="utf-8")
            env_b = {**base_env, "CARGO_TARGET_DIR": str(target_b) + "-clean", "PILOT_MODE_TAG": "candidate"}
            ledger_b = self.pilot.run_pilot(
                manifest=two_cfg_manifest,
                root=ROOT,
                mode="candidate",
                list_only=False,
                artifacts_dir=tmp_path / "artifacts-b",
                env=env_b,
                verify_shell_parity=False,
                allow_script_test_binary=True,
                inspect_dylibs=False,
                candidate_jobs="default",
            )
            self.assertNotEqual(
                ledger_b["configs"][0]["locked_artifact"]["executable"],
                ledger_b["configs"][1]["locked_artifact"]["executable"],
            )
            self.assertNotEqual(
                ledger_b["configs"][0]["locked_artifact"]["executable_sha256"],
                ledger_b["configs"][1]["locked_artifact"]["executable_sha256"],
            )
            comp = self.pilot.compare_ledgers(ledger_a, ledger_b)
            self.assertTrue(comp["equivalent"])
            self.assertEqual(comp["suite_id"], "e2-replay")

            # 3. Cross-suite ledger comparison must fail closed
            wrong_suite = copy.deepcopy(ledger_b)
            wrong_suite["suite_id"] = "e2-two-source-unit"
            with self.assertRaises(self.pilot.ShadowPilotError) as ctx2:
                self.pilot.compare_ledgers(ledger_a, wrong_suite)
            self.assertEqual(ctx2.exception.kind, "ab_mismatch")


class ShadowWorkflowStaticContracts(unittest.TestCase):
    def test_formal_ci_and_e2_script_are_unmodified_and_shadow_is_isolated(self):
        self.assertTrue(SHADOW_WORKFLOW_PATH.is_file(), "missing ci-e2-shadow-pilot.yml")
        shadow = SHADOW_WORKFLOW_PATH.read_text(encoding="utf-8")
        formal = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")

        # Formal CI does not reference shadow pilot
        self.assertNotIn("e2_two_source_shadow_pilot", formal)
        self.assertNotIn("ci-e2-shadow-pilot", formal)

        # One-click kill switch present
        self.assertIn("KOVEN_E2_SHADOW_PILOT_DISABLED", shadow)
        self.assertIn("workflow_dispatch:", shadow)

        # Dual-host parity + pinned container digest
        self.assertIn("os: [macos-15, ubuntu-26.04]", shadow)
        self.assertIn("arm: [baseline, candidate]", shadow)
        self.assertIn(
            "ghcr.io/halckon/koven-ci-toolchain@sha256:b5ce8a4479579ecc57194e385f13109a52022114f0e19cab93fc426bdbb9500e",
            shadow,
        )
        self.assertIn("options: --user 1001:1001", shadow)

        # Isolated CARGO_TARGET_DIR and exact-key-only cache (isolated per suite + per arm, no restore-keys fallback, no v0-rust / v1-test-debug1)
        self.assertIn("CARGO_TARGET_DIR:", shadow)
        self.assertIn("target-e2-shadow-${{ inputs.suite || 'e2-replay' }}-${{ matrix.arm }}", shadow)
        self.assertIn("e2-shadow-pilot-v2-suite-${SUITE_ID}-", shadow)
        self.assertIn("scripts/e2_replay_manifest.json", shadow)
        self.assertIn("scripts/e2_two_source_unit_manifest.json", shadow)
        self.assertNotIn("restore-keys:", shadow)
        self.assertNotIn("v0-rust", shadow)
        self.assertNotIn("v1-test-debug1", shadow)
        self.assertNotIn("Swatinem/rust-cache", shadow)


if __name__ == "__main__":
    unittest.main()
