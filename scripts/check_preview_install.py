#!/usr/bin/env python3
"""Validate and consume one preview archive without a checkout or Cargo build."""

import argparse
import base64
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time

PREFIX = 'koven-preview'
EXAMPLE = 'examples/parameter-report'
TOOLS = ('check_tutorial.py', 'check_preview_install.py', 'preview_host.py',
         'install_preview_dependencies.sh', 'install_ci_llvm.sh')
TARGET_HOSTS = {'aarch64-apple-darwin': ('Darwin', 'arm64'),
                'x86_64-unknown-linux-gnu': ('Linux', 'x86_64')}
REQUIRED = {'bin/kovenc', 'README.md', 'licenses/NOTICE',
            f'{EXAMPLE}/project.toml', f'{EXAMPLE}/contract.json',
            *(f'tools/{name}' for name in TOOLS),
            *(f'{EXAMPLE}/src/app/{name}.ko' for name in ('model', 'processor', 'main'))}
ENVIRONMENT_KEYS = ('PATH', 'CC', 'CXX', 'LLVM_SYS_211_PREFIX', 'LLVM_CONFIG_PATH',
                    'DYLD_LIBRARY_PATH', 'DYLD_FALLBACK_LIBRARY_PATH', 'LD_LIBRARY_PATH',
                    'SDKROOT', 'DEVELOPER_DIR', 'MACOSX_DEPLOYMENT_TARGET',
                    'LD_PRELOAD', 'DYLD_INSERT_LIBRARIES', 'LANG', 'LC_ALL', 'TMPDIR')


def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def safe_path(name):
    """Require canonical archive paths before any write or package import."""
    path = PurePosixPath(name)
    if (not name or path.is_absolute() or any(part in ('', '.', '..') for part in name.split('/'))
            or '\\' in name or path.as_posix() != name):
        raise ValueError(f'unsafe package path: {name!r}')
    return path


def load_json(data):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError(f'duplicate JSON key: {key}')
            result[key] = value
        return result
    return json.loads(data, object_pairs_hook=pairs)


def verify_archive(archive, checksum):
    """Preflight the complete archive and manifest; never execute unchecked tools."""
    line = checksum.read_text(encoding='utf-8').strip()
    match = re.fullmatch(r'([a-fA-F0-9]{64})\s+\*?([^\n]+)', line)
    if not match or match[2] != archive.name or match[1].lower() != sha256(archive):
        raise ValueError('archive SHA256 or checksum filename mismatch')
    with tarfile.open(archive, 'r:*') as tar:
        entries = {}
        for member in tar.getmembers():
            # A trailing slash is the conventional directory spelling only.
            name = member.name[:-1] if member.isdir() and member.name.endswith('/') else member.name
            path = safe_path(name)
            if path.parts[0] != PREFIX or member.type not in (tarfile.REGTYPE, tarfile.AREGTYPE, tarfile.DIRTYPE):
                raise ValueError(f'unsupported archive member: {member.name}')
            if name in entries:
                raise ValueError(f'duplicate archive member: {name}')
            entries[name] = member
        manifest_member = entries.get(f'{PREFIX}/manifest.json')
        if manifest_member is None or not manifest_member.isfile():
            raise ValueError('missing package manifest')
        manifest = load_json(tar.extractfile(manifest_member).read())
        if (manifest.get('schema') != 'koven.preview.v1'
                or not re.fullmatch(r'[a-f0-9]{40}', manifest.get('source_commit', ''))
                or not isinstance(manifest.get('target'), str) or not manifest['target']
                or manifest.get('profile') != 'release' or not isinstance(manifest.get('host'), dict)
                or not isinstance(manifest.get('files'), dict)):
            raise ValueError('invalid preview manifest identity or files')
        declared = manifest['files']
        if (manifest['target'] not in TARGET_HOSTS or
                (manifest['host'].get('system'), manifest['host'].get('arch')) != TARGET_HOSTS[manifest['target']]):
            raise ValueError('preview target and declared host must agree')
        if (not re.fullmatch(r'[a-f0-9]{40}', manifest.get('source_tree', ''))
                or not re.fullmatch(r'[a-f0-9]{64}', manifest.get('lock_sha256', ''))
                or not isinstance(manifest.get('rustc'), str)
                or f"host: {manifest['target']}" not in manifest['rustc'].splitlines()
                or 'release: 1.96.0' not in manifest['rustc'].splitlines()
                or not isinstance(manifest.get('cargo'), str) or not manifest['cargo'].startswith('cargo 1.96.0')
                or not isinstance(manifest.get('build_argv'), list) or not manifest['build_argv']
                or any(not isinstance(arg, str) for arg in manifest['build_argv'])
                or '--locked' not in manifest['build_argv'] or '--release' not in manifest['build_argv']
                or not re.fullmatch(r'[a-f0-9]{64}', manifest.get('binary_sha256', ''))):
            raise ValueError('missing or invalid source/build identity')
        if not REQUIRED <= set(declared):
            raise ValueError('missing required package files')
        if manifest['binary_sha256'] != declared['bin/kovenc'].get('sha256'):
            raise ValueError('binary identity differs from the declared CLI file')
        for name, metadata in declared.items():
            safe_path(name)
            if name not in REQUIRED and not name.startswith(('licenses/', 'provenance/')):
                raise ValueError(f'undeclared package layout: {name}')
            if (not isinstance(metadata, dict) or set(metadata) != {'sha256', 'size', 'mode'}
                    or not isinstance(metadata['sha256'], str)
                    or not re.fullmatch(r'[a-f0-9]{64}', metadata['sha256'])
                    or type(metadata['size']) is not int or metadata['size'] < 0
                    or type(metadata['mode']) is not int or metadata['mode'] not in (0o644, 0o755)):
                raise ValueError(f'invalid file metadata: {name}')
        expected_files = {f'{PREFIX}/{name}' for name in declared} | {f'{PREFIX}/manifest.json'}
        actual_files = {name for name, member in entries.items() if member.isfile()}
        if actual_files != expected_files:
            raise ValueError('archive file list differs from manifest')
        allowed_dirs = {PREFIX}
        for name in expected_files:
            allowed_dirs.update(parent.as_posix() for parent in PurePosixPath(name).parents
                                if parent.as_posix() != '.')
        if any(member.isdir() and name not in allowed_dirs for name, member in entries.items()):
            raise ValueError('extra archive directory')
        for name, metadata in declared.items():
            member = entries[f'{PREFIX}/{name}']
            data = tar.extractfile(member).read()
            if (member.size != metadata['size'] or member.mode != metadata['mode']
                    or hashlib.sha256(data).hexdigest() != metadata['sha256']):
                raise ValueError(f'package file checksum, size or mode mismatch: {name}')
        if not declared['bin/kovenc']['mode'] & 0o111:
            raise ValueError('packaged CLI is not executable')
        if sha256(Path(__file__).resolve()) != declared['tools/check_preview_install.py']['sha256']:
            raise ValueError('consumer bootstrap differs from packaged consumer')
        row = load_json(tar.extractfile(entries[f'{PREFIX}/{EXAMPLE}/contract.json']).read())
        validate_contract(row)
    return manifest, row


def validate_contract(row):
    """Require all four selected argv shapes and complete byte-oriented oracles."""
    sources = {f'src/app/{name}.ko' for name in ('model', 'processor', 'main')}
    if (not isinstance(row, dict) or row.get('id') != 'parameter-report'
            or row.get('status') != 'executable' or row.get('entry') != 'app.main'
            or not isinstance(row.get('files'), dict) or set(row['files']) != sources
            or any(key in row for key in ('args', 'artifact', 'run'))
            or not isinstance(row.get('cases'), list) or len(row['cases']) != 4):
        raise ValueError('invalid parameter-report contract')
    expected_args = [[], ['alpha'], ['alpha', '你好', 'tail'], ['']]
    outputs = [row.get('build')]
    for case, args in zip(row['cases'], expected_args):
        if not isinstance(case, dict) or set(case) != {'args', 'artifact', 'run'} or case['args'] != args:
            raise ValueError('missing or changed parameter-report argv case')
        outputs.extend((case['artifact'], case['run']))
    for output in outputs:
        if (not isinstance(output, dict) or set(output) != {'exit', 'stdout', 'stderr'}
                or type(output['exit']) is not int or output['exit'] != 0
                or not isinstance(output['stdout'], str) or not isinstance(output['stderr'], str)):
            raise ValueError('incomplete parameter-report output oracle')


def extract_verified(archive, directory):
    """Write only preflighted regular files into our newly-created directory."""
    with tarfile.open(archive, 'r:*') as tar:
        for member in tar.getmembers():
            destination = directory / member.name
            if member.isdir():
                destination.mkdir(parents=True, exist_ok=True)
            else:
                destination.parent.mkdir(parents=True, exist_ok=True)
                with destination.open('xb') as target:
                    shutil.copyfileobj(tar.extractfile(member), target)
                destination.chmod(member.mode)
    return directory / PREFIX


def import_tool(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


def execute(command, expected, directory, records, timeout=120):
    """Keep exact byte evidence even when a command times out or cannot start."""
    environment = dict(os.environ)
    record = {'argv': command, 'cwd': str(directory),
              'env': {key: environment[key] for key in ENVIRONMENT_KEYS if key in environment},
              'timeout_seconds': timeout, 'exit': None, 'timed_out': False}
    started = time.monotonic()
    stdout, stderr = b'', b''
    failure = None
    try:
        result = subprocess.run(command, cwd=directory, env=environment,
                                capture_output=True, stdin=subprocess.DEVNULL, timeout=timeout)
        record['exit'] = result.returncode
        stdout, stderr = result.stdout, result.stderr
    except subprocess.TimeoutExpired as error:
        record['timed_out'] = True
        stdout, stderr = error.output or b'', error.stderr or b''
        failure = 'command timed out'
    except OSError as error:
        failure = f'command could not start: {error}'
    record['elapsed_seconds'] = time.monotonic() - started
    record['stdout_base64'] = base64.b64encode(stdout).decode('ascii')
    record['stderr_base64'] = base64.b64encode(stderr).decode('ascii')
    record['expected'] = expected
    if failure is None:
        if expected is None:
            if record['exit'] != 0:
                failure = 'dependency preparation exited unsuccessfully'
        elif (record['exit'] != expected['exit']
              or stdout != expected['stdout'].encode('utf-8')
              or stderr != expected['stderr'].encode('utf-8')):
            failure = 'exit/stdout/stderr bytes differ from contract'
    record['success'] = failure is None
    if failure:
        record['failure'] = failure
    records.append(record)
    if failure:
        raise AssertionError(f'{command}: {failure}')


def check(archive, checksum, evidence, prepare_dependencies=False, work_parent=None):
    """Run the independent install contract, leaving JSON evidence on every outcome."""
    archive, checksum, evidence = Path(archive).resolve(), Path(checksum).resolve(), Path(evidence).resolve()
    result = {'schema': 'koven.preview.install.v1', 'success': False, 'commands': [],
              'preparation': [], 'archive': str(archive), 'archive_sha256': None,
              'owned_root': None, 'cleanup': {'removed': False}, 'sentinel': {'unchanged': False},
              'preinstalled': {'python': sys.version, 'system': dict(zip(('system', 'node', 'release', 'version', 'arch'), os.uname())),
                               'tools': {name: shutil.which(name) for name in
                                         ('bash', 'cc', 'clang', 'readelf', 'ldd', 'brew',
                                         'llvm-config', 'cargo', 'rustc', 'curl', 'gpg', 'sudo',
                                         'apt-get', 'awk', 'xcrun')}}}
    owned_root = None
    sentinel = None
    sentinel_bytes = b'Koven preview owned-directory cleanup sentinel\n'
    evidence.mkdir(parents=True, exist_ok=True)
    try:
        # Only this fresh root is ever removed; work_parent and evidence are caller-owned.
        owned_root = Path(tempfile.mkdtemp(prefix='koven-preview-', dir=work_parent)).resolve()
        result['owned_root'] = str(owned_root)
        sentinel = owned_root / '同级 哨兵.txt'
        sentinel.write_bytes(sentinel_bytes)
        result['sentinel']['path'] = str(sentinel)
        # Freeze the untrusted archive in our owned root before verification and extraction.
        input_directory = owned_root / 'input'
        input_directory.mkdir()
        snapshot = input_directory / archive.name
        shutil.copyfile(archive, snapshot)
        manifest, row = verify_archive(snapshot, checksum)
        result['archive_sha256'] = sha256(snapshot)
        result['manifest'] = manifest
        install_directory = owned_root / '安装 中文 空格'
        install_directory.mkdir()
        package = extract_verified(snapshot, install_directory)
        result['install_path'] = str(package)
        if prepare_dependencies:
            execute(['bash', str(package / 'tools/install_preview_dependencies.sh')],
                    None, package,
                    result['preparation'], timeout=1200)
        host = import_tool('preview_consumer_host', package / 'tools/preview_host.py')
        result['host_inspection'] = {'commands': [], 'partial': {}}
        consumer_host = host.inspect_host(package / 'bin/kovenc', evidence=result['host_inspection'])
        result['host'] = consumer_host
        errors = host.compare_hosts(manifest['host'], consumer_host)
        if errors:
            raise ValueError(f'consumer host does not satisfy manifest: {errors}')
        tutorial = import_tool('preview_consumer_tutorial', package / 'tools/check_tutorial.py')
        projects = []
        for index, case in enumerate(row['cases']):
            directory = owned_root / f'项目 中文 空格 {index + 1}'
            directory.mkdir()
            projects.append(directory)
            result['project_paths'] = [str(path) for path in projects]
            for path in (*row['files'], 'project.toml'):
                destination = directory / path
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(package / EXAMPLE / path, destination)
            tutorial.run_case(package / 'bin/kovenc', row, case, directory,
                              execute=lambda command, expected, cwd:
                              execute(command, expected, cwd, result['commands']))
        if len(result['commands']) != 12 or not all(record['success'] for record in result['commands']):
            raise ValueError('normal installation did not execute all twelve commands')
        for directory in [install_directory, input_directory, *projects]:
            shutil.rmtree(directory)
        result['sentinel']['unchanged'] = (sentinel.is_file() and not sentinel.is_symlink()
                                           and sentinel.read_bytes() == sentinel_bytes)
        if not result['sentinel']['unchanged']:
            raise ValueError('cleanup changed the sibling sentinel')
        sentinel.unlink()
        owned_root.rmdir()
        result['cleanup']['removed'] = not owned_root.exists()
        if not result['cleanup']['removed']:
            raise ValueError('owned installation directory was not removed')
        result['success'] = True
    except Exception as error:
        result['failure'] = f'{type(error).__name__}: {error}'
        if hasattr(error, 'evidence'):
            result['host_failure'] = error.evidence
        if owned_root is not None:
            result['cleanup']['retained_for_failure'] = str(owned_root)
    finally:
        if sentinel is not None and sentinel.exists():
            try:
                result['sentinel']['unchanged'] = (sentinel.is_file() and not sentinel.is_symlink()
                                                   and sentinel.read_bytes() == sentinel_bytes)
            except OSError as error:
                result['sentinel']['verification_error'] = f'{type(error).__name__}: {error}'
                result['sentinel']['unchanged'] = False
        (evidence / 'results.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n',
                                               encoding='utf-8')
    print(f"preview install: {'passed' if result['success'] else 'failed'}; evidence: {evidence / 'results.json'}")
    return 0 if result['success'] else 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', required=True, type=Path)
    parser.add_argument('--sha256', required=True, type=Path)
    parser.add_argument('--evidence', required=True, type=Path)
    parser.add_argument('--prepare-dependencies', action='store_true')
    parser.add_argument('--work-parent', type=Path)
    args = parser.parse_args()
    raise SystemExit(check(args.archive, args.sha256, args.evidence,
                           prepare_dependencies=args.prepare_dependencies, work_parent=args.work_parent))
