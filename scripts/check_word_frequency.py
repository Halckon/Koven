#!/usr/bin/env python3
"""SPEC-0274: public project commands with independent byte oracles and durable evidence."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess

from check_tutorial import load_examples, run_case


def frequency_bytes(arguments):
    # Dict insertion order is an independent reference for first occurrence order.
    counts = {}
    for word in arguments:
        counts[word] = counts.get(word, 0) + 1
    return b''.join(word.encode('utf-8') + b'\t' + str(count).encode('ascii') + b'\n'
                    for word, count in counts.items())


def prepare(directory, sources):
    directory.mkdir()
    for path, source in sources.items():
        destination = directory / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(source, encoding='utf-8')
    (directory / 'project.toml').write_text(
        'schema = "koven.project"\nversion = 1\n\n[project]\n'
        'name = "word-frequency"\nsource-roots = ["src"]\n', encoding='utf-8')


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(cli, evidence):
    cli = cli.resolve(strict=True)
    evidence.mkdir(parents=True, exist_ok=False)
    ledger = dict(schema='koven.word-frequency-evidence.v1', success=False,
                  compiler=dict(path=str(cli), sha256=sha256(cli)), projects=[], commands=[])

    def execute(command, expected, directory, abort=False):
        record = dict(
            argv=[os.fsdecode(arg) for arg in command],
            argv_base64=[base64.b64encode(os.fsencode(arg)).decode('ascii') for arg in command],
            cwd=str(directory), exit=None, stdout_base64='', stderr_base64='',
            oracle=(dict(kind='abort-before-output', exit=expected['exit']) if abort else expected),
            success=False,
            timed_out=False, process_error=None)
        ledger['commands'].append(record)
        try:
            result = subprocess.run(command, cwd=directory, capture_output=True, timeout=120)
        except subprocess.TimeoutExpired as error:
            record.update(timed_out=True,
                          stdout_base64=base64.b64encode(error.output or b'').decode('ascii'),
                          stderr_base64=base64.b64encode(error.stderr or b'').decode('ascii'))
            raise
        except OSError as error:
            record['process_error'] = str(error)
            raise
        outputs = {key: getattr(result, key) for key in ('stdout', 'stderr')}
        success = (result.returncode == expected['exit'] and result.stdout == b'' if abort else
                   result.returncode == expected['exit'] and
                   all(outputs[key] == expected[key].encode('utf-8') for key in outputs))
        record.update(exit=result.returncode,
            stdout_base64=base64.b64encode(result.stdout).decode('ascii'),
            stderr_base64=base64.b64encode(result.stderr).decode('ascii'),
            success=success)
        if not success:
            raise AssertionError(f'public command failed: {ledger["commands"][-1]!r}')

    def project(name, sources):
        directory = evidence / name
        prepare(directory, sources)
        ledger['projects'].append(dict(path=str(directory), files={
            path.relative_to(directory).as_posix(): sha256(path)
            for path in sorted(directory.rglob('*')) if path.is_file()}))
        return directory

    inputs = ['--project', 'project.toml', '--entry']
    blank = dict(exit=0, stdout='', stderr='')
    try:
        row, sources = next(item for item in load_examples()
                            if item[0]['id'] == 'argv-word-frequency')
        for index, case in enumerate(row['cases']):
            oracle = frequency_bytes(case['args'])
            for kind in ('artifact', 'run'):
                if case[kind] != dict(exit=0, stdout=oracle.decode('utf-8'), stderr=''):
                    raise AssertionError('tutorial output differs from the independent reference')
            directory = project(f'正常 项目 {index}', sources)
            run_case(cli, row, case, directory, execute=execute)

        # The helper is the actual tutorial fence; only the test entry is additional source.
        boundaries = [0, 1, 9, 10, 99, 100, 2147483647]
        probes = dict(sources)
        probes['src/probe/main.ko'] = ('package probe\nfun main(): Unit {\n' +
            ''.join(f'println(app.decimal({value}))\n' for value in boundaries) + '}\n'
            'fun negative(): Unit { println(app.decimal(-1))\nprintln("after") }\n'
            'fun entry(args: Array<String>): Unit { println("entered")\napp.main(args) }\n')
        directory = project('边界 项目', probes)
        execute([str(cli), 'build', *inputs, 'probe.main', '-o', 'decimal'], blank, directory)
        decimal = dict(exit=0, stdout=''.join(str(value) + '\n' for value in boundaries), stderr='')
        execute([str(directory / 'decimal')], decimal, directory)
        execute([str(cli), 'run', *inputs, 'probe.main', '--'], decimal, directory)
        execute([str(cli), 'build', *inputs, 'probe.negative', '-o', 'negative'], blank, directory)
        execute([str(directory / 'negative')], dict(exit=-signal.SIGABRT), directory, abort=True)
        execute([str(cli), 'run', *inputs, 'probe.negative', '--'], dict(exit=1), directory, abort=True)

        # Native argv cannot contain NUL. Invalid UTF-8 must be refused before either entry.
        if os.name != 'posix':
            raise RuntimeError('the selected acceptance hosts require POSIX byte argv')
        invalid = b'word\xfftail'
        refused = dict(exit=1, stdout='', stderr='')
        normal = evidence / '正常 项目 0'
        execute([os.fsencode(normal / 'program'), invalid], refused, normal)
        execute([str(cli), 'run', *inputs, 'app.main', '--', invalid], refused, normal)
        execute([str(cli), 'build', *inputs, 'probe.entry', '-o', 'entry'], blank, directory)
        entered = dict(exit=0, stdout='entered\nalpha\t1\n', stderr='')
        execute([str(directory / 'entry'), 'alpha'], entered, directory)
        execute([str(cli), 'run', *inputs, 'probe.entry', '--', 'alpha'], entered, directory)
        execute([os.fsencode(directory / 'entry'), invalid], refused, directory)
        execute([str(cli), 'run', *inputs, 'probe.entry', '--', invalid], refused, directory)
        if sha256(cli) != ledger['compiler']['sha256']:
            raise AssertionError('compiler changed during acceptance')
        ledger['success'] = True
    finally:
        (evidence / 'results.json').write_text(json.dumps(ledger, ensure_ascii=True, indent=2) + '\n')
    print(f'word-frequency: {len(ledger["commands"])} public commands passed; {evidence}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--cli', type=Path, default=Path('target/debug/kovenc'))
    parser.add_argument('--evidence', type=Path, required=True)
    args = parser.parse_args()
    check(args.cli, args.evidence.resolve())
