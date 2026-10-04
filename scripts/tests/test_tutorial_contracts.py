"""Tutorial source authority, project extraction and selection contracts."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('tutorial', ROOT / 'scripts/check_tutorial.py')
TUTORIAL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TUTORIAL)


class TutorialContracts(unittest.TestCase):
    def test_fixed_branch_resource_case_executes_full_public_cli_contract(self):
        examples = {row['id']: row for row, _ in TUTORIAL.load_examples()}
        self.assertEqual(examples['gap-scope-branch']['status'], 'executable')
        self.assertEqual(examples['planned-thread']['status'], 'planned')
        calls = []

        def record(command, expected, directory):
            source = (directory / 'source.ko').read_text()
            self.assertIn('if (true) {', source)
            self.assertIn('val first = Resource("first")', source)
            self.assertIn('val second = Resource("second")', source)
            calls.append((command, expected, directory))

        with mock.patch.object(TUTORIAL, 'assert_output', side_effect=record):
            TUTORIAL.check(Path('/test-cli'), ['planned-thread', 'gap-scope-branch'])
        self.assertEqual(len(calls), 3, 'the repaired case must build, execute artifact, and run')
        self.assertEqual(calls[0][0], ['/test-cli', 'build', 'source.ko', '-o', 'program'])
        self.assertEqual(calls[0][1], {'exit': 0, 'stdout': '', 'stderr': ''})
        self.assertEqual(calls[1][0], [str(calls[1][2] / 'program')])
        self.assertEqual(calls[2][0], ['/test-cli', 'run', 'source.ko', '--'])
        expected = {'exit': 0, 'stdout': 'inner\nsecond\nfirst\nafter\nouter\n', 'stderr': ''}
        self.assertEqual(calls[1][1], expected)
        self.assertEqual(calls[2][1], expected)

    def test_new_combinations_extract_canonical_sources_and_execute_each_contract(self):
        selected = ['numbers-bitwise', 'scope-cleanup', 'unit-loop-cleanup',
                    'reject-immutable-place', 'reject-iteration-move']
        calls = []

        def record(command, expected, directory):
            files = set(path.relative_to(directory).as_posix() for path in directory.rglob('*.ko'))
            if '--project' in command or len(files) == 2:
                self.assertEqual(files, {'src/worker/Work.ko', 'src/app/Main.ko'})
                self.assertIn('for (item in mutableListOf(', (directory / 'src/worker/Work.ko').read_text())
                self.assertIn('worker.work(2)', (directory / 'src/app/Main.ko').read_text())
            else:
                self.assertEqual(files, {'source.ko'})
            calls.append((command, expected, directory))

        with mock.patch.object(TUTORIAL, 'assert_output', side_effect=record):
            TUTORIAL.check(Path('/test-cli'), selected)
        self.assertEqual(len(calls), 11, 'three positive triples and two JSON diagnostic builds')
        for index in range(3):
            build, artifact, run = calls[index * 3:index * 3 + 3]
            inputs = (['--project', 'project.toml', '--entry', 'app.main']
                      if index == 2 else ['source.ko'])
            self.assertEqual(build[0], ['/test-cli', 'build', *inputs, '-o', 'program'])
            self.assertEqual(artifact[0], [str(artifact[2] / 'program')])
            self.assertEqual(run[0], ['/test-cli', 'run', *inputs, '--'])
            self.assertEqual(artifact[1], run[1])
        self.assertEqual(calls[1][1]['stdout'], 'literals\nbits\n')
        self.assertEqual(calls[4][1]['stdout'], 'inner\nsecond\nfirst\nafter\nouter\n')
        for command, expected, _ in calls[9:]:
            self.assertEqual(command, ['/test-cli', '--message-format=json', 'build',
                                       'source.ko', '-o', 'program'])
            self.assertEqual(expected['exit'], 2)

    def test_parameter_report_executes_all_four_cases_from_one_source_set(self):
        calls = []

        def record(command, expected, directory):
            sources = set(path.relative_to(directory).as_posix() for path in directory.rglob('*.ko'))
            self.assertEqual(sources, {'src/app/model.ko', 'src/app/processor.ko', 'src/app/main.ko'})
            self.assertIn('for (argument in args)', (directory / 'src/app/processor.ko').read_text())
            calls.append((command, expected, directory))

        with mock.patch.object(TUTORIAL, 'assert_output', side_effect=record):
            TUTORIAL.check(Path('/test-cli'), ['parameter-report'])
        cases = [([], 'processed\ndone\n'), (['alpha'], 'alpha\nprocessed\ndone\n'),
                 (['alpha', '你好', 'tail'], 'alpha\n你好\ntail\nprocessed\ndone\n'),
                 ([''], '\nprocessed\ndone\n')]
        self.assertEqual(len(calls), 12, 'each argv case must build, execute artifact, and run')
        self.assertEqual(len({directory for _, _, directory in calls}), 4)
        for index, (args, stdout) in enumerate(cases):
            build, artifact, run = calls[index * 3:index * 3 + 3]
            inputs = ['--project', 'project.toml', '--entry', 'app.main']
            self.assertEqual(build[0], ['/test-cli', 'build', *inputs, '-o', 'program'])
            self.assertEqual(build[1], {'exit': 0, 'stdout': '', 'stderr': ''})
            self.assertEqual(artifact[0], [str(artifact[2] / 'program'), *args])
            self.assertEqual(run[0], ['/test-cli', 'run', *inputs, '--', *args])
            self.assertEqual(artifact[1], {'exit': 0, 'stdout': stdout, 'stderr': ''})
            self.assertEqual(run[1], artifact[1])

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

    def test_argv_cases_cannot_silently_omit_or_duplicate_execution(self):
        row = next(row for row in self.manifest['examples'] if row['id'] == 'parameter-report')
        valid = row['cases']
        for cases in ([], {}, [valid[0], valid[0]], [{**valid[0], 'args': 'alpha'}],
                      [{**valid[0], 'args': [1]}], [{'args': []}]):
            with self.subTest(cases=cases):
                row['cases'] = cases
                self.write()
                with self.assertRaises(ValueError):
                    TUTORIAL.load_examples(self.root)
        row['cases'] = valid
        row['args'] = []
        self.write()
        with self.assertRaises(ValueError):
            TUTORIAL.load_examples(self.root)

    def test_legacy_single_project_and_diagnostic_keep_their_cli_commands(self):
        with mock.patch.object(TUTORIAL, 'assert_output') as execute:
            TUTORIAL.check(Path('/test-cli'), ['hello', 'cross-file', 'reject-typed'])
        commands = [call.args[0] for call in execute.call_args_list]
        self.assertEqual(len(commands), 7)
        self.assertEqual(commands[0], ['/test-cli', 'build', 'source.ko', '-o', 'program'])
        self.assertEqual(commands[2], ['/test-cli', 'run', 'source.ko', '--'])
        project = ['--project', 'project.toml', '--entry', 'app.start']
        self.assertEqual(commands[3], ['/test-cli', 'build', *project, '-o', 'program'])
        self.assertEqual(commands[5], ['/test-cli', 'run', *project, '--'])
        self.assertEqual(commands[6], ['/test-cli', '--message-format=json', 'build', 'source.ko', '-o', 'program'])

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
