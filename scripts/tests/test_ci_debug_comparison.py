"""Static contracts for a cold, CI-only Rust debug comparison; never run Cargo."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]


class DebugComparisonTests(unittest.TestCase):
    def setUp(self):
        self.workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.job = self.workflow.split('\n  test:\n', 1)[1].split('\n  preview-', 1)[0]

    def test_default_hosts_and_additional_linux_arm(self):
        self.assertIn("os: [macos-15, ubuntu-26.04]\n        debug: ['2']\n        include:\n          - os: ubuntu-26.04\n            debug: '1'", self.job)
        self.assertIn("matrix.debug == '1' && ' debug=1 experiment' || ''", self.job)
        self.assertIn('fail-fast: false', self.job)
        self.assertNotIn('continue-on-error', self.job)

    def test_profile_overrides_are_only_in_test_job(self):
        for key in ('CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG'):
            self.assertEqual(self.workflow.count(key), 1)
            self.assertIn(key + ': ${{ matrix.debug }}', self.job)
        self.assertNotIn('CARGO_INCREMENTAL:', self.workflow)
        self.assertNotIn('[profile.', (ROOT / 'Cargo.toml').read_text())

    def test_linux_cache_has_unique_run_attempt_and_arm_prefix(self):
        self.assertIn("format('debug-comparison-{0}-{1}-{2}', github.run_id, github.run_attempt, matrix.debug)", self.job)
        self.assertIn("prefix-key: ${{ runner.os == 'Linux'", self.job)
        self.assertIn("save-if: ${{ runner.os != 'Linux' }}", self.job)
        self.assertNotIn('restore-keys:', self.job)

    def test_all_matrix_evidence_names_are_distinct(self):
        for prefix in ('word-frequency', 'conformance', 'native-sanitizers', 'generated-owners'):
            self.assertIn('name: ' + prefix + '-${{ runner.os }}-${{ github.sha }}-debug-${{ matrix.debug }}', self.job)

    def test_existing_gates_and_disk_sampling_remain(self):
        for command in ('bash scripts/check_integration.sh',
                        'python3 scripts/check_word_frequency.py',
                        'python3 scripts/conformance_gate.py',
                        'python3 scripts/check_native_sanitizers.py',
                        'python3 scripts/check_generated_owners.py'):
            self.assertIn(command, self.job)
        self.assertIn('python3 scripts/ci_disk_usage.py before-setup', self.job)
        self.assertIn('python3 scripts/ci_disk_usage.py after-composition', self.job)
        summary = self.workflow.split('\n  ci-passed:\n', 1)[1]
        self.assertIn('test,', summary.split('steps:', 1)[0])
        self.assertIn('run: python3 scripts/check_ci_results.py', summary)


if __name__ == '__main__':
    unittest.main()
