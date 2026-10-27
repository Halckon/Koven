"""Synthetic libz hash drift; no claim of measured ABI or package provenance."""
import copy
import importlib.util
from pathlib import Path
import unittest


class LibzCompatibilityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = Path(__file__).resolve().parents[1] / 'preview_host.py'
        spec = importlib.util.spec_from_file_location('libz_host', path)
        cls.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.module)

    def setUp(self):
        self.producer = {
            'system': 'Linux', 'arch': 'x86_64', 'os_version': '24.04',
            'llvm_version': '21.1.8', 'llvm_prefix': '/usr/lib/llvm-21',
            'c_driver': {'path': '/usr/bin/cc', 'version': 'cc 13'},
            'glibc': 'glibc 2.39',
            'libraries': [
                {'name': name, 'resolved': '/usr/lib/x86_64-linux-gnu/' + filename,
                 'sha256': 'a' * 64}
                for name, filename in [('libz.so.1', 'libz.so.1.3'),
                                       ('libzstd.so.1', 'libzstd.so.1.5.5'),
                                       ('libLLVM.so.21.1', 'libLLVM.so.21.1'),
                                       ('libc.so.6', 'libc.so.6')]],
        }
        self.consumer = copy.deepcopy(self.producer)
        self.consumer['libraries'][0]['sha256'] = 'b' * 64

    def test_hash_drift_both_directions_preserves_measured_evidence(self):
        original = copy.deepcopy((self.producer, self.consumer))
        self.assertEqual([], self.module.compare_hosts(self.producer, self.consumer))
        self.assertEqual([], self.module.compare_hosts(self.consumer, self.producer))
        self.assertEqual(original, (self.producer, self.consumer))

    def test_same_hash_and_reordered_closure_still_pass(self):
        self.assertEqual([], self.module.compare_hosts(self.producer, self.producer))
        self.consumer['libraries'].reverse()
        self.assertEqual([], self.module.compare_hosts(self.producer, self.consumer))

    def test_every_other_dependency_hash_remains_strict(self):
        for index in range(1, len(self.consumer['libraries'])):
            consumer = copy.deepcopy(self.consumer)
            consumer['libraries'][index]['sha256'] = 'c' * 64
            with self.subTest(index=index):
                self.assertTrue(self.module.compare_hosts(self.producer, consumer))

    def test_libz_identity_and_hash_evidence_are_required_on_each_side(self):
        for side in (0, 1):
            for key, value in [('name', 'libz.so.2'), ('resolved', '/tmp/libz.so.1.3'),
                               ('resolved', '/usr/lib/x86_64-linux-gnu/libz.so.1.4'),
                               ('sha256', ''), ('sha256', 'not-a-sha256')]:
                pair = copy.deepcopy([self.producer, self.consumer])
                pair[side]['libraries'][0][key] = value
                with self.subTest(side=side, key=key, value=value):
                    self.assertTrue(self.module.compare_hosts(*pair))
        for side in (0, 1):
            for mutation in ('missing', 'duplicate', 'extra'):
                pair = copy.deepcopy([self.producer, self.consumer])
                libraries = pair[side]['libraries']
                if mutation == 'missing':
                    libraries.pop(0)
                elif mutation == 'duplicate':
                    libraries.append(copy.deepcopy(libraries[0]))
                else:
                    libraries.append({'name': 'extra', 'resolved': '/usr/lib/extra',
                                      'sha256': 'c' * 64})
                with self.subTest(side=side, mutation=mutation):
                    self.assertTrue(self.module.compare_hosts(*pair))

    def test_matching_non_target_hosts_and_paths_do_not_get_exception(self):
        for key, value in [('system', 'Darwin'), ('arch', 'aarch64'),
                           ('os_version', '24.10')]:
            pair = copy.deepcopy([self.producer, self.consumer])
            for host in pair:
                host[key] = value
                host['minimum_os'] = '14.0'
            with self.subTest(key=key):
                self.assertTrue(self.module.compare_hosts(*pair))
        for host in (self.producer, self.consumer):
            host['libraries'][0]['resolved'] = '/tmp/libz.so.1.3'
        self.assertTrue(self.module.compare_hosts(self.producer, self.consumer))

    def test_host_contract_drift_remains_rejected(self):
        for key, value in [('llvm_version', '21.1.9'), ('llvm_prefix', '/tmp/llvm'),
                           ('c_driver', {}), ('glibc', 'glibc 2.40'),
                           ('system', 'Darwin'), ('arch', 'aarch64'),
                           ('os_version', '24.10'), ('sdk', 'new'), ('sdk_path', '/tmp')]:
            with self.subTest(key=key):
                self.assertTrue(self.module.compare_hosts(
                    self.producer, {**self.consumer, key: value}))


if __name__ == '__main__':
    unittest.main()
