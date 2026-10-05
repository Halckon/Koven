"""Mock package protocol checks; these do not prove native installation."""
import base64
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


TUTORIAL = module('tutorial_install_test', ROOT / 'scripts/check_tutorial.py')


class PreviewInstallProtocol(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.work = self.root / 'work parent'
        self.work.mkdir()
        self.sibling = self.work / 'user-sentinel.txt'
        self.sibling.write_bytes(b'user data untouched')
        self.evidence = self.root / 'evidence'
        self.consumer = module('preview_install_test', ROOT / 'scripts/check_preview_install.py')
        self.row, self.sources = next(item for item in TUTORIAL.load_examples()
                                      if item[0]['id'] == 'parameter-report')

    def package(self, mutate=None, extra_members=()):
        files = {
            'bin/kovenc': (b'mock binary', 0o755),
            'README.md': (b'mock documentation', 0o644),
            'licenses/NOTICE': (b'mock licenses', 0o644),
            'tools/check_tutorial.py': ((ROOT / 'scripts/check_tutorial.py').read_bytes(), 0o644),
            'tools/check_preview_install.py': ((ROOT / 'scripts/check_preview_install.py').read_bytes(), 0o644),
            'tools/preview_host.py': (b'def inspect_host(binary, evidence=None):\n return {"mock": True}\n'
                                      b'def compare_hosts(producer, consumer):\n return []\n', 0o644),
            'tools/install_preview_dependencies.sh': (b'#!/bin/sh\nexit 0\n', 0o755),
            'tools/install_ci_llvm.sh': (b'#!/bin/sh\nexit 0\n', 0o755),
            'examples/parameter-report/contract.json': (json.dumps(self.row).encode(), 0o644),
            'examples/parameter-report/project.toml': (b'schema = "koven.project"\nversion = 1\n', 0o644),
        }
        for path, source in self.sources.items():
            files['examples/parameter-report/' + path] = (source.encode(), 0o644)
        manifest = {'schema': 'koven.preview.v1', 'source_commit': 'a' * 40,
                    'source_tree': 'b' * 40, 'lock_sha256': 'c' * 64,
                    'rustc': 'rustc 1.96.0\nrelease: 1.96.0\nhost: aarch64-apple-darwin',
                    'cargo': 'cargo 1.96.0',
                    'build_argv': ['cargo', 'build', '--locked', '--release', '-p', 'lang-cli',
                                   '--bin', 'kovenc', '--target', 'aarch64-apple-darwin', '--message-format=json'],
                    'binary_sha256': hashlib.sha256(files['bin/kovenc'][0]).hexdigest(),
                    'target': 'aarch64-apple-darwin', 'profile': 'release',
                    'host': {'system': 'Darwin', 'arch': 'arm64'},
                    'files': {path: {'sha256': hashlib.sha256(data).hexdigest(),
                                     'size': len(data), 'mode': mode}
                              for path, (data, mode) in files.items()}}
        if mutate:
            mutate(files, manifest)
        archive = self.root / 'candidate.tar.gz'
        with tarfile.open(archive, 'w:gz') as tar:
            for path, (data, mode) in {**files, 'manifest.json':
                                      (json.dumps(manifest).encode(), 0o644)}.items():
                info = tarfile.TarInfo('koven-preview/' + path)
                info.size, info.mode = len(data), mode
                tar.addfile(info, io.BytesIO(data))
            for info, data in extra_members:
                tar.addfile(info, io.BytesIO(data) if data is not None else None)
        checksum = self.root / 'candidate.tar.gz.sha256'
        checksum.write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '  ' + archive.name + '\n')
        return archive, checksum

    def install(self, archive, checksum, **kwargs):
        return self.consumer.check(archive, checksum, self.evidence,
                                   work_parent=self.work, **kwargs)

    def results(self):
        return json.loads((self.evidence / 'results.json').read_text())

    def mock_execute(self, command, **kwargs):
        self.assertFalse(kwargs.get('text', False), 'consumer must compare raw bytes')
        if command[0] == 'bash':
            return subprocess.CompletedProcess(command, 0, b'dependency install log\n', b'')
        if command[1:2] == ['build']:
            expected = self.row['build']
        else:
            argv = command[command.index('--') + 1:] if '--' in command else command[1:]
            case = next(case for case in self.row['cases'] if case['args'] == argv)
            expected = case['run'] if '--' in command else case['artifact']
        return subprocess.CompletedProcess(command, expected['exit'],
                                           expected['stdout'].encode(), expected['stderr'].encode())

    def test_four_independent_cases_execute_twelve_commands_and_clean_only_owned_paths(self):
        archive, checksum = self.package()
        with mock.patch.object(self.consumer.subprocess, 'run', side_effect=self.mock_execute):
            self.assertEqual(self.install(archive, checksum), 0)
        results = self.results()
        self.assertTrue(results['success'])
        self.assertEqual(len(results['commands']), 12)
        self.assertEqual(len({row['cwd'] for row in results['commands']}), 4)
        self.assertIn('中文 空格', results['install_path'])
        self.assertEqual(results['commands'][-1]['argv'][-1], '')
        self.assertTrue(results['sentinel']['unchanged'])
        self.assertTrue(results['cleanup']['removed'])
        self.assertFalse(Path(results['owned_root']).exists())
        self.assertEqual(self.sibling.read_bytes(), b'user data untouched')
        self.assertEqual(list(self.work.iterdir()), [self.sibling])

    def test_corrupt_checksum_fails_without_running_package_tools(self):
        archive, checksum = self.package()
        checksum.write_text('0' * 64 + '  ' + archive.name + '\n')
        with mock.patch.object(self.consumer.subprocess, 'run') as execute:
            self.assertEqual(self.install(archive, checksum), 1)
            self.assertEqual(execute.call_count, 0)
        self.assertFalse(self.results()['success'])
        self.assertEqual(self.results()['commands'], [])

    def test_missing_extra_tampered_and_bootstrap_mismatch_files_fail_before_execution(self):
        mutations = [
            lambda files, manifest: files.pop('README.md'),
            lambda files, manifest: files.update({'undeclared.txt': (b'extra', 0o644)}),
            lambda files, manifest: files.update({'bin/kovenc': (b'changed', 0o755)}),
            lambda files, manifest: manifest['files'].pop('README.md'),
            lambda files, manifest: files.update({'tools/check_preview_install.py': (b'pass\n', 0o644)}),
            lambda files, manifest: manifest['files']['bin/kovenc'].update(mode=0o644),
        ]
        for mutate in mutations:
            with self.subTest(mutate=mutate):
                archive, checksum = self.package(mutate)
                with mock.patch.object(self.consumer.subprocess, 'run') as execute:
                    self.assertEqual(self.install(archive, checksum), 1)
                    self.assertEqual(execute.call_count, 0)
                self.assertFalse(self.results()['success'])

    def test_valid_manifest_hash_cannot_substitute_another_bootstrap(self):
        def mutate(files, manifest):
            path, data = 'tools/check_preview_install.py', b'pass\n'
            files[path] = (data, 0o644)
            manifest['files'][path].update(sha256=hashlib.sha256(data).hexdigest(), size=len(data))
        archive, checksum = self.package(mutate)
        with mock.patch.object(self.consumer.subprocess, 'run') as execute:
            self.assertEqual(self.install(archive, checksum), 1)
            self.assertEqual(execute.call_count, 0)
        self.assertIn('consumer bootstrap differs', self.results()['failure'])

    def test_nonexecutable_cli_is_rejected_even_if_manifest_and_archive_agree(self):
        def mutate(files, manifest):
            data, _ = files['bin/kovenc']
            files['bin/kovenc'] = (data, 0o644)
            manifest['files']['bin/kovenc']['mode'] = 0o644
        archive, checksum = self.package(mutate)
        with mock.patch.object(self.consumer.subprocess, 'run') as execute:
            self.assertEqual(self.install(archive, checksum), 1)
            self.assertEqual(execute.call_count, 0)
        self.assertIn('CLI is not executable', self.results()['failure'])

    def test_links_traversal_duplicate_and_extra_directories_are_rejected(self):
        for name, kind in [('koven-preview/../escape', tarfile.REGTYPE),
                           ('/absolute', tarfile.REGTYPE),
                           ('koven-preview/bin/link', tarfile.SYMTYPE),
                           ('koven-preview/bin/hard', tarfile.LNKTYPE),
                           ('koven-preview/bin/kovenc', tarfile.REGTYPE),
                           ('koven-preview/extra', tarfile.DIRTYPE)]:
            with self.subTest(name=name):
                info = tarfile.TarInfo(name)
                info.type = kind
                info.linkname = 'kovenc' if kind in (tarfile.SYMTYPE, tarfile.LNKTYPE) else ''
                archive, checksum = self.package(extra_members=[(info, b'' if kind == tarfile.REGTYPE else None)])
                with mock.patch.object(self.consumer.subprocess, 'run') as execute:
                    self.assertEqual(self.install(archive, checksum), 1)
                    self.assertEqual(execute.call_count, 0)

    def test_missing_cases_or_oracles_cannot_be_zero_command_success(self):
        for mutation in (lambda row: row.update(cases=[]),
                         lambda row: row.update(cases=row['cases'][:3]),
                         lambda row: row['cases'][0].pop('artifact'),
                         lambda row: row['cases'][0]['artifact'].pop('stderr')):
            with self.subTest(mutation=mutation):
                def mutate(files, manifest):
                    row = json.loads(json.dumps(self.row))
                    mutation(row)
                    path = 'examples/parameter-report/contract.json'
                    data = json.dumps(row).encode()
                    files[path] = (data, 0o644)
                    manifest['files'][path].update(sha256=hashlib.sha256(data).hexdigest(), size=len(data))
                archive, checksum = self.package(mutate)
                with mock.patch.object(self.consumer.subprocess, 'run') as execute:
                    self.assertEqual(self.install(archive, checksum), 1)
                    self.assertEqual(execute.call_count, 0)
                self.assertEqual(self.results()['commands'], [])

    def test_timeout_retains_original_bytes_command_and_owned_directory(self):
        archive, checksum = self.package()
        with mock.patch.object(self.consumer.subprocess, 'run',
                               side_effect=subprocess.TimeoutExpired(['kovenc'], 120,
                                                                     output=b'\xffpartial\r\n', stderr=b'\x80err')):
            self.assertEqual(self.install(archive, checksum), 1)
        results = self.results()
        command = results['commands'][0]
        self.assertTrue(command['timed_out'])
        self.assertIsNone(command['exit'])
        self.assertEqual(base64.b64decode(command['stdout_base64']), b'\xffpartial\r\n')
        self.assertEqual(base64.b64decode(command['stderr_base64']), b'\x80err')
        self.assertIn('env', command)
        self.assertGreaterEqual(command['elapsed_seconds'], 0)
        self.assertTrue(Path(results['owned_root']).exists())
        self.assertTrue(results['sentinel']['unchanged'])

    def test_unavailable_command_is_a_recorded_failure(self):
        archive, checksum = self.package()
        with mock.patch.object(self.consumer.subprocess, 'run', side_effect=FileNotFoundError('missing executable')):
            self.assertEqual(self.install(archive, checksum), 1)
        command = self.results()['commands'][0]
        self.assertIsNone(command['exit'])
        self.assertFalse(command['success'])
        self.assertIn('could not start', command['failure'])

    def test_newline_normalization_and_nonzero_exit_are_not_accepted(self):
        archive, checksum = self.package()
        for output in (subprocess.CompletedProcess([], 0, b'\r\n', b''),
                       subprocess.CompletedProcess([], 1, b'', b''),
                       subprocess.CompletedProcess([], 0, b'', b'warning')):
            with self.subTest(output=output):
                with mock.patch.object(self.consumer.subprocess, 'run', return_value=output):
                    self.assertEqual(self.install(archive, checksum), 1)
                self.assertFalse(self.results()['success'])

    def test_host_mismatch_prevents_smoke_and_records_host(self):
        def mutate(files, manifest):
            path = 'tools/preview_host.py'
            data = (b'def inspect_host(binary, evidence=None):\n return {"mock": True}\n'
                    b'def compare_hosts(producer, consumer):\n return ["LLVM mismatch"]\n')
            files[path] = (data, 0o644)
            manifest['files'][path].update(sha256=hashlib.sha256(data).hexdigest(), size=len(data))
        archive, checksum = self.package(mutate)
        with mock.patch.object(self.consumer.subprocess, 'run') as execute:
            self.assertEqual(self.install(archive, checksum), 1)
            self.assertEqual(execute.call_count, 0)
        self.assertEqual(self.results()['host'], {'mock': True})
        self.assertIn('LLVM mismatch', self.results()['failure'])

    def test_target_and_declared_host_must_agree_before_package_execution(self):
        for target in ('mock-target', 'x86_64-unknown-linux-gnu'):
            archive, checksum = self.package(lambda files, manifest: manifest.update(target=target))
            with mock.patch.object(self.consumer.subprocess, 'run') as execute:
                self.assertEqual(self.install(archive, checksum), 1)
                self.assertEqual(execute.call_count, 0)
            self.assertIn('target', self.results()['failure'])

    def test_missing_source_or_toolchain_identity_is_not_an_acceptable_candidate(self):
        for field in ('source_tree', 'lock_sha256', 'rustc', 'cargo', 'build_argv', 'binary_sha256'):
            with self.subTest(field=field):
                archive, checksum = self.package(lambda files, manifest: manifest.pop(field))
                with mock.patch.object(self.consumer.subprocess, 'run') as execute:
                    self.assertEqual(self.install(archive, checksum), 1)
                    self.assertEqual(execute.call_count, 0)

    def test_host_inspection_failure_preserves_partial_raw_command_evidence(self):
        host = mock.Mock()
        error = ValueError('loaded library differs')
        error.evidence = {'commands': [{'exit': 5, 'stdout_base64': 'b3V0', 'stderr_base64': 'ZXJy'}],
                          'partial': {'llvm_version': '21.1.8'}}
        host.inspect_host.side_effect = error
        archive, checksum = self.package()
        with mock.patch.object(self.consumer, 'import_tool', return_value=host):
            self.assertEqual(self.install(archive, checksum), 1)
        self.assertEqual(self.results()['host_failure'], error.evidence)
        self.assertEqual(self.results()['commands'], [])

    def test_special_files_are_not_regular_package_files(self):
        for kind in (tarfile.FIFOTYPE, tarfile.CHRTYPE, tarfile.BLKTYPE, tarfile.CONTTYPE):
            with self.subTest(kind=kind):
                info = tarfile.TarInfo('koven-preview/licenses/special')
                info.type = kind
                archive, checksum = self.package(extra_members=[(info, b'' if kind == tarfile.CONTTYPE else None)])
                with mock.patch.object(self.consumer.subprocess, 'run') as execute:
                    self.assertEqual(self.install(archive, checksum), 1)
                    self.assertEqual(execute.call_count, 0)
                self.assertIn('unsupported archive member', self.results()['failure'])

    def test_cleanup_cannot_succeed_if_sibling_sentinel_was_changed(self):
        archive, checksum = self.package()
        def execute(command, **kwargs):
            result = self.mock_execute(command, **kwargs)
            if '--' in command and command[-1] == '':
                (kwargs['cwd'].parent / '同级 哨兵.txt').write_bytes(b'changed')
            return result
        with mock.patch.object(self.consumer.subprocess, 'run', side_effect=execute):
            self.assertEqual(self.install(archive, checksum), 1)
        self.assertEqual(len(self.results()['commands']), 12)
        self.assertFalse(self.results()['sentinel']['unchanged'])
        self.assertFalse(self.results()['success'])
        self.assertEqual(self.sibling.read_bytes(), b'user data untouched')

    def test_sentinel_directory_error_does_not_prevent_failure_evidence(self):
        archive, checksum = self.package()
        def execute(command, **kwargs):
            result = self.mock_execute(command, **kwargs)
            if '--' in command and command[-1] == '':
                sentinel = kwargs['cwd'].parent / '同级 哨兵.txt'
                sentinel.unlink()
                sentinel.mkdir()
            return result
        with mock.patch.object(self.consumer.subprocess, 'run', side_effect=execute):
            self.assertEqual(self.install(archive, checksum), 1)
        self.assertFalse(self.results()['success'])
        self.assertEqual(len(self.results()['commands']), 12)
        self.assertIn('sentinel', self.results()['failure'])

    def test_dependency_preparation_is_recorded_and_failure_prevents_smoke(self):
        archive, checksum = self.package()
        with mock.patch.object(self.consumer.subprocess, 'run', side_effect=self.mock_execute):
            self.assertEqual(self.install(archive, checksum, prepare_dependencies=True), 0)
        self.assertEqual(len(self.results()['commands']), 12)
        self.assertEqual(len(self.results()['preparation']), 1)
        with mock.patch.object(self.consumer.subprocess, 'run',
                               return_value=subprocess.CompletedProcess([], 1, b'', b'failed')):
            self.assertEqual(self.install(archive, checksum, prepare_dependencies=True), 1)
        self.assertEqual(self.results()['commands'], [])


if __name__ == '__main__':
    unittest.main()
