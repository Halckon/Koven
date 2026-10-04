"""Tutorial source authority, project extraction and selection contracts."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('tutorial', ROOT / 'scripts/check_tutorial.py')
TUTORIAL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TUTORIAL)


class TutorialContracts(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / 'docs/tutorials').mkdir(parents=True)
        self.manifest = json.loads((ROOT / 'docs/tutorials/examples.json').read_text())
        self.text = (ROOT / 'docs/tutorials/koven-tour.md').read_text()

    def write(self):
        (self.root / 'docs/tutorials/examples.json').write_text(json.dumps(self.manifest))
        (self.root / 'docs/tutorials/koven-tour.md').write_text(self.text)

    def test_project_sources_are_read_from_each_markdown_fence(self):
        self.write()
        rows = dict((row['id'], sources) for row, sources in TUTORIAL.load_examples(self.root))
        project = rows['cross-file']
        self.assertEqual({'src/app/Main.ko', 'src/app/Values.ko'}, set(project))
        self.assertIn('fun start()', project['src/app/Main.ko'])
        self.assertIn('fun message()', project['src/app/Values.ko'])

    def test_missing_duplicate_and_unreferenced_sources_are_rejected(self):
        original = self.text
        for text in (original.replace('```koven hello', '```koven absent', 1),
                     original + '\n```koven hello\nfun main(): Unit {}\n```\n',
                     original + '\n```koven unused\nfun main(): Unit {}\n```\n'):
            with self.subTest(text=text[-70:]):
                self.text = text
                self.write()
                with self.assertRaises(ValueError):
                    TUTORIAL.load_examples(self.root)

    def test_project_paths_cannot_escape_the_temporary_directory(self):
        row = next(row for row in self.manifest['examples'] if row['id'] == 'cross-file')
        for path in ('../escape.ko', '/absolute.ko', 'src/../escape.ko', 'src/data.txt'):
            with self.subTest(path=path):
                row['files'] = {path: 'cross-file', 'src/app/Values.ko': 'cross-file-values'}
                self.write()
                with self.assertRaises(ValueError):
                    TUTORIAL.load_examples(self.root)

    def test_unknown_or_duplicate_selection_fails_before_cli_execution(self):
        for selected in (['absent'], ['hello', 'hello']):
            with self.subTest(selected=selected):
                with self.assertRaises(ValueError):
                    TUTORIAL.check(Path('/no-cli-must-be-started'), selected)

    def test_duplicate_project_fence_reference_is_rejected(self):
        row = next(row for row in self.manifest['examples'] if row['id'] == 'cross-file')
        row['files']['src/app/Values.ko'] = 'cross-file'
        self.write()
        with self.assertRaises(ValueError):
            TUTORIAL.load_examples(self.root)

    def test_unknown_status_and_duplicate_example_id_are_rejected(self):
        original = self.manifest['examples'][0]['status']
        self.manifest['examples'][0]['status'] = 'unchecked'
        self.write()
        with self.assertRaises(ValueError):
            TUTORIAL.load_examples(self.root)
        self.manifest['examples'][0]['status'] = original
        self.manifest['examples'].append(self.manifest['examples'][0])
        self.write()
        with self.assertRaises(ValueError):
            TUTORIAL.load_examples(self.root)


if __name__ == '__main__':
    unittest.main()
