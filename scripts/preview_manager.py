#!/usr/bin/env python3
"""Manage trusted local preview archives in an explicitly owned installation root."""
import argparse
from contextlib import contextmanager, redirect_stdout
import fcntl
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import stat
import sys
import tempfile
import types


def load_consumer():
    path = Path(__file__).with_name('check_preview_install.py')
    spec = importlib.util.spec_from_file_location('preview_manager_consumer', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CONSUMER = load_consumer()
MARKER = 'koven.preview.manager.v1\n'
VERSION = re.compile(r'[a-f0-9]{40}-(?:aarch64-apple-darwin|x86_64-unknown-linux-gnu)')


def regular(path):
    return not path.is_symlink() and path.is_file()


@contextmanager
def locked_root(root):
    """Only initialize a new dedicated root; serialize publication and removal."""
    root = Path(os.path.abspath(root))
    if root.is_symlink():
        raise ValueError('installation root must not be a symlink')
    try:
        root.mkdir(parents=True)
    except FileExistsError:
        marker = root / '.koven-manager'
        if not regular(marker) or marker.read_text() != MARKER:
            raise ValueError('installation root is not owned by this manager')
    else:
        (root / '.koven-manager').write_text(MARKER)
        (root / 'versions').mkdir()
        (root / '.lock').touch()
    if (not regular(root / '.lock') or (root / 'versions').is_symlink()
            or not (root / 'versions').is_dir()):
        raise ValueError('invalid manager layout')
    with (root / '.lock').open('rb') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError('another manager operation is active') from error
        yield root


def package_path(root, version):
    if not isinstance(version, str) or not VERSION.fullmatch(version):
        raise ValueError('invalid version ID')
    path = root / 'versions' / version
    if path.is_symlink() or not path.is_dir():
        raise ValueError('version is not an installed directory')
    return path


def identity(package):
    path = package / 'manifest.json'
    if not regular(path):
        raise ValueError('missing regular manifest')
    manifest = CONSUMER.load_json(path.read_bytes())
    version = f"{manifest.get('source_commit')}-{manifest.get('target')}"
    if not VERSION.fullmatch(version) or version != package.name:
        raise ValueError('installed manifest identity mismatch')
    return manifest


def current_version(root):
    current = root / 'current'
    if not current.is_symlink():
        if current.exists():
            raise ValueError('current must be a manager symlink')
        return None
    target = os.readlink(current)
    version = target.removeprefix('versions/')
    if target != f'versions/{version}':
        raise ValueError('invalid current target')
    package_path(root, version)
    return version


def verify_installed(package, manifest):
    """Refuse activation of damaged files or links substituted after installation."""
    files = manifest.get('files')
    if not isinstance(files, dict) or not CONSUMER.REQUIRED <= files.keys():
        raise ValueError('invalid installed file manifest')
    for name, metadata in files.items():
        CONSUMER.safe_path(name)
        path = package / name
        if any((package / parent).is_symlink() for parent in Path(name).parents):
            raise ValueError('installed package contains a symlink parent')
        if (not regular(path) or path.stat().st_size != metadata['size']
                or stat.S_IMODE(path.stat().st_mode) != metadata['mode']
                or CONSUMER.sha256(path) != metadata['sha256']):
            raise ValueError(f'installed file changed: {name}')


def check_host(package, manifest):
    # Execute verified source directly: importlib can consume unlisted cached bytecode.
    source = package / 'tools/preview_host.py'
    host = types.ModuleType('preview_manager_host')
    host.__file__ = str(source)
    exec(compile(source.read_bytes(), str(source), 'exec'), host.__dict__)
    actual = host.inspect_host(package / 'bin/kovenc')
    errors = host.compare_hosts(manifest['host'], actual)
    if errors:
        raise ValueError(f'host does not satisfy package: {errors}')


def install(root, archive, checksum):
    """Snapshot, run the existing consumer, then publish without touching current."""
    work = Path(tempfile.mkdtemp(prefix='.install-', dir=root))
    result = None
    try:
        archive = Path(archive).absolute()
        snapshot = work / archive.name
        shutil.copyfile(archive, snapshot)
        checksum_copy = work / 'input.sha256'
        shutil.copyfile(checksum, checksum_copy)
        manifest, _ = CONSUMER.verify_archive(snapshot, checksum_copy)
        version = f"{manifest['source_commit']}-{manifest['target']}"
        destination = root / 'versions' / version
        if destination.exists() or destination.is_symlink():
            raise ValueError('version already installed; refusing overwrite')
        # Keep failure evidence outside the temporary install tree.
        evidence = Path(tempfile.mkdtemp(prefix='evidence-', dir=root))
        with redirect_stdout(sys.stderr):
            status = CONSUMER.check(snapshot, checksum_copy, evidence, work_parent=evidence)
        if status != 0:
            raise ValueError(f'preview validation failed; evidence: {evidence}')
        extracted = work / 'extracted'
        extracted.mkdir()
        package = CONSUMER.extract_verified(snapshot, extracted)
        package.rename(destination)
        result = {'id': version, 'evidence': str(evidence)}
    finally:
        try:
            shutil.rmtree(work)
        except OSError as error:
            warning = f'temporary files retained at {work}: {error}'
            if result is not None:
                result['cleanup_warning'] = warning
            else:
                print(warning, file=sys.stderr)
    return result


def manage(root, command, *, version=None, archive=None, checksum=None):
    with locked_root(root) as root:
        current = current_version(root)
        if command == 'install':
            return install(root, archive, checksum)
        if command == 'list':
            versions = []
            for entry in sorted((root / 'versions').iterdir()):
                manifest = identity(package_path(root, entry.name))
                versions.append({'id': entry.name, 'source_commit': manifest['source_commit'],
                                 'target': manifest['target']})
            return {'current': current, 'versions': versions}
        if command == 'deactivate':
            if current is not None:
                (root / 'current').unlink()
            return {'current': None}
        package = package_path(root, version)
        manifest = identity(package)
        if command == 'use':
            verify_installed(package, manifest)
            check_host(package, manifest)
            descriptor, temporary = tempfile.mkstemp(prefix='.activate-', dir=root)
            os.close(descriptor)
            link = Path(temporary)
            try:
                link.unlink()
                link.symlink_to(f'versions/{version}')
                # Publication is the last fallible operation on the success path.
                os.replace(link, root / 'current')
            except OSError:
                link.unlink(missing_ok=True)
                raise
            return {'current': version}
        if command == 'uninstall':
            if current == version:
                raise ValueError('deactivate or select another version before uninstalling current')
            shutil.rmtree(package)
            return {'removed': version}
        raise ValueError(f'unknown operation: {command}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', required=True, type=Path)
    commands = parser.add_subparsers(dest='command', required=True)
    install_parser = commands.add_parser('install')
    install_parser.add_argument('--archive', required=True, type=Path)
    install_parser.add_argument('--sha256', required=True, type=Path)
    for name in ('use', 'uninstall'):
        commands.add_parser(name).add_argument('version')
    for name in ('list', 'deactivate'):
        commands.add_parser(name)
    args = parser.parse_args()
    try:
        result = manage(args.root, args.command, version=getattr(args, 'version', None),
                        archive=getattr(args, 'archive', None), checksum=getattr(args, 'sha256', None))
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f'preview manager: {error}', file=sys.stderr)
        return 1
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main())
