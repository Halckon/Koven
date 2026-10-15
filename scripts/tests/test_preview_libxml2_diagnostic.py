"""SPEC-0322 CI-only diagnostic protocol; no compatibility acceptance oracle."""
import base64
import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest
from unittest import mock


PATH = Path(__file__).resolve().parents[1] / 'preview_host.py'


class Libxml2DiagnosticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location('preview_xml_diagnostic', PATH)
        cls.host = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.host)

    def test_public_ubuntu_uris_only_survive_redaction(self):
        text = ('http://archive.ubuntu.com/ubuntu noble/main\n'
                'https://azure.archive.ubuntu.com/ubuntu/ noble-updates/main\n'
                'http://security.ubuntu.com/ubuntu noble-security/main\n')
        self.assertEqual(self.host._redact_apt_sources(text), (text, False))

    def test_credentials_ports_queries_fragments_and_other_sources_are_redacted(self):
        for uri in ('https://name:secret@archive.ubuntu.com/ubuntu',
                    'https://archive.ubuntu.com:443/ubuntu',
                    'https://archive.ubuntu.com/ubuntu?token=secret',
                    'https://archive.ubuntu.com/ubuntu#secret',
                    'https://private.example/secret', 'file:/private/secret',
                    'ftp://archive.ubuntu.com/ubuntu',
                    'https://archive.ubuntu.com/private/secret'):
            with self.subTest(uri=uri):
                text, redacted = self.host._redact_apt_sources('source ' + uri + '\n')
                self.assertTrue(redacted)
                self.assertEqual(text, 'source <redacted-source>\n')

    def test_real_child_bytes_and_exit_are_recorded(self):
        records = []
        result = self.host._diagnostic_probe(
            [sys.executable, '-c', 'import os; os.write(1,b"hello"); os.write(2,b"detail")'], records)
        self.assertEqual(result, 'hello')
        self.assertEqual(records[0]['exit'], 0)
        self.assertEqual(base64.b64decode(records[0]['stderr_base64']), b'detail')
        self.assertTrue(records[0]['success'])

    def test_nonzero_child_is_diagnostic_failure_with_original_bytes(self):
        records = []
        result = self.host._diagnostic_probe(
            [sys.executable, '-c', 'import os; os.write(2,b"detail"); raise SystemExit(7)'], records)
        self.assertIsNone(result)
        self.assertEqual(records[0]['exit'], 7)
        self.assertFalse(records[0]['success'])
        self.assertEqual(base64.b64decode(records[0]['stderr_base64']), b'detail')

    def test_output_limit_stops_child_with_bounded_record(self):
        records = []
        self.assertIsNone(self.host._diagnostic_probe(
            [sys.executable, '-c', 'import os; os.write(1,b"x"*8192)'], records, output_limit=128))
        self.assertTrue(records[0]['output_limited'])
        self.assertLessEqual(len(base64.b64decode(records[0]['stdout_base64'])), 128)
        self.assertIsNotNone(records[0]['exit'])

    def test_timeout_stops_and_reaps_child(self):
        records = []
        self.assertIsNone(self.host._diagnostic_probe(
            [sys.executable, '-c', 'import time; time.sleep(30)'], records, timeout=0.05))
        self.assertTrue(records[0]['timed_out'])
        self.assertIsNotNone(records[0]['exit'])

    def test_startup_error_is_contained(self):
        records = []
        with mock.patch.object(self.host.subprocess, 'Popen', side_effect=OSError('cannot start')):
            self.assertIsNone(self.host._diagnostic_probe(['/usr/bin/readelf'], records))
        self.assertFalse(records[0]['success'])
        self.assertEqual(records[0]['error'], 'OSError')

    def test_apt_both_streams_are_redacted_before_recording(self):
        records = []
        actual_popen = subprocess.Popen
        def child(_args, **kwargs):
            return actual_popen([sys.executable, '-c',
                'import os; os.write(1,b"https://name:secret@private.example/repo"); '
                'os.write(2,b"https://private.example/path?secret=yes")'], **kwargs)
        with mock.patch.object(self.host.subprocess, 'Popen', side_effect=child):
            self.assertEqual(self.host._diagnostic_probe(['/usr/bin/apt-cache', 'policy'], records),
                             '<redacted-source>')
        self.assertTrue(records[0]['redacted'])
        for stream in ('stdout_base64', 'stderr_base64'):
            self.assertEqual(base64.b64decode(records[0][stream]), b'<redacted-source>')

    def test_missing_or_duplicate_library_does_not_probe(self):
        row = {'name': 'libxml2.so.2', 'resolved': '/usr/lib/libxml2.so.2', 'sha256': 'a' * 64}
        for libraries in ([], [row, row]):
            evidence = {'commands': []}
            with mock.patch.object(self.host, '_diagnostic_probe') as probe:
                self.host.collect_libxml2_diagnostic('/tmp/kovenc', libraries, evidence)
            probe.assert_not_called()
            self.assertFalse(evidence['libxml2_diagnostic']['complete'])

    def test_only_measured_direct_requesters_get_symbol_probes(self):
        libraries = [{'name': name, 'resolved': '/usr/lib/' + name, 'sha256': 'a' * 64}
                     for name in ('libxml2.so.2', 'libLLVM.so.21.1', 'libc.so.6')]
        calls = []
        def probe(args, records):
            calls.append(args)
            records.append({'success': True})
            if '-d' in args and args[-1].endswith('libLLVM.so.21.1'):
                return '0x1 (NEEDED) Shared library: [libxml2.so.2]\n'
            return ''
        evidence = {'commands': []}
        with mock.patch.object(self.host, '_diagnostic_probe', side_effect=probe):
            self.host.collect_libxml2_diagnostic('/tmp/kovenc', libraries, evidence)
        diagnostic = evidence['libxml2_diagnostic']
        self.assertTrue(diagnostic['complete'])
        self.assertEqual(diagnostic['requesters'], ['/usr/lib/libLLVM.so.21.1'])
        self.assertEqual({args[-1] for args in calls if '--dyn-syms' in args},
                         {'/usr/lib/libLLVM.so.21.1', '/usr/lib/libxml2.so.2'})
        self.assertEqual(evidence['commands'], [])

    def test_collector_unexpected_error_is_separate_from_host_result(self):
        evidence = {'commands': [], 'partial': {'system': 'Linux'}}
        row = {'name': 'libxml2.so.2', 'resolved': '/usr/lib/libxml2.so.2', 'sha256': 'a' * 64}
        with mock.patch.object(self.host, '_diagnostic_probe', side_effect=RuntimeError('unexpected')):
            self.host.collect_libxml2_diagnostic('/tmp/kovenc', [row], evidence)
        self.assertFalse(evidence['libxml2_diagnostic']['complete'])
        self.assertEqual(evidence['partial'], {'system': 'Linux'})


if __name__ == '__main__':
    unittest.main()
