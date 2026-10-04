"""A candidate's dependencies must be resolved on the consuming host."""
import importlib.util
from pathlib import Path
import unittest
from unittest import mock
import base64
import subprocess

PATH = Path(__file__).resolve().parents[1] / 'preview_host.py'


class PreviewHostTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location('preview_host', PATH)
        cls.host = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.host)

    def test_missing_elf_dependency_is_not_a_resolved_library(self):
        with self.assertRaises(ValueError):
            self.host.parse_ldd('libLLVM.so.21.1 => not found\n')

    def test_elf_loader_and_transitive_dependencies_are_preserved(self):
        libraries = self.host.parse_ldd(
            'linux-vdso.so.1 (0x001)\n'
            'libLLVM.so.21.1 => /usr/lib/libLLVM.so.21.1 (0x002)\n'
            '/lib64/ld-linux-x86-64.so.2 (0x003)\n')
        self.assertEqual(libraries, {
            'libLLVM.so.21.1': '/usr/lib/libLLVM.so.21.1',
            'ld-linux-x86-64.so.2': '/lib64/ld-linux-x86-64.so.2'})

    def test_compare_checks_actual_dependencies_not_only_llvm_version(self):
        producer = {'system': 'Darwin', 'arch': 'arm64', 'llvm_version': '21.1.8',
                    'libraries': [{'name': 'libzstd', 'sha256': 'abc'}],
                    'c_driver': {'version': 'clang 21'}, 'sdk': '26.5',
                    'minimum_os': '15.0', 'os_version': '26.6'}
        consumer = {**producer, 'os_version': '15.0'}
        self.assertEqual([], self.host.compare_hosts(producer, consumer))
        for mutation in ({'libraries': []}, {'llvm_version': '21.1.9'},
                         {'c_driver': {'version': 'clang 20'}}, {'os_version': '14.0'}):
            with self.subTest(mutation=mutation):
                self.assertTrue(self.host.compare_hosts(producer, {**consumer, **mutation}))

    def test_macho_minimum_is_the_maximum_over_external_load_closure(self):
        output = ('Load command 0\n cmd LC_BUILD_VERSION\n minos 26.0\n sdk 26.2\n'
                  ' ntools 1\n tool LD\n version 1267.0\n'
                  'Load command 1\n cmd LC_VERSION_MIN_MACOSX\n version 15.0\n sdk 15.4\n')
        self.assertEqual(self.host.macho_minimum(output), '26.0')

    def test_trailing_zero_os_versions_have_the_same_requirement(self):
        self.assertEqual(self.host.version_tuple('14.0'), self.host.version_tuple('14.0.0'))

    def test_loader_redirect_environment_cannot_hide_dependency_changes(self):
        with self.assertRaises(ValueError):
            self.host.check_loader_environment({'DYLD_LIBRARY_PATH': '/private/tmp'})
        with self.assertRaises(ValueError):
            self.host.check_loader_environment({'LD_LIBRARY_PATH': '/private/tmp'})

    def test_normalized_search_paths_cannot_escape_allowed_roots(self):
        with self.assertRaises(ValueError):
            self.host.allowed_path('/usr/lib/../../private/tmp', ['/usr/lib', '/lib'])

    def test_actual_dyld_loads_must_match_declared_libraries(self):
        text = 'dyld[1]: <UUID> /opt/homebrew/lib/changed.dylib\n'
        with self.assertRaises(ValueError):
            self.host.verify_dyld_loads(text, Path('/private/tmp/kovenc'),
                                       [{'resolved': '/opt/homebrew/lib/expected.dylib'}])

    def test_failed_inspection_command_preserves_exit_and_original_bytes(self):
        evidence = {'commands': []}
        with mock.patch.object(self.host.subprocess, 'run', return_value=
                               subprocess.CompletedProcess([], 5, b'\xffout', b'\x80err')):
            with self.assertRaises(ValueError):
                self.host.command(['otool'], evidence)
        record = evidence['commands'][0]
        self.assertEqual(record['exit'], 5)
        self.assertEqual(base64.b64decode(record['stdout_base64']), b'\xffout')
        self.assertEqual(base64.b64decode(record['stderr_base64']), b'\x80err')


if __name__ == '__main__':
    unittest.main()
