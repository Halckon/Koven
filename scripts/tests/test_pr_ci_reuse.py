"""Ordinary metadata fixtures for conservative PR code-CI reuse."""
import copy
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
import os
import re
import subprocess
import tempfile

SPEC = importlib.util.spec_from_file_location('pr_ci_reuse', Path(__file__).resolve().parents[1] / 'pr_ci_reuse.py')
CI = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CI)


class ReuseTests(unittest.TestCase):
    def setUp(self):
        self.context = dict(repository='owner/repo', head_repository='owner/fork',
                            number=7, base='b' * 40, head='c' * 40,
                            merge='e' * 40, fingerprint='d' * 64, workflow_id=9, run_id=20)
        self.flags = dict(rust='true', preview='true', editors='true')
        self.run = dict(id=10, workflow_id=9, display_title='CI pull_request PR 7', event='pull_request', status='completed',
                        conclusion='success', run_attempt=1, head_sha='a' * 40,
                        repository={'full_name': 'owner/repo'},
                        head_repository={'full_name': 'owner/fork'},
                        pull_requests=[{'number': 7}], path='.github/workflows/ci.yml')
        self.jobs = [dict(name=name, status='completed', conclusion='success', run_id=10)
                     for name in CI.required_job_names(self.flags) | {CI.marker_name({**self.context, 'head': 'a' * 40})}]

    def valid(self):
        return CI.valid_evidence(self.run, self.jobs, self.context, self.flags,
                                 lambda head, merge, base: 'd' * 64)

    def test_successful_same_inputs(self):
        self.assertTrue(self.valid())

    def test_empty_pr_array_uses_immutable_event_derived_run_name(self):
        self.run['pull_requests'] = []
        self.assertTrue(self.valid())
        self.run.pop('display_title')
        self.assertFalse(self.valid())

    def test_pr_identity_cannot_be_replaced_by_free_title_or_conflicting_metadata(self):
        for title in ('Update README', 'CI pull_request PR 8', 'prefix CI pull_request PR 7'):
            self.run['display_title'] = title
            self.assertFalse(self.valid())
        self.run['display_title'] = 'CI pull_request PR 7'
        self.run['pull_requests'] = [{'number': 8}]
        self.assertFalse(self.valid())
        self.run['pull_requests'] = []
        for job in self.jobs:
            if job['name'].startswith('Code CI Evidence'):
                job['name'] = job['name'].replace('PR7 ', 'PR8 ')
        self.assertFalse(self.valid())

    def test_api_workflow_path_may_include_documented_ref_suffix(self):
        self.run['path'] += '@refs/pull/7/merge'
        self.assertTrue(self.valid())

    def test_every_real_job_must_succeed(self):
        for index in range(len(self.jobs)):
            for result in ('skipped', 'failure', 'cancelled', None):
                jobs = copy.deepcopy(self.jobs)
                jobs[index]['conclusion'] = result
                self.assertFalse(CI.valid_evidence(self.run, jobs, self.context, self.flags,
                                                   lambda head, merge, base: 'd' * 64))
        self.jobs.pop()
        self.assertFalse(self.valid())

    def test_identity_and_status_are_bound(self):
        for key, value in [('event', 'push'), ('workflow_id', 8), ('status', 'in_progress'),
                           ('conclusion', 'cancelled'), ('display_title', 'CI pull_request PR 8'),
                           ('head_repository', {'full_name': 'other/fork'}),
                           ('repository', {'full_name': 'other/repo'}), ('run_attempt', 0),
                           ('path', '.github/workflows/other.yml'), ('head_sha', 'bad')]:
            with self.subTest(key=key):
                run = {**self.run, key: value}
                self.assertFalse(CI.valid_evidence(run, self.jobs, self.context, self.flags,
                                                   lambda head, merge, base: 'd' * 64))

    def test_marker_cannot_replace_git_tree_evidence(self):
        self.assertFalse(CI.valid_evidence(self.run, self.jobs, self.context, self.flags,
                                           lambda head, merge, base: 'e' * 64))
        self.context['base'] = 'f' * 40
        self.assertFalse(self.valid())

    def test_duplicate_job_is_ambiguous(self):
        self.jobs.append(self.jobs[0].copy())
        self.assertFalse(self.valid())

    def test_safe_document_allowlist_and_unknown_inputs(self):
        for path in ('README.md', 'docs/specs/active/0293-example.md',
                     'docs/architecture/compiler.md', 'docs/archive/specs/0001-old.md'):
            self.assertTrue(CI.documentation_only(path, '100644'))
        for path in ('Cargo.lock', '.cargo/config.toml', 'unknown.md',
                     'docs/guide/01-lexical.md', 'docs/compiler-specs/checker.md',
                     'docs/tutorials/start.md', 'docs/development/preview-candidate.md',
                     'crates/lang-std/koven/test.ko', 'scripts/check_docs.py',
                     '.github/workflows/ci.yml', 'docs/specs/example.py'):
            self.assertFalse(CI.documentation_only(path, '100644'))
        self.assertFalse(CI.documentation_only('README.md', '120000'))

    def test_required_jobs_follow_existing_path_flags(self):
        self.assertEqual(set(), CI.required_job_names(dict(rust='false', preview='false', editors='false')))
        self.assertEqual({'Tree-sitter CLI Corpus'}, CI.required_job_names(dict(rust='false', preview='false', editors='true')))
        self.assertEqual(9, len(CI.required_job_names(dict(rust='true', preview='true', editors='false'))))

    def test_history_search_reaches_physical_baseline_not_reuse_marker(self):
        reused = {**self.run, 'id': 15}
        api = self.fake_api([reused, self.run], {15: [], 10: self.jobs})
        with patch.object(CI, 'fingerprint', return_value='d' * 64):
            found, reason = CI.find_evidence(api, self.context, self.flags)
        self.assertEqual(10, found['id'])
        self.assertIn('physical', reason)

    def fake_api(self, runs, jobs):
        class FakeApi:
            def get(inner, path):
                if path.startswith('workflows/'):
                    return {'workflow_runs': runs, 'total_count': len(runs)}
                return next(run for run in runs if str(run['id']) == path.split('/')[-1])

            def jobs(inner, run):
                return jobs[run['id']]
        return FakeApi()

    def test_newer_unsuccessful_same_pr_blocks_old_success(self):
        for state, conclusion in (('in_progress', None), ('completed', 'failure'),
                                  ('queued', None), ('completed', 'cancelled')):
            newer = {**self.run, 'id': 15, 'status': state, 'conclusion': conclusion}
            api = self.fake_api([self.run, newer], {10: self.jobs})
            found, reason = CI.find_evidence(api, self.context, self.flags)
            self.assertIsNone(found)
            self.assertIn('not succeeded', reason)

    def test_push_other_pr_and_current_run_never_supply_evidence(self):
        for run in ({**self.run, 'event': 'push'}, {**self.run, 'display_title': 'CI pull_request PR 8'},
                    {**self.run, 'id': self.context['run_id']}):
            found, _ = CI.find_evidence(self.fake_api([run], {}), self.context, self.flags)
            self.assertIsNone(found)

    def test_final_recheck_refreshes_newer_success_before_crossing_it(self):
        newer = {**self.run, 'id': 15}
        api = self.fake_api([newer, self.run], {10: self.jobs})
        original = api.get
        def refreshed(path):
            if path == 'runs/15':
                return {**newer, 'status': 'in_progress', 'run_attempt': 2}
            return original(path)
        with patch.object(api, 'get', side_effect=refreshed):
            found, reason = CI.find_evidence(api, self.context, self.flags, 10)
        self.assertIsNone(found)
        self.assertIn('changed', reason)

    def test_verify_selected_rejects_changed_attempt_and_current_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory) / 'event'
            event.write_text('{}')
            env = dict(GITHUB_EVENT_NAME='pull_request', GITHUB_REPOSITORY='owner/repo',
                       GITHUB_API_URL='https://api.github.com', GITHUB_EVENT_PATH=str(event))
            outputs = dict(self.flags, fingerprint='d' * 64, reuse_run='10', reuse_attempt='1')
            with patch.dict(os.environ, env), patch.object(CI, 'current_context', return_value=self.context), patch.object(CI, 'find_evidence', return_value=(self.run, 'verified')):
                self.assertTrue(CI.verify_selected(outputs))
                self.assertFalse(CI.verify_selected({**outputs, 'fingerprint': 'e' * 64}))
                self.assertFalse(CI.verify_selected({**outputs, 'reuse_attempt': '2'}))

    def test_incomplete_run_page_cannot_hide_a_newer_failure(self):
        api = self.fake_api([self.run], {10: self.jobs})
        with patch.object(api, 'get', return_value={'workflow_runs': [self.run], 'total_count': 2}):
            with self.assertRaises(ValueError):
                CI.find_evidence(api, self.context, self.flags)

    def test_final_recheck_must_find_exact_selected_run(self):
        with patch.object(CI, 'fingerprint', return_value='d' * 64):
            api = self.fake_api([self.run], {10: self.jobs})
            self.assertIsNone(CI.find_evidence(api, self.context, self.flags, 11)[0])
            self.assertEqual(10, CI.find_evidence(api, self.context, self.flags, 10)[0]['id'])

    def test_attempt_specific_job_page_must_be_complete(self):
        with patch.dict(os.environ, {'GITHUB_API_URL': 'https://api.github.com'}):
            api = CI.Actions('owner/repo')
        with patch.object(api, 'get', return_value={'total_count': 2, 'jobs': [{}]}) as request:
            with self.assertRaises(ValueError):
                api.jobs({**self.run, 'run_attempt': 3})
            self.assertEqual('runs/10/attempts/3/jobs?per_page=100', request.call_args.args[0])

    def test_api_error_falls_back_without_leaking_token(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'output'
            summary = Path(directory) / 'summary'
            event = Path(directory) / 'event'
            event.write_text('{}')
            environment = dict(GITHUB_EVENT_NAME='pull_request', GITHUB_REPOSITORY='owner/repo',
                               GITHUB_SERVER_URL='https://github.com', GITHUB_API_URL='https://api.github.com',
                               GITHUB_EVENT_PATH=str(event), GITHUB_OUTPUT=str(output),
                               GITHUB_STEP_SUMMARY=str(summary), CI_RUST='true', CI_EDITORS='false', CI_PREVIEW='false')
            with patch.dict(os.environ, environment), patch.object(CI, 'current_context', side_effect=OSError('secret-token')):
                CI.main()
            self.assertIn('reuse=false', output.read_text())
            self.assertNotIn('secret-token', summary.read_text())
            self.assertIn('evidence unavailable', summary.read_text())

    def test_workflow_wiring_preserves_paths_and_independent_summary(self):
        workflow = (Path(__file__).resolve().parents[2] / '.github/workflows/ci.yml').read_text()
        self.assertIn('rust: ${{ steps.filter.outputs.rust }}', workflow)
        self.assertIn('reuse: ${{ steps.reuse.outputs.reuse }}', workflow)
        self.assertEqual(2, workflow.count('actions: read'))
        self.assertIn('run-name: CI ${{ github.event_name }} PR ${{ github.event.pull_request.number || 0 }}', workflow)
        self.assertNotIn('pull_request_target', workflow)
        def job(name):
            return re.split(r'^  [a-z][a-z-]+:\s*$',
                            re.split(r'^  ' + name + r':\s*$', workflow, flags=re.M)[1],
                            maxsplit=1, flags=re.M)[0]
        self.assertNotIn('outputs.reuse', job('docs'))
        for name in ('editors', 'fmt', 'clippy', 'test', 'preview-macos-produce', 'preview-linux-produce'):
            self.assertIn("needs.changes.outputs.reuse != 'true'", job(name))
        self.assertIn('python3 -m unittest discover -s scripts/tests -v', workflow)
        self.assertIn("github.event_name == 'pull_request' || needs.changes.outputs.docs", workflow)
        for path in ('docs/guide/**', 'docs/compiler-specs/**', 'scripts/**'):
            self.assertIn("- '" + path + "'", workflow.split('            rust:\n')[1].split('            editors:\n')[0])


class GitInputTests(unittest.TestCase):
    def test_ordinary_git_docs_commit_preserves_key_but_code_mode_and_base_do_not(self):
        with tempfile.TemporaryDirectory() as directory:
            def git(*args, text=True, input=None):
                return subprocess.check_output(['git', '-C', directory, *args], text=text, input=input,
                                               stderr=subprocess.PIPE).strip()
            git('init', '-q')
            git('config', 'user.email', 'fixture@example.invalid')
            git('config', 'user.name', 'Fixture')
            root = Path(directory)
            (root / 'source.rs').write_text('fn main() {}\n')
            (root / 'README.md').write_text('first\n')
            git('add', '.')
            git('commit', '-qm', 'base')
            base = git('rev-parse', 'HEAD')
            (root / 'source.rs').write_text('fn main() { println!("hello"); }\n')
            git('commit', '-qam', 'code')
            head = git('rev-parse', 'HEAD')
            merge = git('commit-tree', git('rev-parse', 'HEAD^{tree}'), '-p', base, '-p', head, input='merge\n')
            def in_repo(*args):
                return subprocess.check_output(['git', '-C', directory, *args], stderr=subprocess.PIPE)
            with patch.object(CI, 'git', side_effect=in_repo):
                before = CI.fingerprint(head, merge, base)
                (root / 'README.md').write_text('second\n')
                git('commit', '-qam', 'docs')
                later = git('rev-parse', 'HEAD')
                later_merge = git('commit-tree', git('rev-parse', 'HEAD^{tree}'), '-p', base, '-p', later, input='merge docs\n')
                self.assertEqual(before, CI.fingerprint(later, later_merge, base))
                with self.assertRaises(ValueError):
                    CI.fingerprint(later, later_merge, head)
                (root / 'source.rs').chmod(0o755)
                git('add', '.')
                git('commit', '-qm', 'mode')
                mode_head = git('rev-parse', 'HEAD')
                mode_merge = git('commit-tree', git('rev-parse', 'HEAD^{tree}'), '-p', base, '-p', mode_head, input='merge mode\n')
                self.assertNotEqual(before, CI.fingerprint(mode_head, mode_merge, base))


if __name__ == '__main__':
    unittest.main()
