#!/usr/bin/env python3
"""Validate Koven's documentation graph and lifecycle layout.

The checker verifies structural facts only. It does not claim semantic equivalence
between the archived source documents and the reorganized live documentation.
"""

from __future__ import annotations

import argparse
import html
import os
import re
import sys
from collections import defaultdict, deque
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import unquote, urlsplit


IGNORED_DIRS = {".git", ".hg", ".svn", "node_modules", "target", ".venv", "venv"}
METADATA_FIELDS = ("**性质**", "**状态**", "**读取时机**", "**唯一真源**")
SPEC_FILE_RE = re.compile(r"^(\d{4})-[a-z0-9][a-z0-9-]*\.md$")
ADR_FILE_RE = SPEC_FILE_RE
HEADING_RE = re.compile(r"^(#{1,6})[ \t]+(.+?)[ \t]*#*[ \t]*$")
INLINE_LINK_RE = re.compile(
    r"!?\[[^\]]*\]\(\s*(?P<target><[^>]+>|[^\s)]+)"
    r"(?:\s+(?:\"[^\"]*\"|'[^']*'|\([^)]*\)))?\s*\)"
)
REFERENCE_LINK_RE = re.compile(r"^\s*\[[^\]]+\]:\s*(?P<target><[^>]+>|\S+)")
INLINE_CODE_RE = re.compile(r"(`+)(.*?)\1")
MARKDOWN_LABEL_RE = re.compile(r"\[([^\]]+)\]\([^)]*\)")
HTML_TAG_RE = re.compile(
    r"</?[A-Za-z][A-Za-z0-9-]*(?:\s+[A-Za-z_:][^>]*)?\s*/?>"
)
CURRENT_GUIDE_RE = re.compile(r"<!--\s*current-guide:\s*v0\.38\s*-->")
SCOPED_AGENT_DIRS = (
    "lang-frontend",
    "lang-codegen",
    "lang-cli",
    "lang-lsp",
    "lang-std",
)
EXPECTED_ARCHIVED_SPEC_IDS = frozenset(
    f"{number:04d}"
    for number in (
        set(range(1, 24))
        | {25, 26}
        | set(range(27, 31))
        | set(range(32, 37))
        | set(range(38, 41))
        | set(range(42, 46))
        | {52}
        | set(range(54, 61))
        | set(range(62, 180))
        | {180, 181}
        | set(range(183, 200))
        | {201, 202, 203, 204, 205, 206, 207, 208, 209, 210}
        | set(range(213, 228))
        | {211, 212}
    )
)
EXPECTED_DRAFT_SPEC_IDS = {
    "v0.36": frozenset(),
    "v0.37": frozenset(),
}
EXPECTED_ACTIVE_SPEC_IDS: frozenset[str] = frozenset({"0182", "0228", "0231"})
EXPECTED_ACCEPTED_ADR_IDS = frozenset(
    {*(f"{number:04d}" for number in range(1, 25)), "0026"}
)
EXPECTED_PROPOSED_ADR_IDS: frozenset[str] = frozenset({"0025"})
EXPECTED_ARCHIVED_ADR_IDS: frozenset[str] = frozenset()
ROUTE_HEADING_RE = re.compile(
    r"(?:默认入口|当前入口|按任务读取|按实现领域读取|必读入口|读取路由)", re.IGNORECASE
)
VAGUE_ROUTE_RE = re.compile(
    r"(?:相关|对应)(?:的)?\s*(?:领域\s*)?(?:guide|adr|accepted|类型/所有权 guide)",
    re.IGNORECASE,
)


@dataclass(frozen=True)
class MarkdownLink:
    source: Path
    line: int
    raw_target: str


def relative(path: Path, root: Path) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return str(path)


def markdown_files(root: Path) -> list[Path]:
    result: list[Path] = []
    for current, dirs, files in os.walk(root):
        dirs[:] = sorted(directory for directory in dirs if directory not in IGNORED_DIRS)
        for name in sorted(files):
            if name.endswith(".md"):
                result.append(Path(current) / name)
    return result


def without_fenced_code(text: str) -> list[tuple[int, str]]:
    visible: list[tuple[int, str]] = []
    fence: str | None = None
    for number, line in enumerate(text.splitlines(), 1):
        stripped = line.lstrip()
        marker = None
        if stripped.startswith("```"):
            marker = "```"
        elif stripped.startswith("~~~"):
            marker = "~~~"
        if marker is not None:
            if fence is None:
                fence = marker
            elif fence == marker:
                fence = None
            continue
        if fence is None:
            visible.append((number, line))
    return visible


def iter_links(path: Path) -> list[MarkdownLink]:
    links: list[MarkdownLink] = []
    text = path.read_text(encoding="utf-8")
    for number, line in without_fenced_code(text):
        line_without_code = INLINE_CODE_RE.sub("", line)
        for match in INLINE_LINK_RE.finditer(line_without_code):
            links.append(MarkdownLink(path, number, match.group("target")))
        reference = REFERENCE_LINK_RE.match(line_without_code)
        if reference:
            links.append(MarkdownLink(path, number, reference.group("target")))
    return links


def markdown_heading_text(heading: str) -> str:
    """Return the visible text that GitHub uses as the heading slug input."""

    value = html.unescape(heading)
    code_spans: list[str] = []

    def protect_code(match: re.Match[str]) -> str:
        code_spans.append(match.group(2))
        return f"\x00CODE{len(code_spans) - 1}\x00"

    value = INLINE_CODE_RE.sub(protect_code, value)
    value = MARKDOWN_LABEL_RE.sub(r"\1", value)
    value = HTML_TAG_RE.sub("", value)
    value = re.sub(r"(?<!\w)_{1,3}(.+?)_{1,3}(?!\w)", r"\1", value)
    value = value.replace("*", "").replace("~", "")
    for index, code in enumerate(code_spans):
        value = value.replace(f"\x00CODE{index}\x00", code)
    return value


def github_slug(heading: str) -> str:
    """Apply GitHub's Unicode heading slug rules to Markdown heading text."""

    value = markdown_heading_text(heading).lower()
    value = re.sub(r"[^\w\- ]", "", value)
    return value.replace(" ", "-")


def unique_slug(base: str, seen: dict[str, int]) -> str:
    """Disambiguate a base slug using GitHub's collision-aware suffixes."""

    if base not in seen:
        seen[base] = 0
        return base
    while True:
        seen[base] += 1
        candidate = f"{base}-{seen[base]}"
        if candidate not in seen:
            seen[candidate] = 0
            return candidate


def heading_anchors(path: Path) -> set[str]:
    seen: dict[str, int] = {}
    anchors: set[str] = set()
    for _, line in without_fenced_code(path.read_text(encoding="utf-8")):
        match = HEADING_RE.match(line)
        if not match:
            continue
        anchors.add(unique_slug(github_slug(match.group(2)), seen))
    for explicit in re.findall(r"<(?:a|[^ >]+)[^>]+(?:id|name)=[\"']([^\"']+)[\"']", path.read_text(encoding="utf-8"), re.IGNORECASE):
        anchors.add(explicit)
    return anchors


def split_local_target(raw_target: str) -> tuple[str, str] | None:
    target = raw_target.strip()
    if target.startswith("<") and target.endswith(">"):
        target = target[1:-1]
    parsed = urlsplit(target)
    if parsed.scheme or parsed.netloc or target.startswith("//"):
        return None
    return unquote(parsed.path), unquote(parsed.fragment)


def resolve_link(source: Path, raw_target: str, root: Path) -> tuple[Path, str] | None:
    parts = split_local_target(raw_target)
    if parts is None:
        return None
    path_text, fragment = parts
    target = source if not path_text else (source.parent / path_text)
    target = target.resolve()
    if target.is_dir() and (target / "README.md").is_file():
        target = target / "README.md"
    return target, fragment


def extract_spec_status(path: Path) -> str | None:
    text = path.read_text(encoding="utf-8")
    match = re.search(r"^\|\s*状态\s*\|\s*`?([^|`\n]+)`?\s*\|", text, re.MULTILINE)
    if not match:
        return None
    return match.group(1).strip().lower().replace("_", "-").replace(" ", "-")


def extract_adr_status(path: Path) -> str | None:
    text = path.read_text(encoding="utf-8")
    match = re.search(r"^##\s+状态\s*$\s*^`?([a-zA-Z-]+)`?\s*$", text, re.MULTILINE)
    return match.group(1).lower() if match else None


class DocsChecker:
    def __init__(self, root: Path):
        self.root = root.resolve()
        self.docs = self.root / "docs"
        self.errors: list[str] = []
        self.files = markdown_files(self.root)
        self.links: dict[Path, list[MarkdownLink]] = {path: iter_links(path) for path in self.files}
        self.graph: dict[Path, set[Path]] = defaultdict(set)
        self._anchors: dict[Path, set[str]] = {}

    def error(self, message: str) -> None:
        self.errors.append(message)

    def check_inventory(
        self,
        label: str,
        actual: set[str],
        expected: frozenset[str],
        item_prefix: str,
    ) -> None:
        """Require the checked-in frozen inventory, not merely internally valid survivors."""

        missing = sorted(expected - actual)
        unexpected = sorted(actual - expected)
        if missing:
            self.error(
                f"{label}: 缺少 {len(missing)} 项: "
                + ", ".join(f"{item_prefix}-{item}" for item in missing)
            )
        if unexpected:
            self.error(
                f"{label}: 存在未登记项: "
                + ", ".join(f"{item_prefix}-{item}" for item in unexpected)
            )

    def run(self) -> list[str]:
        self.check_links()
        self.check_spec_lifecycle()
        self.check_adr_lifecycle()
        self.check_current_guide()
        self.check_metadata()
        self.check_indexes()
        self.check_reachability()
        self.check_scoped_agents()
        self.check_default_routes()
        self.check_stale_paths()
        self.check_migration_ledger()
        self.check_line_budgets()
        self.check_generated_graph()
        return sorted(set(self.errors))

    def check_generated_graph(self) -> None:
        """Verify generated dependency-graph artifacts are byte-fresh."""
        from gen_spec_dag import drift_errors

        for message in drift_errors(self.root):
            self.error(message)

    def check_links(self) -> None:
        for source, links in self.links.items():
            for link in links:
                resolved = resolve_link(source, link.raw_target, self.root)
                if resolved is None:
                    continue
                target, fragment = resolved
                try:
                    target.relative_to(self.root)
                except ValueError:
                    self.error(
                        f"{relative(source, self.root)}:{link.line}: 本地链接越出仓库: {link.raw_target}"
                    )
                    continue
                if not target.exists():
                    self.error(
                        f"{relative(source, self.root)}:{link.line}: 本地链接不存在: {link.raw_target}"
                    )
                    continue
                if target.is_file() and target.suffix == ".md":
                    self.graph[source].add(target)
                    if fragment:
                        anchors = self._anchors.setdefault(target, heading_anchors(target))
                        if fragment not in anchors:
                            self.error(
                                f"{relative(source, self.root)}:{link.line}: Markdown fragment 不存在: "
                                f"{link.raw_target}"
                            )

    def check_spec_lifecycle(self) -> None:
        candidates = list((self.docs / "specs").rglob("*.md")) + list(
            (self.docs / "archive/specs").glob("*.md")
        )
        ids: dict[str, list[Path]] = defaultdict(list)
        archived: set[str] = set()
        active: set[str] = set()
        drafts: dict[str, set[str]] = defaultdict(set)
        for path in candidates:
            match = SPEC_FILE_RE.match(path.name)
            if not match:
                continue
            spec_id = match.group(1)
            ids[spec_id].append(path)
            status = extract_spec_status(path)
            rel = relative(path, self.root)
            title_match = re.search(r"^#\s+SPEC-(\d{4})\b", path.read_text(encoding="utf-8"), re.MULTILINE)
            if not title_match or title_match.group(1) != spec_id:
                self.error(f"{rel}: Spec 文件名与标题编号不一致")
            if status is None:
                self.error(f"{rel}: 缺少 Spec 状态")
                continue
            if status == "draft":
                expected = "docs/specs/drafts/"
            elif status in {"approved", "in-progress"}:
                expected = "docs/specs/active/"
            elif status in {"done", "superseded"}:
                expected = "docs/archive/specs/"
            else:
                self.error(f"{rel}: 未知 Spec 状态 {status}")
                continue
            if not rel.startswith(expected):
                self.error(f"{rel}: 状态 {status} 与目录不一致，应位于 {expected}")
            if rel.startswith("docs/archive/specs/"):
                archived.add(spec_id)
                if spec_id in EXPECTED_ARCHIVED_SPEC_IDS and status != "done":
                    self.error(
                        f"{rel}: 冻结 inventory 中的 SPEC-{spec_id} 状态必须保持 done"
                    )
            elif rel.startswith("docs/specs/active/"):
                active.add(spec_id)
            elif rel.startswith("docs/specs/drafts/"):
                parts = Path(rel).parts
                if len(parts) >= 4:
                    drafts[parts[3]].add(spec_id)
        for spec_id, paths in ids.items():
            if len(paths) > 1:
                names = ", ".join(relative(path, self.root) for path in sorted(paths))
                self.error(f"SPEC-{spec_id}: 编号重复: {names}")
        self.check_inventory(
            "归档 Spec inventory", archived, EXPECTED_ARCHIVED_SPEC_IDS, "SPEC"
        )
        self.check_inventory(
            "Active Spec inventory", active, EXPECTED_ACTIVE_SPEC_IDS, "SPEC"
        )
        for version, expected_ids in EXPECTED_DRAFT_SPEC_IDS.items():
            self.check_inventory(
                f"Draft Spec {version} inventory",
                drafts.pop(version, set()),
                expected_ids,
                "SPEC",
            )
        for version, actual_ids in sorted(drafts.items()):
            self.check_inventory(
                f"Draft Spec {version} inventory", actual_ids, frozenset(), "SPEC"
            )

    def check_adr_lifecycle(self) -> None:
        candidates = list((self.docs / "adr").rglob("*.md")) + list(
            (self.docs / "archive/adr").rglob("*.md") if (self.docs / "archive/adr").exists() else []
        )
        ids: dict[str, list[Path]] = defaultdict(list)
        accepted: set[str] = set()
        proposed: set[str] = set()
        archived: set[str] = set()
        for path in candidates:
            match = ADR_FILE_RE.match(path.name)
            if not match:
                continue
            adr_id = match.group(1)
            ids[adr_id].append(path)
            status = extract_adr_status(path)
            rel = relative(path, self.root)
            title_match = re.search(r"^#\s+ADR-(\d{4})\b", path.read_text(encoding="utf-8"), re.MULTILINE)
            if not title_match or title_match.group(1) != adr_id:
                self.error(f"{rel}: ADR 文件名与标题编号不一致")
            expected = {
                "accepted": "docs/adr/accepted/",
                "proposed": "docs/adr/proposed/",
                "rejected": "docs/archive/adr/",
                "superseded": "docs/archive/adr/",
            }.get(status or "")
            if expected is None:
                self.error(f"{rel}: 缺少或未知 ADR 状态 {status!r}")
            elif not rel.startswith(expected):
                self.error(f"{rel}: 状态 {status} 与目录不一致，应位于 {expected}")
            if rel.startswith("docs/adr/accepted/"):
                accepted.add(adr_id)
            elif rel.startswith("docs/adr/proposed/"):
                proposed.add(adr_id)
            elif rel.startswith("docs/archive/adr/"):
                archived.add(adr_id)
        for adr_id, paths in ids.items():
            if len(paths) > 1:
                names = ", ".join(relative(path, self.root) for path in sorted(paths))
                self.error(f"ADR-{adr_id}: 编号重复: {names}")
        self.check_inventory(
            "Accepted ADR inventory", accepted, EXPECTED_ACCEPTED_ADR_IDS, "ADR"
        )
        self.check_inventory(
            "Proposed ADR inventory", proposed, EXPECTED_PROPOSED_ADR_IDS, "ADR"
        )
        self.check_inventory(
            "Archived ADR inventory", archived, EXPECTED_ARCHIVED_ADR_IDS, "ADR"
        )

    def check_current_guide(self) -> None:
        live_texts = [
            path.read_text(encoding="utf-8")
            for path in self.files
            if "docs/archive/" not in relative(path, self.root)
        ]
        count = sum(len(CURRENT_GUIDE_RE.findall(text)) for text in live_texts)
        if count != 1:
            self.error(f"current guide marker 必须恰好一个，实际为 {count}")
        guide = self.docs / "guide"
        expected = {guide / "README.md"} | {
            guide / f"{number:02d}-{name}.md"
            for number, name in enumerate(
                [
                    "lexical",
                    "names-files-packages",
                    "types-generics",
                    "expressions-operators",
                    "declarations-callables",
                    "blocks-control-flow",
                    "calls-lambdas-closures",
                    "class-family-members",
                    "nullability-errors",
                    "ownership-borrowing-drop",
                    "copyability-layout-construction",
                    "collections-destructuring",
                    "program-runtime-standard-library",
                    "syntax-index",
                    "conformance-and-staging",
                ],
                1,
            )
        }
        actual = set(guide.glob("*.md")) if guide.exists() else set()
        missing = expected - actual
        extra = actual - expected
        for path in sorted(missing):
            self.error(f"缺少 guide 页面: {relative(path, self.root)}")
        for path in sorted(extra):
            self.error(f"guide 目录存在非标准页面: {relative(path, self.root)}")

    def live_metadata_files(self) -> list[Path]:
        result: list[Path] = []
        for path in self.files:
            rel = relative(path, self.root)
            if rel == "docs/AGENTS.md" or rel == "docs/README.md":
                result.append(path)
            elif rel.startswith(
                (
                    "docs/guide/",
                    "docs/architecture/",
                    "docs/development/",
                    "docs/proposals/",
                    "docs/specs/",
                    "docs/adr/",
                )
            ):
                result.append(path)
        return result

    def check_metadata(self) -> None:
        for path in self.live_metadata_files():
            top = "\n".join(path.read_text(encoding="utf-8").splitlines()[:16])
            missing = [field for field in METADATA_FIELDS if field not in top]
            if missing:
                self.error(
                    f"{relative(path, self.root)}: 顶部缺少页面元信息: {', '.join(missing)}"
                )
        proposals = self.docs / "proposals"
        for path in proposals.glob("*.md") if proposals.exists() else []:
            if path.name == "README.md":
                continue
            top = "\n".join(path.read_text(encoding="utf-8").splitlines()[:12])
            if "未启用" not in top or "非规范" not in top:
                self.error(f"{relative(path, self.root)}: proposal 必须明确标记非规范且未启用")

    def direct_markdown_targets(self, path: Path) -> set[Path]:
        return {
            target
            for link in self.links.get(path, [])
            if (resolved := resolve_link(path, link.raw_target, self.root)) is not None
            for target, _ in [resolved]
            if target.is_file() and target.suffix == ".md"
        }

    def require_direct_coverage(self, index: Path, members: set[Path], label: str) -> None:
        if not index.is_file():
            self.error(f"缺少 {label} 索引: {relative(index, self.root)}")
            return
        linked = self.direct_markdown_targets(index)
        for path in sorted(members - linked):
            self.error(
                f"{relative(index, self.root)}: {label} 索引遗漏 {relative(path, self.root)}"
            )

    def check_indexes(self) -> None:
        guide = self.docs / "guide"
        self.require_direct_coverage(
            guide / "README.md", set(guide.glob("[0-9][0-9]-*.md")), "guide"
        )
        architecture = self.docs / "architecture"
        self.require_direct_coverage(
            architecture / "README.md",
            set(architecture.glob("*.md")) - {architecture / "README.md"},
            "architecture",
        )
        development = self.docs / "development"
        self.require_direct_coverage(
            development / "README.md",
            set(development.glob("*.md")) - {development / "README.md"},
            "development",
        )
        proposals = self.docs / "proposals"
        self.require_direct_coverage(
            proposals / "README.md",
            set(proposals.glob("*.md")) - {proposals / "README.md"},
            "proposal",
        )
        for version in EXPECTED_DRAFT_SPEC_IDS:
            directory = self.docs / "specs/drafts" / version
            self.require_direct_coverage(
                directory / "README.md",
                set(directory.glob("[0-9][0-9][0-9][0-9]-*.md")),
                f"draft {version}",
            )
        active = self.docs / "specs/active"
        self.require_direct_coverage(
            active / "README.md",
            set(active.glob("[0-9][0-9][0-9][0-9]-*.md")),
            "active Spec",
        )
        archived_specs = self.docs / "archive/specs"
        self.require_direct_coverage(
            archived_specs / "README.md",
            set(archived_specs.glob("[0-9][0-9][0-9][0-9]-*.md")),
            "archived Spec",
        )
        adr = self.docs / "adr"
        self.require_direct_coverage(
            adr / "README.md",
            set((adr / "accepted").glob("[0-9][0-9][0-9][0-9]-*.md"))
            | set((adr / "proposed").glob("[0-9][0-9][0-9][0-9]-*.md")),
            "ADR",
        )

    def check_reachability(self) -> None:
        start = self.docs / "README.md"
        if not start.is_file():
            self.error("缺少 docs/README.md")
            return
        distance = {start: 0}
        queue = deque([start])
        while queue:
            current = queue.popleft()
            if distance[current] >= 4:
                continue
            for target in self.graph.get(current, set()):
                if target not in distance:
                    distance[target] = distance[current] + 1
                    queue.append(target)
        live = {
            path
            for path in self.live_metadata_files()
            if "docs/archive/" not in relative(path, self.root)
        }
        for path in sorted(live - set(distance)):
            self.error(
                f"{relative(path, self.root)}: live 文档无法在 4 跳内从 docs/README.md 到达"
            )

    def route_sections(self, path: Path) -> list[tuple[str, list[tuple[int, str]]]]:
        """Return only explicitly named default-routing sections."""

        sections: list[tuple[str, list[tuple[int, str]]]] = []
        active_title: str | None = None
        active_level = 0
        active_lines: list[tuple[int, str]] = []
        for number, line in without_fenced_code(path.read_text(encoding="utf-8")):
            heading = HEADING_RE.match(line)
            if heading:
                level = len(heading.group(1))
                if active_title is not None and level <= active_level:
                    sections.append((active_title, active_lines))
                    active_title = None
                    active_lines = []
                if ROUTE_HEADING_RE.search(heading.group(2)):
                    active_title = heading.group(2)
                    active_level = level
                continue
            if active_title is not None:
                active_lines.append((number, line))
        if active_title is not None:
            sections.append((active_title, active_lines))
        return sections

    def route_entries(
        self, title: str, lines: list[tuple[int, str]]
    ) -> list[tuple[int, str]]:
        """Turn table rows and Markdown bullets into logical task routes."""

        entries = [
            (number, line)
            for number, line in lines
            if line.lstrip().startswith("|")
            and not re.match(r"^\s*\|(?:\s*:?-+:?\s*\|)+\s*$", line)
        ]
        bullets: list[tuple[int, str]] = []
        current: tuple[int, list[str]] | None = None
        for number, line in lines:
            stripped = line.lstrip()
            if stripped.startswith(("- ", "* ")):
                if current is not None:
                    bullets.append((current[0], " ".join(current[1])))
                current = (number, [stripped[2:]])
            elif current is not None and (line.startswith(("  ", "\t")) or not stripped):
                if stripped:
                    current[1].append(stripped)
            elif current is not None:
                bullets.append((current[0], " ".join(current[1])))
                current = None
        if current is not None:
            bullets.append((current[0], " ".join(current[1])))
        if "必读入口" in title and bullets:
            entries.append((bullets[0][0], " ".join(text for _, text in bullets)))
        else:
            entries.extend(bullets)
        return entries

    def route_document_targets(self, text: str) -> set[str]:
        """Extract unique Markdown document paths, including fragments and code paths."""

        targets: set[str] = set()
        for match in INLINE_LINK_RE.finditer(INLINE_CODE_RE.sub("", text)):
            parts = split_local_target(match.group("target"))
            if parts is not None and parts[0].lower().endswith(".md"):
                targets.add(parts[0])
        for raw in re.findall(r"`([^`]+\.md(?:#[^`]*)?)`", text, re.IGNORECASE):
            parts = split_local_target(raw)
            if parts is not None and parts[0].lower().endswith(".md"):
                targets.add(parts[0])
        return targets

    def check_default_routes(self) -> None:
        route_files = [
            self.root / "AGENTS.md",
            self.docs / "README.md",
            self.docs / "guide/README.md",
            self.docs / "architecture/README.md",
            self.docs / "development/README.md",
        ] + list((self.root / "crates").glob("*/AGENTS.md"))
        for path in route_files:
            if not path.is_file():
                continue
            for title, lines in self.route_sections(path):
                for number, entry in self.route_entries(title, lines):
                    targets = self.route_document_targets(entry)
                    if any(
                        re.search(r"(?:^|/)(?:archive|proposals)(?:/|$)", target)
                        for target in targets
                    ):
                        self.error(
                            f"{relative(path, self.root)}:{number}: "
                            "默认任务路由不得直接加载 archive/proposals"
                        )
                    if len(targets) > 5:
                        self.error(
                            f"{relative(path, self.root)}:{number}: "
                            f"单条任务路由包含 {len(targets)} 份文档，超过上限 5"
                        )
                    if (
                        path.parent.name in SCOPED_AGENT_DIRS
                        and VAGUE_ROUTE_RE.search(entry)
                    ):
                        self.error(
                            f"{relative(path, self.root)}:{number}: "
                            "scoped 路由使用未解析的相关/对应文档泛称"
                        )

    def check_scoped_agents(self) -> None:
        for directory in SCOPED_AGENT_DIRS:
            path = self.root / "crates" / directory / "AGENTS.md"
            if not path.is_file():
                self.error(f"缺少 scoped AGENTS: {relative(path, self.root)}")
                continue
            text = path.read_text(encoding="utf-8")
            if not re.search(r"^##\s+(?:按任务读取|必读入口)\s*$", text, re.MULTILINE):
                self.error(f"{relative(path, self.root)}: 缺少读取路由")
            if not any(
                resolve_link(path, link.raw_target, self.root) is not None
                for link in self.links.get(path, [])
            ):
                self.error(f"{relative(path, self.root)}: 读取路由没有本地文档入口")

    def check_stale_paths(self) -> None:
        old_guide = {
            "00-index.md",
            "01-design-decisions.md",
            "02-lexical-spec.md",
            "03-grammar-core.md",
            "04-grammar-declarations-blocks.md",
            "05-grammar-calls-lambda.md",
            "06-roadmap.md",
            "07-changelog-archive.md",
        }
        for name in old_guide:
            if (self.docs / "guide" / name).exists():
                self.error(f"旧 guide 路径仍存在: docs/guide/{name}")
        patterns = [
            re.compile(r"docs/guide/(?:" + "|".join(re.escape(name) for name in old_guide) + r")"),
            re.compile(r"docs/specs/\d{4}-[a-z0-9-]+\.md"),
            re.compile(r"docs/adr/\d{4}-[a-z0-9-]+\.md"),
            re.compile(r"docs/(?:agent-language-design-guide-v0\.[0-9]+|koven-language-tour)\.md"),
        ]
        for path in self.files:
            rel = relative(path, self.root)
            if rel.startswith("docs/archive/"):
                continue
            text = path.read_text(encoding="utf-8")
            for pattern in patterns:
                if pattern.search(text):
                    self.error(f"{rel}: 残留旧公共文档路径 {pattern.pattern}")

    def check_migration_ledger(self) -> None:
        snapshot = self.docs / "archive/guides/v0.34-pre-restructure"
        ledger = self.docs / "archive/migrations/v0.34-document-restructure.md"
        if not snapshot.is_dir() or not ledger.is_file():
            self.error("缺少 v0.34 guide 快照或迁移账本")
            return
        expected: set[str] = set()
        for path in sorted(snapshot.glob("[0-9][0-9]-*.md")):
            seen: dict[str, int] = {}
            for _, line in without_fenced_code(path.read_text(encoding="utf-8")):
                match = HEADING_RE.match(line)
                if not match or len(match.group(1)) not in {2, 3}:
                    continue
                anchor = unique_slug(github_slug(match.group(2)), seen)
                expected.add(f"docs/guide/{path.name}#{anchor}")
        ledger_text = ledger.read_text(encoding="utf-8")
        actual = set(
            re.findall(
                r"^\| `([^`]+)` \|",
                ledger_text,
                re.MULTILINE,
            )
        )
        for item in sorted(expected - actual):
            self.error(f"迁移账本遗漏旧标题: {item}")
        for item in sorted(actual - expected):
            self.error(f"迁移账本存在未知旧标题: {item}")
        for number, line in enumerate(ledger_text.splitlines(), 1):
            if not line.startswith("| `docs/guide/"):
                continue
            cells = line.split("|", 4)
            targets = re.findall(r"`(docs/[^`]+\.md)`", cells[3] if len(cells) > 3 else "")
            if not targets:
                self.error(f"{relative(ledger, self.root)}:{number}: 迁移项缺少新归属")
                continue
            for target in targets:
                if not (self.root / target).is_file():
                    self.error(
                        f"{relative(ledger, self.root)}:{number}: 新归属不存在: {target}"
                    )

    def check_line_budgets(self) -> None:
        limits: dict[Path, int] = {
            self.root / "AGENTS.md": 160,
            self.docs / "AGENTS.md": 120,
            self.docs / "guide/README.md": 160,
            self.docs / "architecture/README.md": 200,
        }
        for path in (self.root / "crates").glob("*/AGENTS.md"):
            limits[path] = 80
        for path in (self.docs / "guide").glob("[0-9][0-9]-*.md"):
            limits[path] = 800
        for path in (self.docs / "architecture").glob("*.md"):
            limits.setdefault(path, 200)
        for path, limit in limits.items():
            if not path.is_file():
                self.error(f"缺少受检入口: {relative(path, self.root)}")
                continue
            lines = len(path.read_text(encoding="utf-8").splitlines())
            if lines > limit:
                self.error(
                    f"{relative(path, self.root)}: {lines} 行，超过入口上限 {limit}"
                )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="repository root (defaults to the parent of scripts/)",
    )
    args = parser.parse_args(argv)
    checker = DocsChecker(args.root)
    errors = checker.run()
    if errors:
        print(f"documentation check failed with {len(errors)} error(s):", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(f"documentation check passed ({len(checker.files)} Markdown files)")
    print("note: structural checks do not prove semantic equivalence")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
