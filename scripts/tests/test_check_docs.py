from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from check_docs import DocsChecker  # noqa: E402


class DocsCheckerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write(self, relative: str, text: str) -> Path:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def link_errors(self) -> list[str]:
        checker = DocsChecker(self.root)
        checker.check_links()
        return checker.errors

    def test_valid_and_invalid_local_paths(self) -> None:
        self.write("docs/target.md", "# 目标\n")
        source = self.write("docs/source.md", "# 来源\n\n[有效](target.md#目标)\n")
        self.assertEqual(self.link_errors(), [])

        source.write_text("# 来源\n\n[失效](missing.md)\n", encoding="utf-8")
        errors = self.link_errors()
        self.assertTrue(any("本地链接不存在" in error for error in errors), errors)

    def test_chinese_and_duplicate_github_anchors(self) -> None:
        self.write("docs/target.md", "# 页面\n\n## 中文 标题！\n\n## 中文 标题！\n")
        source = self.write(
            "docs/source.md",
            "# 来源\n\n[一](target.md#中文-标题) [二](target.md#中文-标题-1)\n",
        )
        self.assertEqual(self.link_errors(), [])

        source.write_text("# 来源\n\n[错](target.md#中文-标题！)\n", encoding="utf-8")
        errors = self.link_errors()
        self.assertTrue(any("fragment 不存在" in error for error in errors), errors)

    def test_generic_markup_and_slug_collisions(self) -> None:
        self.write(
            "docs/target.md",
            "# Result<T, E> 与 `Rc<T>`\n\n"
            "## Echo\n\n## Echo\n\n## Echo 1\n\n## Echo-1\n\n## Echo\n\n## Straße\n",
        )
        self.write(
            "docs/source.md",
            "# 来源\n\n"
            "[泛型](target.md#resultt-e-与-rct)\n"
            "[一](target.md#echo) [二](target.md#echo-1) "
            "[三](target.md#echo-1-1) [四](target.md#echo-1-2) [五](target.md#echo-2) "
            "[Unicode 小写](target.md#straße)\n",
        )
        self.assertEqual(self.link_errors(), [])

    def test_duplicate_spec_id_across_lifecycle_directories(self) -> None:
        self.write(
            "docs/specs/drafts/v0.35/0001-a.md",
            "# SPEC-0001: A\n\n| 字段 | 值 |\n|---|---|\n| 状态 | `draft` |\n",
        )
        self.write(
            "docs/archive/specs/0001-b.md",
            "# SPEC-0001: B\n\n| 字段 | 值 |\n|---|---|\n| 状态 | done |\n",
        )
        checker = DocsChecker(self.root)
        checker.check_spec_lifecycle()
        self.assertTrue(any("SPEC-0001: 编号重复" in error for error in checker.errors), checker.errors)

    def test_spec_status_must_match_directory(self) -> None:
        self.write(
            "docs/specs/active/0002-wrong.md",
            "# SPEC-0002: Wrong\n\n| 字段 | 值 |\n|---|---|\n| 状态 | `draft` |\n",
        )
        checker = DocsChecker(self.root)
        checker.check_spec_lifecycle()
        self.assertTrue(any("状态 draft 与目录不一致" in error for error in checker.errors), checker.errors)

    def test_multiple_current_guide_markers_are_rejected(self) -> None:
        marker = "<!-- current-guide: v0.37 -->\n"
        self.write("docs/guide/README.md", "# Guide\n" + marker)
        self.write("docs/proposals/accidental.md", "# Proposal\n" + marker)
        checker = DocsChecker(self.root)
        checker.check_current_guide()
        self.assertTrue(
            any("current guide marker 必须恰好一个，实际为 2" in error for error in checker.errors),
            checker.errors,
        )

    def test_archive_cannot_enter_default_route(self) -> None:
        self.write(
            "docs/README.md",
            "# Docs\n\n## 当前入口\n\n| 任务 | 入口 |\n|---|---|\n"
            "| 开发 | [错误默认入口](archive/README.md) |\n\n"
            "## 历史材料\n\n按需读取。\n",
        )
        checker = DocsChecker(self.root)
        checker.check_default_routes()
        self.assertTrue(
            any("不得直接加载 archive/proposals" in error for error in checker.errors),
            checker.errors,
        )

    def test_proposal_cannot_enter_default_route(self) -> None:
        self.write(
            "docs/README.md",
            "# Docs\n\n## 默认入口\n\n| 任务 | 入口 |\n|---|---|\n"
            "| 开发 | [候选](proposals/README.md) |\n",
        )
        checker = DocsChecker(self.root)
        checker.check_default_routes()
        self.assertTrue(
            any("不得直接加载 archive/proposals" in error for error in checker.errors),
            checker.errors,
        )

    def test_bullet_route_counts_fragment_links(self) -> None:
        links = []
        for number in range(6):
            self.write(f"docs/{number}.md", "# 目标\n")
            links.append(f"- [入口 {number}](../../docs/{number}.md#目标)")
        self.write(
            "crates/lang-frontend/AGENTS.md",
            "# AGENTS\n\n## 必读入口\n\n" + "\n".join(links) + "\n",
        )
        checker = DocsChecker(self.root)
        checker.check_default_routes()
        self.assertTrue(any("包含 6 份文档" in error for error in checker.errors), checker.errors)

    def test_scoped_route_rejects_unresolved_document_placeholder(self) -> None:
        self.write(
            "crates/lang-codegen/AGENTS.md",
            "# AGENTS\n\n## 按任务读取\n\n| 任务 | 文档 |\n|---|---|\n"
            "| runtime | [架构](../../docs/README.md)、相关 accepted ADR |\n",
        )
        self.write("docs/README.md", "# Docs\n")
        checker = DocsChecker(self.root)
        checker.check_default_routes()
        self.assertTrue(any("未解析的相关/对应文档泛称" in error for error in checker.errors), checker.errors)

    def test_scoped_agents_must_be_complete_and_routed(self) -> None:
        for directory in ("lang-frontend", "lang-codegen", "lang-cli", "lang-lsp"):
            self.write(
                f"crates/{directory}/AGENTS.md",
                "# AGENTS\n\n## 必读入口\n\n[入口](../../docs/README.md)\n",
            )
        self.write("docs/README.md", "# Docs\n")
        checker = DocsChecker(self.root)
        checker.check_scoped_agents()
        self.assertTrue(
            any("crates/lang-std/AGENTS.md" in error for error in checker.errors),
            checker.errors,
        )

    def test_migration_ledger_targets_must_exist(self) -> None:
        self.write("docs/archive/guides/v0.34-pre-restructure/00-index.md", "# Guide\n\n## 旧标题\n")
        self.write(
            "docs/archive/migrations/v0.34-document-restructure.md",
            "# Ledger\n\n| 旧标题 | 分类 | 新归属 |\n|---|---|---|\n"
            "| `docs/guide/00-index.md#旧标题` | 现行 | `docs/guide/missing.md` |\n",
        )
        checker = DocsChecker(self.root)
        checker.check_migration_ledger()
        self.assertTrue(any("新归属不存在" in error for error in checker.errors), checker.errors)

    def test_frozen_spec_inventory_rejects_missing_documents(self) -> None:
        self.write(
            "docs/archive/specs/0001-a.md",
            "# SPEC-0001: A\n\n| 字段 | 值 |\n|---|---|\n| 状态 | done |\n",
        )
        checker = DocsChecker(self.root)
        checker.check_spec_lifecycle()
        self.assertTrue(any("归档 Spec inventory: 缺少" in error for error in checker.errors), checker.errors)

    def test_frozen_done_spec_cannot_silently_become_superseded(self) -> None:
        self.write(
            "docs/archive/specs/0001-a.md",
            "# SPEC-0001: A\n\n| 字段 | 值 |\n|---|---|\n| 状态 | superseded |\n",
        )
        checker = DocsChecker(self.root)
        checker.check_spec_lifecycle()
        self.assertTrue(any("状态必须保持 done" in error for error in checker.errors), checker.errors)

    def test_adr_status_and_frozen_inventory_are_checked(self) -> None:
        self.write(
            "docs/adr/accepted/0001-a.md",
            "# ADR-0001: A\n\n## 状态\n\nproposed\n",
        )
        checker = DocsChecker(self.root)
        checker.check_adr_lifecycle()
        self.assertTrue(any("状态 proposed 与目录不一致" in error for error in checker.errors), checker.errors)
        self.assertTrue(any("Accepted ADR inventory: 缺少" in error for error in checker.errors), checker.errors)

    def test_live_page_metadata_is_required(self) -> None:
        self.write("docs/architecture/orphan.md", "# Orphan\n")
        checker = DocsChecker(self.root)
        checker.check_metadata()
        self.assertTrue(any("顶部缺少页面元信息" in error for error in checker.errors), checker.errors)

    def test_index_must_link_every_member(self) -> None:
        self.write("docs/guide/README.md", "# Guide\n")
        self.write("docs/guide/01-orphan.md", "# Orphan\n")
        checker = DocsChecker(self.root)
        checker.check_indexes()
        self.assertTrue(any("guide 索引遗漏" in error for error in checker.errors), checker.errors)

    def test_live_page_must_be_reachable(self) -> None:
        self.write("docs/README.md", "# Docs\n")
        self.write(
            "docs/architecture/orphan.md",
            "# Orphan\n\n> **性质** x **状态** x **读取时机** x **唯一真源** x\n",
        )
        checker = DocsChecker(self.root)
        checker.check_links()
        checker.check_reachability()
        self.assertTrue(any("无法在 4 跳内" in error for error in checker.errors), checker.errors)

    def test_stale_public_path_is_rejected(self) -> None:
        self.write(
            "docs/live.md",
            "# Live\n\n旧链接：docs/guide/00-index.md\n",
        )
        checker = DocsChecker(self.root)
        checker.check_stale_paths()
        self.assertTrue(any("残留旧公共文档路径" in error for error in checker.errors), checker.errors)

    def test_entry_line_budget_is_enforced(self) -> None:
        self.write("AGENTS.md", "# AGENTS\n" + "line\n" * 160)
        checker = DocsChecker(self.root)
        checker.check_line_budgets()
        self.assertTrue(any("超过入口上限 160" in error for error in checker.errors), checker.errors)

    def test_architecture_task_routes_are_checked(self) -> None:
        links = []
        for number in range(6):
            self.write(f"docs/{number}.md", "# Target\n")
            links.append(f"[入口 {number}](../{number}.md)")
        self.write(
            "docs/architecture/README.md",
            "# Architecture\n\n## 按实现领域读取\n\n| 任务 | 文档 |\n|---|---|\n"
            "| 跨域 | " + "、".join(links) + " |\n",
        )
        checker = DocsChecker(self.root)
        checker.check_default_routes()
        self.assertTrue(any("包含 6 份文档" in error for error in checker.errors), checker.errors)


if __name__ == "__main__":
    unittest.main()
