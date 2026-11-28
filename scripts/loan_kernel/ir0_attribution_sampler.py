"""P0-2：IR0 §1 所有权与降低修复历史归因统计（C1–C4）与止损门槛评估原型。

非规范、不被编译器引用。对齐 docs/proposals/ir0-ownership-graph-draft.md §1 与演进总纲之八 §2.5：
- 仅计入触及 lang-frontend/ownership_checking 或 lang-codegen/ssa 的 fix 提交；
- 主因互斥分类为 C1（两处规则不一致）、C2（事实协议缺失/过宽/InvalidModel/MissingFact）、
  C3（新语法/控制流构造两处重复分派/UnsupportedNode）、C4（单层局部算法/Span/ABI 缺陷）；
- 计算 (C1 + C2 + C3) / total 占比并按止损门槛决定：建议启动 IR1+新 ADR，还是降级为仅参考检查器使用。
"""
from __future__ import annotations

from dataclasses import dataclass
import re
from typing import Dict, List, Optional, Tuple

OWNERSHIP_PATH_MARKERS = (
    "lang-frontend/src/ownership_checking",
    "lang-frontend/ownership_checking",
    "lang-codegen/src/ssa",
    "lang-codegen/ssa",
)

FIX_SUBJECT_RE = re.compile(r"^\s*fix(?:\([^)]*\))?!?:\s*", re.IGNORECASE)


@dataclass(frozen=True)
class CommitRecord:
    sha: str
    subject: str
    files: Tuple[str, ...]


@dataclass(frozen=True)
class AttributionEntry:
    sha: str
    category: str  # C1 | C2 | C3 | C4
    subject: str
    reason: str


@dataclass(frozen=True)
class AttributionSummary:
    total_fix_commits: int
    counts: Dict[str, int]
    entries: Tuple[AttributionEntry, ...]
    graph_solvable_ratio: float
    threshold: float
    recommendation: str  # PROCEED_IR1_ADR | DOWNGRADE_REFERENCE_ONLY


def is_in_ir0_scope(record: CommitRecord) -> bool:
    """只计入触及 ownership_checking 或 codegen/ssa 的 fix 提交。"""
    if not FIX_SUBJECT_RE.match(record.subject):
        return False
    return any(any(marker in path for marker in OWNERSHIP_PATH_MARKERS) for path in record.files)


def classify_commit(record: CommitRecord) -> AttributionEntry:
    """按 IR0 §1.1 互斥主因规则对单条修复提交归因（C2 -> C3 -> C1 -> C4）。"""
    subj_low = record.subject.lower()
    touches_fe = any("ownership_checking" in p for p in record.files)
    touches_be = any("/ssa" in p or "lang-codegen" in p for p in record.files)

    # C2：事实协议缺失、过宽或 InvalidModel / MissingFact / fact handoff
    if any(k in subj_low for k in ("missingfact", "missing fact", "invalidmodel", "invalid model", "fact protocol", "handoff fact", "ownership fact")):
        return AttributionEntry(record.sha, "C2", record.subject, "事实协议缺失或模型失配（C2）")

    # C3：新语法/控制流构造未覆盖（UnsupportedNode / AST+lowering 双处分派）
    if any(k in subj_low for k in ("unsupportednode", "unsupported node", "unsupportedsource", "unsupported source", "missing dispatch", "lower ast")):
        return AttributionEntry(record.sha, "C3", record.subject, "新语法或控制流构造缺少分派覆盖（C3）")

    # C1：同一规则两处实现不一致（同时触及 frontend 所有权与 codegen SSA，或明确提及一致性/分歧）
    if (touches_fe and touches_be) or any(k in subj_low for k in ("mismatch", "divergence", "inconsistent", "align ownership", "sync drop")):
        return AttributionEntry(record.sha, "C1", record.subject, "frontend 与 codegen 同一规则实现分歧（C1）")

    # C4：单层局部算法、诊断文案、Span 或 ABI 细节
    return AttributionEntry(record.sha, "C4", record.subject, "单层局部算法或边界缺陷（C4）")


def parse_git_log_text(text: str) -> List[CommitRecord]:
    """解析 `git log --format='COMMIT %H %s' --name-only` 格式输出。"""
    records: List[CommitRecord] = []
    cur_sha: Optional[str] = None
    cur_subject = ""
    cur_files: List[str] = []

    def flush() -> None:
        if cur_sha is not None:
            records.append(CommitRecord(cur_sha, cur_subject, tuple(cur_files)))

    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line:
            continue
        if line.startswith("COMMIT "):
            flush()
            parts = line.split(" ", 2)
            cur_sha = parts[1] if len(parts) > 1 else ""
            cur_subject = parts[2] if len(parts) > 2 else ""
            cur_files = []
        else:
            cur_files.append(line)
    flush()
    return records


def summarize_attribution(
    records: List[CommitRecord],
    stop_loss_threshold: float = 0.5,
) -> AttributionSummary:
    """按 IR0 §1.2 汇总 C1–C4 占比并应用降级止损门槛。"""
    counts = {"C1": 0, "C2": 0, "C3": 0, "C4": 0}
    entries: List[AttributionEntry] = []
    for rec in records:
        if not is_in_ir0_scope(rec):
            continue
        entry = classify_commit(rec)
        counts[entry.category] += 1
        entries.append(entry)
    total = len(entries)
    solvable = counts["C1"] + counts["C2"] + counts["C3"]
    ratio = (solvable / total) if total > 0 else 0.0
    rec_action = "PROCEED_IR1_ADR" if (total > 0 and ratio >= stop_loss_threshold) else "DOWNGRADE_REFERENCE_ONLY"
    return AttributionSummary(
        total_fix_commits=total,
        counts=counts,
        entries=tuple(entries),
        graph_solvable_ratio=ratio,
        threshold=stop_loss_threshold,
        recommendation=rec_action,
    )
