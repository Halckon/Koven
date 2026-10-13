"""SPEC-0304: strict declaration and lifecycle fixtures, without CI drivers."""
from __future__ import annotations

import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import check_docs


class SpecInventoryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.inventory = self.root / 'docs/specs/inventory.md'
        self.inventory.parent.mkdir(parents=True)
        self.write_inventory({'active': ['4001'], 'archived': ['4002']})
        self.spec('active', '4001', 'in-progress')
        self.spec('archive', '4002', 'done')
        for version, ids in check_docs.EXPECTED_DRAFT_SPEC_IDS.items():
            for spec_id in ids:
                self.spec(f'drafts/{version}', spec_id, 'draft')

    def write_inventory(self, data):
        body = data if isinstance(data, str) else json.dumps(data)
        self.inventory.write_text('# Spec inventory\n\n```json\n' + body + '\n```\n', encoding='utf-8')

    def spec(self, area, spec_id, status):
        directory = 'docs/archive/specs' if area == 'archive' else f'docs/specs/{area}'
        path = self.root / directory / f'{spec_id}-example.md'
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f'# SPEC-{spec_id}: Example\n\n| 状态 | {status} |\n', encoding='utf-8')
        return path

    def lifecycle_errors(self):
        checker = check_docs.DocsChecker(self.root)
        checker.check_spec_lifecycle()
        return checker.errors

    def test_document_declaration_is_the_independent_expected_inventory(self):
        self.assertEqual([], self.lifecycle_errors())

    def test_declaration_is_read_from_the_checked_root(self):
        active, archived = check_docs.read_spec_inventory(self.root)
        self.assertEqual(frozenset({'4001'}), active)
        self.assertEqual(frozenset({'4002'}), archived)

    def test_empty_lists_are_valid_only_when_explicit(self):
        self.write_inventory({'active': [], 'archived': []})
        self.assertEqual((frozenset(), frozenset()), check_docs.read_spec_inventory(self.root))
        self.assertTrue(any('未登记项' in error for error in self.lifecycle_errors()))

    def test_schema_and_id_errors_are_rejected_before_set_conversion(self):
        cases = [None, [], {'active': []}, {'active': [], 'archived': [], 'allow_missing': True},
                 {'active': '4001', 'archived': []}, {'active': [], 'archived': {}},
                 *({'active': [value], 'archived': []} for value in
                   (4001, True, None, [], {}, '1', '04001', ' 4001', '4001\n', '４００１', '../1')),
                 {'active': ['4001', '4001'], 'archived': []},
                 {'active': [], 'archived': ['4002', '4002']},
                 {'active': ['4001'], 'archived': ['4001']}]
        for data in cases:
            with self.subTest(data=data):
                self.write_inventory(data)
                with self.assertRaises(ValueError):
                    check_docs.read_spec_inventory(self.root)

    def test_duplicate_keys_and_nonstandard_json_are_rejected(self):
        for body in ('{"active":[],"active":[],"archived":[]}',
                     '{"active":[],"archived":[],"archived":[]}',
                     '{"active":[],"archived":[NaN]}',
                     '{"active":[],"archived":[Infinity]}',
                     '{"active":[],"archived":[-Infinity]}',
                     '{"active":[],"archived":[],}', '{} {}'):
            with self.subTest(body=body):
                self.write_inventory(body)
                with self.assertRaises(ValueError):
                    check_docs.read_spec_inventory(self.root)

    def test_exact_single_fence_is_required(self):
        valid = self.inventory.read_text()
        for text in ('# No declaration\n', valid + valid,
                     valid.replace('```json', '```python'), valid.replace('```json', ' ```json'),
                     valid.replace('```', '~~~'), valid.rsplit('```', 1)[0],
                     valid + '\n```\n', valid.replace('```json', '```json extra')):
            with self.subTest(text=text):
                self.inventory.write_text(text)
                with self.assertRaises(ValueError):
                    check_docs.read_spec_inventory(self.root)

    def test_non_json_whitespace_is_not_normalized_into_valid_json(self):
        for whitespace in ('\v', '\f', '\x85', '\u2028', '\u2029', '\x1c'):
            with self.subTest(whitespace=repr(whitespace)):
                self.write_inventory('{"active": [],' + whitespace + '"archived": []}')
                with self.assertRaises(ValueError):
                    check_docs.read_spec_inventory(self.root)

    def test_missing_file_does_not_fall_back_to_directory_contents(self):
        self.inventory.unlink()
        errors = self.lifecycle_errors()
        self.assertEqual(1, len(errors))
        self.assertIn('docs/specs/inventory.md', errors[0])

    def test_invalid_declaration_blocks_whole_checker_with_stable_error(self):
        self.write_inventory({'active': []})
        checker = check_docs.DocsChecker(self.root)
        errors = checker.run()
        self.assertEqual(1, len(errors))
        self.assertIn('docs/specs/inventory.md', errors[0])
        self.assertNotIn(str(self.root), errors[0])
        output = io.StringIO()
        with contextlib.redirect_stderr(output):
            self.assertEqual(1, check_docs.main(['--root', str(self.root)]))
        self.assertIn('documentation check failed', output.getvalue())

    def test_invalid_utf8_is_rejected_before_markdown_reads(self):
        self.inventory.write_bytes(b'\xff')
        errors = check_docs.DocsChecker(self.root).run()
        self.assertEqual(1, len(errors))
        self.assertIn('UTF-8', errors[0])

    def test_inventory_symlink_is_rejected_before_reading_target(self):
        self.inventory.unlink()
        self.inventory.symlink_to(self.root / 'nonexistent')
        self.assertEqual(1, len(check_docs.DocsChecker(self.root).run()))

    def test_parent_directory_symlinks_are_rejected(self):
        for component in ('docs', 'docs/specs'):
            with self.subTest(component=component), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                target = root / component
                target.parent.mkdir(parents=True, exist_ok=True)
                target.symlink_to(self.inventory.parent if component.endswith('specs') else self.root / 'docs',
                                  target_is_directory=True)
                with self.assertRaises(ValueError):
                    check_docs.read_spec_inventory(root)

    def test_directory_or_executable_inventory_is_rejected(self):
        self.inventory.chmod(0o755)
        with self.assertRaises(ValueError):
            check_docs.read_spec_inventory(self.root)
        self.inventory.unlink()
        self.inventory.mkdir()
        self.assertEqual(1, len(check_docs.DocsChecker(self.root).run()))

    def test_invalid_inventory_directory_contents_are_not_read_as_markdown(self):
        self.inventory.unlink()
        self.inventory.mkdir()
        (self.inventory / 'invalid.md').write_bytes(b'\xff')
        self.assertEqual(1, len(check_docs.DocsChecker(self.root).run()))

    @unittest.skipIf(os.geteuid() == 0, 'root bypasses unreadable-file permission fixture')
    def test_unreadable_inventory_is_a_structural_error(self):
        self.inventory.chmod(0)
        try:
            self.assertEqual(1, len(check_docs.DocsChecker(self.root).run()))
        finally:
            self.inventory.chmod(0o644)

    def test_fifo_is_rejected_without_opening_or_blocking(self):
        self.inventory.unlink()
        os.mkfifo(self.inventory)
        program = ('import sys; from pathlib import Path; sys.path.insert(0, sys.argv[1]); '
                   'from check_docs import DocsChecker; '
                   'errors = DocsChecker(Path(sys.argv[2])).run(); '
                   'assert len(errors) == 1, errors; print(errors[0])')
        result = subprocess.run([sys.executable, '-c', program, str(SCRIPTS), str(self.root)],
                                text=True, capture_output=True, timeout=5)
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertIn('docs/specs/inventory.md', result.stdout)

    def test_fifo_inside_invalid_inventory_directory_is_never_opened(self):
        self.inventory.unlink()
        self.inventory.mkdir()
        os.mkfifo(self.inventory / 'nested.md')
        program = ('import sys; from pathlib import Path; sys.path.insert(0, sys.argv[1]); '
                   'from check_docs import DocsChecker; '
                   'assert len(DocsChecker(Path(sys.argv[2])).run()) == 1')
        result = subprocess.run([sys.executable, '-c', program, str(SCRIPTS), str(self.root)],
                                text=True, capture_output=True, timeout=5)
        self.assertEqual(0, result.returncode, result.stderr)

    def test_archive_transition_requires_document_and_inventory_updates(self):
        source = self.root / 'docs/specs/active/4001-example.md'
        target = self.root / 'docs/archive/specs/4001-example.md'
        source.rename(target)
        self.assertTrue(any('与目录不一致' in error for error in self.lifecycle_errors()))
        target.write_text(target.read_text().replace('in-progress', 'done'))
        self.assertTrue(any('缺少' in error for error in self.lifecycle_errors()))
        self.assertTrue(any('未登记项' in error for error in self.lifecycle_errors()))
        self.write_inventory({'active': [], 'archived': ['4001', '4002']})
        self.assertEqual([], self.lifecycle_errors())

    def test_missing_and_unregistered_documents_are_both_rejected(self):
        (self.root / 'docs/archive/specs/4002-example.md').unlink()
        self.spec('archive', '4003', 'done')
        errors = self.lifecycle_errors()
        self.assertTrue(any('缺少 1 项: SPEC-4002' in error for error in errors), errors)
        self.assertTrue(any('未登记项: SPEC-4003' in error for error in errors), errors)

    def test_valid_inventory_does_not_replace_spec_index_coverage(self):
        for area in ('docs/specs/active', 'docs/archive/specs'):
            (self.root / area / 'README.md').write_text('# Index\n')
        checker = check_docs.DocsChecker(self.root)
        checker.check_indexes()
        self.assertTrue(any('active Spec 索引遗漏' in error for error in checker.errors))
        self.assertTrue(any('archived Spec 索引遗漏' in error for error in checker.errors))
        for area, spec_id in (('docs/specs/active', '4001'), ('docs/archive/specs', '4002')):
            (self.root / area / 'README.md').write_text(f'# Index\n\n[Spec]({spec_id}-example.md)\n')
        checker = check_docs.DocsChecker(self.root)
        checker.check_indexes()
        self.assertFalse(any('active Spec' in error or 'archived Spec' in error
                             for error in checker.errors), checker.errors)

    def test_valid_lifecycle_transition_still_requires_fresh_graphs(self):
        from gen_spec_dag import render_targets

        for path, text in render_targets(self.root).items():
            path.write_text(text, encoding='utf-8')
        checker = check_docs.DocsChecker(self.root)
        checker.check_generated_graph()
        self.assertEqual([], checker.errors)
        source = self.root / 'docs/specs/active/4001-example.md'
        target = self.root / 'docs/archive/specs/4001-example.md'
        source.rename(target)
        target.write_text(target.read_text().replace('in-progress', 'done'))
        self.write_inventory({'active': [], 'archived': ['4001', '4002']})
        self.assertEqual([], self.lifecycle_errors())
        checker = check_docs.DocsChecker(self.root)
        checker.check_generated_graph()
        self.assertEqual(4, len(checker.errors), checker.errors)
        self.assertTrue(all('生成物过期' in error for error in checker.errors))

    def test_frozen_done_title_status_and_global_id_guards_remain(self):
        path = self.root / 'docs/archive/specs/4002-example.md'
        original = path.read_text()
        for text, expected in ((original.replace('done', 'superseded'), '状态必须保持 done'),
                               (original.replace('SPEC-4002', 'SPEC-4003'), '标题编号不一致'),
                               (original.replace('done', 'unknown'), '未知 Spec 状态'),
                               (original.replace('| 状态 | done |', ''), '缺少 Spec 状态')):
            with self.subTest(expected=expected):
                path.write_text(text)
                self.assertTrue(any(expected in error for error in self.lifecycle_errors()))
        path.write_text(original)
        self.spec('active', '4002', 'approved')
        self.assertTrue(any('编号重复' in error for error in self.lifecycle_errors()))

    def test_draft_inventory_is_not_read_from_declaration(self):
        self.spec('drafts/v9.99', '4003', 'draft')
        self.assertTrue(any('Draft Spec v9.99 inventory: 存在未登记项' in error
                            for error in self.lifecycle_errors()))


if __name__ == '__main__':
    unittest.main()
