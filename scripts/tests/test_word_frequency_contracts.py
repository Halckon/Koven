"""Independent oracles, fail-closed evidence, and required two-host wiring."""
import base64
import copy
import importlib.util
import json
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
SPEC = importlib.util.spec_from_file_location('word_frequency', ROOT / 'scripts/check_word_frequency.py')
WORD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(WORD)


class WordFrequencyContracts(unittest.TestCase):
    def test_reference_preserves_first_occurrence_case_unicode_and_control_bytes(self):
        words = ['界', '', 'a\rb', '界', 'é', 'e\u0301', '', 'a\rb', 'A', 'a']
        self.assertEqual(WORD.frequency_bytes(words),
                         '界\t2\n\t2\na\rb\t2\né\t1\né\t1\nA\t1\na\t1\n'.encode())
        self.assertEqual(WORD.frequency_bytes([]), b'')

    def fixture(self):
        row, sources = next(item for item in WORD.load_examples()
                            if item[0]['id'] == 'argv-word-frequency')
        return copy.deepcopy(row), sources

    def test_wrong_manifest_oracle_is_refused_before_any_compiler_command(self):
        row, sources = self.fixture()
        row['cases'][0]['artifact']['stdout'] = 'invented\n'
        with tempfile.TemporaryDirectory() as temporary:
            evidence = Path(temporary) / 'evidence'
            with mock.patch.object(WORD, 'load_examples', return_value=[(row, sources)]), \
                    mock.patch.object(WORD.subprocess, 'run') as execute:
                with self.assertRaises(AssertionError):
                    WORD.check(Path(sys.executable), evidence)
                execute.assert_not_called()
            ledger = json.loads((evidence / 'results.json').read_text())
            self.assertFalse(ledger['success'])
            self.assertEqual(ledger['commands'], [])

    def test_failed_build_keeps_exact_byte_evidence_and_stops_execution(self):
        failed = subprocess.CompletedProcess([], 7, b'partial\r\n', b'error\xff')
        with tempfile.TemporaryDirectory() as temporary:
            evidence = Path(temporary) / 'evidence'
            with mock.patch.object(WORD.subprocess, 'run', return_value=failed) as execute:
                with self.assertRaises(AssertionError):
                    WORD.check(Path(sys.executable), evidence)
                self.assertEqual(execute.call_count, 1)
            ledger = json.loads((evidence / 'results.json').read_text())
            self.assertFalse(ledger['success'])
            command = ledger['commands'][0]
            self.assertEqual(command['exit'], 7)
            self.assertEqual(base64.b64decode(command['stdout_base64']), failed.stdout)
            self.assertEqual(base64.b64decode(command['stderr_base64']), failed.stderr)
            self.assertTrue((Path(command['cwd']) / 'project.toml').is_file())
            for argument, encoded in zip(command['argv'], command['argv_base64']):
                self.assertEqual(base64.b64decode(encoded), argument.encode())

    def test_negative_decimal_cannot_pass_with_a_different_crash_signal(self):
        row, sources = self.fixture()
        row['cases'] = [row['cases'][0]]
        blank = subprocess.CompletedProcess([], 0, b'', b'')
        boundaries = subprocess.CompletedProcess([], 0, b'0\n1\n9\n10\n99\n100\n2147483647\n', b'')
        wrong_crash = subprocess.CompletedProcess([], -signal.SIGSEGV, b'', b'')
        results = [blank, blank, blank, blank, boundaries, boundaries, blank, wrong_crash,
                   FileNotFoundError('must stop before the next command')]
        with tempfile.TemporaryDirectory() as temporary:
            evidence = Path(temporary) / 'evidence'
            with mock.patch.object(WORD, 'load_examples', return_value=[(row, sources)]), \
                    mock.patch.object(WORD.subprocess, 'run', side_effect=results) as execute:
                with self.assertRaises(AssertionError):
                    WORD.check(Path(sys.executable), evidence)
                self.assertEqual(execute.call_count, 8)
            ledger = json.loads((evidence / 'results.json').read_text())
            self.assertFalse(ledger['success'])
            self.assertFalse(ledger['commands'][-1]['success'])
            self.assertEqual(ledger['commands'][-1]['exit'], -signal.SIGSEGV)

    def test_dual_host_acceptance_and_evidence_upload_cannot_be_skipped(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        job = re.search(r'^  test:\n(.*?)(?=^  [a-z-]+:|\Z)', workflow, re.M | re.S)[1]
        self.assertIn('os: [macos-14, ubuntu-24.04]', job)
        acceptance = job.split('      - name: Word frequency byte and boundary contracts\n', 1)[1]
        step = acceptance.split('      - name:', 1)[0]
        self.assertNotIn('if:', step)
        self.assertIn('run: python3 scripts/check_word_frequency.py --evidence', step)
        upload = job.split('      - name: Preserve word frequency evidence\n', 1)[1]
        upload = upload.split('      - name:', 1)[0]
        self.assertIn('if: always()', upload)
        self.assertIn('if-no-files-found: error', upload)
        self.assertIn("- 'scripts/check_word_frequency.py'", workflow.split('            editors:', 1)[0])

    def test_timeout_and_spawn_failure_keep_the_attempted_command(self):
        errors = [subprocess.TimeoutExpired(['compiler'], 120, b'partial\r', b'failed\xff'),
                  FileNotFoundError('compiler disappeared')]
        for error in errors:
            with self.subTest(error=type(error).__name__), tempfile.TemporaryDirectory() as temporary:
                evidence = Path(temporary) / 'evidence'
                with mock.patch.object(WORD.subprocess, 'run', side_effect=error) as execute:
                    with self.assertRaises(type(error)):
                        WORD.check(Path(sys.executable), evidence)
                    self.assertEqual(execute.call_count, 1)
                ledger = json.loads((evidence / 'results.json').read_text())
                self.assertFalse(ledger['success'])
                self.assertEqual(len(ledger['commands']), 1)
                command = ledger['commands'][0]
                self.assertIn('build', command['argv'])
                self.assertIsNone(command['exit'])
                self.assertFalse(command['success'])
                if isinstance(error, subprocess.TimeoutExpired):
                    self.assertTrue(command['timed_out'])
                    self.assertEqual(base64.b64decode(command['stdout_base64']), b'partial\r')
                    self.assertEqual(base64.b64decode(command['stderr_base64']), b'failed\xff')
                else:
                    self.assertIn('compiler disappeared', command['process_error'])


if __name__ == '__main__':
    unittest.main()
