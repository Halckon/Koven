"""Staged host migration and fail-closed package selection; no real package operations."""
from pathlib import Path
import importlib.util
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'scripts/install_ci_llvm_ubuntu26.sh'


class Ubuntu26Ci(unittest.TestCase):
    def test_core_and_preview_have_separate_host_contracts(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertEqual(2, workflow.count('os: [macos-15, ubuntu-26.04]'))
        self.assertEqual(2, workflow.count('runs-on: ubuntu-24.04'))
        self.assertEqual(8, workflow.count('runs-on: ubuntu-26.04'))
        self.assertNotIn('ubuntu-latest', workflow)
        for name in ('Preview Producer', 'Preview Independent Consumer'):
            self.assertIn(f'name: {name} (ubuntu-24.04)', workflow)
        spec = importlib.util.spec_from_file_location('reuse26', ROOT / 'scripts/pr_ci_reuse.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        names = module.required_job_names(dict(rust='true', preview='true', editors='false'))
        self.assertIn('Targeted Tests (ubuntu-26.04)', names)
        self.assertNotIn('Targeted Tests (ubuntu-24.04)', names)
        self.assertIn('Preview Producer (ubuntu-24.04)', names)
        self.assertNotIn('Preview Producer (ubuntu-26.04)', names)

    def probe(self, mode):
        # Execute the actual selection/install/identity code under fail-closed mocks.
        # End before host LLVM binaries; this is not an LLVM execution test.
        script = SCRIPT.read_text().split('[[ "$(/usr/lib/llvm-21/bin/llvm-config')[0]
        mocks = r'''
source() { ID=ubuntu; VERSION_ID=26.04; [[ "$MODE" != host ]] || VERSION_ID=24.04; }
uname() { printf 'x86_64\n'; }
sudo() {
  [[ "$1" == apt-get ]] || return 97
  printf 'APT %s\n' "$*"
  if [[ "$*" == *' update' && "$MODE" == update ]]; then return 100; fi
  if [[ "$*" == *' install '* && "$MODE" == install ]]; then return 100; fi
}
apt-cache() {
  [[ "$1" == show ]] || return 97
  local package=${2%%:*}
  [[ "$MODE" != missing ]] || return 100
  if [[ "$MODE" == hash ]]; then printf 'SHA256: wrong\n'; return; fi
  while read -r name hash; do
    [[ "$name" != "$package" ]] || printf 'SHA256: %s\n' "$hash"
  done <<< "$pins"
}
dpkg-query() {
  if [[ "$*" == *Architecture* ]]; then
    [[ "$MODE" != identity ]] || { printf 'wrong amd64'; return; }
    printf '%s amd64' "$version"
  else
    [[ "$MODE" != downgrade ]] || { printf '1:21.1.9-1'; return; }
    printf '%s' "$version"
  fi
}
dpkg() { [[ "$MODE" == downgrade ]]; }
'''
        result = subprocess.run(['bash', '-c', f'MODE={mode}\n' + mocks + script],
                                text=True, capture_output=True, timeout=10)
        return result

    def test_exact_signed_index_packages_reach_install(self):
        result = self.probe('valid')
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertIn(' --reinstall ', result.stdout)
        self.assertIn('APT::Get::AllowUnauthenticated=false', result.stdout)
        self.assertIn('Acquire::AllowInsecureRepositories=false', result.stdout)
        self.assertIn('llvm-21-dev:amd64=1:21.1.8-6ubuntu1', result.stdout)
        self.assertNotIn('liblzma5:amd64=', result.stdout)

    def test_failures_cannot_silently_continue(self):
        for mode in ('host', 'update', 'missing', 'hash', 'downgrade', 'install', 'identity'):
            with self.subTest(mode=mode):
                result = self.probe(mode)
                self.assertNotEqual(0, result.returncode)
                if mode not in ('install', 'identity'):
                    self.assertNotIn(' install ', result.stdout)


if __name__ == '__main__':
    unittest.main()
