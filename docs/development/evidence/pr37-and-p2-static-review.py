#!/usr/bin/env python3
"""Frozen read-only Git/blob review; never builds, fetches or changes Git history."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
EVIDENCE = ROOT / 'docs/development/evidence'
RECOVERY = '7c7cc254a4e8e36ebfa2d858a334d98806c2091d'
MERGE = '385bb23e1123c3a4ba00ec9fe5d964ebc95f1493'
LOCAL = '03395c64217e5bc9b11a6d2d48b49226a0e39440'


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT).decode()


def sha(value):
    return hashlib.sha256(value.encode()).hexdigest()


def commit(short):
    return git('rev-parse', short + '^{commit}').strip()


def blob(ref, path):
    return git('show', ref + ':' + path)


def data(name):
    return json.loads((EVIDENCE / (name + '.json')).read_text())


def main():
    result = {
        'kind': 'static Git/blob evidence, no behavioral or performance execution',
        'recovery': RECOVERY, 'pr37': MERGE, 'local_reviewed_head': LOCAL,
        'trees': {ref: git('rev-parse', ref + '^{tree}').strip()
                  for ref in (RECOVERY, MERGE, LOCAL)},
        'pr37_parents': git('show', '-s', '--format=%P', MERGE).strip().split(),
        'merge_base': git('merge-base', MERGE, LOCAL).strip(),
        'left_right_commits': git('rev-list', '--left-right', '--count', MERGE + '...' + LOCAL).split(),
        'recovery_to_pr37_paths': git('diff', '--name-only', RECOVERY, MERGE).splitlines(),
        'pr37_to_local_compiler_paths': git('diff', '--name-only', MERGE, LOCAL, '--', 'crates').splitlines(),
        'structural_pairs': [], 'block_hash_checks': [], 'production_file_hash_checks': [],
    }
    assert result['trees'][RECOVERY] == result['trees'][MERGE]
    assert result['merge_base'] == MERGE
    assert not result['recovery_to_pr37_paths']
    assert not result['pr37_to_local_compiler_paths']
    pairs = [
        ('lsp', '3418904', '1f3991b'), ('receiver', '7318d53', '7e542a5'),
        ('plan-tests', '5e9a295', '7720ea9'), ('iteration-tests', '9f9ee52', '1df4df1'),
        ('ownership-iteration', 'e30f9af', '5b53bf6'), ('multifile-type', 'b36d504', '25f8977'),
        ('multifile-ownership', 'f28359f', '422786e'), ('runtime-layout', 'f329efa', '61e6816'),
        ('iteration-production', '14612da', '1b24f45'), ('unit-planner', '1b24f45', 'fa277de'),
    ]
    for name, before, after in pairs:
        before, after = commit(before), commit(after)
        inputs = git('diff', '--name-only', before, after, '--',
                     'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates/*/Cargo.toml', '.cargo').splitlines()
        assert not inputs
        result['structural_pairs'].append(dict(name=name, before=before, after=after,
            before_tree=git('rev-parse', before+'^{tree}').strip(),
            after_tree=git('rev-parse', after+'^{tree}').strip(),
            changed_paths=git('diff', '--name-only', before, after).splitlines(),
            changed_cargo_toolchain_inputs=inputs))
    cache = {}
    for name, before, after in [('multifile-type-test-migration', 'b36d504', '25f8977'),
                                ('multifile-ownership-test-migration', 'f28359f', '422786e')]:
        record = data(name)
        for row in record['fidelity']:
            for side, ref, path, start, end, expected in [
                ('before', before, row.get('old_file', record['source']), row['old_start'], row['old_end'], row['original_block_sha256']),
                ('after', after, row['new_file'], row['new_start'], row['new_end'], row['new_block_sha256']),
            ]:
                key = (ref, path)
                if key not in cache:
                    cache[key] = blob(ref, path).splitlines(keepends=True)
                actual = sha(''.join(cache[key][start-1:end]).rstrip())
                assert actual == expected, (name, row['name'], side)
                result['block_hash_checks'].append(dict(record=name, name=row['name'], side=side,
                    commit=commit(ref), path=path, lines=[start, end], sha256=actual, matched=True))
    for name, ref, field in [('iteration-production-migration', '1b24f45', 'current_files'),
                              ('unit-planner-responsibility-equivalence', 'fa277de', 'source_files')]:
        for row in data(name)[field]:
            actual = sha(blob(ref, row['path']))
            assert actual == row['sha256'], (name, row['path'])
            result['production_file_hash_checks'].append(dict(record=name, commit=commit(ref),
                path=row['path'], sha256=actual, matched=True))
    result['result'] = dict(block_comparisons=len(result['block_hash_checks']),
                           production_file_comparisons=len(result['production_file_hash_checks']),
                           unreported_failures=0, budget_accepted=False, linux_executed=False)
    original = data('pr37-and-p2-static-review')
    assert result['block_hash_checks'] == original['block_hash_checks']
    assert result['production_file_hash_checks'] == original['production_file_hash_checks']
    output = EVIDENCE / 'publication-relationship.json'
    publication = {key: value for key, value in result.items()
                   if key not in {'structural_pairs', 'block_hash_checks', 'production_file_hash_checks'}}
    publication['historical_evidence_sha256'] = sha((EVIDENCE / 'pr37-and-p2-static-review.json').read_text())
    output.write_text(json.dumps(publication, ensure_ascii=False, indent=2) + '\n')
    print('PR37 tree-identical; ten pairs available; 400 block and nine production-file hashes matched')


if __name__ == '__main__':
    main()
