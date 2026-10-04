#!/usr/bin/env python3
"""Bounded Darwin LSP migration pilot; retain raw output, never clean shared caches."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent
REPO = Path('/Users/ckfei/Workspace/RustProjects/Koven')
SOURCES = ROOT / 'sources'
TARGETS = ROOT / 'targets'
RAW = ROOT / 'raw'
START = time.monotonic()
ROWS = []
COMMITS = {
    'before': '34189046319a8b727285d471596647d5de56996e',
    'after': '1f3991b6bbaeaf0a5f485bdb139b5b7efa07d3da',
}


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def limits():
    if time.monotonic() - START >= 7200:
        raise RuntimeError('two-hour limit reached')
    if shutil.disk_usage(ROOT).free < 40 * 1024**3:
        raise RuntimeError('free space below 40 GiB')
    kib = int(subprocess.check_output(['du', '-sk', str(TARGETS)], text=True).split()[0])
    if kib * 1024 >= 12 * 1024**3:
        raise RuntimeError('target cache limit reached (12 GiB)')
    return kib * 1024


def wrapper():
    mode = sys.argv[1]
    args = sys.argv[2:]
    directory = Path(os.environ['P2_EVENTS'])
    ident = str(os.getpid())
    env = dict(os.environ)
    if mode == 'rustc':
        env['P2_RUSTC_ID'] = ident
        command = args
    else:
        command = ['/usr/bin/cc', *args]
    started = time.monotonic_ns()
    result = subprocess.run(['/usr/bin/time', '-l', '-o',
                             str(directory / (mode + '-' + ident + '.time')),
                             *command], env=env)
    ended = time.monotonic_ns()
    save(directory / (mode + '-' + ident + '.json'), {
        'kind': mode, 'id': ident, 'rustc_id': env.get('P2_RUSTC_ID'),
        'argv': command, 'start_ns': started, 'end_ns': ended,
        'seconds': (ended - started) / 1e9, 'exit_code': result.returncode,
    })
    raise SystemExit(result.returncode)


def sample(name, side, command, target, phase):
    limits()
    directory = RAW / name
    directory.mkdir()
    env = dict(os.environ)
    env.update(CARGO_INCREMENTAL='0', CARGO_TARGET_DIR=str(target),
               RUSTC_WRAPPER=str(ROOT / 'rustc-wrapper'),
               RUSTFLAGS='-C linker=' + str(ROOT / 'linker-wrapper'),
               P2_EVENTS=str(directory))
    begin = time.monotonic()
    with (directory / 'stdout.log').open('wb') as out, (directory / 'stderr.log').open('wb') as err:
        process = subprocess.Popen(['/usr/bin/time', '-l', '-o',
                                    str(directory / 'cargo.time'), *command],
                                   cwd=SOURCES / side, env=env, stdout=out, stderr=err,
                                   start_new_session=True)
        try:
            while process.poll() is None:
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    limits()
                    if time.monotonic() - begin >= 1200:
                        raise RuntimeError('per-command limit reached (20 minutes)')
        except BaseException:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            raise
    row = dict(name=name, side=side, phase=phase, argv=command,
               seconds=time.monotonic() - begin, exit_code=process.returncode,
               target_bytes=limits())
    events = [json.loads(p.read_text()) for p in sorted(directory.glob('*.json'))]
    rustcs = [e for e in events if e['kind'] == 'rustc']
    links = [e for e in events if e['kind'] == 'link']
    row['rustc_invocations'] = len(rustcs)
    row['linker_invocations'] = len(links)
    row['linker_seconds_sum'] = sum(e['seconds'] for e in links)
    row['rustc_nonlink_seconds_sum'] = sum(
        e['seconds'] - sum(l['seconds'] for l in links if l['rustc_id'] == e['id'])
        for e in rustcs)
    rss = []
    for path in directory.glob('*.time'):
        match = re.search(r'(\d+)\s+maximum resident set size', path.read_text())
        if match:
            rss.append(int(match[1]))
    row['maximum_reported_process_rss_bytes'] = max(rss, default=0)
    artifacts = []
    if phase not in ('run', 'identity'):
        for line in (directory / 'stdout.log').read_text().splitlines():
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            if obj.get('reason') == 'compiler-artifact':
                artifacts.append(dict(name=obj['target']['name'], kind=obj['target']['kind'],
                                      fresh=obj['fresh'], executable=obj.get('executable')))
    row['artifacts'] = artifacts
    ROWS.append(row)
    save(ROOT / 'samples.json', ROWS)
    print(json.dumps({k: row[k] for k in ('name', 'seconds', 'exit_code', 'target_bytes') }), flush=True)
    if process.returncode:
        raise RuntimeError(name + ': command failed; raw retained')
    if phase in ('warm', 'calibration-warm'):
        rebuilt = [a['name'] for a in artifacts if not a['fresh']]
        if rebuilt != ['lang-lsp']:
            raise RuntimeError(name + ': warm freshness mismatch: ' + repr(rebuilt))
    if phase == 'noop' and (any(not a['fresh'] for a in artifacts) or links):
        raise RuntimeError(name + ': no-op unexpectedly rebuilt')
    if phase == 'run':
        text = (directory / 'stdout.log').read_text()
        if '11 passed; 0 failed; 0 ignored;' not in text:
            raise RuntimeError(name + ': expected exactly 11 mapped server tests')
    return row


def build_command():
    return ['cargo', 'test', '--locked', '--offline', '-j', '2', '-p', 'lang-lsp',
            '--bin', 'lang-lsp', '--no-run', '--message-format=json']


def executable(row):
    bins = [a['executable'] for a in row['artifacts']
            if a['name'] == 'lang-lsp' and a['executable']]
    if len(bins) != 1:
        raise RuntimeError('ambiguous LSP test executable')
    return bins[0]


def main():
    for p in (SOURCES, TARGETS, RAW):
        p.mkdir(exist_ok=True)
    manifest = dict(commits=COMMITS, started_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
                    host=subprocess.check_output(['sw_vers'], text=True),
                    rustc=subprocess.check_output(['rustc', '-vV'], text=True),
                    cargo=subprocess.check_output(['cargo', '-V'], text=True),
                    jobs=2, incremental=0, flags='identical linker timing wrapper',
                    limits=dict(seconds=7200, cache_bytes=12*1024**3, free_bytes=40*1024**3),
                    measurement='Darwin process RSS bytes; process CPU spans may overlap; rustc nonlink is elapsed minus nested linker including rustc overhead, not exclusive CPU or whole-build wall',
                    acceptance='not approved; investigation thresholds only; no P2 closure',
                    cold='empty target directory; existing registry cache and uncontrolled OS cache',
                    cohort='11 server::tests, one test thread; existing 15 other tests not executed')
    save(ROOT / 'protocol.json', manifest)
    for side, commit in COMMITS.items():
        destination = SOURCES / side
        destination.mkdir()
        archive = subprocess.Popen(['git', 'archive', commit], cwd=REPO, stdout=subprocess.PIPE)
        extract = subprocess.run(['tar', '-x', '-C', str(destination)], stdin=archive.stdout)
        archive.stdout.close()
        if extract.returncode or archive.wait():
            raise RuntimeError('source export failed')
    for mode in ('rustc', 'link'):
        path = ROOT / ('rustc-wrapper' if mode == 'rustc' else 'linker-wrapper')
        path.write_text('#!/bin/sh\nexec ' + sys.executable + ' ' + str(ROOT / 'probe.py') + ' ' + mode + ' "$@"\n')
        path.chmod(0o755)
    target = TARGETS / 'calibration-before'
    first = sample('calibration-build', 'before', build_command(), target, 'prewarm')
    binary = executable(first)
    warm = []
    runs = []
    for i in range(3):
        (SOURCES / 'before/crates/lang-lsp/src/main.rs').touch()
        warm.append(sample(f'calibration-warm-{i}', 'before', build_command(), target, 'calibration-warm'))
    for i in range(5):
        runs.append(sample(f'calibration-run-{i}', 'before',
                           [binary, 'server::tests', '--test-threads=1'], target, 'run'))
    noise = {}
    for key, rows in [('warm', warm), ('run', runs)]:
        values = [r['seconds'] for r in rows]
        median = statistics.median(values)
        noise[key] = dict(median_seconds=median, range_seconds=max(values)-min(values),
                          mad_seconds=statistics.median(abs(x-median) for x in values))
    save(ROOT / 'calibration.json', dict(noise=noise,
         thresholds_registered_before_after_sampling=dict(build='max(20%, 1 second)',
                                                          rss='max(10%, 64 MiB)', run='max(30%, 20 ms)'),
         interpretation='investigation triggers, not accepted SLO or budget waiver'))
    for key in noise:
        if noise[key]['mad_seconds'] > max(.02, .15*noise[key]['median_seconds']):
            raise RuntimeError('baseline calibration too noisy; no after samples')
    latest = {}
    counts = dict(before=0, after=0)
    for side in ['before', 'after', 'after', 'before', 'before', 'after']:
        i = counts[side]
        counts[side] += 1
        target = TARGETS / f'cold-{side}-{i}'
        target.mkdir()
        row = sample(f'cold-{side}-{i}', side, build_command(), target, 'cold')
        if i == 0:
            latest[side] = (target, executable(row))
    names = {}
    for side, (target, binary) in latest.items():
        row = sample('identity-' + side, side, [binary, 'server::tests', '--list'], target, 'identity')
        lines = (RAW / row['name'] / 'stdout.log').read_text().splitlines()
        names[side] = sorted(line.split(': test')[0].split('::')[-1] for line in lines if line.endswith(': test'))
    if len(names['before']) != 11 or names['before'] != names['after']:
        raise RuntimeError('mapped server test identities differ')
    save(ROOT / 'identities.json', names)
    for i in range(5):
        for side in (['before', 'after'] if i % 2 == 0 else ['after', 'before']):
            target, binary = latest[side]
            (SOURCES / side / 'crates/lang-lsp/src/main.rs').touch()
            sample(f'warm-{side}-{i}', side, build_command(), target, 'warm')
            sample(f'run-{side}-{i}', side, [binary, 'server::tests', '--test-threads=1'], target, 'run')
    for i in range(3):
        for side in ['after', 'before']:
            sample(f'noop-{side}-{i}', side, build_command(), latest[side][0], 'noop')
    save(ROOT / 'outcome.json', dict(status='sampling-complete', elapsed_seconds=time.monotonic()-START,
                                    cache_bytes=limits(), acceptance='pending; no speedup/equivalence claim'))


if __name__ == '__main__':
    if len(sys.argv) > 1:
        wrapper()
    else:
        try:
            main()
        except BaseException as error:
            save(ROOT / 'outcome.json', dict(status='stopped', reason=str(error),
                                            elapsed_seconds=time.monotonic()-START))
            print('STOPPED: ' + str(error), flush=True)
            raise
