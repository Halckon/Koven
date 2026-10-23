from pathlib import Path
import unittest
from unittest.mock import patch
import sys
import tempfile
import json

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import check_api_examples as api

class ApiExamples(unittest.TestCase):
    def test_inventory_rejects_empty_duplicate_planned_and_missing_sources(self):
        good = {'example': 'function', 'file': 'source.ko', 'function': 'greeting'}
        for rows in ([], [good, good], [{'example': 'planned-thread', 'file': 'source.ko', 'function': 'main'}],
                     [{**good, 'file': '../absent.ko'}], [{**good, 'function': ''}]):
            with self.subTest(rows=rows), tempfile.TemporaryDirectory() as temp:
                path = Path(temp) / 'inventory.json'
                path.write_text(json.dumps({'version': 1, 'entries': rows}))
                with self.assertRaises(ValueError):
                    api.load_inventory(path)

    def test_doc_failure_never_runs_examples_or_returns_success(self):
        with patch.object(api, 'extract', side_effect=ValueError('bad declaration')), \
             patch.object(api, 'check_tutorial') as runner:
            with self.assertRaises(ValueError):
                api.check(Path('/missing/cli'))
            runner.assert_not_called()

    def test_unknown_or_ambiguous_symbol_is_not_accepted(self):
        for functions in ([], [{'name': 'greeting'}, {'name': 'greeting'}]):
            with patch.object(api, 'extract', return_value={'functions': functions}), \
                 patch.object(api, 'check_tutorial') as runner:
                with self.assertRaises(ValueError):
                    api.check(Path('/missing/cli'))
                runner.assert_not_called()

    def test_tutorial_failure_propagates_after_all_declarations_resolve(self):
        documents = [{'functions': [{'name': name}], 'compilerVersion': 'test'}
                     for name in ('greeting', 'inspect')]
        with patch.object(api, 'extract', side_effect=documents), \
             patch.object(api, 'check_tutorial', side_effect=AssertionError('wrong stdout')) as runner:
            with self.assertRaisesRegex(AssertionError, 'wrong stdout'):
                api.check(Path('/test/cli'))
            runner.assert_called_once_with(Path('/test/cli'), ['function', 'borrowing'])

if __name__ == '__main__':
    unittest.main()
