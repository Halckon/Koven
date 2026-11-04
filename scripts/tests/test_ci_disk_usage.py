"""Read-only disk telemetry must not become a test gate or cleanup tool."""
import importlib.util
import pathlib
import os
import signal
import sys
import tempfile
import time
import subprocess
import unittest
from unittest import mock

PATH = pathlib.Path(__file__).resolve().parents[1] / 'ci_disk_usage.py'


class DiskUsageTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location('ci_disk_usage', PATH)
        self.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.module)

    def test_measurement_commands_are_bounded_and_read_only(self):
        with mock.patch.object(self.module.subprocess, 'run') as run:
            self.module.measure(['df', '-h', '.'])
        self.assertEqual(run.call_args.args[0], ['df', '-h', '.'])
        self.assertEqual(run.call_args.kwargs['timeout'], 15)
        self.assertFalse(run.call_args.kwargs['check'])

    def test_measurement_failure_does_not_replace_test_result(self):
        for error in (OSError('missing tool'), subprocess.TimeoutExpired('du', 15)):
            with mock.patch.object(self.module.subprocess, 'run', side_effect=error):
                self.module.measure(['du', '-sx', '.'])

    def test_capacity_samples_include_bytes_and_inodes(self):
        with mock.patch.object(self.module, 'measure') as measure:
            self.module.sample('test', sizes=False)
        calls = [call.args[0] for call in measure.call_args_list]
        self.assertEqual([call[:2] for call in calls], [['df', '-h'], ['df', '-i']])

    def test_sizes_are_only_selected_directories(self):
        with mock.patch.object(self.module, 'measure') as measure:
            self.module.sample('test', sizes=True)
        commands = [call.args[0] for call in measure.call_args_list]
        self.assertEqual(commands[-1][:2], ['du', '-sx'])
        self.assertNotIn('/', commands[-1][2:])
        self.assertNotIn('sudo', commands[-1])

    def test_nested_sizes_are_separate_commands(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            (root / 'target').mkdir()
            with mock.patch.dict(os.environ, {'GITHUB_WORKSPACE': directory}), mock.patch.object(self.module, 'measure') as measure:
                self.module.sample('test', sizes=True)
            commands = [call.args[0] for call in measure.call_args_list]
            self.assertIn(['du', '-sx', str(root)], commands)
            self.assertIn(['du', '-sx', str(root / 'target')], commands)
            for command in commands[2:]:
                self.assertEqual(len(command), 3)

    def test_sigterm_reaps_active_measurement(self):
        with tempfile.TemporaryDirectory() as directory:
            pid_file = pathlib.Path(directory) / 'child.pid'
            child = "import os,time,pathlib; pathlib.Path(%r).write_text(str(os.getpid())); time.sleep(60)" % str(pid_file)
            code = ("import sys,signal; sys.path.insert(0,%r); import ci_disk_usage as d; "
                    "signal.signal(signal.SIGTERM,d.stop_monitor); d.measure([sys.executable,'-c',%r])") % (str(PATH.parent), child)
            process = subprocess.Popen([sys.executable, '-c', code])
            try:
                for _ in range(100):
                    if pid_file.exists():
                        break
                    time.sleep(0.02)
                self.assertTrue(pid_file.exists())
                pid = int(pid_file.read_text())
                process.terminate()
                self.assertEqual(process.wait(timeout=3), 0)
                with self.assertRaises(ProcessLookupError):
                    os.kill(pid, 0)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()

    def test_diagnostic_errors_do_not_replace_shell_exit_status(self):
        source = (PATH.parent / 'check_integration.sh').read_text()
        start = source.index('# Diagnostics are CI/Linux-only')
        end = source.index('bash scripts/check_core.sh', start)
        prefix = source[start:end]
        with tempfile.TemporaryDirectory() as directory:
            fake_python = pathlib.Path(directory) / 'python3'
            fake_python.write_text('#!/bin/sh\nexit 99\n')
            fake_python.chmod(0o755)
            env = dict(os.environ, GITHUB_ACTIONS='true', RUNNER_OS='Linux',
                       PATH=directory + os.pathsep + os.environ['PATH'])
            for status in (0, 7):
                result = subprocess.run(['bash', '-euc', prefix + f'\nexit {status}'],
                                        env=env, timeout=3, check=False)
                self.assertEqual(result.returncode, status)

    def test_monitor_has_finite_samples(self):
        with mock.patch.object(self.module, 'sample') as sample, mock.patch.object(self.module.time, 'sleep'):
            self.module.monitor()
        self.assertEqual(sample.call_count, 360)
        self.assertEqual(sum(call.kwargs['sizes'] for call in sample.call_args_list), 72)


if __name__ == '__main__':
    unittest.main()
