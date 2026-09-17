#!/usr/bin/env python3
"""Generate the Spec-level dependency graph (Mermaid markdown + SVG).

Reads active/draft Spec front matter ("前置 Spec" rows) under docs/specs,
collapses archived (done) Specs into one aggregate node, and renders:

- docs/specs/dependency-graph.md  (Mermaid source + node links)
- docs/specs/dependency-graph.svg (standalone vector image)

Both outputs are deterministic (sorted iteration only) so check_docs.py can
verify freshness by regenerating and comparing bytes. Topology only: the graph
never encodes per-Spec acceptance status; status lives in specs/README.md.
"""

from __future__ import annotations

import html
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


@dataclass
class SpecNode:
    """One drawable node: a live Spec or the collapsed archive aggregate."""

    number: str  # "0227" or "archive"
    title: str
    partition: str  # "active" / "drafts/<version>" / "archive"
    relpath: str  # markdown path relative to docs/specs ("" for archive)
    preds: frozenset[str]


def partition_rank(partition: str) -> int:
    """Fixed lane order: archive leftmost, then active, then drafts ascending."""
    if partition == "archive":
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


def load_graph(root: Path) -> tuple[list[SpecNode], dict[str, set[str]], int]:
    """Build visible nodes, edges (source -> target), and archive count."""
    specs = specs_root(root)
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
                relpath=str(path.relative_to(specs).as_posix()),
                preds=preds,
            )
        )
    nodes.sort(key=lambda node: (partition_rank(node.partition), node.number))

    by_number = {node.number: node for node in nodes}
    edges: dict[str, set[str]] = {}
    for node in nodes:
        for pred in sorted(node.preds):
            source = pred if pred in by_number else "archive"
            edges.setdefault(source, set()).add(node.number)

    archived = len(numbered_specs(root / "docs/archive/specs"))
    return nodes, edges, archived


def compute_levels(nodes: list[SpecNode], edges: dict[str, set[str]]) -> dict[str, int]:
    """Longest-path leveling over predecessors; the archive aggregate sits at 0."""
    preds_map: dict[str, set[str]] = {}
    for source, targets in edges.items():
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
        level = max((walk(pred) + 1 for pred in sorted(preds_map.get(number, ()))), default=0)
        visiting.discard(number)
        depth[number] = level
        return level

    for number in ["archive"] + [node.number for node in nodes]:
        walk(number)
    return depth


def partition_label(partition: str, archived: int) -> str:
    """Human-readable lane title."""
    if partition == "archive":
        return f"已完成（archive，{archived} 份）"
    if partition == "active":
        return "现行 active"
    return f"{partition.removeprefix('drafts/')}（draft，未启用）"


def wrap_title(title: str, width: int = 14, max_lines: int = 3) -> list[str]:
    """Wrap a CJK title into fixed-width lines with ellipsis on overflow."""
    lines = [title[i : i + width] for i in range(0, len(title), width)][:max_lines]
    if len(title) > max_lines * width:
        lines[-1] = lines[-1][:-1] + "…"
    return lines


def node_label(node: SpecNode, archived: int) -> list[str]:
    """First line is the Spec id; remaining lines are the wrapped title."""
    if node.number == "archive":
        return ["已完成 Spec", f"archive，{archived} 份"]
    return [f"SPEC-{node.number}"] + wrap_title(node.title.replace("`", ""))


def mermaid_id(number: str) -> str:
    """Mermaid-safe node identifier."""
    return "ARCH" if number == "archive" else f"S{number}"


def render_mermaid(nodes: list[SpecNode], edges: dict[str, set[str]], archived: int) -> str:
    """Render the graph as a Mermaid flowchart for GitHub rendering."""
    lines = ["flowchart TD"]
    lines.append(f'ARCH(("已完成<br/>archive {archived} 份"))')
    for partition in sorted({node.partition for node in nodes}, key=partition_rank):
        gid = "G" + partition.replace("/", "_").replace(".", "")
        lines.append(f'subgraph {gid}["{partition_label(partition, archived)}"]')
        for node in (item for item in nodes if item.partition == partition):
            escaped = html.escape(node.title, quote=False).replace("`", "")
            lines.append(
                f'  {mermaid_id(node.number)}["{mermaid_id(node.number)}<br/>{escaped}"]'
            )
        lines.append("end")
    for source in sorted(edges):
        for target in sorted(edges[source]):
            lines.append(f"{mermaid_id(source)} --> {mermaid_id(target)}")
    return "\n".join(lines)


def render_markdown(
    nodes: list[SpecNode], edges: dict[str, set[str]], archived: int
) -> str:
    """Render docs/specs/dependency-graph.md."""
    lines = [
        "# Spec 依赖图",
        "",
        "> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：查看 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文",
        "",
        "由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见 [README](README.md)。",
        "重建时机：guide 版本启用或新增/迁移 draft Spec。SVG 版本：[dependency-graph.svg](dependency-graph.svg)。",
        "",
        "```mermaid",
        render_mermaid(nodes, edges, archived),
        "```",
        "",
        "## 节点链接",
        "",
        "| 节点 | 分区 | 文档 |",
        "|---|---|---|",
    ]
    for node in nodes:
        lines.append(
            f"| SPEC-{node.number} | {node.partition} | [{node.relpath}]({node.relpath}) |"
        )
    lines.append(
        f"| 已完成 Spec（{archived} 份） | archive | [archive/specs/README.md](../archive/specs/README.md) |"
    )
    lines.append("")
    return "\n".join(lines)


def compute_layout(
    nodes: list[SpecNode], edges: dict[str, set[str]], archived: int
) -> tuple[dict[str, tuple[float, float, float, float]], dict[str, int], float, float]:
    """Compute lane-based positions: one lane per partition, rows by level.

    Returns (rects, depth, total_width, total_height); each rect is
    (x, y, width, height) keyed by Spec number, with the archive aggregate
    under key "archive".
    """
    depth = compute_levels(nodes, edges)
    archive = SpecNode("archive", "", "archive", "", frozenset())
    drawable = [("archive", archive)] + [(node.number, node) for node in nodes]

    labels = {number: node_label(node, archived) for number, node in drawable}
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

    # 泳道宽度：同级节点会横向错开，取最大槽位数决定泳道宽度。
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
        cursor += 2 * LANE_PAD_X + lane_slots[partition] * (NODE_W + SLOT_GAP_X) - SLOT_GAP_X + LANE_GAP

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


def render_svg(nodes: list[SpecNode], edges: dict[str, set[str]], archived: int) -> str:
    """Render the graph as a standalone deterministic SVG."""
    depth = compute_levels(nodes, edges)
    rects, _, width, height = compute_layout(nodes, edges, archived)
    partition_of = {node.number: node.partition for node in nodes}
    partition_of["archive"] = "archive"
    archive = SpecNode("archive", "", "archive", "", frozenset())
    labels = {
        number: node_label(node, archived)
        for number, node in [("archive", archive)] + [(n.number, n) for n in nodes]
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
    for partition, member_rects in sorted(lanes.items(), key=lambda item: partition_rank(item[0])):
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
            f'font-size="13" fill="{LANE_TEXT}">{html.escape(partition_label(partition, archived))}</text>'
        )

    # 依赖边：同泳道自上而下走中线，跨泳道从侧面进入目标。
    edge_paths: list[str] = []
    for source in sorted(edges):
        for target in sorted(edges[source]):
            sx, sy, sw, sh = rects[source]
            tx, ty, tw, _th = rects[target]
            if partition_of[source] == partition_of[target] and sy < ty:
                x1_, y1_ = sx + sw / 2, sy + sh
                x2_, y2_ = tx + tw / 2, ty
                mid1, mid2 = y1_ + LEVEL_GAP / 2, y2_ - LEVEL_GAP / 2
                path = f"M {x1_:.0f} {y1_:.0f} C {x1_:.0f} {mid1:.0f}, {x2_:.0f} {mid2:.0f}, {x2_:.0f} {y2_:.0f}"
            else:
                if sx < tx:
                    x1_, y1_ = sx + sw, sy + sh / 2
                    x2_, y2_ = tx, ty + _th / 2
                else:
                    x1_, y1_ = sx, sy + sh / 2
                    x2_, y2_ = tx + tw, ty + _th / 2
                mid = (x1_ + x2_) / 2
                path = f"M {x1_:.0f} {y1_:.0f} C {mid:.0f} {y1_:.0f}, {mid:.0f} {y2_:.0f}, {x2_:.0f} {y2_:.0f}"
            edge_paths.append(
                f'  <path d="{path}" fill="none" stroke="{INK}" stroke-width="1.5" marker-end="url(#arrow)"/>'
            )
    out.extend(edge_paths)

    # 节点：按编号排序保证确定性
    for number in sorted(rects):
        x, y, w, h = rects[number]
        is_archive = number == "archive"
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


def drift_errors(root: Path) -> list[str]:
    """Regenerate both artifacts and report byte drift against on-disk files."""
    nodes, edges, archived = load_graph(root)
    if not nodes:
        return []
    try:
        compute_levels(nodes, edges)
    except ValueError as exc:
        return [f"docs/specs 依赖图: {exc}"]
    specs = specs_root(root)
    expected = {
        specs / "dependency-graph.md": render_markdown(nodes, edges, archived),
        specs / "dependency-graph.svg": render_svg(nodes, edges, archived),
    }
    problems: list[str] = []
    for path, text in expected.items():
        name = path.relative_to(root).as_posix()
        if not path.is_file():
            problems.append(f"{name}: 缺少生成物，请重跑 scripts/gen_spec_dag.py")
        elif path.read_text(encoding="utf-8") != text:
            problems.append(f"{name}: 生成物过期，请重跑 scripts/gen_spec_dag.py")
    return problems


def main() -> int:
    """Write both generated artifacts; print a short summary."""
    try:
        nodes, edges, archived = load_graph(ROOT)
        compute_levels(nodes, edges)
    except ValueError as exc:
        print(f"error: {exc}")
        return 1
    specs = specs_root(ROOT)
    (specs / "dependency-graph.md").write_text(
        render_markdown(nodes, edges, archived), encoding="utf-8"
    )
    (specs / "dependency-graph.svg").write_text(
        render_svg(nodes, edges, archived), encoding="utf-8"
    )
    print(f"generated: {len(nodes)} visible specs + archive {archived} -> docs/specs/")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
