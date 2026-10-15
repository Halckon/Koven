"""CI-only finite parser/collector cases. All ELF tables here are SYNTHETIC.

Unlike the retained CI excerpts, these small tables have complete declared
counts. They test protocol boundaries, never real-host ABI/native acceptance.
"""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
from unittest import mock

PATH = Path(__file__).resolve().parents[1] / 'preview_host.py'
XML = '/usr/lib/x86_64-linux-gnu/libxml2.so.2.9.14'
LLVM = '/usr/lib/x86_64-linux-gnu/libLLVM.so.21.1'
LIBC = '/usr/lib/x86_64-linux-gnu/libc.so.6'
BINARY = '/fixture/kovenc'
CONTENT = b'synthetic ELF bytes; never executed'
SHA = hashlib.sha256(CONTENT).hexdigest()
MD5 = hashlib.md5(CONTENT, usedforsecurity=False).hexdigest()
HEADER = """ELF Header:
  Class: ELF64
  Data: 2's complement, little endian
  Version: 1 (current)
  Machine: Advanced Micro Devices X86-64
  Version: 0x1
"""


def dynamic(*rows):
    return ('Dynamic section at offset 0x100 contains ' + str(len(rows) + 1) + ' entries:\n'
            '  Tag        Type                         Name/Value\n'
            + ''.join(f' 0x0001 ({tag}) Shared library: [{value}]\n' for tag, value in rows)
            + ' 0x0000 (NULL) 0x0\n')


def symbols(kind='OBJECT', defined=False, name='xmlFree', version=True):
    spelling = name + (('@@' if defined else '@') + 'LIBXML2_2.4.30' if version else '')
    if version and not defined:
        spelling += ' (2)'
    return ("Symbol table '.dynsym' contains 2 entries:\n"
            ' Num:    Value          Size Type    Bind   Vis      Ndx Name\n'
            ' 0: 00000000 0 NOTYPE LOCAL DEFAULT UND\n'
            f' 1: 00000000 {8 if defined else 0} {kind} GLOBAL DEFAULT '
            f'{"12" if defined else "UND"} {spelling}\n')


def versions(defined=False):
    start = ("Version symbols section '.gnu.version' contains 2 entries:\n"
             ' Addr: 0x1 Offset: 0x1 Link: 1 (.dynsym)\n'
             ' 000: 0 (*local*) 2 (LIBXML2_2.4.30)\n')
    if defined:
        return start + ("Version definition section '.gnu.version_d' contains 1 entry:\n"
                        ' Addr: 0x1 Offset: 0x1 Link: 1 (.dynstr)\n'
                        ' 0x0000: Rev: 1 Flags: none Index: 2 Cnt: 1 Name: LIBXML2_2.4.30\n')
    return start + ("Version needs section '.gnu.version_r' contains 1 entry:\n"
                    ' Addr: 0x1 Offset: 0x1 Link: 1 (.dynstr)\n'
                    ' 0x0000: Version: 1 File: libxml2.so.2 Cnt: 1\n'
                    ' 0x0010: Name: LIBXML2_2.4.30 Flags: none Version: 2\n')


class Libxml2PolicyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location('libxml_policy_test', PATH)
        cls.host = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.host)

    def test_complete_synthetic_undefined_and_defined_tables_link_versions(self):
        for defined in (False, True):
            with self.subTest(defined=defined):
                rows = self.host._elf_symbols(symbols(defined=defined))
                needs, definitions = self.host._elf_versions(versions(defined), rows)
                self.assertEqual(rows[1]['version_index'], 2)
                self.assertEqual(rows[1]['size'], 8 if defined else 0)
                self.assertEqual((definitions if defined else needs)[2]['name'], 'LIBXML2_2.4.30')

    def test_real_observed_hex_size_and_unprefixed_version_offsets_are_accepted(self):
        # CI38667368062 producer probe09 row46622 prints size 0x3b252;
        # probes10/26 first version block offset is 000000, without 0x.
        # Keep these lexical forms in small complete SYNTHETIC tables.
        table = symbols(defined=True).replace(' 8 OBJECT', ' 0x3b252 OBJECT')
        rows = self.host._elf_symbols(table)
        self.assertEqual(rows[1]['size'], 0x3b252)
        for defined in (False, True):
            rows = self.host._elf_symbols(symbols(defined=defined))
            self.host._elf_versions(versions(defined).replace('0x0000:', '000000:'), rows)
            self.assertEqual(rows[1]['version_index'], 2)

    def test_unversioned_object_legitimately_omits_both_version_tables(self):
        rows = self.host._elf_symbols(symbols(kind='IFUNC', name='other', version=False))
        self.assertEqual(self.host._elf_versions('No version information found in this file.\n', rows), ({}, {}))

    def test_clipped_or_unknown_dynsym_rows_are_rejected(self):
        for text in (symbols().replace('contains 2', 'contains 3'),
                     symbols().replace(' 1:', ' 0:'), symbols() + 'unknown row\n',
                     symbols().replace('xmlFree@', 'xmlFree@@@')):
            with self.subTest(text=text):
                with self.assertRaises(ValueError):
                    self.host._elf_symbols(text)

    def test_orphan_conflicting_hidden_and_count_facts_are_not_silently_lost(self):
        bad = [versions().replace('Version: 2\n', 'Version: 3\n'),
               versions().replace('contains 2 entries', 'contains 3 entries'),
               versions().replace('Cnt: 1', 'Cnt: 2'),
               versions().replace('2 (LIBXML2_2.4.30)', '2 (OTHER)'),
               versions() + ' 0x0020: Name: OTHER Flags: none Version: 2\n',
               versions().split('Version needs section')[0], 'No version information found in this file.\n',
               versions().replace('.gnu.version_r', '.unknown')]
        for text in bad:
            with self.subTest(text=text):
                with self.assertRaises(ValueError):
                    self.host._elf_versions(text, self.host._elf_symbols(symbols()))
        rows = self.host._elf_symbols(symbols())
        self.host._elf_versions(versions().replace('2 (LIBXML', '2h (LIBXML'), rows)
        self.assertTrue(rows[1]['version_hidden'])

    def test_empty_mixed_or_duplicate_no_info_output_is_rejected(self):
        no_info = 'No version information found in this file.\n'
        for text in ('', '  \n', no_info + no_info, no_info + versions()):
            with self.subTest(text=text):
                with self.assertRaises(ValueError):
                    self.host._elf_versions(text, self.host._elf_symbols(symbols(version=False)))

    def test_versioned_undefined_cannot_use_a_definition_or_omit_version_name(self):
        with self.assertRaises(ValueError):
            self.host._elf_versions(versions(True), self.host._elf_symbols(symbols()))
        with self.assertRaises(ValueError):
            self.host._elf_versions(versions(), self.host._elf_symbols(symbols(version=False)))

    def test_reusing_one_version_index_for_multiple_symbols_is_valid(self):
        table = symbols().replace('contains 2', 'contains 3') + (
            ' 2: 00000000 0 FUNC GLOBAL DEFAULT UND xmlRead@LIBXML2_2.4.30 (2)\n')
        info = versions().replace('contains 2 entries', 'contains 3 entries').replace(
            '2 (LIBXML2_2.4.30)\n', '2 (LIBXML2_2.4.30) 2 (LIBXML2_2.4.30)\n')
        rows = self.host._elf_symbols(table)
        self.host._elf_versions(info, rows)
        self.assertEqual([row['version_index'] for row in rows], [0, 2, 2])

    def test_dynamic_and_header_are_finite_and_fail_on_missing_facts(self):
        self.assertEqual(self.host._elf_dynamic(dynamic(('SONAME', 'libxml2.so.2')))['SONAME'], ['libxml2.so.2'])
        for text in (dynamic().replace('1 entries', '2 entries'), dynamic() + 'unknown\n'):
            with self.assertRaises(ValueError):
                self.host._elf_dynamic(text)
        for text in (HEADER.replace('ELF64', 'ELF32'), HEADER.replace('Machine:', 'Other:')):
            with self.assertRaises(ValueError):
                self.host._elf_header(text)

    def test_complete_measured_headers_accept_only_the_two_standard_version_fields(self):
        fixture = Path(__file__).parent / 'fixtures/preview_libxml2/ci_38667368062_excerpt.json'
        observations = json.loads(fixture.read_text())['roles']
        # Whole probe24 stdout retained with its original SHA. These bytes also
        # exactly match both provider headers in CI38675369744 policy probe59.
        for role, observation in observations.items():
            with self.subTest(role=role):
                excerpt = observation['excerpts']['24']
                text = excerpt['text']
                self.assertEqual(hashlib.sha256(text.encode()).hexdigest(), excerpt['original_stdout_sha256'])
                self.assertEqual(self.host._elf_header(text),
                                 {'class': 'ELF64', 'data': 'little', 'machine': 'EM_X86_64'})

    def test_ambiguous_duplicate_header_keys_and_invalid_versions_stay_rejected(self):
        for text in (HEADER + '  Class: ELF64\n', HEADER + '  Machine: Advanced Micro Devices X86-64\n',
                     HEADER + '  Version: 0x1\n', HEADER.replace('0x1', '0x2'),
                     HEADER.replace('1 (current)', '2 (current)'), HEADER.replace('  Version: 0x1\n', ''),
                     HEADER.replace('1 (current)', 'swap').replace('0x1', '1 (current)').replace('swap', '0x1'),
                     HEADER.replace('EM_X86_64', 'unknown').replace('Advanced Micro Devices X86-64', 'AArch64')):
            with self.subTest(text=text):
                with self.assertRaises(ValueError):
                    self.host._elf_header(text)

    def fixture(self):
        libraries = [{'name': name, 'resolved': path, 'sha256': SHA} for name, path in
                     [('libxml2.so.2', XML), ('libLLVM.so.21.1', LLVM), ('libc.so.6', LIBC)]]
        distro = {'ID': 'ubuntu', 'VERSION_ID': '24.04', 'VERSION_CODENAME': 'noble'}
        return libraries, distro

    def reply(self, args, records):
        records.append({'argv': args, 'synthetic': True})
        if args[0] == '/usr/bin/dpkg-query':
            if args[1] == '-S':
                return f'libxml2:amd64: {XML}\n'
            if args[1] == '-W':
                return ('libxml2:amd64\tinstall ok installed\tamd64\t2.9.14+dfsg-1.3ubuntu3.9\t'
                        'libxml2\t2.9.14+dfsg-1.3ubuntu3.9\n')
            return f'{MD5}  {XML[1:]}\n'
        if args[0] == '/usr/bin/apt-cache':
            if len(args) == 3:
                return ('libxml2:\n  Installed: 2.9.14+dfsg-1.3ubuntu3.9\n'
                        '  Candidate: 2.9.14+dfsg-1.3ubuntu3.10\n  Version table:\n'
                        '     2.9.14+dfsg-1.3ubuntu3.10 500\n'
                        '        500 <redacted-source> noble-security/main amd64 Packages\n'
                        ' *** 2.9.14+dfsg-1.3ubuntu3.9 100\n        100 /var/lib/dpkg/status\n')
            return ('Package files:\n 500 <redacted-source> noble-security/main amd64 Packages\n'
                    '     release v=24.04,o=Ubuntu,a=noble-security,n=noble,l=Ubuntu,c=main,b=amd64\n')
        option, target = args[-2:]
        if option == '-h':
            return HEADER
        if option == '-d':
            if target == XML:
                return dynamic(('NEEDED', 'libc.so.6'), ('SONAME', 'libxml2.so.2'))
            return dynamic(('NEEDED', 'libxml2.so.2')) if target == LLVM else dynamic()
        if option == '--dyn-syms':
            return symbols(defined=target == XML) if target in (XML, LLVM) else symbols(name='puts', version=False)
        if option == '--version-info':
            return versions(target == XML) if target in (XML, LLVM) else 'No version information found in this file.\n'
        self.fail(f'unexpected synthetic probe: {args}')

    def collect(self, reply=None):
        libraries, distro = self.fixture()
        evidence = {}
        with mock.patch.object(self.host, '_diagnostic_probe', side_effect=reply or self.reply), \
                mock.patch.object(self.host.Path, 'read_bytes', return_value=CONTENT):
            policy = self.host._collect_libxml2_policy(BINARY, libraries, distro, evidence)
        return policy, evidence

    def test_complete_synthetic_collector_keeps_unproven_source_and_owned_file(self):
        policy, evidence = self.collect()
        self.assertIsNotNone(policy)
        self.assertEqual(evidence['libxml2_policy']['errors'], [])
        self.assertEqual(policy['installed_origin'], 'unproven')
        self.assertEqual(policy['source_uri_status'], 'unproven')
        self.assertEqual(policy['file_md5']['actual'], MD5)
        self.assertEqual(policy['unversioned_imports'], ['puts'])
        libraries, _ = self.fixture()
        host = {'system': 'Linux', 'arch': 'x86_64', 'os_version': '24.04',
                'libraries': libraries, 'libxml2_policy': policy}
        self.assertTrue(self.host._valid_libxml_policy(host, libraries[0]))

    def test_each_probe_failure_retains_evidence_and_never_grants_policy(self):
        _, completed = self.collect()
        count = len(completed['libxml2_policy']['commands'])
        self.assertGreater(count, 10)
        for fail_at in range(count):
            calls = []
            def reply(args, records):
                calls.append(args)
                if len(calls) == fail_at + 1:
                    records.append({'argv': args, 'success': False, 'synthetic': True})
                    return None
                return self.reply(args, records)
            with self.subTest(fail_at=fail_at):
                policy, evidence = self.collect(reply)
                self.assertIsNone(policy)
                self.assertTrue(evidence['libxml2_policy']['errors'])
                self.assertEqual(len(calls), fail_at + 1)

    def test_copy_relocation_and_special_required_definition_are_rejected(self):
        for target, replacement in ((LLVM, ('DEFAULT UND xmlFree', 'DEFAULT 12 xmlFree')),
                                    (XML, ('DEFAULT 12 xmlFree', 'DEFAULT ABS xmlFree'))):
            def reply(args, records):
                text = self.reply(args, records)
                if args[-2:] == ['--dyn-syms', target]:
                    return text.replace(*replacement)
                return text
            with self.subTest(target=target):
                policy, evidence = self.collect(reply)
                self.assertIsNone(policy)
                self.assertTrue(evidence['libxml2_policy']['errors'])

    def test_library_changing_after_elf_probes_cannot_grant_policy(self):
        libraries, distro = self.fixture()
        with mock.patch.object(self.host, '_diagnostic_probe', side_effect=self.reply), \
                mock.patch.object(self.host.Path, 'read_bytes', side_effect=[CONTENT, CONTENT, b'changed']):
            evidence = {}
            self.assertIsNone(self.host._collect_libxml2_policy(BINARY, libraries, distro, evidence))
        self.assertIn('after ELF inspection', evidence['libxml2_policy']['errors'][0])

    def test_duplicate_owner_checksum_and_changed_disk_are_rejected(self):
        for kind in ('owner', 'checksum', 'disk'):
            def reply(args, records):
                text = self.reply(args, records)
                if kind == 'owner' and args[:2] == ['/usr/bin/dpkg-query', '-S']:
                    return text + text
                if kind == 'checksum' and args[:2] == ['/usr/bin/dpkg-query', '--control-show']:
                    return text + text
                return text
            with self.subTest(kind=kind):
                if kind == 'disk':
                    libraries, distro = self.fixture()
                    with mock.patch.object(self.host, '_diagnostic_probe', side_effect=reply), \
                            mock.patch.object(self.host.Path, 'read_bytes', return_value=b'changed'):
                        policy = self.host._collect_libxml2_policy(BINARY, libraries, distro, {})
                else:
                    policy, _ = self.collect(reply)
                self.assertIsNone(policy)


if __name__ == '__main__':
    unittest.main()
