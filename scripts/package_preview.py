#!/usr/bin/env python3
"""Build a clean-commit CLI preview with actual artifact and license provenance."""
import argparse
import gzip
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = 'registry+https://github.com/rust-lang/crates.io-index'
TOOLS = ('check_tutorial.py', 'check_preview_install.py', 'preview_host.py',
         'install_preview_dependencies.sh', 'install_ci_llvm.sh')
COMPILER_OVERRIDES = ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC', 'RUSTC_WRAPPER',
                      'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTC',
                      'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER')
BUILD_ENV_KEYS = (*COMPILER_OVERRIDES, 'PATH', 'HOME', 'CARGO_HOME', 'CARGO_TARGET_DIR',
                  'LLVM_SYS_211_PREFIX', 'RUSTUP_HOME', 'RUSTUP_TOOLCHAIN', 'CC', 'CXX',
                  'SDKROOT', 'MACOSX_DEPLOYMENT_TARGET')


def import_tool(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


TUTORIAL = import_tool('preview_package_tutorial', ROOT / 'scripts/check_tutorial.py')
HOST = import_tool('preview_package_host', ROOT / 'scripts/preview_host.py')


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + '\n', encoding='utf-8')


def command(args, root, evidence, name, timeout=120, environment=None):
    """Keep raw producer commands outside the tar, including failure/timeout output."""
    record = {'argv': args, 'cwd': str(root), 'timeout_seconds': timeout, 'exit': None}
    if environment is not None:
        record['env'] = {key: environment[key] for key in BUILD_ENV_KEYS if key in environment}
    stdout, stderr = b'', b''
    started = time.monotonic()
    try:
        result = subprocess.run(args, cwd=root, capture_output=True, stdin=subprocess.DEVNULL,
                                timeout=timeout, env=environment)
        record['exit'] = result.returncode
        stdout, stderr = result.stdout, result.stderr
        if result.returncode:
            raise RuntimeError(f'{name} exited {result.returncode}')
        return stdout.decode('utf-8')
    except subprocess.TimeoutExpired as error:
        stdout, stderr = error.output or b'', error.stderr or b''
        record['failure'] = 'timed out'
        raise RuntimeError(f'{name} timed out') from error
    except Exception as error:
        record['failure'] = str(error)
        raise
    finally:
        record['elapsed_seconds'] = time.monotonic() - started
        evidence.mkdir(parents=True, exist_ok=True)
        (evidence / f'{name}.stdout').write_bytes(stdout)
        (evidence / f'{name}.stderr').write_bytes(stderr)
        write_json(evidence / f'{name}.json', record)


def build_environment(root, sysroot, compiler, environment):
    """Use encoded remaps so paths with spaces never pass through shell splitting."""
    home = Path.home()
    cargo = Path(environment.get('CARGO_HOME') or home / '.cargo')
    if not cargo.is_absolute():
        cargo = root / cargo
    sources = [('checkout', root, '/koven/source'),
               ('rust-toolchain', sysroot, '/koven/rust-toolchain'),
               ('cargo', cargo, '/koven/cargo'),
               ('host-home', home, '/koven/host-home')]
    remaps = [{'role': role, 'source_prefix': str(path.resolve()), 'target_prefix': target}
              for role, path, target in sources]
    for row in remaps:
        if row['source_prefix'] == '/' or any(character in row['source_prefix'] for character in ('\x1f', '=')):
            raise ValueError('private source prefix cannot be represented as a rustc remap')
    # rustc selects the last matching remap; put more specific paths after home.
    flags = [f"--remap-path-prefix={row['source_prefix']}={row['target_prefix']}"
             for row in sorted(remaps, key=lambda row: len(row['source_prefix']))]
    effective = dict(environment)
    effective['RUSTC'] = str(compiler)
    effective['RUSTC_WRAPPER'] = ''
    effective['RUSTC_WORKSPACE_WRAPPER'] = ''
    effective.pop('RUSTFLAGS', None)
    effective['CARGO_ENCODED_RUSTFLAGS'] = '\x1f'.join(flags)
    private = {str(path.absolute()) for _, path, _ in sources} | {row['source_prefix'] for row in remaps}
    return effective, remaps, sorted(private)


def check_private_binary(binary, prefixes, evidence, stage):
    """Reject surviving known private prefixes; never rewrite an emitted binary."""
    data = binary.read_bytes()
    found = [prefix for prefix in prefixes if prefix.encode('utf-8') in data]
    write_json(evidence / f'binary-private-paths-{stage}.json',
               {'binary': str(binary), 'sha256': digest(data), 'checked_prefixes': prefixes,
                'found_prefixes': found})
    if found:
        raise ValueError(f'binary retains private source prefixes: {found}')


def source_state(root, evidence, suffix):
    status = command(['git', 'status', '--porcelain=v1', '--untracked-files=all'], root,
                     evidence, 'status-' + suffix)
    head = command(['git', 'rev-parse', 'HEAD'], root, evidence, 'head-' + suffix).strip()
    tree = command(['git', 'rev-parse', 'HEAD^{tree}'], root, evidence, 'tree-' + suffix).strip()
    if status or not re.fullmatch(r'[a-f0-9]{40}', head) or not re.fullmatch(r'[a-f0-9]{40}', tree):
        raise ValueError('preview requires a clean checkout and exact Git commit/tree')
    return {'source_commit': head, 'source_tree': tree, 'lock_sha256': digest((root / 'Cargo.lock').read_bytes())}


def parse_build(text):
    rows = [json.loads(line) for line in text.splitlines() if line.strip()]
    finished = [row for row in rows if row.get('reason') == 'build-finished']
    if len(finished) != 1 or finished[0].get('success') is not True:
        raise ValueError('missing or unsuccessful Cargo build-finished record')
    artifacts = [row for row in rows if row.get('reason') == 'compiler-artifact']
    binaries = [row for row in artifacts if row.get('target', {}).get('name') == 'kovenc'
                and row.get('target', {}).get('kind') == ['bin'] and row.get('executable')]
    if len(binaries) != 1:
        raise ValueError('missing or ambiguous real kovenc executable artifact')
    binary = binaries[0]
    manifest = tomllib.loads(Path(binary['manifest_path']).read_text(encoding='utf-8'))
    if manifest.get('package', {}).get('name') != 'lang-cli':
        raise ValueError('kovenc binary artifact does not belong to lang-cli')
    if binary.get('profile', {}).get('opt_level') != '3' or binary['profile'].get('test') is not False:
        raise ValueError('kovenc artifact is not a release non-test build')
    path = Path(binary['executable']).resolve(strict=True)
    if not path.is_file() or path.name != 'kovenc' or path not in {Path(name).resolve() for name in binary.get('filenames', [])}:
        raise ValueError('Cargo executable is not a declared regular kovenc artifact')
    return artifacts, path


def parse_normal_tree(text):
    packages = set()
    for line in text.splitlines():
        match = re.fullmatch(r'([A-Za-z0-9_-]+) v([^\s]+)(?: \([^\n]+\))?', line)
        if not match:
            raise ValueError(f'unrecognized target dependency tree row: {line!r}')
        packages.add(match.groups())
    if not packages:
        raise ValueError('empty target dependency tree')
    return packages


def package_identity(manifest, workspace):
    package = manifest['package']
    version = package['version']
    if version == {'workspace': True}:
        version = workspace['workspace']['package']['version']
    if not isinstance(package['name'], str) or not isinstance(version, str):
        raise ValueError('unsupported Cargo package identity')
    return package['name'], version


def artifact_components(artifacts, normal, root):
    """Cross actual CLI artifacts with its normal tree, never workspace resolve."""
    workspace = tomllib.loads((root / 'Cargo.toml').read_text(encoding='utf-8'))
    locked = tomllib.loads((root / 'Cargo.lock').read_text(encoding='utf-8'))['package']
    components = {}
    seen_normal = set()
    for artifact in artifacts:
        path = Path(artifact['manifest_path']).resolve(strict=True)
        manifest = tomllib.loads(path.read_text(encoding='utf-8'))
        identity = package_identity(manifest, workspace)
        name, version = identity
        lock = [row for row in locked if (row['name'], row['version']) == identity]
        if len(lock) != 1:
            raise ValueError(f'missing or ambiguous locked artifact: {name}@{version}')
        package = manifest['package']
        source = lock[0].get('source')
        package_id = artifact.get('package_id', '')
        if source:
            if (source != REGISTRY or not package_id.startswith(source + '#')
                    or not re.fullmatch(r'[a-f0-9]{64}', lock[0].get('checksum', ''))):
                raise ValueError(f'unsupported or unmatched registry source/checksum: {name}@{version}')
        elif not path.is_relative_to(root) or not package_id.startswith('path+file://'):
            raise ValueError(f'unlocked external path artifact: {name}@{version}')
        roles = set()
        kinds = artifact['target']['kind']
        profile = artifact['profile']
        if ('lib' in kinds or 'bin' in kinds) and profile.get('opt_level') == '3' and profile.get('test') is False:
            if identity not in normal:
                raise ValueError(f'release target artifact absent from normal tree: {identity}')
            roles.add('target-normal')
            seen_normal.add(identity)
        elif 'proc-macro' in kinds:
            roles.add('build-proc-macro')
        else:
            roles.add('build-tool')
        if identity in components:
            components[identity]['roles'].update(roles)
            continue
        license_value = package.get('license')
        if license_value == {'workspace': True}:
            license_value = workspace['workspace']['package']['license']
        repository = package.get('repository')
        if repository and not repository.startswith(('https://', 'http://')):
            raise ValueError('repository provenance must be a public HTTP URL')
        components[identity] = {'name': name, 'version': version, 'source': source or 'project',
                                'checksum': lock[0].get('checksum'), 'license': license_value,
                                'repository': repository, 'roles': roles, '_directory': path.parent,
                                '_license_file': package.get('license-file'),
                                'license_scope': ('crate manifest declaration' if license_value else
                                    'repository materials; member has no license declaration')}
    if seen_normal != normal:
        raise ValueError(f'target normal tree and actual artifacts differ: {sorted(normal - seen_normal)}')
    return [components[key] for key in sorted(components)]


def material(source, destination, package, label, mapping=None):
    """Copy only real nonempty license text and record its original source label."""
    source = Path(source)
    if not source.is_file() or source.is_symlink():
        raise ValueError(f'missing regular license material: {label}')
    data = source.read_bytes()
    if not data.strip():
        raise ValueError(f'empty license material: {label}')
    data.decode('utf-8')
    target = package / destination
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)
    target.chmod(0o644)
    result = {'path': destination, 'sha256': digest(data), 'source': label}
    if mapping:
        result['mapping'] = mapping
    return result


def crate_materials(components, root, package):
    by_identity = {(row['name'], row['version']): row for row in components}
    records = []
    for original in components:
        row = {key: (sorted(value) if key == 'roles' else value)
               for key, value in original.items() if not key.startswith('_')}
        name, version = row['name'], row['version']
        directory = original['_directory']
        materials = []
        if row['source'] == 'project':
            for filename in ('LICENSE', 'LICENSE-MIT', 'LICENSE-APACHE'):
                destination = f'licenses/project/{name}-{version}/{filename}'
                materials.append(material(root / filename, destination, package, f'project/{filename}'))
        else:
            paths = sorted(path for path in directory.iterdir() if path.is_file()
                           and (path.name.startswith(('LICENSE', 'COPYING', 'NOTICE')) or path.name == 'UNLICENSE'))
            license_file = original['_license_file']
            if license_file:
                file = (directory / license_file).resolve(strict=True)
                if not file.is_relative_to(directory):
                    raise ValueError('crate license-file escapes the packaged registry source')
                if file not in paths:
                    paths.append(file)
            if not paths and (name, version) == ('inkwell_internals', '0.15.0'):
                upstream = by_identity.get(('inkwell', '0.10.0'))
                if upstream is None:
                    raise ValueError('inkwell macro license requires actual inkwell root artifact')
                vcs = json.loads((directory / '.cargo_vcs_info.json').read_text(encoding='utf-8'))
                root_vcs = json.loads((upstream['_directory'] / '.cargo_vcs_info.json').read_text(encoding='utf-8'))
                commit = vcs.get('git', {}).get('sha1')
                if (not isinstance(commit, str) or not re.fullmatch(r'[a-f0-9]{40}', commit)
                        or commit != root_vcs.get('git', {}).get('sha1')
                        or vcs.get('path_in_vcs') != 'internal_macros' or root_vcs.get('path_in_vcs') != ''):
                    raise ValueError('inkwell root/macro license VCS mapping mismatch')
                destination = f'licenses/crates/{name}-{version}/LICENSE'
                materials.append(material(upstream['_directory'] / 'LICENSE', destination, package,
                                          'inkwell-0.10.0/LICENSE',
                                          f'same VCS commit {commit}; root license applies to internal_macros'))
            else:
                for path in paths:
                    relative = path.relative_to(directory).as_posix()
                    destination = f'licenses/crates/{name}-{version}/{relative}'
                    materials.append(material(path, destination, package, f'{name}-{version}/{relative}'))
            if not materials:
                raise ValueError(f'missing crate license materials: {name}@{version}')
        row['materials'] = materials
        records.append(row)
    return records


def platform_materials(sysroot, host, components, package):
    rust_doc = sysroot / 'share/doc/rust'
    rust = [material(rust_doc / 'COPYRIGHT-library.html', 'licenses/rust/COPYRIGHT-library.html',
                     package, 'rustc-sysroot/share/doc/rust/COPYRIGHT-library.html')]
    paths = sorted(path for path in (rust_doc / 'licenses').rglob('*') if path.is_file())
    if not paths:
        raise ValueError('missing actual Rust standard library license directory')
    for path in paths:
        relative = path.relative_to(rust_doc / 'licenses').as_posix()
        rust.append(material(path, 'licenses/rust/licenses/' + relative, package,
                             'rustc-sysroot/share/doc/rust/licenses/' + relative))
    prefix = Path(host['llvm_prefix'])
    if host['system'] == 'Darwin':
        llvm_paths = [(prefix / 'LICENSE.TXT', 'LICENSE.TXT'),
                      (prefix / 'include/llvm/Support/LICENSE.TXT', 'Support-LICENSE.TXT')]
    else:
        llvm_paths = [(Path('/usr/share/doc') / name / 'copyright', name + '-copyright')
                      for name in ('llvm-21', 'llvm-21-dev', 'libllvm21')
                      if (Path('/usr/share/doc') / name / 'copyright').is_file()]
        if not llvm_paths:
            raise ValueError('missing installed LLVM 21 Debian copyright material')
    llvm = [material(path, 'licenses/llvm/' + filename, package,
                     'external LLVM installation/' + filename) for path, filename in llvm_paths]
    header = prefix / 'include/llvm-c/Target.h'
    if not header.is_file():
        raise ValueError('missing actual LLVM Target.h input')
    llvm_sys = next((row for row in components if row['name'] == 'llvm-sys'), None)
    if llvm_sys is None or not (llvm_sys['_directory'] / 'wrappers/target.c').is_file():
        raise ValueError('missing actual llvm-sys target wrapper source')
    wrapper = llvm_sys['_directory'] / 'wrappers/target.c'
    return {'rust': {'materials': rust, 'scope': 'Full standard library copyright list may include '
                    'cross-platform build components; this is not the final retained object set.'},
            'llvm': {'version': host['llvm_version'], 'materials': llvm,
                     'header': {'source': 'external LLVM include/llvm-c/Target.h', 'sha256': digest(header.read_bytes())},
                     'wrapper': {'source': f"llvm-sys-{llvm_sys['version']}/wrappers/target.c",
                                 'sha256': digest(wrapper.read_bytes())},
                     'scope': 'LLVM remains external. Header and wrapper are static link inputs; '
                              'input attribution does not claim all generated machine code is retained.'}}


def assemble(package, binary, root, identity, target, rustc, cargo, build_args, host, components, sysroot, remaps, consumer):
    (package / 'bin').mkdir(parents=True)
    shutil.copyfile(binary, package / 'bin/kovenc')
    (package / 'bin/kovenc').chmod(0o755)
    (package / 'tools').mkdir()
    for filename in TOOLS:
        destination = package / 'tools' / filename
        shutil.copyfile(root / 'scripts' / filename, destination)
        destination.chmod(0o755 if filename.endswith('.sh') else 0o644)
    examples = TUTORIAL.load_examples(root)
    rows = [(row, sources) for row, sources in examples if row['id'] == 'parameter-report']
    if len(rows) != 1:
        raise ValueError('missing canonical parameter-report tutorial')
    row, sources = rows[0]
    consumer.validate_contract(row)
    example = package / 'examples/parameter-report'
    for filename, source in sources.items():
        destination = example / filename
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(source, encoding='utf-8')
    (example / 'project.toml').write_text('schema = "koven.project"\nversion = 1\n\n[project]\n'
                                       'name = "tutorial"\nsource-roots = ["src"]\n', encoding='utf-8')
    write_json(example / 'contract.json', row)
    crates = crate_materials(components, root, package)
    platform = platform_materials(sysroot, host, components, package)
    composition = {'crates': crates, **platform}
    (package / 'licenses/NOTICE').write_text(
        'Koven preview candidate, built from the source identity in manifest.json.\n'
        'Project and every actual Cargo artifact input have separate license records.\n'
        'Target-normal records describe conservative target inputs, not every retained object.\n'
        'Build macro/tool records are provenance materials; no macro binaries are bundled.\n'
        'Members lacking a license field do not automatically inherit the workspace declaration.\n'
        'Rust materials include the full standard library copyright list, which may describe\n'
        'cross-platform build components and is not an exact retained-object bill of materials.\n'
        'LLVM is externally installed. LLVM header and llvm-sys wrapper static input attribution\n'
        'does not establish that every input function remains in the final binary.\n', encoding='utf-8')
    (package / 'README.md').write_text(
        '# Koven preview candidate\n\n'
        'This CI artifact is a reviewable candidate, not a public Release, signed/notarized\n'
        'distribution, or a guarantee of minimum operating-system compatibility. The measured\n'
        'OS, architecture, SDK/glibc and actual loading closure are in manifest.json.\n\n'
        'Verify the external SHA256 before extraction. Install into a directory you own.\n'
        'Python 3.11+, bash, and the documented external installation tools are required.\n'
        'On macOS arm64, install Homebrew at /opt/homebrew and the Apple C driver/SDK first;\n'
        'run bash tools/install_preview_dependencies.sh to install external llvm@21.\n'
        'On Ubuntu 24.04 x86_64, the same script installs the signed pinned LLVM 21.1.8\n'
        'packages via the included installer, plus /usr/bin/cc. curl, gnupg (gpg), sudo,\n'
        'apt-get and awk must already be available; sudo/network access is required.\n'
        'No LLVM/system dynamic libraries, Cargo cache, or development checkout are bundled.\n\n'
        'Use the absolute installation path to bin/kovenc. In examples/parameter-report run:\n'
        '`/absolute/install/bin/kovenc build --project project.toml --entry app.main -o program`\n'
        'then `./program alpha "你好" tail` or\n'
        '`/absolute/install/bin/kovenc run --project project.toml --entry app.main -- alpha "你好" tail`.\n'
        'The canonical contract.json contains four complete build/artifact/run byte oracles.\n\n'
        'Independent normal-install acceptance starts with the external same-artifact bootstrap:\n'
        '`python3 check_preview_install.py --archive CANDIDATE.tar.gz --sha256 CANDIDATE.tar.gz.sha256 '
        '--evidence EVIDENCE --prepare-dependencies`. It validates every package file before tools\n'
        'run, checks the actual loading closure, executes all 12 commands in independent Chinese/space\n'
        'paths, and preserves results.json on success/failure. Cleanup targets only owned directories.\n'
        'Dependency versions/closure must match this candidate; unknown or missing inputs fail.\n', encoding='utf-8')
    provenance = {**identity, 'target': target, 'profile': 'release', 'rustc': rustc,
                  'cargo': cargo, 'build_argv': build_args, 'binary_sha256': digest(binary.read_bytes()),
                  'compiler_origin': 'rustup-selected absolute rustc, verified release 1.96.0',
                  'path_remaps': [{key: value for key, value in row.items() if key != 'source_prefix'}
                                  for row in remaps]}
    write_json(package / 'provenance/build.json', provenance)
    write_json(package / 'provenance/components.json', composition)
    files = {}
    for path in sorted(package.rglob('*')):
        if path.is_file():
            mode = 0o755 if path.relative_to(package).as_posix() == 'bin/kovenc' or path.suffix == '.sh' else 0o644
            path.chmod(mode)
            data = path.read_bytes()
            files[path.relative_to(package).as_posix()] = {'sha256': digest(data), 'size': len(data), 'mode': mode}
    manifest = {'schema': 'koven.preview.v1', **provenance, 'host': host, 'components': composition, 'files': files}
    write_json(package / 'manifest.json', manifest)
    return manifest


def archive_package(package, destination):
    """Produce a stable all-regular-file tar with canonical ownership and timestamps."""
    with destination.open('xb') as output:
        with gzip.GzipFile(filename='', fileobj=output, mode='wb', mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as tar:
                for path in sorted(path for path in package.rglob('*') if path.is_file()):
                    data = path.read_bytes()
                    info = tarfile.TarInfo('koven-preview/' + path.relative_to(package).as_posix())
                    info.size = len(data)
                    info.mode = path.stat().st_mode & 0o777
                    info.uid = info.gid = info.mtime = 0
                    info.uname = info.gname = ''
                    tar.addfile(info, io.BytesIO(data))


def package(output, root=ROOT):
    root, output = Path(root).resolve(), Path(output).resolve()
    if output.exists() or output.is_relative_to(root) or root.is_relative_to(output):
        raise ValueError('output must be a new directory outside the checkout')
    output.mkdir(parents=True)
    evidence = output / 'evidence'
    result = {'schema': 'koven.preview.producer.v1', 'success': False}
    try:
        inherited = dict(os.environ)
        overrides = [key for key in COMPILER_OVERRIDES if inherited.get(key)]
        if overrides:
            raise ValueError(f'preview refuses external Rust flags/compiler selection: {overrides}')
        identity = source_state(root, evidence, 'before')
        compiler = Path(command(['rustup', 'which', 'rustc'], root, evidence, 'rustc-selection').strip()).resolve(strict=True)
        rustc = command([str(compiler), '-vV'], root, evidence, 'rustc').strip()
        if re.findall(r'^release: (\S+)$', rustc, re.M) != ['1.96.0']:
            raise ValueError('preview requires actual selected rustc release 1.96.0')
        sysroot = Path(command([str(compiler), '--print', 'sysroot'], root, evidence, 'rust-sysroot').strip())
        effective, remaps, private = build_environment(root, sysroot, compiler, inherited)
        hosts = re.findall(r'^host: (\S+)$', rustc, re.M)
        if len(hosts) != 1 or hosts[0] not in ('aarch64-apple-darwin', 'x86_64-unknown-linux-gnu'):
            raise ValueError('unsupported or missing actual rustc host')
        target = hosts[0]
        cargo = command(['cargo', '--version'], root, evidence, 'cargo').strip()
        build_args = ['cargo', 'build', '--locked', '--release', '-p', 'lang-cli', '--bin',
                      'kovenc', '--target', target, '--message-format=json']
        text = command(build_args, root, evidence, 'build', timeout=1800, environment=effective)
        if source_state(root, evidence, 'after-build') != identity:
            raise ValueError('source commit/tree/lock changed during the build')
        artifacts, binary = parse_build(text)
        check_private_binary(binary, private, evidence, 'after-build')
        tree_args = ['cargo', 'tree', '--locked', '-p', 'lang-cli', '--target', target, '--edges',
                     'normal,no-proc-macro', '--prefix', 'none', '--format', '{p}', '--no-dedupe']
        normal = parse_normal_tree(command(tree_args, root, evidence, 'normal-tree', environment=effective))
        components = artifact_components(artifacts, normal, root)
        host_evidence = {'commands': [], 'partial': {}}
        try:
            host = HOST.inspect_host(binary, evidence=host_evidence)
        finally:
            write_json(evidence / 'host-inspection.json', host_evidence)
        consumer = import_tool('preview_package_consumer', root / 'scripts/check_preview_install.py')
        with tempfile.TemporaryDirectory(prefix='assemble-', dir=output) as temporary:
            assembled = Path(temporary) / 'koven-preview'
            manifest = assemble(assembled, binary, root, identity, target, rustc, cargo, build_args, host, components, sysroot, remaps, consumer)
            check_private_binary(assembled / 'bin/kovenc', private, evidence, 'after-assembly')
            if digest(binary.read_bytes()) != manifest['files']['bin/kovenc']['sha256']:
                raise ValueError('actual Cargo binary changed during package assembly')
            if source_state(root, evidence, 'after-assembly') != identity:
                raise ValueError('source identity changed during package assembly')
            basename = f"koven-preview-{identity['source_commit'][:12]}-{target}"
            archive = output / (basename + '.tar.gz')
            archive_package(assembled, archive)
            (output / (archive.name + '.sha256')).write_text(digest(archive.read_bytes()) + '  ' + archive.name + '\n')
            consumer.verify_archive(archive, output / (archive.name + '.sha256'))
            shutil.copyfile(assembled / 'tools/check_preview_install.py', output / 'check_preview_install.py')
            result.update(success=True, archive=archive.name, manifest=manifest)
    except Exception as error:
        result['failure'] = f'{type(error).__name__}: {error}'
    finally:
        write_json(output / 'producer-results.json', result)
    print(f"preview package: {'passed' if result['success'] else 'failed'}; evidence: {output}")
    return 0 if result['success'] else 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    raise SystemExit(package(args.output))
