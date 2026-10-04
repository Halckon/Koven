"""Mock producer protocol tests; no native Cargo build is performed here."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
SOURCE = 'registry+https://github.com/rust-lang/crates.io-index'


def load_producer():
    spec = importlib.util.spec_from_file_location('preview_package_test', ROOT / 'scripts/package_preview.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class PreviewPackageProtocol(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root = self.base / 'repo'
        self.root.mkdir()
        self.output = self.base / 'candidate'
        self.producer = load_producer()
        for directory in ('scripts', 'docs/tutorials'):
            (self.root / directory).mkdir(parents=True)
        for name in ('check_tutorial.py', 'check_preview_install.py', 'preview_host.py',
                     'install_preview_dependencies.sh', 'install_ci_llvm.sh'):
            shutil.copyfile(ROOT / 'scripts' / name, self.root / 'scripts' / name)
        for name in ('examples.json', 'koven-tour.md'):
            shutil.copyfile(ROOT / 'docs/tutorials' / name, self.root / 'docs/tutorials' / name)
        (self.root / 'Cargo.toml').write_text('[workspace.package]\nversion="0.1.0"\nlicense="MIT OR Apache-2.0"\n')
        for name in ('LICENSE', 'LICENSE-MIT', 'LICENSE-APACHE'):
            (self.root / name).write_text(name + ' mock project text\n')
        self.binary = self.root / 'target/mock-target/release/kovenc'
        self.binary.parent.mkdir(parents=True)
        self.binary.write_bytes(b'mock native executable')
        self.artifacts = []
        self.packages = []
        self.add_package('lang-cli', '0.1.0', local=True, kind='bin', executable=str(self.binary))
        self.add_package('target-lib', '1.0.0')
        self.add_package('build-tool', '2.0.0', kind='proc-macro', opt='0')
        llvm_sys = self.add_package('llvm-sys', '211.0.1')
        (llvm_sys / 'wrappers').mkdir()
        (llvm_sys / 'wrappers/target.c').write_text('mock LLVM wrapper source')
        self.normal = ('lang-cli v0.1.0 (' + str(self.root / 'crates/lang-cli') + ')\n'
                       'target-lib v1.0.0\nllvm-sys v211.0.1\n')
        self.write_lock()
        self.sysroot = self.base / 'rust-sysroot'
        (self.sysroot / 'bin').mkdir(parents=True)
        self.rustc = self.sysroot / 'bin/rustc'
        self.rustc.write_bytes(b'mock rustc')
        self.rustc.chmod(0o755)
        doc = self.sysroot / 'share/doc/rust'
        (doc / 'licenses').mkdir(parents=True)
        (doc / 'COPYRIGHT-library.html').write_text('mock standard library copyright')
        (doc / 'licenses/MIT.txt').write_text('mock Rust license')
        self.llvm = self.base / 'llvm'
        (self.llvm / 'include/llvm/Support').mkdir(parents=True)
        (self.llvm / 'include/llvm-c').mkdir()
        (self.llvm / 'LICENSE.TXT').write_text('mock LLVM license')
        (self.llvm / 'include/llvm/Support/LICENSE.TXT').write_text('mock LLVM support license')
        (self.llvm / 'include/llvm-c/Target.h').write_text('mock static inline header')
        self.host = {'system': 'Darwin', 'arch': 'arm64', 'llvm_prefix': str(self.llvm),
                     'llvm_version': '21.1.8', 'libraries': [{'name': 'libLLVM.dylib',
                                                           'sha256': 'f' * 64}],
                     'os_version': 'mock', 'minimum_os': 'mock'}
        self.status = ''
        self.commit = 'a' * 40
        self.build_finished = True
        self.calls = []
        self.build_environment = None

    def add_package(self, name, version, local=False, kind='lib', opt='3', executable=None, license_text=True):
        directory = (self.root / 'crates' / name if local else self.base / 'registry' / f'{name}-{version}')
        directory.mkdir(parents=True)
        package = f'name="{name}"\n' + ('version.workspace=true\n' if local else f'version="{version}"\nlicense="MIT"\nrepository="https://example.org/source"\n')
        (directory / 'Cargo.toml').write_text('[package]\n' + package)
        if license_text and not local:
            (directory / 'LICENSE-MIT').write_text(f'{name} mock copyright\n')
        self.packages.append({'name': name, 'version': version, **({} if local else
                             {'source': SOURCE, 'checksum': hashlib.sha256(name.encode()).hexdigest()})})
        self.artifacts.append({'reason': 'compiler-artifact', 'package_id':
                               f'path+file://{directory}#{version}' if local else f'{SOURCE}#{name}@{version}',
                               'manifest_path': str(directory / 'Cargo.toml'),
                               'target': {'kind': [kind], 'name': 'kovenc' if executable else name},
                               'profile': {'opt_level': opt, 'test': False}, 'executable': executable,
                               'filenames': [str(self.binary)] if executable else []})
        return directory

    def write_lock(self):
        content = 'version=4\n'
        for row in self.packages:
            content += '\n[[package]]\n' + ''.join(f'{key}="{value}"\n' for key, value in row.items())
        (self.root / 'Cargo.lock').write_text(content)

    def command(self, command, **kwargs):
        self.calls.append(command)
        if command[:2] == ['git', 'status']:
            output = self.status
        elif command[:3] == ['git', 'rev-parse', 'HEAD']:
            output = self.commit + '\n'
        elif command[:3] == ['git', 'rev-parse', 'HEAD^{tree}']:
            output = 'b' * 40 + '\n'
        elif command == ['rustup', 'which', 'rustc']:
            output = str(self.rustc) + '\n'
        elif command in (['rustc', '-vV'], [str(self.rustc.resolve()), '-vV']):
            output = 'rustc 1.96.0\nrelease: 1.96.0\nhost: aarch64-apple-darwin\ncommit-hash: ' + 'c' * 40 + '\n'
        elif command in (['rustc', '--print', 'sysroot'], [str(self.rustc.resolve()), '--print', 'sysroot']):
            output = str(self.sysroot) + '\n'
        elif command == ['cargo', '--version']:
            output = 'cargo 1.96.0\n'
        elif command[:2] == ['cargo', 'build']:
            self.build_environment = kwargs.get('env')
            output = '\n'.join(json.dumps(row) for row in [*self.artifacts,
                                    {'reason': 'build-finished', 'success': self.build_finished}]) + '\n'
        elif command[:2] == ['cargo', 'tree']:
            output = self.normal
        else:
            raise AssertionError(f'unexpected mock command: {command}')
        return subprocess.CompletedProcess(command, 0, output.encode(), b'')

    def package(self):
        with mock.patch.object(self.producer.subprocess, 'run', side_effect=self.command), \
                mock.patch.object(self.producer.HOST, 'inspect_host', return_value=self.host):
            return self.producer.package(self.output, root=self.root)

    def manifest(self):
        with tarfile.open(next(self.output.glob('*.tar.gz'))) as archive:
            return json.load(archive.extractfile('koven-preview/manifest.json'))

    def test_real_artifact_protocol_builds_target_and_distinguishes_build_materials(self):
        self.assertEqual(self.package(), 0)
        manifest = self.manifest()
        self.assertEqual(manifest['schema'], 'koven.preview.v1')
        self.assertEqual(manifest['source_commit'], self.commit)
        self.assertEqual(manifest['source_tree'], 'b' * 40)
        self.assertEqual(manifest['target'], 'aarch64-apple-darwin')
        command = next(command for command in self.calls if command[:2] == ['cargo', 'build'])
        self.assertEqual(command, ['cargo', 'build', '--locked', '--release', '-p', 'lang-cli', '--bin',
                                   'kovenc', '--target', 'aarch64-apple-darwin', '--message-format=json'])
        tree = next(command for command in self.calls if command[:2] == ['cargo', 'tree'])
        self.assertIn('normal,no-proc-macro', tree)
        self.assertIn('--no-dedupe', tree)
        dependencies = manifest['components']['crates']
        target = next(row for row in dependencies if row['name'] == 'target-lib')
        macro = next(row for row in dependencies if row['name'] == 'build-tool')
        project = next(row for row in dependencies if row['name'] == 'lang-cli')
        self.assertEqual(target['roles'], ['target-normal'])
        self.assertEqual(macro['roles'], ['build-proc-macro'])
        self.assertIsNone(project['license'])
        self.assertEqual(project['license_scope'], 'repository materials; member has no license declaration')
        self.assertTrue((self.output / 'evidence/build.stdout').exists())
        self.assertEqual((self.output / 'check_preview_install.py').read_bytes(),
                         (ROOT / 'scripts/check_preview_install.py').read_bytes())
        with tarfile.open(next(self.output.glob('*.tar.gz'))) as archive:
            names = archive.getnames()
            self.assertIn('koven-preview/examples/parameter-report/contract.json', names)
            self.assertFalse(any('target/' in name or 'Cargo.lock' in name for name in names))
            self.assertEqual(names, sorted(names))
            for member in archive.getmembers():
                self.assertTrue(member.isfile())
                self.assertEqual((member.uid, member.gid, member.mtime), (0, 0, 0))
                if member.name != 'koven-preview/manifest.json':
                    metadata = manifest['files'][member.name.removeprefix('koven-preview/')]
                    self.assertEqual(metadata['mode'], member.mode)
                    self.assertEqual(metadata['sha256'], hashlib.sha256(archive.extractfile(member).read()).hexdigest())
            for name in ('provenance/build.json', 'provenance/components.json'):
                self.assertNotIn(str(self.base), archive.extractfile('koven-preview/' + name).read().decode())

    def test_dirty_checkout_and_invalid_output_fail_before_cargo_build(self):
        self.status = '?? changed.py\n'
        self.assertEqual(self.package(), 1)
        self.assertFalse(any(command[:2] == ['cargo', 'build'] for command in self.calls))
        for output in (self.root / 'candidate', self.base):
            with self.subTest(output=output):
                self.output = output
                with self.assertRaises(ValueError):
                    self.package()

    def test_failed_missing_and_wrong_profile_artifacts_never_publish_tar(self):
        mutations = (lambda: setattr(self, 'build_finished', False),
                     lambda: self.artifacts[0].update(executable=None),
                     lambda: self.artifacts[0]['profile'].update(opt_level='0'),
                     lambda: self.artifacts[0]['profile'].update(test=True),
                     lambda: self.artifacts.pop(1))
        original = json.loads(json.dumps(self.artifacts))
        for index, mutate in enumerate(mutations):
            with self.subTest(index=index):
                self.output = self.base / f'failed-{index}'
                self.artifacts = json.loads(json.dumps(original))
                self.build_finished = True
                mutate()
                self.assertEqual(self.package(), 1)
                self.assertFalse(list(self.output.glob('*.tar.gz')))
                self.assertFalse(json.loads((self.output / 'producer-results.json').read_text())['success'])

    def test_missing_license_and_standard_library_material_fail(self):
        (self.base / 'registry/target-lib-1.0.0/LICENSE-MIT').unlink()
        self.assertEqual(self.package(), 1)
        self.assertFalse(list(self.output.glob('*.tar.gz')))
        (self.base / 'registry/target-lib-1.0.0/LICENSE-MIT').write_text('mock')
        (self.sysroot / 'share/doc/rust/COPYRIGHT-library.html').unlink()
        self.output = self.base / 'missing-std'
        self.assertEqual(self.package(), 1)
        self.assertFalse(list(self.output.glob('*.tar.gz')))

    def test_changed_source_after_build_is_rejected(self):
        original = self.command
        def command(args, **kwargs):
            result = original(args, **kwargs)
            if args[:2] == ['cargo', 'build']:
                self.commit = 'd' * 40
            return result
        with mock.patch.object(self.producer.subprocess, 'run', side_effect=command), \
                mock.patch.object(self.producer.HOST, 'inspect_host', return_value=self.host):
            self.assertEqual(self.producer.package(self.output, root=self.root), 1)
        self.assertFalse(list(self.output.glob('*.tar.gz')))

    def test_registry_identity_requires_matching_lock_source_checksum(self):
        self.packages[1]['source'] = 'registry+https://unapproved.example/index'
        self.write_lock()
        self.assertEqual(self.package(), 1)
        self.assertFalse(list(self.output.glob('*.tar.gz')))

    def test_macro_root_license_fallback_requires_same_vcs_commit(self):
        self.artifacts.pop(2)
        self.packages.pop(2)
        root = self.add_package('inkwell', '0.10.0', license_text=False)
        (root / 'LICENSE').write_text('mock Apache 2.0')
        (root / '.cargo_vcs_info.json').write_text(json.dumps({'git': {'sha1': 'e' * 40}, 'path_in_vcs': ''}))
        macro = self.add_package('inkwell_internals', '0.15.0', kind='proc-macro', opt='0', license_text=False)
        (macro / '.cargo_vcs_info.json').write_text(json.dumps({'git': {'sha1': 'e' * 40}, 'path_in_vcs': 'internal_macros'}))
        self.normal += 'inkwell v0.10.0\n'
        self.write_lock()
        self.assertEqual(self.package(), 0)
        internals = next(row for row in self.manifest()['components']['crates'] if row['name'] == 'inkwell_internals')
        self.assertIn('same VCS commit', internals['materials'][0]['mapping'])
        (macro / '.cargo_vcs_info.json').write_text(json.dumps({'git': {'sha1': 'f' * 40}, 'path_in_vcs': 'internal_macros'}))
        self.output = self.base / 'mismatched-commit'
        self.assertEqual(self.package(), 1)

    def test_produced_mock_package_satisfies_external_consumer_preflight(self):
        self.assertEqual(self.package(), 0)
        spec = importlib.util.spec_from_file_location('preview_consumer_package_test', ROOT / 'scripts/check_preview_install.py')
        consumer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(consumer)
        archive = next(self.output.glob('*.tar.gz'))
        manifest, row = consumer.verify_archive(archive, self.output / (archive.name + '.sha256'))
        self.assertEqual(row['id'], 'parameter-report')
        self.assertEqual(len(row['cases']), 4)
        self.assertEqual(manifest['files']['bin/kovenc']['mode'], 0o755)

    def test_host_failure_preserves_raw_evidence_and_never_creates_archive(self):
        def inspect(binary, evidence=None):
            evidence['commands'].append({'argv': ['/usr/bin/otool', '-L', str(binary)],
                                         'exit': 1, 'stdout_base64': '', 'stderr_base64': 'ZXJyb3I='})
            evidence['partial']['llvm_version'] = '21.1.8'
            raise ValueError('loading closure unavailable')
        with mock.patch.object(self.producer.subprocess, 'run', side_effect=self.command), \
                mock.patch.object(self.producer.HOST, 'inspect_host', side_effect=inspect):
            self.assertEqual(self.producer.package(self.output, root=self.root), 1)
        self.assertFalse(list(self.output.glob('*.tar.gz')))
        evidence = json.loads((self.output / 'evidence/host-inspection.json').read_text())
        self.assertEqual(evidence['commands'][0]['exit'], 1)
        self.assertEqual(evidence['partial']['llvm_version'], '21.1.8')

    def test_license_file_and_notices_are_carried_with_source_hashes(self):
        directory = self.base / 'registry/target-lib-1.0.0'
        (directory / 'terms').mkdir()
        (directory / 'terms/custom.txt').write_text('custom license text')
        (directory / 'NOTICE-extra').write_text('required mock notice')
        with (directory / 'Cargo.toml').open('a') as manifest:
            manifest.write('license-file="terms/custom.txt"\n')
        self.assertEqual(self.package(), 0)
        row = next(row for row in self.manifest()['components']['crates'] if row['name'] == 'target-lib')
        self.assertEqual({Path(row['path']).name for row in row['materials']},
                         {'LICENSE-MIT', 'NOTICE-extra', 'custom.txt'})

    def test_missing_llvm_header_is_not_replaced_with_downloaded_material(self):
        (self.llvm / 'include/llvm-c/Target.h').unlink()
        self.assertEqual(self.package(), 1)
        self.assertFalse(list(self.output.glob('*.tar.gz')))
        self.assertFalse(any(args[0] in ('curl', 'wget') for args in self.calls))

    def test_build_uses_encoded_remap_flags_and_preserves_required_external_environment(self):
        cargo_home = self.base / 'cargo 中文 directory'
        host_home = self.base / 'host 中文 directory'
        with mock.patch.dict(os.environ, {'CARGO_HOME': str(cargo_home), 'HOME': str(host_home),
                                         'LLVM_SYS_211_PREFIX': str(self.llvm),
                                         'CARGO_TARGET_DIR': str(self.base / 'custom target')}):
            self.assertEqual(self.package(), 0)
        environment = self.build_environment
        self.assertIsNotNone(environment)
        flags = environment['CARGO_ENCODED_RUSTFLAGS'].split('\x1f')
        pairs = [flag.removeprefix('--remap-path-prefix=').rsplit('=', 1) for flag in flags]
        self.assertEqual(dict(pairs), {str(self.root.resolve()): '/koven/source',
                                      str(self.sysroot.resolve()): '/koven/rust-toolchain',
                                      str(cargo_home.resolve()): '/koven/cargo',
                                      str(host_home.resolve()): '/koven/host-home'})
        self.assertEqual(environment['RUSTC'], str(self.rustc.resolve()))
        self.assertIn([str(self.rustc.resolve()), '-vV'], self.calls)
        self.assertNotIn(['rustc', '-vV'], self.calls)
        self.assertEqual(environment['LLVM_SYS_211_PREFIX'], str(self.llvm))
        self.assertEqual(environment['CARGO_TARGET_DIR'], str(self.base / 'custom target'))
        raw = json.loads((self.output / 'evidence/build.json').read_text())['env']
        self.assertEqual(raw['CARGO_ENCODED_RUSTFLAGS'], environment['CARGO_ENCODED_RUSTFLAGS'])
        provenance = self.manifest()
        self.assertNotIn(str(self.base), json.dumps(provenance['path_remaps']))
        self.assertEqual({row['target_prefix'] for row in provenance['path_remaps']},
                         {'/koven/source', '/koven/rust-toolchain', '/koven/cargo', '/koven/host-home'})

    def test_existing_rust_flags_and_compiler_selection_are_rejected_before_build(self):
        for index, name in enumerate(('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC', 'RUSTC_WRAPPER',
                                      'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTC',
                                      'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER')):
            with self.subTest(name=name):
                self.output = self.base / f'unsafe-env-{index}'
                self.calls = []
                with mock.patch.dict(os.environ, {name: 'external override'}):
                    self.assertEqual(self.package(), 1)
                self.assertFalse(any(args[:2] == ['cargo', 'build'] for args in self.calls))
                self.assertFalse(list(self.output.glob('*.tar.gz')))

    def test_private_binary_prefixes_fail_instead_of_binary_rewriting(self):
        cargo_home, host_home = self.base / 'cargo-home', self.base / 'host-home'
        for index, prefix in enumerate((self.root, self.sysroot, cargo_home, host_home)):
            with self.subTest(prefix=prefix):
                self.output = self.base / f'private-binary-{index}'
                data = b'native-prefix\0' + str(prefix.resolve()).encode() + b'/panic/source.rs\0'
                self.binary.write_bytes(data)
                with mock.patch.dict(os.environ, {'CARGO_HOME': str(cargo_home), 'HOME': str(host_home)}):
                    self.assertEqual(self.package(), 1)
                self.assertEqual(self.binary.read_bytes(), data, 'producer must never patch a binary')
                self.assertFalse(list(self.output.glob('*.tar.gz')))

    def test_missing_or_changed_selected_argv_contract_never_publishes_success(self):
        original = json.loads((self.root / 'docs/tutorials/examples.json').read_text())
        for index, mutation in enumerate((lambda row: row['cases'].pop(),
                                           lambda row: row['cases'][1].update(args=['wrong']))):
            with self.subTest(index=index):
                self.output = self.base / f'bad-contract-{index}'
                manifest = json.loads(json.dumps(original))
                row = next(row for row in manifest['examples'] if row['id'] == 'parameter-report')
                mutation(row)
                (self.root / 'docs/tutorials/examples.json').write_text(json.dumps(manifest))
                self.assertEqual(self.package(), 1)
                self.assertFalse(json.loads((self.output / 'producer-results.json').read_text())['success'])

    def test_other_rust_toolchain_version_is_rejected_before_cargo_build(self):
        original = self.command
        def command(args, **kwargs):
            result = original(args, **kwargs)
            if args[-1:] == ['-vV']:
                result.stdout = result.stdout.replace(b'release: 1.96.0', b'release: 1.97.0')
            return result
        with mock.patch.object(self.producer.subprocess, 'run', side_effect=command):
            self.assertEqual(self.producer.package(self.output, root=self.root), 1)
        self.assertFalse(any(args[:2] == ['cargo', 'build'] for args in self.calls))

    def test_final_archive_preflight_failure_cannot_publish_success(self):
        spec = importlib.util.spec_from_file_location('preview_consumer_final_test', ROOT / 'scripts/check_preview_install.py')
        consumer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(consumer)
        with mock.patch.object(self.producer.subprocess, 'run', side_effect=self.command), \
                mock.patch.object(self.producer.HOST, 'inspect_host', return_value=self.host), \
                mock.patch.object(self.producer, 'import_tool', return_value=consumer), \
                mock.patch.object(consumer, 'verify_archive', side_effect=ValueError('invalid assembled archive')):
            self.assertEqual(self.producer.package(self.output, root=self.root), 1)
        result = json.loads((self.output / 'producer-results.json').read_text())
        self.assertFalse(result['success'])
        self.assertIn('invalid assembled archive', result['failure'])
        self.assertFalse((self.output / 'check_preview_install.py').exists())

    def test_stable_archive_bytes_for_same_inputs(self):
        self.assertEqual(self.package(), 0)
        original = next(self.output.glob('*.tar.gz')).read_bytes()
        self.output = self.base / 'candidate-again'
        self.assertEqual(self.package(), 0)
        self.assertEqual(next(self.output.glob('*.tar.gz')).read_bytes(), original)


if __name__ == '__main__':
    unittest.main()
