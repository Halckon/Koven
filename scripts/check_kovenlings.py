#!/usr/bin/env python3
"""Verify bounded local exercises through the existing public CLI oracle."""
import argparse
import json
from pathlib import Path
import re
import tempfile

from check_tutorial import run_case

ROOT = Path(__file__).resolve().parents[1]
EXERCISES = ROOT / 'docs/tutorials/kovenlings'


def load_exercises(root=EXERCISES):
    """Require a nonempty, unambiguous inventory before executing any source."""
    data = json.loads((root / 'exercises.json').read_text())
    if set(data) != {'version', 'exercises'} or data['version'] != 1:
        raise ValueError('unsupported exercise inventory')
    rows = data['exercises']
    if not isinstance(rows, list) or not rows:
        raise ValueError('exercise inventory must not be empty')
    ids = set()
    for row in rows:
        if set(row) != {'id', 'hint', 'starter', 'solution'}:
            raise ValueError('exercise requires id, hint, starter and solution')
        identity = row['id']
        if not isinstance(identity, str) or not re.fullmatch(r'[a-z][a-z0-9-]*', identity) or identity in ids:
            raise ValueError('invalid or duplicate exercise ID')
        ids.add(identity)
        if not isinstance(row['hint'], str) or not row['hint'].strip():
            raise ValueError('exercise hint must not be empty')
        for variant in ('starter', 'solution'):
            path = root / identity / (variant + '.ko')
            if not path.is_file() or not path.read_text().strip():
                raise ValueError('exercise source missing or empty')
            contract = row[variant]
            diagnostic = variant == 'starter'
            keys = {'status', 'build'} if diagnostic else {'status', 'build', 'args', 'artifact', 'run'}
            if set(contract) != keys or contract['status'] != ('diagnostic' if diagnostic else 'executable'):
                raise ValueError('exercise contract shape mismatch')
            for key in ('build',) if diagnostic else ('build', 'artifact', 'run'):
                output = contract[key]
                if (set(output) != {'exit', 'stdout', 'stderr'} or type(output['exit']) is not int
                        or not isinstance(output['stdout'], str) or not isinstance(output['stderr'], str)):
                    raise ValueError('exercise requires complete output oracles')
                if output['exit'] != (2 if diagnostic else 0):
                    raise ValueError('starter must fail compilation and solution must succeed')
            if diagnostic:
                diagnostics = [json.loads(line) for line in contract['build']['stderr'].splitlines()]
                if not diagnostics or any(item.get('severity') != 'error' or not item.get('code')
                                          for item in diagnostics):
                    raise ValueError('starter must have structured errors')
            elif contract['args'] != []:
                raise ValueError('first exercise slice takes no arguments')
    return rows


def check(cli, selected=None, source=None, root=EXERCISES):
    """Audit both variants, or check a learner copy against the answer behavior."""
    rows = load_exercises(root)
    if selected is not None and selected not in {row['id'] for row in rows}:
        raise ValueError('unknown exercise ID')
    if source is not None and selected is None:
        raise ValueError('--source requires --exercise')
    checked = 0
    for row in rows:
        if selected is not None and row['id'] != selected:
            continue
        for variant in ('solution',) if source is not None else ('starter', 'solution'):
            original = source if source is not None else root / row['id'] / (variant + '.ko')
            with tempfile.TemporaryDirectory(prefix='kovenlings-') as temporary:
                directory = Path(temporary)
                (directory / 'source.ko').write_bytes(original.read_bytes())
                contract = row[variant]
                run_case(cli, contract, contract, directory)
            checked += 1
    print(f'kovenlings: {checked} source variants verified')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, default=ROOT / 'target/debug/kovenc')
    parser.add_argument('--exercise', help='stable exercise ID')
    parser.add_argument('--source', type=Path, help='check an edited copy against the solution contract')
    parser.add_argument('--hint', action='store_true', help='show the selected exercise hint')
    args = parser.parse_args()
    try:
        if args.hint:
            if args.source is not None or args.exercise is None:
                raise ValueError('--hint requires --exercise and cannot be combined with --source')
            rows = {row['id']: row for row in load_exercises()}
            if args.exercise not in rows:
                raise ValueError('unknown exercise ID')
            print(rows[args.exercise]['hint'])
        else:
            check(args.cli.resolve(), args.exercise, args.source)
    except (ValueError, OSError, AssertionError) as error:
        parser.exit(1, f'kovenlings: {error}\n')


if __name__ == '__main__':
    main()
