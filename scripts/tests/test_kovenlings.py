"""Exercises must reject vacuous inventories and validate learner behavior."""
import json
import shutil
import tempfile
from unittest import mock
import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import check_kovenlings as exercises


class KovenlingsContracts(unittest.TestCase):
    def test_real_inventory_has_failing_starters_and_executable_answers(self):
        rows = exercises.load_exercises()
        self.assertEqual([row['id'] for row in rows], ['mutable-place', 'borrow-owner'])
        for row in rows:
            self.assertTrue(row['hint'].strip())
            self.assertEqual(row['starter']['status'], 'diagnostic')
            self.assertEqual(row['starter']['build']['exit'], 1)
            self.assertEqual(row['solution']['status'], 'executable')
            self.assertEqual(row['solution']['artifact']['exit'], 0)

    def test_inventory_rejects_empty_duplicates_escape_and_missing_contracts(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / 'exercises'
            shutil.copytree(exercises.EXERCISES, root)
            original = json.loads((root / 'exercises.json').read_text())
            invalid = []
            for kind in ('empty', 'duplicate', 'escape', 'hint', 'output', 'no-error'):
                data = json.loads(json.dumps(original))
                row = data['exercises'][0]
                if kind == 'empty': data['exercises'] = []
                elif kind == 'duplicate': data['exercises'].append(row)
                elif kind == 'escape': row['id'] = '../outside'
                elif kind == 'hint': row['hint'] = ''
                elif kind == 'output': del row['solution']['run']['stderr']
                elif kind == 'no-error': row['starter']['build']['stderr'] = ''
                invalid.append((kind, data))
            for kind, data in invalid:
                with self.subTest(kind=kind):
                    (root / 'exercises.json').write_text(json.dumps(data))
                    with self.assertRaises(ValueError): exercises.load_exercises(root)
            (root / 'exercises.json').write_text(json.dumps(original))
            (root / 'mutable-place/solution.ko').unlink()
            with self.assertRaises(ValueError): exercises.load_exercises(root)

    def test_audit_reuses_full_cli_oracles_and_cleans_temporary_sources(self):
        calls = []
        def record(cli, row, case, directory):
            calls.append((row['status'], directory, (directory / 'source.ko').read_text()))
        with mock.patch.object(exercises, 'run_case', side_effect=record):
            exercises.check(Path('/unused-cli'))
        self.assertEqual([call[0] for call in calls], ['diagnostic', 'executable'] * 2)
        self.assertIn('val number', calls[0][2])
        self.assertIn('var number', calls[1][2])
        self.assertTrue(all(not call[1].exists() for call in calls))

    def test_learner_copy_uses_answer_contract_and_is_never_modified(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / 'edited.ko'
            source = 'fun main(): Unit { println("replaced") }\n'
            path.write_text(source)
            def verify(cli, row, case, directory):
                self.assertEqual(row['status'], 'executable')
                self.assertEqual(row['artifact']['stdout'], 'replaced\n')
                self.assertEqual((directory / 'source.ko').read_text(), source)
            with mock.patch.object(exercises, 'run_case', side_effect=verify) as run:
                exercises.check(Path('/unused-cli'), 'mutable-place', path)
            self.assertEqual(run.call_count, 1)
            self.assertEqual(path.read_text(), source)

    def test_unknown_selection_and_source_without_selection_do_not_run(self):
        with mock.patch.object(exercises, 'run_case') as run:
            with self.assertRaises(ValueError): exercises.check(Path('/unused'), 'missing')
            with self.assertRaises(ValueError): exercises.check(Path('/unused'), source=Path('missing'))
            run.assert_not_called()

    def test_failed_oracle_propagates_and_still_cleans_source(self):
        directories = []
        def fail(cli, row, case, directory):
            directories.append(directory)
            raise AssertionError('incorrect learner result')
        with mock.patch.object(exercises, 'run_case', side_effect=fail):
            with self.assertRaisesRegex(AssertionError, 'incorrect learner result'):
                exercises.check(Path('/unused-cli'))
        self.assertEqual(len(directories), 1)
        self.assertFalse(directories[0].exists())


if __name__ == '__main__':
    unittest.main()
