#!/usr/bin/env python3
"""Read-only, fail-closed reuse of actual same-PR code jobs (SPEC-0293)."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import urllib.request

SHA = r'[0-9a-f]{40}'
MARKER = re.compile(rf'Code CI Evidence v1 PR([1-9][0-9]*) ({SHA}) ({SHA}) ({SHA}) ([0-9a-f]{{64}})')
WORKFLOW = '.github/workflows/ci.yml'
DOC_ROOTS = ('docs/specs/', 'docs/archive/', 'docs/architecture/', 'docs/development/',
             'docs/adr/', 'docs/proposals/')


def documentation_only(path, mode):
    """Only ordinary, regular Markdown/SVG documents are exempt; unknown = code."""
    if mode != '100644' or path == 'docs/development/preview-candidate.md':
        return False
    return path in ('README.md', 'AGENTS.md', 'docs/AGENTS.md') or (
        path.startswith(DOC_ROOTS) and path.endswith(('.md', '.svg')))


def git(*args):
    return subprocess.check_output(['git', *args], stderr=subprocess.PIPE, timeout=60)


def ensure_commit(sha):
    if not re.fullmatch(SHA, sha):
        raise ValueError('invalid commit identity')
    try:
        git('cat-file', '-e', f'{sha}^{{commit}}')
    except subprocess.CalledProcessError:
        # Origin is the checked-out base repository, never an API-supplied URL.
        git('fetch', '--no-tags', 'origin', sha)


def tree_inputs(sha):
    ensure_commit(sha)
    records = []
    for record in git('ls-tree', '-rz', '--full-tree', sha).split(b'\0'):
        if not record:
            continue
        metadata, path = record.split(b'\t', 1)
        mode, kind, oid = metadata.decode('ascii').split()
        if not documentation_only(path.decode('utf-8'), mode):
            records.append(record + b'\0')
    return b''.join(records)


def fingerprint(head, merge, base):
    """Bind both checkout modes and the exact immutable merge parents."""
    ensure_commit(merge)
    parents = git('rev-list', '--parents', '-n', '1', merge).decode().split()
    if parents != [merge, base, head]:
        raise ValueError('checkout is not the event base/head merge')
    digest = hashlib.sha256(b'koven-pr-ci-v1\0' + base.encode() + b'\0')
    for sha in (head, merge):
        digest.update(hashlib.sha256(tree_inputs(sha)).digest())
    return digest.hexdigest()


def marker_name(context):
    return (f"Code CI Evidence v1 PR{context['number']} " + ' '.join(context[key]
            for key in ('base', 'head', 'merge', 'fingerprint')))


def required_job_names(flags):
    names = set()
    if flags['rust'] == 'true' or flags['preview'] == 'true':
        names.add('Rust Code Formatting')
        for host in ('macos-14', 'ubuntu-24.04'):
            names.add(f'Workspace Check & Clippy ({host})')
            names.add(f'Targeted Tests ({host})')
    if flags['preview'] == 'true':
        for host in ('macos-14', 'ubuntu-24.04'):
            names.add(f'Preview Producer ({host})')
            names.add(f'Preview Independent Consumer ({host})')
    if flags['editors'] == 'true':
        names.add('Tree-sitter CLI Corpus')
    return names


def same_pr(run, context):
    return (run.get('repository', {}).get('full_name') == context['repository']
            and run.get('head_repository', {}).get('full_name') == context['head_repository']
            and run.get('event') == 'pull_request'
            and run.get('workflow_id') == context['workflow_id']
            and run.get('path', '').split('@', 1)[0] == WORKFLOW
            and run.get('display_title') == f"CI pull_request PR {context['number']}"
            and all(pr.get('number') == context['number'] for pr in run.get('pull_requests', [])))


def valid_evidence(run, jobs, context, flags, fingerprint_fn=None):
    """Metadata is not a cache attestation: reconstruct historical Git inputs."""
    if (not same_pr(run, context) or run.get('status') != 'completed'
            or run.get('conclusion') != 'success' or run.get('run_attempt', 0) < 1
            or run.get('id', context['run_id']) >= context['run_id']):
        return False
    by_name = {}
    for job in jobs:
        name = job.get('name')
        if name in by_name or job.get('run_id') != run['id']:
            return False
        by_name[name] = job
    markers = [(name, MARKER.fullmatch(name)) for name in by_name if isinstance(name, str)]
    markers = [(name, match) for name, match in markers if match]
    if len(markers) != 1 or not required_job_names(flags):
        return False
    name, match = markers[0]
    number, base, head, merge, digest = match.groups()
    if (int(number) != context['number'] or base != context['base'] or digest != context['fingerprint']
            or run.get('head_sha') not in (head, merge)):
        return False
    for required in required_job_names(flags) | {name}:
        job = by_name.get(required, {})
        if job.get('status') != 'completed' or job.get('conclusion') != 'success':
            return False
    return (fingerprint_fn or fingerprint)(head, merge, base) == context['fingerprint']


class Actions:
    def __init__(self, repository):
        self.root = f"{os.environ['GITHUB_API_URL']}/repos/{repository}/actions/"

    def get(self, path):
        request = urllib.request.Request(self.root + path, headers={
            'Authorization': f"Bearer {os.environ['GH_TOKEN']}",
            'Accept': 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28'})
        with urllib.request.urlopen(request, timeout=20) as response:
            return json.load(response)

    def jobs(self, run):
        # An attempt-specific endpoint never stitches together partial reruns.
        result = self.get(f"runs/{run['id']}/attempts/{run['run_attempt']}/jobs?per_page=100")
        if result['total_count'] != len(result['jobs']):
            raise ValueError('incomplete job page')
        return result['jobs']


def current_context(api, event):
    pr = event['pull_request']
    context = dict(repository=os.environ['GITHUB_REPOSITORY'],
                   head_repository=pr['head']['repo']['full_name'], number=pr['number'],
                   base=pr['base']['sha'], head=pr['head']['sha'], merge=os.environ['GITHUB_SHA'],
                   run_id=int(os.environ['GITHUB_RUN_ID']))
    current = api.get(f"runs/{context['run_id']}")
    context['workflow_id'] = current['workflow_id']
    if not same_pr(current, context):
        raise ValueError('current workflow/PR identity unavailable')
    context['fingerprint'] = fingerprint(context['head'], context['merge'], context['base'])
    return context


def find_evidence(api, context, flags, selected=None):
    # A bounded search is an optimization only. Missing/older evidence runs CI.
    page = api.get(f"workflows/{context['workflow_id']}/runs?event=pull_request&per_page=100")
    if len(page['workflow_runs']) != min(page['total_count'], 100):
        raise ValueError('incomplete workflow run page')
    runs = sorted(page['workflow_runs'], key=lambda run: run['id'], reverse=True)
    for run in runs:
        if run['id'] >= context['run_id'] or not same_pr(run, context):
            continue
        if run.get('status') != 'completed' or run.get('conclusion') != 'success':
            return None, 'newer same-PR run has not succeeded'
        # Refresh status/attempt so a rerun cannot hide behind stale list data.
        run = api.get(f"runs/{run['id']}")
        if run.get('status') != 'completed' or run.get('conclusion') != 'success':
            return None, 'candidate status changed'
        if selected is not None and run['id'] != selected:
            continue
        if valid_evidence(run, api.jobs(run), context, flags):
            return run, 'verified physical code jobs and both Git input trees'
        if selected is not None:
            return None, 'selected evidence no longer valid'
    return None, 'no complete matching physical-code evidence in search window'


def verify_selected(outputs):
    if os.environ['GITHUB_EVENT_NAME'] != 'pull_request':
        return False
    api = Actions(os.environ['GITHUB_REPOSITORY'])
    context = current_context(api, json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text()))
    if outputs.get('fingerprint') != context['fingerprint']:
        return False
    run, _ = find_evidence(api, context, outputs, int(outputs['reuse_run']))
    return run is not None and str(run['run_attempt']) == outputs.get('reuse_attempt')


def main():
    outputs = dict(reuse='false', reuse_run='', reuse_attempt='', fingerprint='', marker='')
    reason = 'non-PR event keeps existing policy'
    context = {}
    if os.environ['GITHUB_EVENT_NAME'] == 'pull_request':
        try:
            flags = {key: os.environ[f'CI_{key.upper()}'] for key in ('rust', 'preview', 'editors')}
            if any(value not in ('true', 'false') for value in flags.values()):
                raise ValueError('invalid path outputs')
            api = Actions(os.environ['GITHUB_REPOSITORY'])
            context = current_context(api, json.loads(Path(os.environ['GITHUB_EVENT_PATH']).read_text()))
            outputs.update(fingerprint=context['fingerprint'], marker=marker_name(context))
            run, reason = find_evidence(api, context, flags)
            if run:
                outputs.update(reuse='true', reuse_run=str(run['id']), reuse_attempt=str(run['run_attempt']))
        except Exception as error:
            # Failure must increase work, never convert an unknown result into green.
            reason = f'evidence unavailable ({type(error).__name__}); run existing required jobs'
    report = dict(outputs, reason=reason, base=context.get('base'),
                  source_url=(f"{os.environ['GITHUB_SERVER_URL']}/{os.environ['GITHUB_REPOSITORY']}"
                              f"/actions/runs/{outputs['reuse_run']}" if outputs['reuse_run'] else None))
    print(json.dumps(report, sort_keys=True))
    with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
        for key, value in outputs.items():
            output.write(f'{key}={value}\n')
    with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as summary:
        summary.write('### PR code CI reuse\n\n' + json.dumps(report, sort_keys=True) + '\n')


if __name__ == '__main__':
    main()
