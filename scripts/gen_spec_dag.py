#!/usr/bin/env python3
"""Generate Spec-level dependency graphs (Mermaid markdown + SVG).

Two views are produced from the same parsed model:

- Current view (docs/specs/dependency-graph.md/.svg): active and draft Specs
  as real nodes; archived (done) Specs collapsed into one aggregate node.
- Full view (docs/archive/specs/dependency-graph-full.md/.svg): every numbered
  Spec including all archived ones, for history tracing.

Both outputs are deterministic (sorted iteration only) so check_docs.py can
verify freshness by regenerating and comparing bytes. Topology only: the
graphs never encode per-Spec acceptance status; status lives in specs/README.md.
"""

from __future__ import annotations

import html
import posixpath
import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

NUMBER_RE = re.compile(r"^(\d{4})-[a-z0-9][a-z0-9-]*\.md$")
TITLE_RE = re.compile(r"^#\s+SPEC-(\d{4})[：:]\s*(.+?)\s*$")
PREDECESSOR_RE = re.compile(r"SPEC-(\d{4}(?:[、/]\d{4})+|\d{4})")

# SVG 布局常量；节点统一宽度，高度随标题行数变化。
NODE_W = 240
LINE_H = 18
PAD_Y = 10
LANE_PAD_X = 20
SLOT_GAP_X = 24
LANE_GAP = 64
LEVEL_GAP = 64
TOP_MARGIN = 64
LEFT_MARGIN = 40

INK = "#475569"
NODE_FILL = "#ffffff"
NODE_STROKE = "#64748b"
ARCHIVE_FILL = "#e2e8f0"
ARCHIVE_STROKE = "#94a3b8"
LANE_FILL = "#f8fafc"
LANE_STROKE = "#e2e8f0"
LANE_TEXT = "#64748b"

ARCHIVE = "archive"


@dataclass
class SpecNode:
    """One drawable node: a live Spec or the collapsed archive aggregate."""

    number: str  # "0227" or "archive" (synthetic aggregate)
    title: str
    partition: str  # "active" / "drafts/<version>" / "archive"
    repo_path: str  # markdown path relative to repo root ("" for aggregate)
    preds: frozenset[str]


@dataclass
class GraphView:
    """One renderable snapshot of the Spec dependency graph."""

    nodes: list[SpecNode]
    edges: dict[str, set[str]]  # source number -> dependents
    archived: int  # archived Spec count for lane labels
    synthetic_archive: bool  # True when archive is one collapsed node
    full: bool


def partition_rank(partition: str) -> int:
    """Fixed lane order: archive leftmost, then active, then drafts ascending."""
    if partition == ARCHIVE:
        return 0
    if partition == "active":
        return 1
    return 2


def numbered_specs(directory: Path) -> list[Path]:
    """List numbered Spec markdown files (README excluded) in one directory."""
    if not directory.is_dir():
        return []
    return sorted(path for path in directory.glob("*.md") if NUMBER_RE.match(path.name))


def parse_spec_file(path: Path) -> tuple[str, str, frozenset[str]]:
    """Extract (number, title, predecessor numbers) from one Spec file."""
    number = NUMBER_RE.match(path.name).group(1)
    title = number
    preds: set[str] = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        heading = TITLE_RE.match(line)
        if heading is not None and heading.group(1) == number:
            title = heading.group(2)
        if line.startswith("| 前置 Spec |"):
            for match in PREDECESSOR_RE.finditer(line):
                preds.update(re.split(r"[、/]", match.group(1)))
    return number, title, frozenset(preds)


def specs_root(root: Path) -> Path:
    """docs/specs directory under a repo root."""
    return root / "docs/specs"


def collect_live_nodes(specs: Path) -> list[SpecNode]:
    """Collect active/draft Spec nodes with paths relative to the repo root."""
    nodes: list[SpecNode] = []
    for path in sorted(p for p in specs.rglob("*.md") if NUMBER_RE.match(p.name)):
        parts = path.relative_to(specs).parts
        # active/ 为一级平铺；drafts/<version>/ 为二级目录；其余位置跳过。
        if parts[0] == "active":
            partition = "active"
        elif parts[0] == "drafts" and len(parts) >= 3:
            partition = f"drafts/{parts[1]}"
        else:
            continue
        number, title, preds = parse_spec_file(path)
        nodes.append(
            SpecNode(
                number=number,
                title=title,
                partition=partition,
                repo_path=str(path.relative_to(specs.parent).as_posix()),
                preds=preds,
            )
        )
    return nodes


def collect_archive_nodes(archive: Path) -> list[SpecNode]:
    """Collect archived Spec nodes for the full view."""
    nodes: list[SpecNode] = []
    for path in numbered_specs(archive):
        number, title, preds = parse_spec_file(path)
        nodes.append(
            SpecNode(
                number=number,
                title=title,
                partition=ARCHIVE,
                # 统一为相对 docs/ 的路径，与 live 节点同一坐标系。
                repo_path=str(path.relative_to(archive.parent.parent).as_posix()),
                preds=preds,
            )
        )
    return nodes


def build_edges(nodes: list[SpecNode]) -> dict[str, set[str]]:
    """Edges from each Spec to the Specs that depend on it; unknown preds
    (not present as nodes) collapse into the synthetic "archive" source."""
    by_number = {node.number: node for node in nodes}
    edges: dict[str, set[str]] = {}
    for node in nodes:
        for pred in sorted(node.preds):
            source = pred if pred in by_number else ARCHIVE
            edges.setdefault(source, set()).add(node.number)
    return edges


def load_view(root: Path, full: bool) -> GraphView:
    """Build one renderable view: current (collapsed archive) or full."""
    specs = specs_root(root)
    archive_dir = root / "docs/archive/specs"
    nodes = collect_live_nodes(specs)
    archived_specs = numbered_specs(archive_dir)
    if full:
        nodes.extend(collect_archive_nodes(archive_dir))
        nodes.sort(key=lambda node: (partition_rank(node.partition), node.number))
        return GraphView(nodes, build_edges(nodes), len(archived_specs), False, True)
    nodes.sort(key=lambda node: (partition_rank(node.partition), node.number))
    return GraphView(nodes, build_edges(nodes), len(archived_specs), True, False)


def compute_levels(view: GraphView) -> dict[str, int]:
    """Longest-path leveling over predecessors; the archive aggregate sits at 0."""
    preds_map: dict[str, set[str]] = {}
    for source, targets in view.edges.items():
        for target in targets:
            preds_map.setdefault(target, set()).add(source)
    depth: dict[str, int] = {}
    visiting: set[str] = set()

    def walk(number: str) -> int:
        if number in depth:
            return depth[number]
        if number in visiting:
            raise ValueError(f"前置依赖成环: {number}")
        visiting.add(number)
        level = max(
            (walk(pred) + 1 for pred in sorted(preds_map.get(number, ()))),
            default=0,
        )
        visiting.discard(number)
        depth[number] = level
        return level

    for number in ([ARCHIVE] if view.synthetic_archive else []) + [
        node.number for node in view.nodes
    ]:
        walk(number)
    return depth


def partition_label(partition: str, view: GraphView) -> str:
    """Human-readable lane title."""
    if partition == ARCHIVE:
        if view.full:
            return f"已完成（archive，{view.archived} 份）"
        return f"已完成（archive，{view.archived} 份）"
    if partition == "active":
        return "现行 active"
    return f"{partition.removeprefix('drafts/')}（draft）"


def wrap_title(title: str, width: int = 14, max_lines: int = 3) -> list[str]:
    """Wrap a CJK title into fixed-width lines with ellipsis on overflow."""
    title = title.replace("`", "")
    lines = [title[i : i + width] for i in range(0, len(title), width)][:max_lines]
    if len(title) > max_lines * width:
        lines[-1] = lines[-1][:-1] + "…"
    return lines


def node_label(node: SpecNode, view: GraphView) -> list[str]:
    """First line is the Spec id; remaining lines are the wrapped title."""
    if node.number == ARCHIVE:
        return ["已完成 Spec", f"archive，{view.archived} 份"]
    return [f"SPEC-{node.number}"] + wrap_title(node.title)


def mermaid_id(number: str) -> str:
    """Mermaid-safe node identifier."""
    return "ARCH" if number == ARCHIVE else f"S{number}"


def markdown_link(node: SpecNode, base: str) -> str:
    """Markdown link target from the output document's directory (docs-relative)."""
    if node.partition == ARCHIVE and not node.repo_path:
        target = "archive/specs/README.md"
    else:
        target = node.repo_path
    return posixpath.relpath(target, base)


def render_mermaid(view: GraphView) -> str:
    """Render the graph as a Mermaid flowchart for GitHub rendering."""
    lines = ["flowchart TD"]
    if view.synthetic_archive:
        lines.append(f'ARCH(("已完成<br/>archive {view.archived} 份"))')
    for partition in sorted({node.partition for node in view.nodes}, key=partition_rank):
        gid = "G" + partition.replace("/", "_").replace(".", "")
        lines.append(f'subgraph {gid}["{partition_label(partition, view)}"]')
        for node in (item for item in view.nodes if item.partition == partition):
            escaped = html.escape(node.title, quote=False).replace("`", "")
            lines.append(
                f'  {mermaid_id(node.number)}["{mermaid_id(node.number)}<br/>{escaped}"]'
            )
        lines.append("end")
    for source in sorted(view.edges):
        for target in sorted(view.edges[source]):
            lines.append(f"{mermaid_id(source)} --> {mermaid_id(target)}")
    return "\n".join(lines)


def render_markdown(view: GraphView, base: str) -> str:
    """Render one dependency-graph markdown page under `base` (posix dir)."""
    scope = "全量" if view.full else "现行拓扑"
    header = (
        "# Spec 全量依赖图"
        if view.full
        else "# Spec 依赖图"
    )
    lines = [
        header,
        "",
        "> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：追溯 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文",
        "",
        "由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见"
        + ("[Specs 索引](../specs/README.md)。" if view.full else "[README](README.md)。"),
        "重建时机：guide 版本启用或新增/迁移 draft Spec。",
    ]
    if not view.full:
        lines.append("SVG 版本：[dependency-graph.svg](dependency-graph.svg)。")
    else:
        lines.append("SVG 版本：[dependency-graph-full.svg](dependency-graph-full.svg)。")
    lines += [
        "",
        "```mermaid",
        render_mermaid(view),
        "```",
        "",
        "## 节点链接",
        "",
        "| 节点 | 分区 | 文档 |",
        "|---|---|---|",
    ]
    for node in view.nodes:
        lines.append(
            f"| SPEC-{node.number} | {node.partition} "
            f"| [{posixpath.basename(node.repo_path)}]({markdown_link(node, base)}) |"
        )
    if view.synthetic_archive:
        lines.append(
            f"| 已完成 Spec（{view.archived} 份） | archive "
            f"| [archive/specs/README.md]({posixpath.relpath('archive/specs/README.md', base)}) |"
        )
    lines.append("")
    return "\n".join(lines)


def compute_layout(view: GraphView) -> tuple[dict[str, tuple[float, float, float, float]], dict[str, int], float, float]:
    """Compute lane-based positions: one lane per partition, rows by level.

    Returns (rects, depth, total_width, total_height); each rect is
    (x, y, width, height) keyed by Spec number; the synthetic archive
    aggregate (current view only) is keyed "archive".
    """
    depth = compute_levels(view)
    aggregate = SpecNode(ARCHIVE, "", ARCHIVE, "", frozenset())
    drawable = ([(ARCHIVE, aggregate)] if view.synthetic_archive else []) + [
        (node.number, node) for node in view.nodes
    ]

    labels = {number: node_label(node, view) for number, node in drawable}
    heights = {
        number: 2 * PAD_Y + len(lines) * LINE_H for number, lines in labels.items()
    }

    partitions = sorted({node.partition for _, node in drawable}, key=partition_rank)
    members_by_partition = {
        partition: sorted(
            (item for item in drawable if item[1].partition == partition),
            key=lambda item: (depth[item[0]], item[0]),
        )
        for partition in partitions
    }

    # 泳道宽度由同级最大槽位数决定（同级节点横向错开）。
    lane_slots = {
        partition: max(
            (
                sum(1 for number, _ in members if depth[number] == level)
                for level in sorted({depth[number] for number, _ in members})
            ),
            default=1,
        )
        for partition, members in members_by_partition.items()
    }
    lane_x: dict[str, float] = {}
    cursor = LEFT_MARGIN
    for partition in partitions:
        lane_x[partition] = cursor
        cursor += (
            2 * LANE_PAD_X
            + lane_slots[partition] * (NODE_W + SLOT_GAP_X)
            - SLOT_GAP_X
            + LANE_GAP
        )

    level_max_h: dict[int, int] = {}
    for number, _ in drawable:
        level = depth[number]
        level_max_h[level] = max(level_max_h.get(level, 0), heights[number])
    level_y: dict[int, float] = {}
    y_cursor = TOP_MARGIN
    for level in sorted(level_max_h):
        level_y[level] = y_cursor
        y_cursor += level_max_h[level] + LEVEL_GAP

    rects: dict[str, tuple[float, float, float, float]] = {}
    per_level: dict[str, dict[int, int]] = {}
    for partition in partitions:
        for number, _ in members_by_partition[partition]:
            slot = per_level.setdefault(partition, {}).get(depth[number], 0)
            per_level[partition][depth[number]] = slot + 1
            x = lane_x[partition] + LANE_PAD_X + slot * (NODE_W + SLOT_GAP_X)
            rects[number] = (x, level_y[depth[number]], NODE_W, heights[number])

    total_width = cursor
    total_height = y_cursor + 24
    return rects, depth, total_width, total_height


def render_svg(view: GraphView) -> str:
    """Render the graph as a standalone deterministic SVG."""
    rects, _, width, height = compute_layout(view)
    partition_of = {node.number: node.partition for node in view.nodes}
    if view.synthetic_archive:
        partition_of[ARCHIVE] = ARCHIVE
    aggregate = SpecNode(ARCHIVE, "", ARCHIVE, "", frozenset())
    labels = {
        number: node_label(node, view)
        for number, node in ([(ARCHIVE, aggregate)] if view.synthetic_archive else [])
        + [(node.number, node) for node in view.nodes]
    }

    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{int(width)}" height="{int(height)}" '
        f'viewBox="0 0 {int(width)} {int(height)}" '
        f'font-family="PingFang SC, Microsoft YaHei, sans-serif">',
        "  <defs>",
        '    <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" '
        'markerHeight="7" orient="auto-start-reverse">',
        f'      <path d="M0,0 L10,5 L0,10 z" fill="{INK}"/>',
        "    </marker>",
        "  </defs>",
        f'  <rect width="{int(width)}" height="{int(height)}" fill="#ffffff"/>',
    ]

    # 泳道背景与分区标题
    lanes: dict[str, list[tuple[float, float, float, float]]] = {}
    for number, rect in rects.items():
        lanes.setdefault(partition_of[number], []).append(rect)
    for partition, member_rects in sorted(
        lanes.items(), key=lambda item: partition_rank(item[0])
    ):
        x0 = min(r[0] for r in member_rects) - LANE_PAD_X
        x1 = max(r[0] + r[2] for r in member_rects) + LANE_PAD_X
        y0 = min(r[1] for r in member_rects) - 34
        y1 = max(r[1] + r[3] for r in member_rects) + 16
        out.append(
            f'  <rect x="{x0:.0f}" y="{y0:.0f}" width="{x1 - x0:.0f}" height="{y1 - y0:.0f}" '
            f'rx="12" fill="{LANE_FILL}" stroke="{LANE_STROKE}"/>'
        )
        out.append(
            f'  <text x="{(x0 + x1) / 2:.0f}" y="{y0 + 20:.0f}" text-anchor="middle" '
            f'font-size="13" fill="{LANE_TEXT}">{html.escape(partition_label(partition, view))}</text>'
        )

    # 依赖边：同泳道自上而下走中线，跨泳道从侧面进入目标。
    for source in sorted(view.edges):
        for target in sorted(view.edges[source]):
            sx, sy, sw, sh = rects[source]
            tx, ty, tw, th = rects[target]
            if partition_of[source] == partition_of[target] and sy < ty:
                x1_, y1_ = sx + sw / 2, sy + sh
                x2_, y2_ = tx + tw / 2, ty
                mid1, mid2 = y1_ + LEVEL_GAP / 2, y2_ - LEVEL_GAP / 2
                path = (
                    f"M {x1_:.0f} {y1_:.0f} C {x1_:.0f} {mid1:.0f}, "
                    f"{x2_:.0f} {mid2:.0f}, {x2_:.0f} {y2_:.0f}"
                )
            else:
                if sx < tx:
                    x1_, y1_ = sx + sw, sy + sh / 2
                    x2_, y2_ = tx, ty + th / 2
                else:
                    x1_, y1_ = sx, sy + sh / 2
                    x2_, y2_ = tx + tw, ty + th / 2
                mid = (x1_ + x2_) / 2
                path = (
                    f"M {x1_:.0f} {y1_:.0f} C {mid:.0f} {y1_:.0f}, "
                    f"{mid:.0f} {y2_:.0f}, {x2_:.0f} {y2_:.0f}"
                )
            out.append(
                f'  <path d="{path}" fill="none" stroke="{INK}" stroke-width="1.5" '
                f'marker-end="url(#arrow)"/>'
            )

    # 节点按编号排序保证确定性；archive 节点用灰色标识"已完成"。
    for number in sorted(rects):
        x, y, w, h = rects[number]
        is_archive = partition_of[number] == ARCHIVE
        fill = ARCHIVE_FILL if is_archive else NODE_FILL
        stroke = ARCHIVE_STROKE if is_archive else NODE_STROKE
        out.append(
            f'  <rect x="{x:.0f}" y="{y:.0f}" width="{w}" height="{h:.0f}" rx="10" '
            f'fill="{fill}" stroke="{stroke}" stroke-width="1.5"/>'
        )
        for index, line in enumerate(labels[number]):
            weight = ' font-weight="600"' if index == 0 and not is_archive else ""
            out.append(
                f'  <text x="{x + w / 2:.0f}" y="{y + PAD_Y + index * LINE_H + 13:.0f}" '
                f'text-anchor="middle" font-size="13"{weight}>{html.escape(line)}</text>'
            )

    out.append("</svg>")
    return "\n".join(out) + "\n"


def render_targets(root: Path) -> dict[Path, str]:
    """Build every generated artifact keyed by its output path."""
    current = load_view(root, full=False)
    full = load_view(root, full=True)
    specs = specs_root(root)
    archive_specs = root / "docs/archive/specs"
    return {
        specs / "dependency-graph.md": render_markdown(current, "specs"),
        specs / "dependency-graph.svg": render_svg(current),
        archive_specs / "dependency-graph-full.md": render_markdown(full, "archive/specs"),
        archive_specs / "dependency-graph-full.svg": render_svg(full),
    }


def drift_errors(root: Path) -> list[str]:
    """Regenerate all artifacts and report byte drift against on-disk files."""
    try:
        current = load_view(root, full=False)
        full = load_view(root, full=True)
        compute_levels(current)
        compute_levels(full)
    except ValueError as exc:
        return [f"docs/specs 依赖图: {exc}"]
    if not current.nodes and not current.archived:
        return []
    expected = render_targets(root)
    problems: list[str] = []
    for path, text in expected.items():
        name = path.relative_to(root).as_posix()
        if not path.is_file():
            problems.append(f"{name}: 缺少生成物，请重跑 scripts/gen_spec_dag.py")
        elif path.read_text(encoding="utf-8") != text:
            problems.append(f"{name}: 生成物过期，请重跑 scripts/gen_spec_dag.py")
    return problems


def main() -> int:
    """Write all generated artifacts; print a short summary."""
    try:
        current = load_view(ROOT, full=False)
        full = load_view(ROOT, full=True)
        compute_levels(current)
        compute_levels(full)
    except ValueError as exc:
        print(f"error: {exc}")
        return 1
    for path, text in render_targets(ROOT).items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    print(
        f"generated: current view ({len(current.nodes)} live + archive {current.archived}), "
        f"full view ({len(full.nodes)} specs incl. {full.archived} archived) "
        "-> docs/specs/ + docs/archive/specs/"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
