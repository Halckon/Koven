"""v1 冻结前兼容性约束（Z4、Z5、Z6、Z8、Z9、Z11）核对与审计原型。

非规范、不修改 conformance/expectations.json 或编译器实现。
对齐 docs/proposals/v1-freeze-decisions.md（D16）：
- Z9：只读扫描真实 conformance/expectations.json，精准统计并列出：
  1. 写入矩阵内部的 kind == "known_unimplemented" 条目（应移出为矩阵外登记）；
  2. 引用编译器内部错误类别名（BE:UnsupportedNode / BE:MissingFact / BE:InvalidSsa / BE:UnsupportedSource 等）的条目；
  3. 携带内部枚举符号名（diagnostic_symbol）的条目。
- Z4：v1 Hashable 仅支持结构推导与泛型上界，禁止公开 hashCode() 成员或用户手动实现；
- Z5：显式流 close() 消费 own this（禁止二次关闭），未显式关闭时 deinit 尽力关闭并丢弃错误；
- Z6：main(): Result<Unit, E> 仅要求 E: Display；
- Z8：project 清单 language_version 字段（缺失按 v1 提示、合法 v1 通过、未知报错并列出支持版本）；
- Z11：Directory.list 遇不可表示的非 UTF-8 文件名时整个调用失败，不得跳过或有损转换。
"""
from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
from typing import Any, Dict, FrozenSet, List, Optional, Tuple

INTERNAL_MARKER_PREFIXES = ("BE:",)
INTERNAL_ERROR_TOKENS = (
    "UnsupportedNode",
    "MissingFact",
    "InvalidSsa",
    "UnsupportedSource",
    "InvalidModel",
)


@dataclass(frozen=True)
class Z9AuditReport:
    total_entries: int
    known_unimplemented_ids: Tuple[str, ...]
    internal_marker_ids: Tuple[str, ...]
    diagnostic_symbol_ids: Tuple[str, ...]

    @property
    def is_z9_clean(self) -> bool:
        return not (self.known_unimplemented_ids or self.internal_marker_ids or self.diagnostic_symbol_ids)


def _has_internal_marker(value: Any) -> bool:
    if isinstance(value, str):
        return value.startswith(INTERNAL_MARKER_PREFIXES) or any(tok in value for tok in INTERNAL_ERROR_TOKENS)
    if isinstance(value, list):
        return any(_has_internal_marker(item) for item in value)
    return False


def audit_conformance_expectations_data(data: Dict[str, Any]) -> Z9AuditReport:
    """按 Z9 审计一致性矩阵 JSON 数据，列出引用内部产物或内嵌 known_unimplemented 的条目。"""
    total = 0
    known_unimpl: List[str] = []
    internal_markers: List[str] = []
    diag_symbols: List[str] = []

    for section in ("probes", "programs"):
        entries = data.get(section, {})
        if not isinstance(entries, dict):
            continue
        for entry_id, entry in sorted(entries.items()):
            if not isinstance(entry, dict):
                continue
            total += 1
            qualified = f"{section}/{entry_id}"
            if entry.get("kind") == "known_unimplemented":
                known_unimpl.append(qualified)
            if _has_internal_marker(entry.get("baseline_marker")) or _has_internal_marker(
                entry.get("allowed_markers")
            ):
                internal_markers.append(qualified)
            if "diagnostic_symbol" in entry or "all_diagnostic_symbols" in entry:
                diag_symbols.append(qualified)

    return Z9AuditReport(
        total_entries=total,
        known_unimplemented_ids=tuple(known_unimpl),
        internal_marker_ids=tuple(internal_markers),
        diagnostic_symbol_ids=tuple(diag_symbols),
    )


def audit_conformance_expectations(path: Path) -> Z9AuditReport:
    data = json.loads(path.read_text(encoding="utf-8"))
    return audit_conformance_expectations_data(data)


# ---- Z4：Hashable 表面约束（v1 仅推导与泛型上界）

def check_z4_hashable_usage(usage_kind: str) -> Optional[str]:
    if usage_kind in ("structural_derive", "generic_bound"):
        return None
    if usage_kind in ("manual_impl", "call_hash_code_member"):
        return f"Z4_HASHABLE_OPAQUE: v1 禁止 {usage_kind}（Hashable 不向用户暴露方法形状）"
    raise ValueError(f"未知 Hashable 用法: {usage_kind}")


# ---- Z5：显式流关闭（消费 own this vs deinit 尽力关闭）

@dataclass
class StreamHandle:
    name: str
    open: bool = True
    close_attempts: int = 0
    last_close_error_swallowed: bool = False

    def close(self, fail_io: bool = False) -> Tuple[str, Optional[str]]:
        if not self.open:
            return "ERR", f"Z5_DOUBLE_CLOSE: 流 {self.name} 已被消费关闭，编译期禁止二次关闭"
        self.open = False
        self.close_attempts += 1
        if fail_io:
            return "Err(IoError)", None
        return "Ok(Unit)", None

    def deinit(self, fail_io: bool = False) -> None:
        if not self.open:
            return
        self.open = False
        self.close_attempts += 1
        if fail_io:
            self.last_close_error_swallowed = True


# ---- Z6：main(): Result<Unit, E> 仅要求 E: Display

def check_z6_main_error_bounds(bounds: FrozenSet[str]) -> Optional[str]:
    if "Display" not in bounds:
        return "Z6_MAIN_ERROR_DISPLAY: main 返回 Result<Unit, E> 时 E 必须满足 Display"
    extra = sorted(bounds - {"Display"})
    if extra:
        return f"Z6_MAIN_ERROR_OVERCONSTRAINED: v1 不得要求 E 满足除 Display 外的额外接口 {extra}"
    return None


# ---- Z8：project 清单 language_version 字段校验

SUPPORTED_V1_VERSIONS = ("1.0",)


def check_z8_manifest_language_version(
    manifest: Dict[str, Any],
    supported: Tuple[str, ...] = SUPPORTED_V1_VERSIONS,
) -> Tuple[Optional[str], Optional[str], Optional[str]]:
    """返回 (resolved_version, warning, error)。"""
    if "language_version" not in manifest:
        return supported[0], f"Z8_MISSING_VERSION_WARN: 清单缺少 language_version，按默认 {supported[0]} 处理", None
    val = manifest["language_version"]
    if val in supported:
        return val, None, None
    return None, None, f"Z8_UNKNOWN_VERSION: 不支持的 language_version={val!r}，当前支持: {list(supported)}"


# ---- Z11：Directory.list 遇不可表示非 UTF-8 文件名时整个调用失败

def check_z11_directory_list(raw_filenames: Tuple[bytes, ...]) -> Tuple[Optional[List[str]], Optional[str]]:
    decoded: List[str] = []
    for raw in raw_filenames:
        try:
            decoded.append(raw.decode("utf-8", errors="strict"))
        except UnicodeDecodeError:
            return None, "Z11_NON_UTF8_FILENAME: 遇到不可表示的 UTF-8 文件名，整个 Directory.list 调用失败"
    return decoded, None
