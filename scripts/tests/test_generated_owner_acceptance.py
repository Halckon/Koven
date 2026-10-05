"""Pure report/exit policy tests; all execution and generation are mocked."""
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock
from scripts import check_generated_owners as gate


def calibration(linux):
    rows = [dict(name='clean-v1', status='pass', exit=0),
            dict(name='clean-v2', status='pass', exit=0),
            dict(name='address', category='address', status='rejected_as_expected', detected_by='asan', fingerprint=['native', 'asan_error', 'stack-buffer-overflow:inspect']),
            dict(name='leak', category='leak', status='rejected_as_expected', detected_by='counter', exit=-6,
                 lsan_status='rejected_as_expected', lsan_fingerprint=['native', 'lsan_error', 'detected memory leaks:malloc']),
            dict(name='missing_deinit', category='missing_deinit', status='rejected_as_expected', detected_by='output_diff', witness='drop:leaf_fixture'),
            dict(name='premature_holder_free', category='premature_holder_free', status='rejected_as_expected', detected_by='counter_order', exit=-6)]
    if linux:
        return dict(status='pass', records=rows)
    rows[2] = dict(name='address', category='address', status='skipped', reason='macos-asan-unsupported',
                   asan_skipped='macos-counter-only', detector_off_verified=True)
    rows[3].pop('lsan_status')
    rows[3]['lsan_skipped'] = 'macos-counter-only'
    return dict(status='partial', records=rows,
                skipped_reasons=['address:macos-asan-unsupported', 'leak:macos-counter-only'])


class AcceptancePolicyTests(unittest.TestCase):
    def test_linux_complete_and_mac_explicit_platform_limits(self):
        self.assertEqual(('pass', []), gate.calibration_verdict(calibration(True), linux=True))
        status, reasons = gate.calibration_verdict(calibration(False), linux=False)
        self.assertEqual('partial', status)
        self.assertEqual(calibration(False)['skipped_reasons'], reasons)

    def test_linux_accepts_tuple_fingerprints_from_live_producer(self):
        calib_tuple = calibration(True)
        calib_tuple['records'][2]['fingerprint'] = ('native', 'asan_error', 'stack-buffer-overflow:inspect')
        calib_tuple['records'][3]['lsan_fingerprint'] = ('native', 'lsan_error', 'detected memory leaks:malloc')
        self.assertEqual(('pass', []), gate.calibration_verdict(calib_tuple, linux=True))

    def test_linux_never_accepts_partial_skipped_missing_or_malformed(self):
        variants = [None, [], {}, dict(status='pass'), calibration(False)]
        for status in ['partial', 'skipped', 'failure', None, True]:
            value = calibration(True); value['status'] = status; variants.append(value)
        for change in ['runtime-unavailable', 'skipped', None]:
            value = calibration(True); value['records'][3]['lsan_status'] = change; variants.append(value)
        value = calibration(True); value['records'].pop(); variants.append(value)
        value = calibration(True); value['records'].append(value['records'][2]); variants.append(value)
        value = calibration(True); value['records'][0]['exit'] = 1; variants.append(value)
        value = calibration(True); value['records'][2]['detected_by'] = 'counter'; variants.append(value)
        for field in ['name', 'fingerprint']:
            value = calibration(True); value['records'][2][field] = None; variants.append(value)
        value = calibration(True); value['records'][3]['lsan_fingerprint'] = []; variants.append(value)
        for value in variants:
            with self.subTest(report=value), self.assertRaises(gate.Failure):
                gate.calibration_verdict(value, linux=True)

    def test_mac_rejects_unlisted_limits_and_other_fault_skips(self):
        variants = [calibration(True), None]
        value = calibration(False); value['skipped_reasons'].append('other'); variants.append(value)
        value = calibration(False); value['records'][4]['status'] = 'skipped'; variants.append(value)
        value = calibration(False); value['records'][3]['lsan_status'] = 'runtime-unavailable'; variants.append(value)
        value = calibration(False); value['records'][2]['detector_off_verified'] = False; variants.append(value)
        for value in variants:
            with self.subTest(report=value), self.assertRaises(gate.Failure):
                gate.calibration_verdict(value, linux=False)

    def test_driver_runs_reduction_and_aggregates_acceptance(self):
        for linux in [True, False]:
            with self.subTest(linux=linux), tempfile.TemporaryDirectory() as temp:
                root = Path(temp) / 'out'
                executor = mock.Mock(linux=linux)
                def prepare(case, folder):
                    folder.mkdir(); (folder / 'case.json').write_text(json.dumps(case))
                with mock.patch.object(gate, 'Execution', return_value=executor), \
                     mock.patch.object(gate.model, 'cases', return_value=[dict(id='fixture')]), \
                     mock.patch.object(gate, 'prepare_case', side_effect=prepare), \
                     mock.patch.object(gate.calibration, 'verify', return_value=calibration(linux)), \
                     mock.patch.object(gate.checker_mutation, 'verify_checker_mutant', return_value=dict(status='pass')), \
                     mock.patch.object(gate.reduction, 'run_reduction', return_value=dict(status='reproduced', confirmation_count=3)) as reducer, \
                     contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(0, gate.main(['--artifacts', str(root)]))
                reducer.assert_called_once()
                report = json.loads((root / 'acceptance.json').read_text())
                if linux:
                    self.assertEqual('pass', report['status'])
                    self.assertTrue(report['requirements_met'])
                    self.assertNotIn('partial_reasons', report)
                else:
                    self.assertEqual('partial', report['status'])
                    self.assertFalse(report['requirements_met'])
                    self.assertEqual(calibration(False)['skipped_reasons'], report['partial_reasons'])
                self.assertEqual('reproduced', report['reduction']['status'])

    def test_driver_fails_when_reduction_incomplete(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / 'out'
            executor = mock.Mock(linux=True)
            def prepare(case, folder):
                folder.mkdir(); (folder / 'case.json').write_text(json.dumps(case))
            with mock.patch.object(gate, 'Execution', return_value=executor), \
                 mock.patch.object(gate.model, 'cases', return_value=[dict(id='fixture')]), \
                 mock.patch.object(gate, 'prepare_case', side_effect=prepare), \
                 mock.patch.object(gate.calibration, 'verify', return_value=calibration(True)), \
                 mock.patch.object(gate.checker_mutation, 'verify_checker_mutant', return_value=dict(status='pass')), \
                 mock.patch.object(gate.reduction, 'run_reduction', return_value=dict(status='minimization_incomplete', confirmation_count=0)), \
                 contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(1, gate.main(['--artifacts', str(root)]))

    def test_linux_detector_gap_stops_before_checker(self):
        with tempfile.TemporaryDirectory() as temp:
            executor = mock.Mock(linux=True)
            with mock.patch.object(gate, 'Execution', return_value=executor), \
                 mock.patch.object(gate.model, 'cases', return_value=[]), \
                 mock.patch.object(gate.calibration, 'verify', return_value=calibration(False)), \
                 mock.patch.object(gate.checker_mutation, 'verify_checker_mutant') as checker, \
                 contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(1, gate.main(['--artifacts', str(Path(temp) / 'out')]))
            checker.assert_not_called()


class CalibrationEntryPolicyTests(unittest.TestCase):
    def test_standalone_calibration_linux_gap_fails_mac_limits_remain_explicit(self):
        for linux, report, expected in [(True, calibration(False), 1), (False, calibration(False), 0)]:
            with self.subTest(linux=linux), tempfile.TemporaryDirectory() as temp:
                executor = mock.Mock(linux=linux)
                with mock.patch.object(gate, 'Execution', return_value=executor), \
                     mock.patch.object(gate.calibration, 'verify', return_value=report), \
                     contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(expected, gate.calibration.main(['--artifacts', temp]))
