"""Keep CI network stalls bounded without retrying package installation itself."""
from pathlib import Path
import re
import shlex
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]


class LlvmInstallBounds(unittest.TestCase):
    def commands(self, prefix):
        script = (ROOT / 'scripts/install_ci_llvm.sh').read_text().replace('\\\n', '')
        return [shlex.split(line) for line in script.splitlines() if line.startswith(prefix)]

    def test_every_setup_call_has_a_total_budget(self):
        calls = 0
        for workflow in (ROOT / '.github/workflows').glob('*.yml'):
            for step in re.split(r'(?m)^      - ', workflow.read_text())[1:]:
                if 'uses: ./.github/actions/setup-llvm' in step:
                    calls += 1
                    budget = re.search(r'(?m)^        timeout-minutes: (\d+)$', step)
                    self.assertIsNotNone(budget, f'unbounded LLVM setup in {workflow}')
                    self.assertGreater(int(budget[1]), 0)
                    self.assertLessEqual(int(budget[1]), 10)
        self.assertGreater(calls, 0)

    def test_key_download_has_bounded_attempts_and_transfer_time(self):
        command, = self.commands('curl ')
        for option, upper in [('--connect-timeout', 15), ('--max-time', 60),
                              ('--retry', 2), ('--retry-max-time', 180)]:
            self.assertIn(option, command)
            value = int(command[command.index(option) + 1])
            self.assertGreater(value, 0)
            self.assertLessEqual(value, upper)

    def test_apt_update_and_install_limit_network_retries(self):
        commands = self.commands('sudo apt-get ')
        self.assertEqual(len(commands), 2)
        for command in commands:
            for setting in ['Acquire::Retries=2', 'Acquire::http::Timeout=30',
                            'Acquire::https::Timeout=30']:
                self.assertIn(setting, command)
        update, = [c for c in commands if 'update' in c]
        self.assertIn('APT::Update::Error-Mode=any', update)

    def test_failed_update_stops_before_install(self):
        script = (ROOT / 'scripts/install_ci_llvm.sh').read_text()
        # Run the real update-to-install control flow with command boundaries mocked.
        start = script.index('sudo apt-get ')
        end = script.index('installed_lzma=')
        probe = script.split('source /etc/os-release')[0]
        probe += 'sudo() { return 100; }\n' + script[start:end]
        result = subprocess.run(['bash', '-c', probe], capture_output=True, text=True)
        self.assertEqual(result.returncode, 100)
        self.assertNotIn('command not found', result.stderr)


if __name__ == '__main__':
    unittest.main()
