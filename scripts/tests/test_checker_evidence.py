"""Ordinary evidence IO contracts; subprocess, mutation and rendering are mocked."""
import difflib
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock
from scripts import check_generated_owner_checker as gate


class CheckerEvidenceTests(unittest.TestCase):
    def exercise(self, root, *, red_exit=0, red_hits=True, cargo_error=None):
        worktree = root / 'fake-worktree'; worktree.mkdir()
        target = worktree / gate.CHECKER_FILE; target.parent.mkdir(parents=True)
        target.write_bytes(b'original\n')
        binary = worktree / 'fake-bin'; binary.write_bytes(b'fake executable, never executed')
        artifacts = root / 'evidence'
        expected = [dict(code='L0131', primary=[10, 15], labels=[[5, 9]])]
        def fake_run(argv, **kwargs):
            if argv[0] == 'cargo':
                if cargo_error: raise cargo_error
                row = dict(reason='compiler-artifact', target=dict(name='lang_codegen'), profile=dict(test=True), executable=str(binary))
                return subprocess.CompletedProcess(argv, 0, json.dumps(row).encode(), b'build stderr\x00')
            if argv[0] == str(binary):
                folder = Path(kwargs['env']['KOVEN_GENERATED_OWNER_CASE'])
                red = target.read_bytes() == b'mutated\n'
                (folder / 'stages.tsv').write_text('parse\t0\nnames\t0\ntypes\t0\nownership\t'+('0' if red else '1')+'\n')
                (folder / 'diagnostics.tsv').write_text('' if red else 'ownership\tL0131\t10\t15\t5:9\n')
                hits = (not red or red_hits)
                output = (f'test {gate.FRONTEND_EXPORT_TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n' if hits else 'test result: ok. 0 passed; 0 failed; 0 ignored;\n').encode()
                return subprocess.CompletedProcess(argv, red_exit if red else 0, output, b'execute stderr\xff')
            if argv[:2] == ['git', 'diff']:
                return subprocess.CompletedProcess(argv, 0, b'', b'')
            if argv[:2] == ['git', 'worktree']:
                return subprocess.CompletedProcess(argv, 0, b'', b'')
            raise AssertionError('unmocked operation: '+str(argv))
        with mock.patch.object(gate.tempfile, 'mkdtemp', return_value=str(worktree)), \
             mock.patch.object(gate.subprocess, 'run', side_effect=fake_run), \
             mock.patch.object(gate, 'mutate_checker', return_value='mutated\n'), \
             mock.patch.object(gate.model, 'cases', return_value=[dict(shape='I1', id='fixture')]), \
             mock.patch.object(gate.model, 'render', return_value=dict(source='fixture source', expected_diagnostics=expected)):
            return gate.verify_checker_mutant(root, artifacts)

    def test_red_green_commands_outputs_results_and_real_patch_preserved(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); report = self.exercise(root); out = root / 'evidence'
            red = out / 'red' / 'case'; green = out / 'green' / 'case'
            self.assertEqual('', (red / 'diagnostics.tsv').read_text())
            self.assertIn('L0131', (green / 'diagnostics.tsv').read_text())
            for phase in ['red', 'green']:
                for operation in ['build', 'export']:
                    folder = out / phase
                    command = json.loads((folder / (operation+'.command.json')).read_text())
                    result = json.loads((folder / (operation+'.result.json')).read_text())
                    self.assertEqual(0, result['exit']); self.assertFalse(result['timed_out'])
                    self.assertTrue(command['argv']); self.assertTrue(command['cwd'])
                    self.assertTrue((folder / (operation+'.stdout')).exists())
                    self.assertTrue((folder / (operation+'.stderr')).exists())
            patch = (out / 'mutant.patch').read_bytes()
            expected = ''.join(difflib.unified_diff(['original\n'], ['mutated\n'], fromfile=gate.CHECKER_FILE, tofile=gate.CHECKER_FILE)).encode()
            self.assertEqual(expected, patch)
            self.assertEqual(hashlib.sha256(patch).hexdigest(), report['patch_sha256'])
            self.assertEqual(report['original_sha256'], report['restored_sha256'])
            self.assertEqual(b'original\n', (out / 'original-source.rs').read_bytes())
            self.assertEqual(b'mutated\n', (out / 'mutated-source.rs').read_bytes())
            self.assertEqual(b'original\n', (out / 'restored-source.rs').read_bytes())

    def test_atomic_evidence_publish_keeps_previous_file_on_replace_failure(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); target = root / 'report.json'; target.write_bytes(b'previous')
            with mock.patch.object(gate.os, 'replace', side_effect=OSError('fixture unavailable')):
                with self.assertRaises(OSError): gate.write_json(target, dict(status='pass'))
            self.assertEqual(b'previous', target.read_bytes())
            self.assertEqual([target], list(root.iterdir()))

    def test_red_failure_or_zero_hits_retains_raw_evidence_and_restoration(self):
        for kwargs in [dict(red_exit=1), dict(red_hits=False)]:
            with self.subTest(kwargs=kwargs), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                with self.assertRaises(gate.Failure): self.exercise(root, **kwargs)
                out = root / 'evidence'
                self.assertTrue((out / 'red' / 'export.result.json').exists())
                self.assertEqual(b'original\n', (out / 'restored-source.rs').read_bytes())
                self.assertFalse((out / 'green' / 'export.result.json').exists())

    def test_timeout_and_spawn_error_are_recorded_without_success(self):
        for error in [subprocess.TimeoutExpired(['cargo'], 900, output=b'partial\x00', stderr=b'error\xff'), OSError('fixture spawn failed')]:
            with self.subTest(error=type(error).__name__), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                with self.assertRaises((gate.Failure, OSError, subprocess.TimeoutExpired)):
                    self.exercise(root, cargo_error=error)
                result = json.loads((root / 'evidence/red/build.result.json').read_text())
                self.assertIsNone(result['exit'])
                self.assertEqual(isinstance(error, subprocess.TimeoutExpired), result['timed_out'])
                self.assertEqual(b'original\n', (root / 'evidence/restored-source.rs').read_bytes())
