#!/usr/bin/env python3
"""Extract documented tutorial declarations, then verify their existing CLI oracles."""
import argparse
import contextlib
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

from check_tutorial import ROOT, load_examples, check as check_tutorial

INVENTORY = ROOT / 'docs/tutorials/api/examples.json'


def load_inventory(path=INVENTORY):
    """Bind only executable tutorials and their actual source files, without copies."""
    data = json.loads(path.read_text())
    if not isinstance(data, dict) or set(data) != {'version', 'entries'} or data['version'] != 1:
        raise ValueError('unsupported API example inventory')
    entries = data['entries']
    if not isinstance(entries, list) or not entries:
        raise ValueError('API example inventory must not be empty')
    examples = {row['id']: (row, sources) for row, sources in load_examples()}
    seen = set()
    bound = []
    for entry in entries:
        if (not isinstance(entry, dict) or set(entry) != {'example', 'file', 'function'}
                or any(not isinstance(value, str) or not value.strip() for value in entry.values())):
            raise ValueError('API entry requires example, file and function')
        identity = tuple(entry[key] for key in ('example', 'file', 'function'))
        if identity in seen:
            raise ValueError('duplicate API example entry')
        seen.add(identity)
        example = examples.get(entry['example'])
        if example is None or example[0]['status'] != 'executable' or entry['file'] not in example[1]:
            raise ValueError('API example must bind an executable tutorial source')
        bound.append((entry, example[0], example[1][entry['file']]))
    return bound


def extract(cli, source):
    """Use the real parser command; no secondary parser or execution implementation."""
    with tempfile.TemporaryDirectory(prefix='koven-api-') as temporary:
        path = Path(temporary) / 'source.ko'
        path.write_text(source, encoding='utf-8')
        result = subprocess.run([str(cli), 'doc', str(path)], capture_output=True)
        if result.returncode != 0 or result.stderr:
            raise ValueError(f'API extraction failed: {result.stderr.decode("utf-8")}')
        document = json.loads(result.stdout)
        if (document.get('schema') != 'koven.api-declarations' or document.get('version') != 1
                or document.get('guideVersion') != '0.45'
                or document.get('validation') != 'syntax-only'):
            raise ValueError('unsupported declaration export')
        return document


def check(cli, inventory=INVENTORY):
    entries = load_inventory(inventory)
    output = []
    selected = []
    for entry, row, source in entries:
        document = extract(cli, source)
        matches = [item for item in document['functions'] if item['name'] == entry['function']]
        if len(matches) != 1:
            raise ValueError('API example must identify exactly one exported function')
        fence = row.get('files', {'source.ko': row['id']})[entry['file']]
        output.append({**entry, 'sourceFence': fence,
                       'sourceSha256': hashlib.sha256(source.encode('utf-8')).hexdigest(),
                       'declaration': matches[0], 'compilerVersion': document['compilerVersion']})
        if entry['example'] not in selected:
            selected.append(entry['example'])
    # The existing checker owns preparation, build, direct execution and run oracles.
    with contextlib.redirect_stdout(sys.stderr):
        check_tutorial(cli, selected)
    return {'schema': 'koven.api-examples', 'version': 1, 'guideVersion': '0.45',
            'compilerSha256': hashlib.sha256(cli.read_bytes()).hexdigest(),
            'tutorial': 'docs/tutorials/koven-tour.md', 'entries': output}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, default=ROOT / 'target/debug/kovenc')
    args = parser.parse_args()
    try:
        document = check(args.cli.resolve())
        print(json.dumps(document, ensure_ascii=False, indent=2))
    except (ValueError, OSError, AssertionError) as error:
        parser.exit(1, f'API examples: {error}\n')


if __name__ == '__main__':
    main()
