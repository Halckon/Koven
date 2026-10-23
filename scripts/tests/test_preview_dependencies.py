"""Execute the shared installer with fail-closed mocks, never real package tools."""
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import unittest

INSTALLER = Path(__file__).resolve().parents[1] / 'install_ci_llvm.sh'
LZMA_VERSION = '5.6.1+really5.4.5-1ubuntu0.4'
DEB_SHA256 = '5bcf5da2339331e21d3f42a831e82c61c54c6b2f29804010e8d59abd0ae52167'
LIB_SHA256 = '26b37d25f7ca2a2d348de8ecabacda12e921cd933b21fa069a179760db77219d'
LIBRARY = '/usr/lib/x86_64-linux-gnu/liblzma.so.5.4.5'
LLVM_VERSION = '1:21.1.8~++20251221032922+2078da43e25a-1~exp1~20251221153059.70'
FINGERPRINT = '6084F3CF814B57C1CF12EFD515CF4D18AF4F7421'

# PATH contains only awk/sort. Every privileged/network/package operation below
# is a shell function, and unexpected invocations fail rather than forwarding.
MOCKS = r'''
set -euo pipefail
mock_installed=0
record() { printf '%q ' "$@" >> "$MOCK_LOG"; printf '\n' >> "$MOCK_LOG"; }
source() {
  [[ "$*" == /etc/os-release ]] || return 97
  ID=ubuntu; VERSION_ID=24.04
  if [[ "$MOCK_MODE" == unsupported_host ]]; then VERSION_ID=22.04; fi
}
uname() { [[ "$*" == -m ]] || return 97; printf 'x86_64\n'; }
mktemp() { [[ $# == 0 ]] || return 97; printf '%s/key\n' "$MOCK_ROOT"; }
rm() { record rm "$@"; [[ "$*" == "-f $MOCK_ROOT/key" ]]; }
curl() {
  record curl "$@"
  [[ "$MOCK_MODE" != key_failure ]] || return 28
  [[ "$*" == "--fail --silent --show-error --location --connect-timeout 15 --max-time 60 --retry 2 --retry-max-time 180 https://apt.llvm.org/llvm-snapshot.gpg.key -o $MOCK_ROOT/key" ]]
}
gpg() {
  record gpg "$@"
  [[ "$*" == "--batch --show-keys --with-colons $MOCK_ROOT/key" ]] || return 97
  printf 'pub:::::::::\n'
  if [[ "$MOCK_MODE" == wrong_key ]]; then
    printf 'fpr:::::::::0000000000000000000000000000000000000000:\n'
  else
    printf 'fpr:::::::::%s:\n' "$MOCK_FINGERPRINT"
  fi
  if [[ "$MOCK_MODE" == multiple_keys ]]; then printf 'pub:::::::::\n'; fi
}
sudo() {
  record sudo "$@"
  if [[ "$1" == apt-get ]]; then
    [[ "${*:2:6}" == '-o Acquire::Retries=2 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30' ]] || return 97
    set -- "$1" "${@:8}"
  fi
  case "$1 $2" in
    'install -m') [[ "$*" == "install -m 0644 $MOCK_ROOT/key /usr/share/keyrings/koven-llvm.asc" ]] ;;
    'tee /etc/apt/sources.list.d/koven-llvm.list')
      local line
      IFS= read -r line
      [[ "$line" == 'deb [arch=amd64 signed-by=/usr/share/keyrings/koven-llvm.asc] https://apt.llvm.org/noble/ llvm-toolchain-noble-21 main' ]] ;;
    'apt-get -o')
      [[ "$*" == 'apt-get -o APT::Update::Error-Mode=any update' ]] || return 97
      [[ "$MOCK_MODE" != update_failure ]] || return 100 ;;
    'apt-get install')
      [[ "$MOCK_MODE" != install_failure ]] || return 100
      mock_installed=1 ;;
    *) return 97 ;;
  esac
}
apt-cache() {
  record apt-cache "$@"
  case "$*" in
    'policy liblzma5:amd64') printf 'liblzma5:amd64: trusted Ubuntu policy (mock)\n' ;;
    "show liblzma5:amd64=$MOCK_VERSION")
      case "$MOCK_MODE" in
        unavailable) return 100 ;;
        status_only) printf 'Package: liblzma5\nVersion: %s\n' "$MOCK_VERSION" ;;
        wrong_index) printf 'SHA256: %064d\n' 0 ;;
        conflicting_index) printf 'SHA256: %s\nSHA256: %064d\n' "$MOCK_DEB_SHA" 0 ;;
        duplicate_index) printf 'SHA256: %s\nSHA256: %s\n' "$MOCK_DEB_SHA" "$MOCK_DEB_SHA" ;;
        *) printf 'SHA256: %s\n' "$MOCK_DEB_SHA" ;;
      esac ;;
    *) return 97 ;;
  esac
}
dpkg() {
  record dpkg "$@"
  [[ "$*" == "--compare-versions $MOCK_BEFORE le $MOCK_VERSION" ]] || return 97
  [[ "$MOCK_MODE" != newer ]]
}
dpkg-query() {
  record dpkg-query "$@"
  case "$1" in
    -W)
      [[ $# == 3 && "$3" == liblzma5:amd64 ]] || return 97
      case "$2" in
        '-f=${Version}') printf '%s' "$MOCK_BEFORE" ;;
        '-f=${Version} ${Architecture}')
          [[ "$mock_installed" == 1 ]] || return 97
          case "$MOCK_MODE" in
            wrong_version) printf 'wrong amd64' ;;
            wrong_arch) printf '%s arm64' "$MOCK_VERSION" ;;
            *) printf '%s amd64' "$MOCK_VERSION" ;;
          esac ;;
        '-f=${binary:Package} ${Version} ${Architecture} ${source:Package} ${source:Version}\n')
          [[ "$mock_installed" == 1 ]] || return 97
          printf 'liblzma5:amd64 %s amd64 xz-utils %s\n' "$MOCK_VERSION" "$MOCK_VERSION" ;;
        *) return 97 ;;
      esac ;;
    -S)
      [[ $# == 2 && "$2" == "$MOCK_LIBRARY" ]] || return 97
      if [[ "$MOCK_MODE" == wrong_owner ]]; then printf 'other: %s\n' "$2"
      else printf 'liblzma5:amd64: %s\n' "$2"; fi ;;
    *) return 97 ;;
  esac
}
readlink() {
  record readlink "$@"
  [[ "$*" == '-e /usr/lib/x86_64-linux-gnu/liblzma.so.5' ]] || return 97
  if [[ "$MOCK_MODE" == wrong_target ]]; then printf '/other/liblzma.so.5.4.5\n'
  else printf '%s\n' "$MOCK_LIBRARY"; fi
}
sha256sum() {
  record sha256sum "$@"
  [[ "$*" == "$MOCK_LIBRARY" ]] || return 97
  if [[ "$MOCK_MODE" == wrong_hash ]]; then printf '%064d  %s\n' 0 "$1"
  else printf '%s  %s\n' "$MOCK_LIB_SHA" "$1"; fi
}
mock_llvm_config() {
  record llvm-config "$@"
  [[ "$*" == --version ]] || return 97
  if [[ "$MOCK_MODE" == wrong_llvm_version ]]; then printf '21.1.9\n'
  else printf '21.1.8\n'; fi
}
mock_clang() {
  record clang "$@"
  [[ "$*" == --print-resource-dir ]] || return 97
  printf '%s/resource\n' "$MOCK_ROOT"
}
'''


class PreviewDependencyInstaller(unittest.TestCase):
    def run_installer(self, mode='success'):
        script = INSTALLER.read_text(encoding='utf-8')
        # Redirect only the two absolute LLVM probes to test-owned functions.
        for tool, replacement in (('llvm-config', 'mock_llvm_config'), ('clang', 'mock_clang')):
            original = f'/usr/lib/llvm-21/bin/{tool}'
            self.assertEqual(script.count(original), 1)
            script = script.replace(original, replacement)
        with tempfile.TemporaryDirectory(prefix='preview-dependencies-') as temporary:
            root = Path(temporary)
            binary = root / 'bin'
            binary.mkdir()
            for tool in ('awk', 'sort'):
                executable = shutil.which(tool)
                self.assertIsNotNone(executable)
                (binary / tool).symlink_to(executable)
            runtime = root / 'resource/lib/linux'
            runtime.mkdir(parents=True)
            for sanitizer in ('asan', 'lsan'):
                if mode != 'missing_sanitizer' or sanitizer != 'lsan':
                    (runtime / f'libclang_rt.{sanitizer}-x86_64.a').touch()
            log = root / 'calls'
            log.touch()
            environment = {'PATH': str(binary), 'LC_ALL': 'C', 'MOCK_ROOT': str(root),
                           'MOCK_LOG': str(log), 'MOCK_MODE': mode, 'MOCK_VERSION': LZMA_VERSION,
                           'MOCK_BEFORE': '99:1' if mode == 'newer' else '5.6.1+really5.4.5-1',
                           'MOCK_LIBRARY': LIBRARY, 'MOCK_DEB_SHA': DEB_SHA256,
                           'MOCK_LIB_SHA': LIB_SHA256, 'MOCK_FINGERPRINT': FINGERPRINT}
            bash = shutil.which('bash')
            self.assertIsNotNone(bash)
            result = subprocess.run([bash, '--noprofile', '--norc'], input=MOCKS + '\n' + script,
                                    text=True, capture_output=True, env=environment, timeout=10)
            calls = [shlex.split(line) for line in log.read_text().splitlines()]
        return result, calls

    @staticmethod
    def installs(calls):
        return [call for call in calls if call[:2] == ['sudo', 'apt-get'] and 'install' in call]

    def test_verified_pins_are_literal(self):
        script = INSTALLER.read_text(encoding='utf-8')
        for name, value in (('lzma_version', LZMA_VERSION), ('lzma_deb_sha256', DEB_SHA256),
                            ('lzma_sha256', LIB_SHA256), ('lzma_library', LIBRARY),
                            ('version', LLVM_VERSION), ('fingerprint', FINGERPRINT)):
            self.assertIn(f"readonly {name}='{value}'", script)

    def test_exact_package_and_actual_bytes_are_required(self):
        result, calls = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.installs(calls), [['sudo', 'apt-get', '-o', 'Acquire::Retries=2',
                         '-o', 'Acquire::http::Timeout=30', '-o', 'Acquire::https::Timeout=30',
                         'install', '--yes',
                         '--no-install-recommends', *[f'{name}={LLVM_VERSION}' for name in
                         ('llvm-21-dev', 'clang-21', 'libclang-cpp21', 'libclang-rt-21-dev')],
                         f'liblzma5:amd64={LZMA_VERSION}', 'build-essential']])
        self.assertIn(['apt-cache', 'show', f'liblzma5:amd64={LZMA_VERSION}'], calls)
        self.assertIn(['sha256sum', LIBRARY], calls)
        self.assertIn('liblzma before:', result.stdout)
        self.assertIn('liblzma after:', result.stdout)
        self.assertIn(LIB_SHA256, result.stdout)

    def test_newer_installed_version_cannot_be_downgraded(self):
        result, calls = self.run_installer('newer')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Refusing to downgrade', result.stderr)
        self.assertFalse(any(call[0] in ('sudo', 'curl') for call in calls))

    def test_missing_or_conflicting_index_never_installs_or_falls_back(self):
        for mode in ('unavailable', 'status_only', 'wrong_index', 'conflicting_index'):
            with self.subTest(mode=mode):
                result, calls = self.run_installer(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.installs(calls), [])
                self.assertFalse(any(call[0] == 'llvm-config' for call in calls))
                self.assertEqual(sum(call[:2] == ['apt-cache', 'show'] for call in calls), 1)

    def test_identical_index_entries_do_not_create_a_false_conflict(self):
        result, _ = self.run_installer('duplicate_index')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_network_failure_never_uses_stale_indexes_or_installs(self):
        for mode, code in (('key_failure', 28), ('update_failure', 100)):
            with self.subTest(mode=mode):
                result, calls = self.run_installer(mode)
                self.assertEqual(result.returncode, code)
                self.assertEqual(self.installs(calls), [])
                self.assertFalse(any(call[0] in ('apt-cache', 'llvm-config') for call in calls))

    def test_install_failure_never_retries_or_verifies_success(self):
        result, calls = self.run_installer('install_failure')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(len(self.installs(calls)), 1)
        self.assertFalse(any(call[0] in ('readlink', 'sha256sum', 'llvm-config') for call in calls))

    def test_installed_identity_or_bytes_mismatch_is_fatal(self):
        for mode in ('wrong_version', 'wrong_arch', 'wrong_owner', 'wrong_target', 'wrong_hash'):
            with self.subTest(mode=mode):
                result, calls = self.run_installer(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(len(self.installs(calls)), 1)
                self.assertFalse(any(call[0] == 'llvm-config' for call in calls))
                self.assertNotIn('Verified liblzma SHA256:', result.stdout)

    def test_original_full_llvm_key_validation_still_precedes_install(self):
        for mode in ('wrong_key', 'multiple_keys'):
            with self.subTest(mode=mode):
                result, calls = self.run_installer(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any(call[0] == 'sudo' for call in calls))

    def test_original_llvm_version_and_sanitizer_checks_still_fail_closed(self):
        for mode, message in (('wrong_llvm_version', 'Expected LLVM 21.1.8'),
                              ('missing_sanitizer', 'Missing pinned LLVM sanitizer archive:')):
            with self.subTest(mode=mode):
                result, _ = self.run_installer(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)

    def test_original_platform_restriction_is_preserved(self):
        result, calls = self.run_installer('unsupported_host')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, [])


if __name__ == '__main__':
    unittest.main()
