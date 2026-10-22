#!/usr/bin/env python3
"""Generate the v1 roadmap views from docs/roadmap/v1.json.

v1.json is the only hand-edited source for v1 scope, priority and status.
Three Markdown views are rendered from it:

- docs/roadmap/by-stage.md: grouped by implementation stage (S0, P0–P5, X)
- docs/roadmap/by-area.md: grouped by language/engineering area
- docs/roadmap/by-phase.md: item × compiler Phase matrix

Output is deterministic so check_docs.py can verify freshness by regenerating
and comparing bytes. Validation rejects unknown enums, duplicate ids, dangling
decision/item references, Spec numbers without a file and missing link targets.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATA = Path("docs/roadmap/v1.json")
VIEWS = {
    "stage": Path("docs/roadmap/by-stage.md"),
    "area": Path("docs/roadmap/by-area.md"),
    "phase": Path("docs/roadmap/by-phase.md"),
}

STATUS = {
    "done": ("✅", "已交付"),
    "partial": ("◐", "部分"),
    "active": ("▶", "进行中"),
    "todo": ("☐", "未开始"),
    "decision": ("❓", "待决策"),
    "deferred": ("⏸", "延后"),
}
STATUS_ORDER = list(STATUS)
PRIORITY = {
    "must": "v1 必做",
    "should": "v1 建议",
    "v1.x": "v1.x",
    "v2": "v2",
    "v3": "v3+",
}
IN_V1 = {"must", "should"}
GUIDE = {"enabled": "已定义", "needed": "需新增", "none": "—"}
PHASES = [
    (0, "骨架"),
    (1, "Parser"),
    (2, "名称/类型"),
    (3, "所有权"),
    (4, "SSA/native"),
    (5, "标准库"),
    (6, "工具"),
]
SPEC_DIRS = ("docs/archive/specs", "docs/specs/active", "docs/specs/drafts")
SPEC_RE = re.compile(r"^(\d{4})-[a-z0-9][a-z0-9-]*\.md$")
ITEM_KEYS = {
    "id", "stage", "area", "priority", "status", "guide", "phases", "title",
    "specs", "links", "decision", "note",
}

HEADER = """# {title}

> **性质**：生成视图 · **状态**：current（生成自 [v1.json](v1.json)） · **读取时机**：{when} · **唯一真源**：[v1.json](v1.json)；语义归 Guide，验收证据归各 Spec

<!-- 生成文件：请编辑 docs/roadmap/v1.json 后运行 python3 scripts/gen_roadmap.py，不要手改本页。 -->

基线：{baseline} · 更新：{updated} · 其他视图：{others} · 维护规则见 [README](README.md)

"""

VIEW_META = {
    "stage": ("Koven v1 Roadmap：按实施阶段", "查看 v1 已完成内容、当前位置与下一步时"),
    "area": ("Koven v1 Roadmap：按领域", "查看某一语言或工程领域的 v1 完成度时"),
    "phase": ("Koven v1 Roadmap：按编译 Phase", "查看某一编译阶段承担哪些 v1 工作时"),
}
VIEW_LABEL = {"stage": "按阶段", "area": "按领域", "phase": "按 Phase"}


class RoadmapError(ValueError):
    pass


def spec_index(root: Path) -> dict[str, Path]:
    index: dict[str, Path] = {}
    for directory in SPEC_DIRS:
        base = root / directory
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*.md")):
            match = SPEC_RE.match(path.name)
            if match:
                index.setdefault(match.group(1), path)
    return index


def load(root: Path) -> dict:
    path = root / DATA
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise RoadmapError(f"{DATA}: JSON 解析失败: {exc}") from exc
    validate(root, data)
    return data


def validate(root: Path, data: dict) -> None:
    problems: list[str] = []
    stages = [stage["id"] for stage in data.get("stages", [])]
    areas = [area["id"] for area in data.get("areas", [])]
    decisions = {decision["id"]: decision for decision in data.get("decisions", [])}
    items = data.get("items", [])
    ids = [item.get("id") for item in items]
    for label, values in (("stage", stages), ("area", areas), ("item", ids)):
        duplicates = sorted({value for value in values if values.count(value) > 1})
        if duplicates:
            problems.append(f"重复 {label} id: {', '.join(map(str, duplicates))}")
    if len(decisions) != len(data.get("decisions", [])):
        problems.append("重复 decision id")
    specs = spec_index(root)
    known = set(ids)
    for item in items:
        name = item.get("id", "?")
        extra = set(item) - ITEM_KEYS
        if extra:
            problems.append(f"{name}: 未知字段 {', '.join(sorted(extra))}")
        for key in ("id", "stage", "area", "priority", "status", "guide", "title"):
            if not item.get(key):
                problems.append(f"{name}: 缺少 {key}")
        checks = (
            ("stage", stages), ("area", areas), ("priority", PRIORITY),
            ("status", STATUS), ("guide", GUIDE),
        )
        for key, allowed in checks:
            if item.get(key) and item[key] not in allowed:
                problems.append(f"{name}: 未知 {key} {item[key]!r}")
        if item.get("priority") in IN_V1 and item.get("status") == "deferred":
            problems.append(f"{name}: v1 范围内条目不能标为 deferred")
        if item.get("stage") == "X" and item.get("priority") in IN_V1:
            problems.append(f"{name}: X 阶段条目的 priority 不能是 v1 范围")
        for phase in item.get("phases", []):
            if phase not in dict(PHASES):
                problems.append(f"{name}: 未知 Phase {phase!r}")
        for number in item.get("specs", []):
            if number not in specs:
                problems.append(f"{name}: 找不到 SPEC-{number} 文件")
        for link in item.get("links", []):
            if not (root / "docs" / link.split("#", 1)[0]).is_file():
                problems.append(f"{name}: 链接目标不存在 docs/{link}")
        decision = item.get("decision")
        if decision and decision not in decisions:
            problems.append(f"{name}: 未知决策 {decision}")
    for decision in decisions.values():
        if decision.get("status") not in ("open", "decided"):
            problems.append(f"{decision['id']}: 未知决策状态 {decision.get('status')!r}")
        for item_id in decision.get("items", []):
            if item_id not in known:
                problems.append(f"{decision['id']}: 引用不存在的条目 {item_id}")
        ref = decision.get("ref")
        if ref and not (root / "docs" / ref.split("#", 1)[0]).is_file():
            problems.append(f"{decision['id']}: 链接目标不存在 docs/{ref}")
    if problems:
        raise RoadmapError("; ".join(problems))


def cell(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ")


def doc_link(target: str) -> str:
    return f"../{target}"


def spec_links(root: Path, item: dict, specs: dict[str, Path]) -> str:
    parts = []
    for number in item.get("specs", []):
        relative = specs[number].relative_to(root / "docs").as_posix()
        parts.append(f"[{number}]({doc_link(relative)})")
    for link in item.get("links", []):
        label = Path(link.split("#", 1)[0]).stem
        parts.append(f"[{label}]({doc_link(link)})")
    return " ".join(parts) or "—"


def status_cell(item: dict) -> str:
    symbol, label = STATUS[item["status"]]
    return f"{symbol} {label}"


def title_cell(item: dict) -> str:
    text = cell(item["title"])
    if item.get("decision"):
        text += f"（{item['decision']}）"
    return text


def note_cell(item: dict) -> str:
    return cell(item.get("note", "")) or "—"


def phases_cell(item: dict) -> str:
    phases = item.get("phases", [])
    return ",".join(str(phase) for phase in phases) or "—"


def counts(items: list[dict]) -> list[int]:
    return [sum(1 for item in items if item["status"] == status) for status in STATUS_ORDER]


def header(view: str, data: dict) -> str:
    title, when = VIEW_META[view]
    others = " · ".join(
        f"[{VIEW_LABEL[name]}]({VIEWS[name].name})" for name in VIEWS if name != view
    )
    meta = data["meta"]
    return HEADER.format(
        title=title, when=when, baseline=meta["baseline"], updated=meta["updated"], others=others
    )


def summary_table(rows: list[tuple[str, list[dict]]], label: str) -> list[str]:
    statuses = [STATUS[status][0] + " " + STATUS[status][1] for status in STATUS_ORDER]
    lines = [
        f"| {label} | " + " | ".join(statuses) + " | 合计 |",
        "|---|" + "---:|" * (len(statuses) + 1),
    ]
    for name, items in rows:
        values = counts(items)
        lines.append(f"| {name} | " + " | ".join(str(v) for v in values) + f" | {len(items)} |")
    return lines


def metrics_section(data: dict) -> list[str]:
    metrics = data["meta"].get("metrics")
    if not metrics:
        return []
    lines = ["## 最近一次实测", "", f"口径：{metrics['measured']}", "", "| 指标 | 结果 |", "|---|---|"]
    lines += [f"| {cell(name)} | {cell(value)} |" for name, value in metrics["rows"]]
    if metrics.get("note"):
        lines += ["", metrics["note"]]
    return lines + [""]


def decisions_section(data: dict) -> list[str]:
    lines = [
        "## 决策",
        "",
        "| ID | 决策 | 推荐 | 状态 | 影响条目 |",
        "|---|---|---|---|---|",
    ]
    for decision in data["decisions"]:
        state = "✅ " + decision["outcome"] if decision["status"] == "decided" else "❓ 待决"
        title = cell(decision["title"])
        if decision.get("ref"):
            title += f"（[依据]({doc_link(decision['ref'])})）"
        lines.append(
            f"| {decision['id']} | {title} | {cell(decision['recommend'])} | {cell(state)} | "
            + ", ".join(decision.get("items", []))
            + " |"
        )
    return lines + [""]


def render_stage(root: Path, data: dict) -> str:
    specs = spec_index(root)
    items = data["items"]
    areas = {area["id"]: area["title"] for area in data["areas"]}
    lines = [header("stage", data).rstrip("\n"), ""]
    meta = data["meta"]
    lines += [f"**v1 的定义**：{meta['v1_definition']}", ""]
    lines += ["## 总览", ""]
    rows = []
    for stage in data["stages"]:
        stage_items = [item for item in items if item["stage"] == stage["id"]]
        rows.append((f"{stage['id']} {stage['title']}", stage_items))
    lines += summary_table(rows, "阶段")
    v1 = [item for item in items if item["priority"] in IN_V1]
    must = [item for item in items if item["priority"] == "must"]
    lines += [
        "",
        f"v1 范围内 {len(v1)} 项（必做 {len(must)}，其中已交付 "
        f"{sum(1 for item in must if item['status'] == 'done')}）；X 阶段只列边界，不计入 v1。",
        "",
    ]
    lines += metrics_section(data)
    lines += decisions_section(data)
    for stage in data["stages"]:
        stage_items = [item for item in items if item["stage"] == stage["id"]]
        lines += [f"## {stage['id']} {stage['title']}", "", stage["goal"], ""]
        if not stage_items:
            lines += ["（无条目）", ""]
            continue
        lines += [
            "| 状态 | ID | 内容 | 优先级 | 领域 | Guide | Phase | Spec / 依据 | 备注 |",
            "|---|---|---|---|---|---|---|---|---|",
        ]
        for item in stage_items:
            lines.append(
                f"| {status_cell(item)} | {item['id']} | {title_cell(item)} | "
                f"{PRIORITY[item['priority']]} | {areas[item['area']]} | {GUIDE[item['guide']]} | "
                f"{phases_cell(item)} | {spec_links(root, item, specs)} | {note_cell(item)} |"
            )
        lines.append("")
    return "\n".join(lines).rstrip("\n") + "\n"


def render_area(root: Path, data: dict) -> str:
    specs = spec_index(root)
    stage_order = {stage["id"]: index for index, stage in enumerate(data["stages"])}
    stage_title = {stage["id"]: stage["title"] for stage in data["stages"]}
    items = data["items"]
    lines = [header("area", data).rstrip("\n"), "", "## 总览", ""]
    rows = []
    for area in data["areas"]:
        area_items = [
            item for item in items if item["area"] == area["id"] and item["priority"] in IN_V1
        ]
        rows.append((area["title"], area_items))
    lines += summary_table(rows, "领域（仅 v1 范围）")
    lines.append("")
    for area in data["areas"]:
        area_items = sorted(
            (item for item in items if item["area"] == area["id"]),
            key=lambda item: (stage_order[item["stage"]], items.index(item)),
        )
        if not area_items:
            continue
        lines += [f"## {area['title']}", ""]
        lines += [
            "| 状态 | ID | 内容 | 阶段 | 优先级 | Guide | Phase | Spec / 依据 |",
            "|---|---|---|---|---|---|---|---|",
        ]
        for item in area_items:
            lines.append(
                f"| {status_cell(item)} | {item['id']} | {title_cell(item)} | "
                f"{item['stage']} {stage_title[item['stage']]} | {PRIORITY[item['priority']]} | "
                f"{GUIDE[item['guide']]} | {phases_cell(item)} | {spec_links(root, item, specs)} |"
            )
        lines.append("")
    return "\n".join(lines).rstrip("\n") + "\n"


def render_phase(root: Path, data: dict) -> str:
    items = [item for item in data["items"] if item["priority"] in IN_V1]
    lines = [header("phase", data).rstrip("\n"), ""]
    lines += [
        "只列 v1 范围内条目；● 表示该条目需要在对应 Phase 产出或验收。"
        "Phase 定义见 [一致性与实施边界](../guide/15-conformance-and-staging.md)。",
        "",
        "## 总览",
        "",
    ]
    rows = []
    for number, name in PHASES:
        phase_items = [item for item in items if number in item.get("phases", [])]
        rows.append((f"Phase {number} {name}", phase_items))
    lines += summary_table(rows, "Phase")
    lines += ["", "## 矩阵", ""]
    head = " | ".join(f"P{number}" for number, _ in PHASES)
    lines += [
        f"| 状态 | ID | 内容 | 阶段 | Guide | {head} |",
        "|---|---|---|---|---|" + "---|" * len(PHASES),
    ]
    for item in items:
        marks = " | ".join("●" if number in item.get("phases", []) else "" for number, _ in PHASES)
        lines.append(
            f"| {status_cell(item)} | {item['id']} | {title_cell(item)} | {item['stage']} | "
            f"{GUIDE[item['guide']]} | {marks} |"
        )
    return "\n".join(lines).rstrip("\n") + "\n"


RENDERERS = {"stage": render_stage, "area": render_area, "phase": render_phase}


def render_targets(root: Path) -> dict[Path, str]:
    data = load(root)
    return {root / VIEWS[name]: renderer(root, data) for name, renderer in RENDERERS.items()}


def drift_errors(root: Path) -> list[str]:
    """Report invalid data or stale views; silent when the roadmap is absent."""
    if not (root / DATA).is_file():
        return []
    try:
        expected = render_targets(root)
    except RoadmapError as exc:
        return [f"{DATA}: {exc}"]
    problems: list[str] = []
    for path, text in expected.items():
        name = path.relative_to(root).as_posix()
        if not path.is_file():
            problems.append(f"{name}: 缺少生成物，请重跑 scripts/gen_roadmap.py")
        elif path.read_text(encoding="utf-8") != text:
            problems.append(f"{name}: 生成物过期，请重跑 scripts/gen_roadmap.py")
    return problems


def main() -> int:
    try:
        targets = render_targets(ROOT)
    except RoadmapError as exc:
        print(f"error: {exc}")
        return 1
    for path, text in targets.items():
        path.write_text(text, encoding="utf-8")
        print(f"wrote {path.relative_to(ROOT).as_posix()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
