"""IR0 §3（之十三 §6）：多阶段路线图 JSON schema 与条目编号防冲突校验原型。

非规范、不被生产 scripts/gen_roadmap.py 引用。对齐 docs/proposals/ir0-ownership-graph-draft.md §3：
1. 保持 docs/roadmap/v1.json 原样不变，从 v1.json 读取已占用的裸编号集合；
2. 阶段一及之后的路线图 JSON（stage1.json、stage2.json...）所有步骤条目 ID 强制带路线前缀
   （如 RL-L0、RC-C0、RS-S2a、RU-U1、RX-X0a、RIR-IR1、RF-F3 等），拒绝任何与 v1 撞名的裸编号；
3. 强制声明 meta.entry_gates，在入口门槛未全部满足时禁止将阶段条目置为 active/in_progress/done；
4. 检验阶段内 depends_on 依赖图严格无环且无悬空引用。
"""
from __future__ import annotations

import json
from pathlib import Path
import re
from typing import Any, Dict, FrozenSet, List, Optional, Set, Tuple

ALLOWED_ROUTE_PREFIXES = (
    "RL-",
    "RC-",
    "RS-",
    "RU-",
    "RX-",
    "RIR-",
    "RF-",
    "RPL-",
    "RFD-",
    "RIO-",
    "RVE-",
    "RBS-",
    "RAT-",
    "RTX-",
    "RE-",
)

PREFIXED_ID_RE = re.compile(
    r"^(?:RL|RC|RS|RU|RX|RIR|RF|RPL|RFD|RIO|RVE|RBS|RAT|RTX|RE)-[A-Za-z0-9][A-Za-z0-9_-]*$"
)


def load_v1_item_ids(v1_json_path: Path) -> FrozenSet[str]:
    """只读提取 docs/roadmap/v1.json 中已占用的全部条目 ID。"""
    data = json.loads(v1_json_path.read_text(encoding="utf-8"))
    ids: Set[str] = set()
    for item in data.get("items", []):
        item_id = item.get("id")
        if isinstance(item_id, str) and item_id:
            ids.add(item_id)
    return frozenset(ids)


def validate_stage_roadmap(
    stage_data: Dict[str, Any],
    v1_ids: FrozenSet[str],
    satisfied_gates: Optional[FrozenSet[str]] = None,
) -> List[str]:
    """校验后续阶段路线图 JSON 的编号前缀、v1 防撞名、入口门槛与依赖无环约束。"""
    errors: List[str] = []
    satisfied = satisfied_gates if satisfied_gates is not None else frozenset()

    meta = stage_data.get("meta")
    if not isinstance(meta, dict):
        errors.append("SCHEMA_META: 缺少顶层 meta 对象")
        entry_gates: List[str] = []
    else:
        stage_id = meta.get("stage_id")
        if not isinstance(stage_id, str) or not stage_id:
            errors.append("SCHEMA_STAGE_ID: meta.stage_id 必须为非空字符串")
        raw_gates = meta.get("entry_gates")
        if not isinstance(raw_gates, list) or not raw_gates or not all(isinstance(g, str) and g for g in raw_gates):
            errors.append("SCHEMA_ENTRY_GATES: meta.entry_gates 必须显式列出至少一项前置门槛")
            entry_gates = []
        else:
            entry_gates = raw_gates

    missing_gates = [g for g in entry_gates if g not in satisfied]

    items = stage_data.get("items")
    if not isinstance(items, list):
        errors.append("SCHEMA_ITEMS: 缺少 items 列表")
        return errors

    seen_ids: Set[str] = set()
    deps_map: Dict[str, List[str]] = {}
    for idx, item in enumerate(items):
        if not isinstance(item, dict):
            errors.append(f"SCHEMA_ITEM: items[{idx}] 必须是对象")
            continue
        item_id = item.get("id", "")
        if not isinstance(item_id, str) or not item_id:
            errors.append(f"SCHEMA_ID: items[{idx}] 缺少有效 id")
            continue
        if item_id in seen_ids:
            errors.append(f"SCHEMA_DUP_ID: 重复条目编号 {item_id}")
        seen_ids.add(item_id)

        if item_id in v1_ids:
            errors.append(f"ID_COLLIDES_V1: 条目编号 {item_id} 与 v1.json 已有编号冲突")
        if not PREFIXED_ID_RE.match(item_id):
            errors.append(
                f"ID_MISSING_ROUTE_PREFIX: 条目编号 {item_id} 缺少规范路线前缀（允许前缀：{', '.join(ALLOWED_ROUTE_PREFIXES)}）"
            )

        status = item.get("status", "planned")
        if missing_gates and status in ("active", "in_progress", "done"):
            errors.append(
                f"ENTRY_GATE_BLOCKED: 条目 {item_id} 在前置门槛 {missing_gates} 未满足前不得设为 {status}"
            )

        raw_deps = item.get("depends_on", [])
        if not isinstance(raw_deps, list) or not all(isinstance(d, str) for d in raw_deps):
            errors.append(f"SCHEMA_DEPS: {item_id} 的 depends_on 必须是字符串列表")
            deps_map[item_id] = []
        else:
            deps_map[item_id] = raw_deps

    # 校验依赖存在性与无环
    known_ids = seen_ids | set(v1_ids)
    for item_id, deps in sorted(deps_map.items()):
        for dep in deps:
            if dep not in known_ids:
                errors.append(f"DEP_NOT_FOUND: {item_id} 依赖不存在的条目 {dep}")

    visiting: Set[str] = set()
    visited: Set[str] = set()

    def dfs(node: str) -> None:
        visiting.add(node)
        for dep in deps_map.get(node, ()):
            if dep not in deps_map:
                continue
            if dep in visiting:
                errors.append(f"DEP_CYCLE: 阶段内依赖存在环路 {node} -> {dep}")
            elif dep not in visited:
                dfs(dep)
        visiting.remove(node)
        visited.add(node)

    for node in sorted(deps_map):
        if node not in visited:
            dfs(node)

    return errors
