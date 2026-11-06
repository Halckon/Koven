"""Fail-closed image bootstrap contracts; no Docker, Cargo, or package operations."""
from pathlib import Path
import re
import io
import json
import os
import textwrap
from unittest.mock import patch
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / '.github/workflows/ci-toolchain-image.yml'


class CiToolchainImageTests(unittest.TestCase):
    def test_base_and_rust_payloads_are_immutable(self):
        dockerfile = (ROOT / '.github/ci-toolchain/Dockerfile').read_text()
        self.assertRegex(dockerfile, r'FROM ubuntu:26\.04@sha256:[0-9a-f]{64}\n')
        self.assertNotIn('ARG ', dockerfile)
        self.assertNotIn('COPY . ', dockerfile)
        self.assertIn('bash /opt/koven-ci/install_ci_llvm_ubuntu26.sh', dockerfile)
        installer = (ROOT / 'scripts/install_ci_image_rust.sh').read_text()
        self.assertIn('readonly release=1.96.0', installer)
        self.assertIn('https://static.rust-lang.org/dist/2026-05-28', installer)
        self.assertEqual(5, len(re.findall(r'^(?:readonly pins=\')?[\w-]+ [0-9a-f]{64}', installer, re.M)))
        self.assertLess(installer.index('sha256sum --check --strict'), installer.index('tar -xJf'))
        self.assertLess(installer.index('tar -xJf'), installer.index('/install.sh'))
        self.assertIn('PATH=/opt/rust/bin:/usr/lib/llvm-21/bin:', dockerfile)
        self.assertNotIn('/root/.cargo', dockerfile)

    def test_bad_rust_payload_cannot_be_extracted_or_executed(self):
        script = (ROOT / 'scripts/install_ci_image_rust.sh').read_text()
        mocks = r'''
uname() { echo x86_64; }
curl() { printf 'untrusted payload' > "${@: -1}"; }
tar() { echo EXTRACTED; return 99; }
'''
        with tempfile.TemporaryDirectory() as temporary:
            result = subprocess.run(['bash', '-c', mocks + script], cwd=temporary,
                                    text=True, capture_output=True, timeout=10)
        self.assertNotEqual(0, result.returncode)
        self.assertIn('FAILED', result.stdout)
        self.assertNotIn('EXTRACTED', result.stdout)

    def test_readiness_has_no_online_repair_path(self):
        script = (ROOT / 'scripts/verify_ci_toolchain_image.sh').read_text()
        commands = '\n'.join(line for line in script.splitlines() if not line.startswith('#'))
        for forbidden in ('apt-get', 'curl ', 'rustup ', '|| true'):
            self.assertNotIn(forbidden, commands)
        for contract in ('21.1.8', '1:21.1.8-6ubuntu1 amd64', 'KOVEN_LLDB_SB_API_OK',
                         'for sanitizer in asan lsan', 'GetFunctionName', 'fullpath'):
            self.assertIn(contract, script)

    def test_pr_build_cannot_publish_or_read_credentials(self):
        workflow = WORKFLOW.read_text()
        validate = workflow.split('\n  validate:\n', 1)[1].split('\n  publish:\n', 1)[0]
        publish = workflow.split('\n  publish:\n', 1)[1]
        self.assertNotIn('pull_request_target', workflow)
        self.assertNotIn('workflow_run:', workflow)
        self.assertIn('permissions:\n  contents: read', workflow)
        self.assertNotIn('packages: write', validate)
        self.assertNotIn('GH_TOKEN', validate)
        self.assertNotIn('docker login', validate)
        self.assertNotIn('docker push', validate)
        self.assertIn('persist-credentials: false', validate)
        self.assertIn('ref: ${{ github.sha }}', validate)
        self.assertIn('needs: validate', publish)
        self.assertIn("if: github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main' && github.repository == 'Halckon/Koven'", publish)
        self.assertEqual(workflow.count('packages: write'), 1)
        self.assertNotIn('actions/checkout', publish)
        self.assertNotIn('docker build', publish)
        self.assertIn('artifact-ids: ${{ needs.validate.outputs.image_artifact }}', publish)
        self.assertIn('docker run --rm --network none "$digest"', publish)
        self.assertIn('anonymous-docker-config', publish)

    def main_ci_probe(self, changed=None):
        workflow = WORKFLOW.read_text()
        code = textwrap.dedent(workflow.split("python3 - <<'PY'\n", 1)[1].split('          PY\n', 1)[0])
        run = dict(id=17, head_sha='a' * 40, head_branch='main', event='push',
                   conclusion='success', run_attempt=2, html_url='https://github.com/example/run/17')
        names = ('Workspace Check & Clippy (macos-15)', 'Workspace Check & Clippy (ubuntu-26.04)',
                 'Targeted Tests (macos-15)', 'Targeted Tests (ubuntu-26.04)',
                 'Preview Producer (macos-15)', 'Preview Independent Consumer (macos-15)',
                 'Preview Producer (ubuntu-24.04)', 'Preview Independent Consumer (ubuntu-24.04)',
                 'CI Passed')
        jobs = [dict(name=name, conclusion='success') for name in names]
        if changed:
            changed(run, jobs)
        def response(request, timeout):
            self.assertEqual(timeout, 30)
            path = request.full_url
            if '/workflows/ci.yml/runs?' in path:
                self.assertIn('head_sha=' + 'a' * 40, path)
                data = {'workflow_runs': [run]}
            else:
                self.assertIn('/actions/runs/17/attempts/2/jobs?', path)
                data = {'jobs': jobs}
            return io.BytesIO(json.dumps(data).encode())
        with patch.dict(os.environ, {'GITHUB_REPOSITORY': 'Halckon/Koven',
                        'GITHUB_SHA': 'a' * 40, 'GH_TOKEN': 'test-only-not-a-credential'}), \
             patch('urllib.request.urlopen', side_effect=response), patch('sys.stdout', new_callable=io.StringIO):
            exec(compile(code, '<publisher-main-ci-check>', 'exec'), {})

    def test_publisher_requires_exact_real_main_ci(self):
        self.main_ci_probe()
        mutations = (
            lambda run, jobs: run.update(head_sha='b' * 40),
            lambda run, jobs: run.update(head_branch='feature/untrusted'),
            lambda run, jobs: run.update(event='pull_request'),
            lambda run, jobs: run.update(conclusion='failure'),
            lambda run, jobs: jobs.pop(0),
            lambda run, jobs: jobs[7].update(conclusion='skipped'),
            lambda run, jobs: jobs.append(dict(name=jobs[0]['name'], conclusion='failure')),
        )
        for mutation in mutations:
            with self.subTest(mutation=mutation), self.assertRaises(SystemExit):
                self.main_ci_probe(mutation)

    def test_no_runtime_security_or_test_coverage_weakening(self):
        workflow = WORKFLOW.read_text()
        for forbidden in ('--privileged', '--cap-add', '--security-opt', '/var/run/docker.sock',
                          'continue-on-error', 'secrets: inherit'):
            self.assertNotIn(forbidden, workflow)
        check = (ROOT / 'scripts/check_ci_toolchain_image.sh').read_text()
        for command in ('cargo check --locked --workspace --all-targets',
                        'cargo clippy --locked --workspace --all-targets -- -D warnings',
                        'bash scripts/check_integration.sh', 'scripts/check_word_frequency.py',
                        'scripts/conformance_gate.py', 'scripts/check_native_sanitizers.py',
                        'scripts/check_generated_owners.py'):
            self.assertIn(command, check)
        self.assertIn('CARGO_PROFILE_DEV_DEBUG=1 CARGO_PROFILE_TEST_DEBUG=1', check)
        self.assertNotIn('|| true', check)
        self.assertIn('CARGO_INCREMENTAL=0', workflow)
        self.assertIn('export CARGO_TARGET_DIR="$PWD/target"', check)
        self.assertIn('test ! -e target', check)
        self.assertIn("-o run -o 'frame info'", check)
        self.assertIn("grep -Fq 'stop reason = breakpoint'", check)

    def test_debugger_preflight_preserves_aslr_and_requires_breakpoint(self):
        script = (ROOT / 'scripts/check_ci_toolchain_image.sh').read_text()
        probe = script[script.index('debugger_probe='):script.index('# Separate Clippy')]
        # Compile the actual C probe; emulate only LLDB's constrained launch and
        # output, so absence of the preserving-ASLR setting reproduces the failure.
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'toolchain-evidence').mkdir()
            debugger = root / 'lldb'
            debugger.write_text("#!/usr/bin/env python3\n" + textwrap.dedent("""\
                import os, sys
                args = sys.argv[1:]
                try:
                    setting = args.index('settings set target.disable-aslr false')
                    launch = args.index('run')
                    assert setting < launch
                except (ValueError, AssertionError):
                    print('personality set failed: Operation not permitted', file=sys.stderr)
                    sys.exit(1)
                print('stop reason = breakpoint' if os.environ['PROBE_BREAKPOINT'] == 'yes' else 'exited')
                """))
            debugger.chmod(0o755)
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ['PATH'],
                       RUNNER_TEMP=temporary, PROBE_BREAKPOINT='yes')
            for expected, breakpoint in ((0, 'yes'), (1, 'no')):
                with self.subTest(breakpoint=breakpoint):
                    env['PROBE_BREAKPOINT'] = breakpoint
                    result = subprocess.run(['bash', '-euo', 'pipefail', '-c', probe],
                                            env=env, capture_output=True, text=True, timeout=20)
                    self.assertEqual(expected, result.returncode, result.stdout + result.stderr)
            self.assertTrue((root / 'toolchain-evidence/debugger-probe').is_file())

    def test_core_is_not_switched_before_real_digest_publication(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertNotIn('container:', workflow)
        self.assertNotIn('ghcr.io/', workflow)
        self.assertEqual(2, workflow.count('os: [macos-15, ubuntu-26.04]'))
        self.assertEqual(2, workflow.count('runs-on: ubuntu-24.04'))
        for group, following in (('docs', 'rust'), ('rust', 'editors'), ('preview', 'frontend')):
            block = workflow.split('            ' + group + ':\n', 1)[1].split('            ' + following + ':\n', 1)[0]
            self.assertIn("'.github/ci-toolchain/**'", block)
            self.assertIn("'.github/workflows/ci-toolchain-image.yml'", block)


if __name__ == '__main__':
    unittest.main()
