"""CI-only Linux limited-debug profile contracts; do not run Cargo."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]


class CiDebugProfileTests(unittest.TestCase):
    def setUp(self):
        self.workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.job = self.workflow.split('\n  test:\n', 1)[1].split('\n  preview-', 1)[0]

    def test_only_original_two_host_matrix(self):
        self.assertIn('os: [macos-15, ubuntu-26.04]', self.job)
        matrix = self.job.split('      matrix:\n', 1)[1].split('    runs-on:', 1)[0]
        self.assertEqual(1, matrix.count('          - os: '))
        self.assertIn('          - os: ubuntu-26.04\n            container:', matrix)
        self.assertIn('              options: --user 1001:1001', matrix)
        self.assertNotIn('matrix.debug', self.job)
        self.assertIn('name: Targeted Tests (${{ matrix.os }})\n', self.job)
        self.assertNotIn('continue-on-error', self.job)

    def test_profile_is_linux_only_and_scoped_to_tests(self):
        for key in ('CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG'):
            self.assertEqual(self.workflow.count(key), 1)
            self.assertIn(key + ": ${{ matrix.os == 'ubuntu-26.04' && '1' || '2' }}", self.job)
        self.assertNotIn('[profile.', (ROOT / 'Cargo.toml').read_text())
        self.assertNotIn('CARGO_INCREMENTAL:', self.workflow)

    def test_cache_is_stable_separate_from_old_linux_artifacts(self):
        self.assertIn("prefix-key: ${{ runner.os == 'Linux' && 'v1-test-debug1' || 'v0-rust' }}", self.job)
        self.assertNotIn('github.run_id', self.job)
        self.assertNotIn('save-if:', self.job)
        self.assertNotIn('restore-keys:', self.job)

    def test_existing_all_test_gates_remain(self):
        for command in ('bash scripts/check_integration.sh',
                        'python3 scripts/check_word_frequency.py',
                        'python3 scripts/conformance_gate.py',
                        'python3 scripts/check_native_sanitizers.py',
                        'python3 scripts/check_generated_owners.py'):
            self.assertIn(command, self.job)
        summary = self.workflow.split('\n  ci-passed:\n', 1)[1]
        self.assertIn('test,', summary.split('steps:', 1)[0])
        self.assertIn('run: python3 scripts/check_ci_results.py', summary)


if __name__ == '__main__':
    unittest.main()
