"""CI-only Linux limited-debug profile contracts; do not run Cargo."""
from pathlib import Path
import importlib.util
import os
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


def _load_shadow_contracts():
    spec = importlib.util.spec_from_file_location(
        'test_e2_two_source_shadow_pilot',
        Path(__file__).resolve().parent / 'test_e2_two_source_shadow_pilot.py',
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.ShadowWorkflowStaticContracts


class CiDebugProfileTests(unittest.TestCase):
    def setUp(self):
        self.workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.job = self.workflow.split('\n  test:\n', 1)[1].split('\n  test-d10:\n', 1)[0]
        self.test_jobs = {
            'test': self.job,
            'test-d10': self.workflow.split('\n  test-d10:\n', 1)[1].split('\n  test-e2:\n', 1)[0],
            'test-e2': self.workflow.split('\n  test-e2:\n', 1)[1].split('\n  preview-', 1)[0],
        }

    def test_only_original_two_host_matrix(self):
        expected_names = {
            'test': 'name: Targeted Tests (${{ matrix.os }})\n',
            'test-d10': 'name: Targeted Tests D10 & Receipts (${{ matrix.os }})\n',
            'test-e2': 'name: Targeted Tests E2 (${{ matrix.os }})\n',
        }
        for job_id, body in self.test_jobs.items():
            with self.subTest(job_id=job_id):
                self.assertIn('os: [macos-15, ubuntu-26.04]', body)
                matrix = body.split('      matrix:\n', 1)[1].split('    runs-on:', 1)[0]
                self.assertEqual(1, matrix.count('          - os: '))
                self.assertIn('          - os: ubuntu-26.04\n            container:', matrix)
                self.assertIn('              options: --user 1001:1001', matrix)
                self.assertNotIn('matrix.debug', body)
                self.assertIn(expected_names[job_id], body)
                self.assertNotIn('continue-on-error', body)

    def test_profile_is_dual_host_limited_debug_and_scoped_to_tests(self):
        for key in ('CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG'):
            self.assertEqual(self.workflow.count(key), 3)
            for job_id, body in self.test_jobs.items():
                with self.subTest(key=key, job_id=job_id):
                    self.assertIn(key + ": '1'", body)
        self.assertNotIn('[profile.', (ROOT / 'Cargo.toml').read_text())
        self.assertNotIn('CARGO_INCREMENTAL:', self.workflow)

    def test_cache_is_stable_separate_from_old_debug2_artifacts(self):
        for job_id, body in self.test_jobs.items():
            with self.subTest(job_id=job_id):
                self.assertIn('prefix-key: v1-test-debug1\n', body)
                self.assertNotIn('v0-rust', body)
                self.assertNotIn('github.run_id', body)
                self.assertNotIn('save-if:', body)
                self.assertNotIn('restore-keys:', body)

    def _assert_all_test_gates(self, test_jobs):
        for command in ('bash scripts/check_integration.sh',
                        'python3 scripts/check_word_frequency.py',
                        'python3 scripts/conformance_gate.py',
                        'python3 scripts/check_native_sanitizers.py',
                        'python3 scripts/check_generated_owners.py'):
            self.assertIn(command, test_jobs['test'])
        contracts = _load_shadow_contracts()
        contracts.assert_formal_ci_e2_steps(
            test_jobs['test-d10'],
            step_names=(
                'Independent D10 catalog identity controls (CI only)',
                'Verify scalar owner entry corruption fails independently',
            ),
        )
        contracts.assert_formal_ci_e2_steps(
            test_jobs['test-e2'],
            step_names=(
                'Experimental E2 replay faults (CI only)',
                'Verify opt-in two-source Unit capability and atomicity',
            ),
        )

    def test_existing_all_test_gates_remain(self):
        self._assert_all_test_gates(self.test_jobs)
        summary = self.workflow.split('\n  ci-passed:\n', 1)[1]
        for job_id in ('test,', 'test-d10,', 'test-e2,'):
            self.assertIn(job_id, summary.split('steps:', 1)[0])
        self.assertIn('run: python3 scripts/check_ci_results.py', summary)

    def test_d10_steps_can_each_roll_back_independently_to_legacy_shell(self):
        d10_block = (
            '      - name: Independent D10 catalog identity controls (CI only)\n'
            '        run: |\n'
            '          python3 scripts/e2_two_source_shadow_pilot.py run \\\n'
            '            --manifest scripts/d10_catalog_identity_manifest.json \\\n'
            '            --mode candidate \\\n'
            '            --candidate-jobs default \\\n'
            '            --formal-gate \\\n'
            '            --artifacts "$RUNNER_TEMP/koven-d10-catalog-identity"\n'
        )
        scalar_block = (
            '      - name: Verify scalar owner entry corruption fails independently\n'
            '        run: |\n'
            '          python3 scripts/e2_two_source_shadow_pilot.py run \\\n'
            '            --manifest scripts/scalar_owner_entry_manifest.json \\\n'
            '            --mode candidate \\\n'
            '            --candidate-jobs default \\\n'
            '            --formal-gate \\\n'
            '            --artifacts "$RUNNER_TEMP/koven-scalar-owner-entry"\n'
        )
        self.assertIn(d10_block, self.test_jobs['test-d10'])
        self.assertIn(scalar_block, self.test_jobs['test-d10'])

        # Rolling back only D10 catalog identity keeps both gates passing
        only_d10_rolled = {
            **self.test_jobs,
            'test-d10': self.test_jobs['test-d10'].replace(
                d10_block,
                '      - name: Independent D10 catalog identity controls (CI only)\n'
                '        run: bash scripts/check_d10_catalog_identity_ci.sh\n',
            ),
        }
        self._assert_all_test_gates(only_d10_rolled)

        # Rolling back only scalar owner entry keeps both gates passing
        only_scalar_rolled = {
            **self.test_jobs,
            'test-d10': self.test_jobs['test-d10'].replace(
                scalar_block,
                '      - name: Verify scalar owner entry corruption fails independently\n'
                '        run: bash scripts/check_scalar_owner_entry_ci.sh\n',
            ),
        }
        self._assert_all_test_gates(only_scalar_rolled)

        # Appending --list-only to a rolled-back D10 step MUST fail
        bad_rollback = {
            **self.test_jobs,
            'test-d10': self.test_jobs['test-d10'].replace(
                d10_block,
                '      - name: Independent D10 catalog identity controls (CI only)\n'
                '        run: bash scripts/check_d10_catalog_identity_ci.sh --list-only\n',
            ),
        }
        with self.assertRaises(AssertionError):
            self._assert_all_test_gates(bad_rollback)

    def test_default_codegen_gate_keeps_exact_selection_and_uses_scoped_build_settings(self):
        with tempfile.TemporaryDirectory() as temp:
            cargo = Path(temp) / 'cargo'
            cargo.write_text('#!/bin/sh\npwd -P\nprintf "%s\\n" "$@"\nexit 37\n')
            cargo.chmod(0o755)
            result = subprocess.run(
                ['bash', str(ROOT / 'scripts/run_codegen_default_tests.sh'),
                 '--offline', '-j1', 'fault::example', '--', '--exact', '--ignored'],
                cwd=temp, env={**os.environ, 'PATH': temp + os.pathsep + os.environ['PATH']},
                capture_output=True, text=True, check=False, timeout=10,
            )
        self.assertEqual(37, result.returncode, result.stderr)
        self.assertEqual([
            str(ROOT.resolve()),
            'test', '--locked', '-p', 'lang-codegen', '--lib',
            '--config', 'profile.test.package.lang-codegen.debug="line-tables-only"',
            '--config', 'profile.test.package.lang-codegen.codegen-units=64',
            '--offline', '-j1', 'fault::example', '--', '--exact', '--ignored',
        ], result.stdout.splitlines())


if __name__ == '__main__':
    unittest.main()
