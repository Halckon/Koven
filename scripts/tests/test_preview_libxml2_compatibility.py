"""CI-only SPEC-0322 compare contract; collection/parser coverage follows separately.

Measured host/16-symbol facts are projected from the retained CI excerpt. The
policy completion (disk MD5, distribution codename and full-closure ambiguity
facts) is explicitly synthetic, not a claim that historical CI passed preflight.
Only the existing compare_hosts API is called: baseline failure must be an
assertion about the original strict verdict, never a missing future API.
"""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = Path(__file__).parent / 'fixtures/preview_libxml2/ci_38667368062_excerpt.json'
POLICY = 'koven.preview.libxml2-ubuntu-noble.v1'


def protocol_host(observation):
    """Complete a measured projection with clearly synthetic policy assertions."""
    host = copy.deepcopy(observation['host'])
    library = next(row for row in host['libraries'] if row['name'] == 'libxml2.so.2')
    llvm = next(row for row in host['libraries'] if row['name'] == 'libLLVM.so.21.1')
    package = observation['excerpts']['1']['text'].strip().split('\t')
    md5, path = observation['excerpts']['2']['text'].strip().split()
    host['libxml2_policy'] = {
        'policy': POLICY,
        'library': copy.deepcopy(library),
        'distribution': {'id': 'ubuntu', 'version': '24.04', 'codename': 'noble'},
        'package': dict(zip(('binary', 'status', 'architecture', 'version', 'source',
                             'source_version'), package)),
        # actual==expected is a synthetic success scenario, not a disk measurement.
        'file_md5': {'path': path, 'expected': md5, 'actual': md5},
        'apt_environment': {
            'candidate': '2.9.14+dfsg-1.3ubuntu3.10',
            'package_sources': [{'suite': 'noble-security', 'component': 'main',
                                 'architecture': 'amd64'}],
            'releases': [{'suite': 'noble-security', 'component': 'main',
                          'architecture': 'amd64', 'origin': 'Ubuntu',
                          'label': 'Ubuntu', 'codename': 'noble', 'version': '24.04'}]},
        'installed_origin': 'unproven',
        'source_uri_status': 'unproven',
        'elf': {'class': 'ELF64', 'data': 'little', 'machine': 'EM_X86_64',
                'soname': 'libxml2.so.2',
                'needed': sorted(['libicuuc.so.74', 'libz.so.1', 'liblzma.so.5',
                                  'libm.so.6', 'libc.so.6']),
                'rpaths': [], 'runpaths': []},
        'requesters': [{'path': llvm['resolved'], 'sha256': llvm['sha256'],
                        'requirements': copy.deepcopy(observation['requirements'])}],
        'definitions': copy.deepcopy(observation['definitions']),
        # Deliberately reduced synthetic closure: no full-closure collection claim.
        'unversioned_imports': ['puts'],
        'exported_names': sorted(row['name'] for row in observation['definitions']),
    }
    return host


class Libxml2CompatibilityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location('compatibility_host', ROOT / 'preview_host.py')
        cls.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.module)
        cls.observations = json.loads(FIXTURE.read_text())['roles']

    def setUp(self):
        self.producer = protocol_host(self.observations['producer'])
        self.consumer = protocol_host(self.observations['consumer'])

    def assert_rejected(self, producer=None, consumer=None):
        self.assertTrue(self.module.compare_hosts(
            self.producer if producer is None else producer,
            self.consumer if consumer is None else consumer))

    def test_measured_revisions_with_synthetic_complete_policy_are_allowed(self):
        """Expected baseline behavioral red: old compare rejects only the SHA delta."""
        self.assertEqual([], self.module.compare_hosts(self.producer, self.consumer))

    def test_reversed_measured_revisions_with_complete_policy_are_allowed(self):
        self.assertEqual([], self.module.compare_hosts(self.consumer, self.producer))

    def test_libz_drift_can_coexist_but_does_not_bypass_libxml_policy(self):
        library = next(row for row in self.consumer['libraries'] if row['name'] == 'libz.so.1')
        library['sha256'] = 'b' * 64
        original = copy.deepcopy((self.producer, self.consumer))
        self.assertEqual([], self.module.compare_hosts(self.producer, self.consumer))
        self.assertEqual([], self.module.compare_hosts(self.consumer, self.producer))
        self.assertEqual(original, (self.producer, self.consumer))
        for field in ('package', 'file_md5', 'elf', 'requesters', 'definitions'):
            consumer = copy.deepcopy(self.consumer)
            consumer['libxml2_policy'].pop(field)
            with self.subTest(field=field):
                self.assert_rejected(consumer=consumer)
        for side in (0, 1):
            for section, field, value in [
                    ('package', 'version', '2.9.14+dfsg-1.3ubuntu3.11'),
                    ('file_md5', 'actual', '0' * 32)]:
                pair = copy.deepcopy([self.producer, self.consumer])
                pair[side]['libxml2_policy'][section][field] = value
                with self.subTest(side=side, section=section, field=field):
                    self.assert_rejected(*pair)

    def test_same_hash_uses_strict_path_without_policy_or_with_unknown_policy(self):
        for policy in (None, {'policy': 'unknown'}):
            with self.subTest(policy=policy):
                producer = copy.deepcopy(self.producer)
                consumer = copy.deepcopy(self.producer)
                producer.pop('libxml2_policy')
                consumer.pop('libxml2_policy')
                if policy is not None:
                    consumer['libxml2_policy'] = policy
                self.assertEqual([], self.module.compare_hosts(producer, consumer))

    def test_function_size_is_not_an_abi_size_contract(self):
        row = next(row for row in self.consumer['libxml2_policy']['definitions']
                   if row['type'] == 'FUNC')
        row['size'] += 16
        self.assertEqual([], self.module.compare_hosts(self.producer, self.consumer))

    def test_object_undefined_zero_requires_equal_provider_size(self):
        for host in (self.producer, self.consumer):
            policy = host['libxml2_policy']
            requirement = next(row for row in policy['requesters'][0]['requirements']
                               if row['name'] == 'xmlFree')
            definition = next(row for row in policy['definitions'] if row['name'] == 'xmlFree')
            self.assertEqual(0, requirement['size'])
            self.assertEqual(8, definition['size'])
        next(row for row in self.consumer['libxml2_policy']['definitions']
             if row['name'] == 'xmlFree')['size'] = 16
        self.assert_rejected()

    def test_missing_unknown_or_unbound_policy_is_rejected(self):
        for mutation in ('missing', 'unknown', 'sha', 'path'):
            with self.subTest(mutation=mutation):
                consumer = copy.deepcopy(self.consumer)
                policy = consumer['libxml2_policy']
                if mutation == 'missing':
                    consumer.pop('libxml2_policy')
                elif mutation == 'unknown':
                    policy['policy'] = 'future-policy'
                else:
                    policy['library']['sha256' if mutation == 'sha' else 'resolved'] = 'wrong'
                self.assert_rejected(consumer=consumer)

    def test_other_dependency_or_host_contract_changes_stay_rejected(self):
        for key, value in [('llvm_version', '21.1.9'), ('llvm_prefix', '/tmp/llvm'),
                           ('c_driver', {}), ('glibc', 'glibc 2.40'),
                           ('system', 'Darwin'), ('arch', 'aarch64'), ('os_version', '24.10')]:
            with self.subTest(key=key):
                consumer = copy.deepcopy(self.consumer)
                consumer[key] = value
                self.assert_rejected(consumer=consumer)
        for mutation in ('other-sha', 'path', 'missing', 'duplicate'):
            with self.subTest(mutation=mutation):
                consumer = copy.deepcopy(self.consumer)
                row = next(row for row in consumer['libraries'] if row['name'] == 'libc.so.6')
                if mutation == 'other-sha':
                    row['sha256'] = '0' * 64
                elif mutation == 'path':
                    row['resolved'] = '/tmp/libc.so.6'
                elif mutation == 'missing':
                    consumer['libraries'].remove(row)
                else:
                    consumer['libraries'].append(copy.deepcopy(row))
                self.assert_rejected(consumer=consumer)

    def test_bounded_package_and_local_consistency_facts_are_required(self):
        for key, value in [('binary', 'other:amd64'), ('status', 'deinstall ok config-files'),
                           ('architecture', 'arm64'), ('source', 'other'),
                           ('version', '2.9.14+dfsg-1.3ubuntu3.11'),
                           ('source_version', '2.9.14+dfsg-1.3ubuntu3.11')]:
            with self.subTest(key=key):
                consumer = copy.deepcopy(self.consumer)
                consumer['libxml2_policy']['package'][key] = value
                self.assert_rejected(consumer=consumer)
        consumer = copy.deepcopy(self.consumer)
        consumer['libxml2_policy']['file_md5']['actual'] = '0' * 32
        self.assert_rejected(consumer=consumer)

    def test_distribution_candidate_and_environment_labels_are_required(self):
        for section, key, value in [('distribution', 'id', 'debian'),
                                    ('distribution', 'codename', 'jammy'),
                                    ('apt_environment', 'candidate', None)]:
            with self.subTest(section=section, key=key):
                consumer = copy.deepcopy(self.consumer)
                consumer['libxml2_policy'][section][key] = value
                self.assert_rejected(consumer=consumer)
        for key, value in [('origin', 'PPA'), ('suite', 'noble-proposed'),
                           ('component', 'universe'), ('architecture', 'arm64')]:
            with self.subTest(key=key):
                consumer = copy.deepcopy(self.consumer)
                consumer['libxml2_policy']['apt_environment']['releases'][0][key] = value
                self.assert_rejected(consumer=consumer)

    def test_elf_shape_and_dependencies_cannot_change(self):
        for key, value in [('class', 'ELF32'), ('data', 'big'), ('machine', 'EM_AARCH64'),
                           ('soname', 'libxml2.so.3'), ('needed', []),
                           ('rpaths', ['/usr/lib']), ('runpaths', ['/usr/lib'])]:
            with self.subTest(key=key):
                consumer = copy.deepcopy(self.consumer)
                consumer['libxml2_policy']['elf'][key] = value
                self.assert_rejected(consumer=consumer)

    def test_missing_duplicate_or_unsupported_required_definition_is_rejected(self):
        for mutation in ('missing', 'duplicate', 'default', 'visibility', 'binding', 'type', 'version'):
            with self.subTest(mutation=mutation):
                consumer = copy.deepcopy(self.consumer)
                rows = consumer['libxml2_policy']['definitions']
                if mutation == 'missing':
                    rows.pop()
                elif mutation == 'duplicate':
                    rows.append(copy.deepcopy(rows[0]))
                else:
                    rows[0][mutation] = {'default': False, 'visibility': 'HIDDEN',
                                         'binding': 'WEAK', 'type': 'IFUNC',
                                         'version': 'LIBXML_UNKNOWN'}[mutation]
                self.assert_rejected(consumer=consumer)

    def test_requester_and_requirement_identity_cannot_be_guessed(self):
        for mutation in ('missing', 'path', 'sha256', 'file', 'type', 'binding', 'version_index'):
            with self.subTest(mutation=mutation):
                consumer = copy.deepcopy(self.consumer)
                requesters = consumer['libxml2_policy']['requesters']
                if mutation == 'missing':
                    requesters.clear()
                elif mutation in ('path', 'sha256'):
                    requesters[0][mutation] = 'wrong'
                else:
                    requesters[0]['requirements'][0][mutation] = {
                        'file': 'libc.so.6', 'type': 'TLS', 'binding': 'WEAK',
                        'version_index': 999}[mutation]
                self.assert_rejected(consumer=consumer)

    def test_unversioned_import_collision_against_either_provider_is_rejected(self):
        for side in ('producer', 'consumer'):
            with self.subTest(side=side):
                producer, consumer = copy.deepcopy(self.producer), copy.deepcopy(self.consumer)
                host = producer if side == 'producer' else consumer
                host['libxml2_policy']['unversioned_imports'].append('xmlFree')
                self.assert_rejected(producer, consumer)

    def test_each_side_and_matching_invalid_policy_facts_are_rejected(self):
        mutations = [
            ('package', 'version', '2.9.14+dfsg-1.3ubuntu3.11'),
            ('package', 'source', 'not-libxml2'),
            ('distribution', 'id', 'debian'),
            ('file_md5', 'actual', '0' * 32),
            ('elf', 'class', 'ELF32'),
            ('elf', 'soname', 'not-libxml2'),
        ]
        for section, field, value in mutations:
            for sides in ((0,), (1,), (0, 1)):
                with self.subTest(section=section, field=field, sides=sides):
                    pair = [copy.deepcopy(self.producer), copy.deepcopy(self.consumer)]
                    for side in sides:
                        pair[side]['libxml2_policy'][section][field] = value
                    self.assert_rejected(*pair)
        for sides in ((0,), (1,), (0, 1)):
            with self.subTest(definition_sides=sides):
                pair = [copy.deepcopy(self.producer), copy.deepcopy(self.consumer)]
                for side in sides:
                    pair[side]['libxml2_policy']['definitions'][0]['binding'] = 'WEAK'
                self.assert_rejected(*pair)

    def test_matching_duplicate_symbol_indices_and_conflicting_versions_are_rejected(self):
        for field in ('requirements', 'definitions'):
            for key in ('symbol_index', 'version_index'):
                pair = [copy.deepcopy(self.producer), copy.deepcopy(self.consumer)]
                for host in pair:
                    policy = host['libxml2_policy']
                    rows = policy['requesters'][0]['requirements'] if field == 'requirements' else policy[field]
                    if key == 'symbol_index':
                        rows[1][key] = rows[0][key]
                    else:
                        # Force LIBXML2_2.6.0 to reuse the 2.4.30 index on both sides.
                        other = next(row for row in rows if row['version'] != rows[0]['version'])
                        other[key] = rows[0][key]
                with self.subTest(field=field, key=key):
                    self.assert_rejected(*pair)

    def test_missing_and_wrongly_typed_policy_fields_fail_closed(self):
        for field in ('package', 'file_md5', 'apt_environment', 'elf', 'requesters',
                      'definitions', 'unversioned_imports', 'exported_names'):
            for value in (None, True, 12, 'wrong'):
                with self.subTest(field=field, value=value):
                    consumer = copy.deepcopy(self.consumer)
                    consumer['libxml2_policy'][field] = value
                    self.assert_rejected(consumer=consumer)


if __name__ == '__main__':
    unittest.main()
