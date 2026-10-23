"""Version-manager protocol tests; fixture binaries do not prove native execution."""
from pathlib import Path
import unittest
from unittest import mock

from scripts.tests import test_preview_install as fixture_module

module, ROOT = fixture_module.module, fixture_module.ROOT


class PreviewManagerTests(unittest.TestCase):
    def setUp(self):
        self.fixture = fixture_module.PreviewInstallProtocol()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.root = self.fixture.root / '版本 管理'
        self.manager = module('preview_manager_test', ROOT / 'scripts/preview_manager.py')
        self.archive, self.checksum = self.fixture.package()
        self.validation = mock.patch.object(self.manager.CONSUMER, 'check', return_value=0)
        self.validation.start()
        self.addCleanup(self.validation.stop)
        self.host = mock.patch.object(self.manager, 'check_host')
        self.host.start()
        self.addCleanup(self.host.stop)

    def run_manager(self, command, **kwargs):
        return self.manager.manage(self.root, command, **kwargs)

    def install(self):
        return self.run_manager('install', archive=self.archive, checksum=self.checksum)

    def test_install_query_activate_deactivate_uninstall_preserves_sibling(self):
        version = self.install()['id']
        self.assertEqual(self.run_manager('list')['current'], None)
        self.assertEqual(self.run_manager('list')['versions'][0]['source_commit'], 'a' * 40)
        self.run_manager('use', version=version)
        self.assertEqual((self.root / 'current/bin/kovenc').read_bytes(), b'mock binary')
        with self.assertRaises(ValueError):
            self.run_manager('uninstall', version=version)
        self.run_manager('deactivate')
        self.run_manager('uninstall', version=version)
        self.assertEqual(self.run_manager('list')['versions'], [])
        self.assertEqual(self.fixture.sibling.read_bytes(), b'user data untouched')

    def test_failed_install_keeps_active_version(self):
        version = self.install()['id']
        self.run_manager('use', version=version)
        self.checksum.write_text('corrupt')
        with self.assertRaises(ValueError):
            self.install()
        self.assertEqual(self.run_manager('list')['current'], version)
        self.assertEqual((self.root / 'current/bin/kovenc').read_bytes(), b'mock binary')

    def test_failed_real_validation_does_not_publish(self):
        with mock.patch.object(self.manager.CONSUMER, 'check', return_value=1):
            with self.assertRaises(ValueError):
                self.install()
        self.assertEqual(self.run_manager('list')['versions'], [])

    def test_duplicate_version_is_never_overwritten(self):
        self.install()
        with self.assertRaises(ValueError):
            self.install()

    def test_unknown_root_and_symlink_root_are_rejected(self):
        self.root.mkdir()
        (self.root / 'mine').write_text('preserve')
        with self.assertRaises(ValueError):
            self.install()
        link = self.fixture.root / 'link'
        link.symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.manager.manage(link, 'list')
        self.assertEqual((self.root / 'mine').read_text(), 'preserve')

    def test_invalid_and_symlink_versions_are_rejected(self):
        version = self.install()['id']
        with self.assertRaises(ValueError):
            self.run_manager('uninstall', version='../work parent')
        package = self.root / 'versions' / version
        moved = self.root / 'moved'
        package.rename(moved)
        package.symlink_to(moved, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.run_manager('uninstall', version=version)

    def test_tampered_binary_cannot_be_activated(self):
        version = self.install()['id']
        (self.root / 'versions' / version / 'bin/kovenc').write_bytes(b'changed')
        with self.assertRaises(ValueError):
            self.run_manager('use', version=version)
        self.assertFalse((self.root / 'current').exists())

    def test_switch_and_failed_switch_preserve_previous_current(self):
        first = self.install()['id']
        self.run_manager('use', version=first)
        self.archive, self.checksum = self.fixture.package(
            mutate=lambda files, manifest: manifest.update(source_commit='b' * 40))
        second = self.install()['id']
        with mock.patch.object(self.manager.os, 'replace', side_effect=OSError('publish failed')):
            with self.assertRaises(OSError):
                self.run_manager('use', version=second)
        self.assertEqual(self.run_manager('list')['current'], first)
        self.run_manager('use', version=second)
        self.assertEqual(self.run_manager('list')['current'], second)

    def test_host_failure_preserves_active_version(self):
        first = self.install()['id']
        self.run_manager('use', version=first)
        self.archive, self.checksum = self.fixture.package(
            mutate=lambda files, manifest: manifest.update(source_commit='b' * 40))
        second = self.install()['id']
        with mock.patch.object(self.manager, 'check_host', side_effect=ValueError('host mismatch')):
            with self.assertRaises(ValueError):
                self.run_manager('use', version=second)
        self.assertEqual(self.run_manager('list')['current'], first)

    def test_failed_publication_leaves_no_version(self):
        with mock.patch.object(Path, 'rename', side_effect=OSError('publish failed')):
            with self.assertRaises(OSError):
                self.install()
        self.assertEqual(self.run_manager('list')['versions'], [])

    def test_published_install_reports_success_with_cleanup_warning(self):
        with mock.patch.object(self.manager.shutil, 'rmtree', side_effect=PermissionError('cleanup')):
            result = self.install()
        self.assertIn('cleanup_warning', result)
        self.assertEqual(self.run_manager('list')['versions'][0]['id'], result['id'])

    def test_successful_switch_has_no_post_commit_directory_cleanup(self):
        version = self.install()['id']
        with mock.patch.object(self.manager.shutil, 'rmtree', side_effect=PermissionError('cleanup')):
            self.run_manager('use', version=version)
        self.assertEqual(self.run_manager('list')['current'], version)

    def test_activation_ignores_unlisted_python_bytecode(self):
        import importlib.util
        import os
        import py_compile
        version = self.install()['id']
        package = self.root / 'versions' / version
        source = package / 'tools/preview_host.py'
        original, timestamp = source.read_bytes(), source.stat().st_mtime
        injected = b"raise RuntimeError('unverified cache executed')\n"
        self.assertLess(len(injected), len(original))
        source.write_bytes(injected + b' ' * (len(original) - len(injected)))
        os.utime(source, (timestamp, timestamp))
        py_compile.compile(str(source), doraise=True)
        source.write_bytes(original)
        os.utime(source, (timestamp, timestamp))
        self.assertTrue(Path(importlib.util.cache_from_source(str(source))).exists())
        self.host.stop()
        self.run_manager('use', version=version)
        self.assertEqual(self.run_manager('list')['current'], version)

    def test_concurrent_operation_is_rejected(self):
        import fcntl
        self.install()
        with (self.root / '.lock').open('rb') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(ValueError):
                self.run_manager('list')


if __name__ == '__main__':
    unittest.main()
