"""CI-only Linux limited-debug profile contracts; do not run Cargo."""
from pathlib import Path
import os
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


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

    def test_profile_is_linux_only_and_scoped_to_tests(self):
        for key in ('CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG'):
            self.assertEqual(self.workflow.count(key), 3)
            for job_id, body in self.test_jobs.items():
                with self.subTest(key=key, job_id=job_id):
                    self.assertIn(key + ": ${{ matrix.os == 'ubuntu-26.04' && '1' || '2' }}", body)
        self.assertNotIn('[profile.', (ROOT / 'Cargo.toml').read_text())
        self.assertNotIn('CARGO_INCREMENTAL:', self.workflow)

    def test_cache_is_stable_separate_from_old_linux_artifacts(self):
        for job_id, body in self.test_jobs.items():
            with self.subTest(job_id=job_id):
                self.assertIn("prefix-key: ${{ runner.os == 'Linux' && 'v1-test-debug1' || 'v0-rust' }}", body)
                self.assertNotIn('github.run_id', body)
                self.assertNotIn('save-if:', body)
                self.assertNotIn('restore-keys:', body)

    def test_existing_all_test_gates_remain(self):
        for command in ('bash scripts/check_integration.sh',
                        'python3 scripts/check_word_frequency.py',
                        'python3 scripts/conformance_gate.py',
                        'python3 scripts/check_native_sanitizers.py',
                        'python3 scripts/check_generated_owners.py'):
            self.assertIn(command, self.job)
        self.assertIn('bash scripts/check_d10_catalog_identity_ci.sh', self.test_jobs['test-d10'])
        self.assertIn('bash scripts/check_scalar_owner_entry_ci.sh', self.test_jobs['test-d10'])
        self.assertIn('--manifest scripts/e2_replay_manifest.json', self.test_jobs['test-e2'])
        self.assertIn('--manifest scripts/e2_two_source_unit_manifest.json', self.test_jobs['test-e2'])
        summary = self.workflow.split('\n  ci-passed:\n', 1)[1]
        for job_id in ('test,', 'test-d10,', 'test-e2,'):
            self.assertIn(job_id, summary.split('steps:', 1)[0])
        self.assertIn('run: python3 scripts/check_ci_results.py', summary)

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
