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
            import platform as _plat
            import struct as _st

            exe = Path(tmp) / "lang_codegen-abc123"
            mach = _plat.machine().lower()
            if sys.platform == "darwin":
                host_triple = "aarch64-apple-darwin" if mach in ("arm64", "aarch64") else "x86_64-apple-darwin"
                cputype = 0x0100000C if "aarch64" in host_triple else 0x01000007
                header = _st.pack("<IIIIIIII", 0xFEEDFACF, cputype, 0, 2, 0, 0, 0, 0) + b"\x00" * 32
            else:
                host_triple = "aarch64-unknown-linux-gnu" if mach in ("arm64", "aarch64") else "x86_64-unknown-linux-gnu"
                e_machine = 183 if "aarch64" in host_triple else 62
                header = b"\x7fELF\x02\x01\x01" + b"\x00" * 9 + _st.pack("<HH", 3, e_machine) + b"\x00" * 44
            exe.write_bytes(header)
            exe.chmod(0o755)
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


class Pr463ReviewP2RegressionContracts(unittest.TestCase):
    """Regression contracts for all 8 P2 findings from PR #463 review."""

    def setUp(self):
        self.pilot = load_pilot_module()

    def test_p2_1_execution_branch_drift_is_rejected_without_faking_ci(self):
        orig_replay = (ROOT / "scripts/check_e2_replay_ci.sh").read_text(encoding="utf-8")
        replay_manifest = self.pilot.load_manifest(REPLAY_MANIFEST_PATH)
        clean_env = dict(os.environ)
        clean_env.pop("CI", None)
        clean_env.pop("GITHUB_ACTIONS", None)

        mutations = [
            (
                "drop_ignored_in_exec_only",
                lambda s: s.replace(
                    'cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- --exact --ignored\n',
                    'cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- --exact\n',
                ),
            ),
            (
                "rename_target_in_exec_only",
                lambda s: s.replace(
                    'cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- --exact --ignored\n',
                    'cargo test --locked -j1 -p lang-codegen --lib --features "$features" "ssa::wrong::target" -- --exact --ignored\n',
                ),
            ),
            (
                "duplicate_run_in_exec_only",
                lambda s: s.replace(
                    'cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- --exact --ignored\n',
                    'cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- --exact --ignored\n'
                    '      cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- --exact --ignored\n',
                ),
            ),
            (
                "delete_run_in_exec_only",
                lambda s: s.replace(
                    'cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- --exact --ignored\n',
                    ':\n',
                ),
            ),
        ]

        with tempfile.TemporaryDirectory() as tmp:
            fake_root = Path(tmp)
            (fake_root / "scripts").mkdir(parents=True)
            for name, mutate in mutations:
                with self.subTest(mutation=name):
                    mutated_script = mutate(orig_replay)
                    self.assertNotEqual(mutated_script, orig_replay)
                    (fake_root / "scripts/check_e2_replay_ci.sh").write_text(mutated_script, encoding="utf-8")
                    with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                        self.pilot.verify_manifest_against_shell_script(replay_manifest, fake_root, env=clean_env)
                    self.assertEqual(ctx.exception.kind, "manifest_drift")

            # Also verify check_e2_two_source_unit_ci.sh execution-branch drift is caught without CI env
            orig_unit = (ROOT / "scripts/check_e2_two_source_unit_ci.sh").read_text(encoding="utf-8")
            unit_manifest = self.pilot.load_manifest(MANIFEST_PATH)
            mutated_unit = orig_unit.replace(
                'cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- "${args[@]}"\n',
                'cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- --exact\n',
            )
            self.assertNotEqual(mutated_unit, orig_unit)
            (fake_root / "scripts/check_e2_two_source_unit_ci.sh").write_text(mutated_unit, encoding="utf-8")
            with self.assertRaises(self.pilot.ShadowPilotError) as ctx_u:
                self.pilot.verify_manifest_against_shell_script(unit_manifest, fake_root, env=clean_env)
            self.assertEqual(ctx_u.exception.kind, "manifest_drift")

    def test_p2_2_compare_job_rerun_resolves_prior_attempt_arm_ledgers_and_rejects_failed_latest_attempt(self):
        shadow = SHADOW_WORKFLOW_PATH.read_text(encoding="utf-8")
        # Workflow download step must NOT hardcode -${{ github.run_attempt }} on arm download pattern
        self.assertIn("pattern: e2-shadow-arm-*-${{ github.run_id }}-*", shadow)

        with tempfile.TemporaryDirectory() as tmp:
            dl = Path(tmp)
            run_id = 999888
            for host_os in ("macos-15", "ubuntu-26.04"):
                for arm in ("baseline", "candidate"):
                    arm_dir = dl / f"e2-shadow-arm-{host_os}-{arm}-{run_id}-1"
                    arm_dir.mkdir(parents=True)
                    (arm_dir / "ledger.json").write_text(json.dumps({"attempt": 1, "arm": arm, "os": host_os}), encoding="utf-8")

            # Attempt 2 where only compare-shadow-ab re-ran: resolves attempt 1 ledgers cleanly
            resolved = self.pilot.resolve_run_arm_ledgers(dl, run_id=run_id, max_attempt=2)
            self.assertEqual(len(resolved), 4)
            self.assertTrue(str(resolved[("macos-15", "baseline")]).endswith("-1/ledger.json"))

            # If an arm DID re-run in attempt 2 and failed (wrote failure.json), must NOT fall back to attempt 1!
            failed_arm_a2 = dl / f"e2-shadow-arm-macos-15-candidate-{run_id}-2"
            failed_arm_a2.mkdir(parents=True)
            (failed_arm_a2 / "failure.json").write_text(json.dumps({"status": "failed"}), encoding="utf-8")
            with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                self.pilot.resolve_run_arm_ledgers(dl, run_id=run_id, max_attempt=2)
            self.assertEqual(ctx.exception.kind, "ab_mismatch")

    def test_p2_3_registry_only_cache_hit_with_empty_target_cannot_be_marked_warm(self):
        shadow = SHADOW_WORKFLOW_PATH.read_text(encoding="utf-8")
        self.assertIn("debug/deps", shadow)

        helper = AntiSkipExpectedFailureAndComparisonContracts()
        helper.setUp()
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            manifest, base_env, target_a, _, _ = helper._make_synthetic_harness(tmp_path)
            # Simulate registry-only cache hit: KOVEN_SHADOW_CACHE_HIT=true & KOVEN_SHADOW_CACHE_PHASE=warm, but target_a is empty!
            bad_warm_env = {
                **base_env,
                "CARGO_TARGET_DIR": str(target_a),
                "KOVEN_SHADOW_CACHE_HIT": "true",
                "KOVEN_SHADOW_CACHE_PHASE": "warm",
            }
            with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                self.pilot.run_pilot(
                    manifest=manifest,
                    root=ROOT,
                    mode="candidate",
                    list_only=True,
                    artifacts_dir=tmp_path / "out-warm-empty",
                    env=bad_warm_env,
                    verify_shell_parity=False,
                    allow_script_test_binary=True,
                    inspect_dylibs=False,
                )
            self.assertEqual(ctx.exception.kind, "cache_contract_violated")

    def test_p2_4_comparator_rejects_mixed_cold_and_warm_arms(self):
        helper = AntiSkipExpectedFailureAndComparisonContracts()
        helper.setUp()
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            manifest, base_env, target_a, target_b, _ = helper._make_synthetic_harness(tmp_path)
            # Pre-populate target_a/debug/deps so baseline is genuinely warm
            deps_a = target_a / "debug/deps"
            deps_a.mkdir(parents=True)
            (deps_a / "libcached.rlib").write_bytes(b"cached-rlib-bytes")

            ledger_a = self.pilot.run_pilot(
                manifest=manifest,
                root=ROOT,
                mode="baseline",
                list_only=True,
                artifacts_dir=tmp_path / "arm-a",
                env={
                    **base_env,
                    "CARGO_TARGET_DIR": str(target_a),
                    "KOVEN_SHADOW_CACHE_PHASE": "warm",
                    "KOVEN_SHADOW_CACHE_HIT": "true",
                },
                verify_shell_parity=False,
                allow_script_test_binary=True,
                inspect_dylibs=False,
            )
            ledger_b = self.pilot.run_pilot(
                manifest=manifest,
                root=ROOT,
                mode="candidate",
                list_only=True,
                artifacts_dir=tmp_path / "arm-b",
                env={
                    **base_env,
                    "CARGO_TARGET_DIR": str(target_b),
                    "KOVEN_SHADOW_CACHE_PHASE": "cold",
                    "KOVEN_SHADOW_CACHE_HIT": "false",
                },
                verify_shell_parity=False,
                allow_script_test_binary=True,
                inspect_dylibs=False,
            )
            with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                self.pilot.compare_ledgers(ledger_a, ledger_b)
            self.assertEqual(ctx.exception.kind, "ab_mismatch")
            self.assertIn("cache", ctx.exception.detail.lower())

    def test_p2_5_reference_ci_run_validates_workflow_event_head_and_merge_sha(self):
        head_sha = "a" * 40
        merge_sha = "b" * 40
        stale_merge_sha = "c" * 40
        other_head_sha = "d" * 40

        routes = {
            "actions/runs/100": {
                "id": 100,
                "event": "pull_request",
                "head_sha": head_sha,
                "path": ".github/workflows/ci-e2-shadow-pilot.yml",
                "pull_requests": [{"number": 463}],
            },
            "actions/runs/100/jobs?per_page=100": {"jobs": []},
            # Wrong workflow
            "actions/runs/201": {
                "id": 201,
                "event": "pull_request",
                "head_sha": head_sha,
                "path": ".github/workflows/parser-formal-f6.yml",
            },
            # Wrong head_sha
            "actions/runs/202": {
                "id": 202,
                "event": "pull_request",
                "head_sha": other_head_sha,
                "path": ".github/workflows/ci.yml",
            },
            # Push CI on same head_sha (did NOT test PR merge tree!)
            "actions/runs/203": {
                "id": 203,
                "event": "push",
                "head_sha": head_sha,
                "path": ".github/workflows/ci.yml",
                "status": "completed",
                "conclusion": "success",
            },
            "actions/runs/203/jobs?per_page=100": {"jobs": []},
            "actions/runs/203/artifacts?per_page=100": {"artifacts": []},
            # PR CI on same head_sha but stale merge commit!
            "actions/runs/204": {
                "id": 204,
                "event": "pull_request",
                "head_sha": head_sha,
                "path": ".github/workflows/ci.yml",
                "display_title": "CI pull_request PR 463",
                "status": "completed",
                "conclusion": "success",
            },
            "actions/runs/204/jobs?per_page=100": {
                "jobs": [
                    {
                        "name": "Targeted Tests (ubuntu-26.04)",
                        "status": "completed",
                        "conclusion": "success",
                        "created_at": "2026-11-25T00:00:00Z",
                        "started_at": "2026-11-25T00:00:02Z",
                        "completed_at": "2026-11-25T00:10:02Z",
                        "steps": [],
                    }
                ]
            },
            "actions/runs/204/artifacts?per_page=100": {
                "artifacts": [{"name": f"word-frequency-Linux-{stale_merge_sha}"}]
            },
            # Exact matching PR CI on same head_sha AND exact merge_sha!
            "actions/runs/205": {
                "id": 205,
                "event": "pull_request",
                "head_sha": head_sha,
                "path": ".github/workflows/ci.yml",
                "display_title": "CI pull_request PR 463",
                "status": "completed",
                "conclusion": "success",
            },
            "actions/runs/205/jobs?per_page=100": {
                "jobs": [
                    {
                        "name": "Targeted Tests (ubuntu-26.04)",
                        "status": "completed",
                        "conclusion": "success",
                        "created_at": "2026-11-25T00:00:00Z",
                        "started_at": "2026-11-25T00:00:03Z",
                        "completed_at": "2026-11-25T00:20:03Z",
                        "steps": [
                            {
                                "name": "Experimental E2 replay faults (CI only)",
                                "started_at": "2026-11-25T00:05:00Z",
                                "completed_at": "2026-11-25T00:10:44Z",
                            }
                        ],
                    }
                ]
            },
            "actions/runs/205/artifacts?per_page=100": {
                "artifacts": [{"name": f"word-frequency-Linux-{merge_sha}"}]
            },
            f"actions/workflows/ci.yml/runs?head_sha={head_sha}&per_page=20": {
                "workflow_runs": [
                    # Push run listed first, stale merge second, exact PR merge third!
                    {"id": 203, "event": "push", "head_sha": head_sha, "path": ".github/workflows/ci.yml", "status": "completed", "conclusion": "success"},
                    {"id": 204, "event": "pull_request", "head_sha": head_sha, "path": ".github/workflows/ci.yml", "display_title": "CI pull_request PR 463", "status": "completed", "conclusion": "success"},
                    {"id": 205, "event": "pull_request", "head_sha": head_sha, "path": ".github/workflows/ci.yml", "display_title": "CI pull_request PR 463", "status": "completed", "conclusion": "success"},
                ]
            },
        }

        def fake_api_get(path: str):
            if path not in routes:
                raise AssertionError(f"unexpected API path: {path}")
            return routes[path]

        # Explicit wrong workflow, wrong head_sha, push CI, or stale merge SHA must ALL fail closed
        for bad_id in (201, 202, 203, 204):
            with self.subTest(bad_explicit_id=bad_id):
                with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                    self.pilot.fetch_workflow_metrics(
                        "Halckon/Koven",
                        100,
                        "tok",
                        head_sha=head_sha,
                        checked_out_sha=merge_sha,
                        reference_ci_run_id=bad_id,
                        formal_step_name="Experimental E2 replay faults (CI only)",
                        api_get_fn=fake_api_get,
                    )
                self.assertEqual(ctx.exception.kind, "invalid_reference_run")

        # Auto-discovery skips push run 203 and stale-merge run 204, selecting exact run 205
        metrics = self.pilot.fetch_workflow_metrics(
            "Halckon/Koven",
            100,
            "tok",
            head_sha=head_sha,
            checked_out_sha=merge_sha,
            formal_step_name="Experimental E2 replay faults (CI only)",
            api_get_fn=fake_api_get,
        )
        self.assertEqual(len(metrics["formal_ci_reference"]), 1)
        self.assertEqual(metrics["formal_ci_reference"][0]["ci_run_id"], 205)
        self.assertEqual(metrics["formal_ci_reference"][0]["e2_step_reference_seconds"], 344.0)

    def test_p2_6_failed_rerun_purges_stale_ledger_and_compare_rejects_failure_sibling(self):
        helper = AntiSkipExpectedFailureAndComparisonContracts()
        helper.setUp()
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            manifest, base_env, target_a, target_b, _ = helper._make_synthetic_harness(tmp_path)
            reused_dir = tmp_path / "reused-artifacts"

            # First run succeeds -> writes ledger.json
            self.pilot.run_pilot(
                manifest=manifest,
                root=ROOT,
                mode="candidate",
                list_only=False,
                artifacts_dir=reused_dir,
                env={**base_env, "CARGO_TARGET_DIR": str(target_b), "PILOT_MODE_TAG": "candidate"},
                verify_shell_parity=False,
                allow_script_test_binary=True,
                inspect_dylibs=False,
            )
            self.assertTrue((reused_dir / "ledger.json").is_file())

            # Second run into SAME artifacts_dir fails mid-run -> stale ledger.json MUST be purged!
            with self.assertRaises(self.pilot.ShadowPilotError):
                self.pilot.run_pilot(
                    manifest=manifest,
                    root=ROOT,
                    mode="candidate",
                    list_only=False,
                    artifacts_dir=reused_dir,
                    env={
                        **base_env,
                        "CARGO_TARGET_DIR": str(target_b),
                        "PILOT_MODE_TAG": "candidate",
                        "FAKE_FAULT_MODE": "zero_list",
                    },
                    verify_shell_parity=False,
                    allow_script_test_binary=True,
                    inspect_dylibs=False,
                )
            self.assertFalse((reused_dir / "ledger.json").exists(), "stale ledger.json survived failed rerun!")
            self.assertTrue((reused_dir / "failure.json").is_file())

            # Even if someone manually leaves both ledger.json and failure.json, compare CLI rejects it
            ledger_a = self.pilot.run_pilot(
                manifest=manifest,
                root=ROOT,
                mode="baseline",
                list_only=False,
                artifacts_dir=tmp_path / "clean-a",
                env={**base_env, "CARGO_TARGET_DIR": str(target_a), "PILOT_MODE_TAG": "baseline"},
                verify_shell_parity=False,
                allow_script_test_binary=True,
                inspect_dylibs=False,
            )
            (reused_dir / "ledger.json").write_text(json.dumps(ledger_a), encoding="utf-8")
            rc = self.pilot.main(
                [
                    "compare",
                    "--baseline",
                    str(tmp_path / "clean-a/ledger.json"),
                    "--candidate",
                    str(reused_dir / "ledger.json"),
                    "--output",
                    str(tmp_path / "cmp.json"),
                ]
            )
            self.assertEqual(rc, 1)

    def test_p2_7_binary_header_check_verifies_cpu_architecture_and_executable_type(self):
        import struct as _st

        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp)

            def write_bin(name: str, data: bytes) -> Path:
                f = p / name
                f.write_bytes(data)
                f.chmod(0o755)
                return f

            # Valid headers pass
            arm64_macho = write_bin("arm64_macho", _st.pack("<IIIIIIII", 0xFEEDFACF, 0x0100000C, 0, 2, 0, 0, 0, 0))
            x86_64_macho = write_bin("x86_64_macho", _st.pack("<IIIIIIII", 0xFEEDFACF, 0x01000007, 3, 2, 0, 0, 0, 0))
            x86_64_elf = write_bin("x86_64_elf", b"\x7fELF\x02\x01\x01" + b"\x00" * 9 + _st.pack("<HH", 3, 62) + b"\x00" * 24)
            arm64_elf = write_bin("arm64_elf", b"\x7fELF\x02\x01\x01" + b"\x00" * 9 + _st.pack("<HH", 2, 183) + b"\x00" * 24)

            self.pilot.verify_binary_matches_host_triple(arm64_macho, "aarch64-apple-darwin")
            self.pilot.verify_binary_matches_host_triple(x86_64_macho, "x86_64-apple-darwin")
            self.pilot.verify_binary_matches_host_triple(x86_64_elf, "x86_64-unknown-linux-gnu")
            self.pilot.verify_binary_matches_host_triple(arm64_elf, "aarch64-unknown-linux-gnu")

            # Invalid: magic + zeros, Rosetta x86_64 on arm64 macOS, arm64 ELF on x86_64 Linux, .o object filetype
            macho_zeros = write_bin("macho_zeros", b"\xcf\xfa\xed\xfe" + b"\x00" * 60)
            elf_zeros = write_bin("elf_zeros", b"\x7fELF" + b"\x00" * 60)
            macho_obj = write_bin("macho_obj", _st.pack("<IIIIIIII", 0xFEEDFACF, 0x0100000C, 0, 1, 0, 0, 0, 0))
            elf_rel = write_bin("elf_rel", b"\x7fELF\x02\x01\x01" + b"\x00" * 9 + _st.pack("<HH", 1, 62) + b"\x00" * 24)

            for label, exe_path, triple in [
                ("macho_magic_plus_zeros", macho_zeros, "aarch64-apple-darwin"),
                ("elf_magic_plus_zeros", elf_zeros, "x86_64-unknown-linux-gnu"),
                ("rosetta_x86_64_on_arm64_macos", x86_64_macho, "aarch64-apple-darwin"),
                ("arm64_on_x86_64_macos", arm64_macho, "x86_64-apple-darwin"),
                ("arm64_elf_on_x86_64_linux", arm64_elf, "x86_64-unknown-linux-gnu"),
                ("x86_64_elf_on_arm64_linux", x86_64_elf, "aarch64-unknown-linux-gnu"),
                ("macho_object_not_executable", macho_obj, "aarch64-apple-darwin"),
                ("elf_relocatable_not_executable", elf_rel, "x86_64-unknown-linux-gnu"),
            ]:
                with self.subTest(case=label):
                    with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                        self.pilot.verify_binary_matches_host_triple(exe_path, triple)
                    self.assertEqual(ctx.exception.kind, "artifact_lock_failed")

    def test_p2_8_dylib_probe_detects_switched_rpath_libllvm_with_identical_install_name(self):
        import shutil as _sh

        cc = "/usr/bin/clang" if sys.platform == "darwin" else (_sh.which("clang") or _sh.which("cc"))
        self.assertIsNotNone(cc, "requires C compiler for native dylib probe test")
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            lib1 = d / "lib1"
            lib2 = d / "lib2"
            lib1.mkdir()
            lib2.mkdir()
            (d / "l1.c").write_text("int koven_llvm_probe_sym(void) { return 11; }\n", encoding="utf-8")
            (d / "l2.c").write_text("int koven_llvm_probe_sym(void) { return 22; }\n", encoding="utf-8")
            (d / "main.c").write_text(
                '#include <stdio.h>\nextern int koven_llvm_probe_sym(void);\n'
                'int main(void) { printf("0 tests, 0 benchmarks\\n"); return koven_llvm_probe_sym() == 0; }\n',
                encoding="utf-8",
            )
            exe = d / "probe_bin"
            if sys.platform == "darwin":
                subprocess.check_call(
                    [cc, "-dynamiclib", str(d / "l1.c"), "-install_name", "@rpath/libLLVMProbeTest.dylib", "-o", str(lib1 / "libLLVMProbeTest.dylib")]
                )
                subprocess.check_call(
                    [cc, "-dynamiclib", str(d / "l2.c"), "-install_name", "@rpath/libLLVMProbeTest.dylib", "-o", str(lib2 / "libLLVMProbeTest.dylib")]
                )
                subprocess.check_call(
                    [cc, str(d / "main.c"), f"-L{lib1}", "-lLLVMProbeTest", f"-Wl,-rpath,{lib1}", "-o", str(exe)]
                )
                env1 = dict(os.environ)
                env1.pop("DYLD_LIBRARY_PATH", None)
                # Force identical DYLD_LIBRARY_PATH display? Even better: test @rpath resolution when lib1 symlink retargets OR DYLD_LIBRARY_PATH overrides!
                # Even without changing DYLD_LIBRARY_PATH env var: retargeting a symlink inside @rpath keeps allowlisted_env & dylib_search_paths 100% identical while changing the actual loaded libLLVM!
                rpath_link = d / "rpath_slot"
                rpath_link.symlink_to(lib1)
                exe_rpath = d / "probe_rpath_bin"
                subprocess.check_call(
                    [cc, str(d / "main.c"), f"-L{rpath_link}", "-lLLVMProbeTest", f"-Wl,-rpath,{rpath_link}", "-o", str(exe_rpath)]
                )
                snap1 = self.pilot.build_probe_snapshot(executable=exe_rpath, cwd=d, env=env1, inspect_dylibs=True)
                rpath_link.unlink()
                rpath_link.symlink_to(lib2)
                snap2 = self.pilot.build_probe_snapshot(executable=exe_rpath, cwd=d, env=env1, inspect_dylibs=True)
            else:
                rpath_link = d / "rpath_slot"
                rpath_link.symlink_to(lib1)
                subprocess.check_call(
                    [cc, "-shared", "-fPIC", str(d / "l1.c"), "-Wl,-soname,libLLVMProbeTest.so", "-o", str(lib1 / "libLLVMProbeTest.so")]
                )
                subprocess.check_call(
                    [cc, "-shared", "-fPIC", str(d / "l2.c"), "-Wl,-soname,libLLVMProbeTest.so", "-o", str(lib2 / "libLLVMProbeTest.so")]
                )
                exe_rpath = d / "probe_rpath_bin"
                subprocess.check_call(
                    [cc, str(d / "main.c"), f"-L{rpath_link}", "-lLLVMProbeTest", f"-Wl,-rpath,{rpath_link}", "-o", str(exe_rpath)]
                )
                env1 = dict(os.environ)
                snap1 = self.pilot.build_probe_snapshot(executable=exe_rpath, cwd=d, env=env1, inspect_dylibs=True)
                rpath_link.unlink()
                rpath_link.symlink_to(lib2)
                snap2 = self.pilot.build_probe_snapshot(executable=exe_rpath, cwd=d, env=env1, inspect_dylibs=True)

            with self.assertRaises(self.pilot.ShadowPilotError) as ctx:
                self.pilot.verify_probe_equivalence(snap1, snap2, host="test", config_id="cfg-rpath")
            self.assertEqual(ctx.exception.kind, "environment_mismatch")
            self.assertIn("resolved_llvm_dylibs", ctx.exception.detail)


if __name__ == "__main__":
    unittest.main()
