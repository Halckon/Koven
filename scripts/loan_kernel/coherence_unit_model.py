"""编译单元依赖（J45）、接口产物哈希（J46）、语言版本（J47）与一致性规则（J37）可执行原型。

非规范、不被编译器引用。对齐：
- docs/proposals/compilation-unit-dependency-rules.md（J45–J47、标准库三分、白盒/黑盒测试轨）
- docs/proposals/evolution-03-route-existential.md §6（J37 孤儿检查、一阶合一重叠检查、R0–R1 禁一揽子实现）
"""
from __future__ import annotations

from collections import deque
from dataclasses import dataclass, field
import hashlib
import json
from typing import Dict, FrozenSet, List, Optional, Set, Tuple, Union


# ---- 类型项与标准一阶合一（J37）

@dataclass(frozen=True)
class TyVar:
    name: str


@dataclass(frozen=True)
class TyCon:
    name: str
    unit: str
    args: Tuple["TyTerm", ...] = ()


TyTerm = Union[TyVar, TyCon]


def _rename_vars(term: TyTerm, suffix: str) -> TyTerm:
    if isinstance(term, TyVar):
        return TyVar(f"{term.name}#{suffix}")
    return TyCon(term.name, term.unit, tuple(_rename_vars(a, suffix) for a in term.args))


def _resolve(term: TyTerm, subst: Dict[str, TyTerm]) -> TyTerm:
    while isinstance(term, TyVar) and term.name in subst:
        term = subst[term.name]
    return term


def _occurs(var_name: str, term: TyTerm, subst: Dict[str, TyTerm]) -> bool:
    term = _resolve(term, subst)
    if isinstance(term, TyVar):
        return term.name == var_name
    return any(_occurs(var_name, a, subst) for a in term.args)


def unify(a: TyTerm, b: TyTerm) -> Optional[Dict[str, TyTerm]]:
    """标准一阶合一（含 occurs check），返回最一般合一替换或 None。"""
    subst: Dict[str, TyTerm] = {}
    work: List[Tuple[TyTerm, TyTerm]] = [(_rename_vars(a, "L"), _rename_vars(b, "R"))]
    while work:
        x, y = work.pop()
        x, y = _resolve(x, subst), _resolve(y, subst)
        if x == y:
            continue
        if isinstance(x, TyVar):
            if _occurs(x.name, y, subst):
                return None
            subst[x.name] = y
        elif isinstance(y, TyVar):
            if _occurs(y.name, x, subst):
                return None
            subst[y.name] = x
        else:
            if x.name != y.name or x.unit != y.unit or len(x.args) != len(y.args):
                return None
            for ax, ay in zip(x.args, y.args):
                work.append((ax, ay))
    return subst


# ---- J45 / J46 / J47 数据模型

@dataclass(frozen=True)
class SymbolDecl:
    name: str
    visibility: str  # public | internal | private
    kind: str  # type | fun | generic_fun | const
    signature: str
    body: str = ""
    callees: Tuple[str, ...] = ()


@dataclass(frozen=True)
class ImplDecl:
    unit: str
    interface_name: str
    interface_unit: str
    target: TyTerm
    at_type_decl: bool = False  # True 表示写在 ConcreteType 声明处


@dataclass(frozen=True)
class CompilationUnit:
    name: str
    language_version: str = "1.0"
    deps: Tuple[str, ...] = ()
    package_edges: Tuple[Tuple[str, str], ...] = ()  # 同一单元内 package 依赖边（允许有环）
    symbols: Tuple[SymbolDecl, ...] = ()
    impls: Tuple[ImplDecl, ...] = ()
    min_caller_language_version: Optional[str] = None
    schema_version: int = 1
    abi_version: int = 1
    identifiers: Tuple[str, ...] = ()  # 用于验证 J47 保留字按单元隔离


@dataclass(frozen=True)
class InterfaceArtifact:
    unit: str
    schema_version: int
    abi_version: int
    language_version: str
    min_caller_language_version: Optional[str]
    dep_hashes: Tuple[Tuple[str, str], ...]
    public_signatures: Tuple[Tuple[str, str, str], ...]
    exported_generic_bodies: Tuple[Tuple[str, str], ...]
    public_consts: Tuple[Tuple[str, str], ...]
    coherence_index: Tuple[Tuple[str, str, str], ...]
    content_hash: str


def _parse_ver(v: str) -> Tuple[int, ...]:
    return tuple(int(p) for p in v.split("."))


# ---- J45：两层依赖图与可见性校验

def find_shortest_unit_cycle(units: Dict[str, CompilationUnit]) -> Optional[List[str]]:
    """J45 外层无环检查：若存在跨编译单元环，返回最短环路 [u1, u2, ..., u1]；否则返回 None。"""
    shortest: Optional[List[str]] = None
    for start in sorted(units):
        queue: deque = deque([(start, [start])])
        visited: Set[str] = set()
        while queue:
            cur, path = queue.popleft()
            if shortest is not None and len(path) >= len(shortest):
                continue
            unit = units.get(cur)
            if unit is None:
                continue
            for nxt in unit.deps:
                if nxt == start:
                    cand = path + [start]
                    if shortest is None or len(cand) < len(shortest):
                        shortest = cand
                elif nxt not in visited and nxt in units:
                    visited.add(nxt)
                    queue.append((nxt, path + [nxt]))
    return shortest


def check_visibility(
    caller_unit: str,
    target_unit: CompilationUnit,
    symbol_name: str,
    is_whitebox_test: bool = False,
) -> Optional[str]:
    """J45 + §4.3：跨单元仅可见 public；同单元（含白盒单元测试）可见 internal；黑盒集成测试按独立单元处理。"""
    sym = next((s for s in target_unit.symbols if s.name == symbol_name), None)
    if sym is None:
        return f"VIS_NOT_FOUND: {target_unit.name}.{symbol_name} 不存在"
    same_unit = (caller_unit == target_unit.name) or is_whitebox_test
    if sym.visibility == "public":
        return None
    if sym.visibility == "internal" and same_unit:
        return None
    return f"VIS_DENIED: {caller_unit} 不可访问 {target_unit.name}.{symbol_name}（visibility={sym.visibility}）"


# ---- J37：一致性检查（孤儿 + R0–R1 禁一揽子 + 一阶合一重叠）

def check_coherence(units: Dict[str, CompilationUnit]) -> List[str]:
    """J37 一致性规则检查（之三 §6 + 之十一 §3.3）。"""
    errors: List[str] = []
    all_impls: List[ImplDecl] = []
    for uname in sorted(units):
        all_impls.extend(units[uname].impls)

    for impl in all_impls:
        if isinstance(impl.target, TyVar):
            errors.append(
                f"J37_BLANKET_FORBIDDEN: {impl.unit} 禁止在 R0–R1 为 {impl.interface_name} 声明一揽子实现"
            )
            continue
        # 孤儿检查：类型实参不参与绕过孤儿规则；只允许写在目标类型声明处，或接口所在单元内
        is_at_type_decl = impl.at_type_decl and impl.unit == impl.target.unit
        is_in_iface_unit = impl.unit == impl.interface_unit
        if not (is_at_type_decl or is_in_iface_unit):
            errors.append(
                f"J37_ORPHAN: {impl.unit} 不能为 ({impl.target.name}, {impl.interface_name}) 声明孤儿或零散外部实现"
            )

    # 一阶合一重叠检查（按同一 interface 分组）
    by_iface: Dict[Tuple[str, str], List[ImplDecl]] = {}
    for impl in all_impls:
        if isinstance(impl.target, TyCon):
            by_iface.setdefault((impl.interface_unit, impl.interface_name), []).append(impl)

    for (_, iface_name), group in sorted(by_iface.items()):
        for i in range(len(group)):
            for j in range(i + 1, len(group)):
                a, b = group[i], group[j]
                if unify(a.target, b.target) is not None:
                    errors.append(
                        f"J37_OVERLAP: 接口 {iface_name} 存在重叠实现 {a.unit}:{a.target} 与 {b.unit}:{b.target}"
                    )
    return errors


# ---- J46：接口产物构建与内容哈希

def build_interface_artifact(
    unit: CompilationUnit,
    dep_artifacts: Dict[str, InterfaceArtifact],
    expected_schema: int = 1,
    expected_abi: int = 1,
) -> Tuple[Optional[InterfaceArtifact], List[str]]:
    """构造 J46 接口产物：含传递依赖的私有泛型体，排除无关非泛型私有实现体；schema/ABI 不兼容快速失败。"""
    errors: List[str] = []
    if unit.schema_version != expected_schema or unit.abi_version != expected_abi:
        return None, [
            f"J46_SCHEMA_MISMATCH: {unit.name} schema={unit.schema_version}/abi={unit.abi_version} "
            f"不匹配编译器 {expected_schema}/{expected_abi}"
        ]
    dep_hashes: List[Tuple[str, str]] = []
    for dep in sorted(unit.deps):
        art = dep_artifacts.get(dep)
        if art is None:
            errors.append(f"J46_MISSING_DEP: {unit.name} 缺少依赖接口产物 {dep}")
            continue
        if art.schema_version != expected_schema or art.abi_version != expected_abi:
            errors.append(f"J46_SCHEMA_MISMATCH: 依赖 {dep} 的接口产物版本不兼容")
            continue
        if art.min_caller_language_version is not None:
            if _parse_ver(unit.language_version) < _parse_ver(art.min_caller_language_version):
                errors.append(
                    f"J47_MIN_CALLER_VERSION: {unit.name}({unit.language_version}) 低于 "
                    f"{dep} 要求的 min_caller_language_version={art.min_caller_language_version}"
                )
        dep_hashes.append((dep, art.content_hash))
    if errors:
        return None, errors

    by_name = {s.name: s for s in unit.symbols}
    public_sigs = tuple(
        sorted((s.name, s.kind, s.signature) for s in unit.symbols if s.visibility == "public")
    )
    public_consts = tuple(
        sorted((s.name, s.body) for s in unit.symbols if s.visibility == "public" and s.kind == "const")
    )

    # 收集公开泛型体及其传递调用到的 internal/private 泛型体（排除非泛型私有辅助函数体）
    generic_queue: deque = deque(
        s.name for s in unit.symbols if s.visibility == "public" and s.kind == "generic_fun"
    )
    exported_generics: Dict[str, str] = {}
    while generic_queue:
        name = generic_queue.popleft()
        if name in exported_generics:
            continue
        sym = by_name.get(name)
        if sym is None or sym.kind != "generic_fun":
            continue
        exported_generics[name] = sym.body
        for callee in sym.callees:
            if callee not in exported_generics:
                generic_queue.append(callee)

    coherence_index = tuple(
        sorted(
            (impl.interface_unit, impl.interface_name, repr(impl.target))
            for impl in unit.impls
        )
    )
    payload = {
        "unit": unit.name,
        "schema": unit.schema_version,
        "abi": unit.abi_version,
        "lang": unit.language_version,
        "min_caller": unit.min_caller_language_version,
        "deps": dep_hashes,
        "public_sigs": public_sigs,
        "generics": sorted(exported_generics.items()),
        "consts": public_consts,
        "coherence": coherence_index,
    }
    digest = hashlib.sha256(json.dumps(payload, sort_keys=True).encode("utf-8")).hexdigest()
    artifact = InterfaceArtifact(
        unit=unit.name,
        schema_version=unit.schema_version,
        abi_version=unit.abi_version,
        language_version=unit.language_version,
        min_caller_language_version=unit.min_caller_language_version,
        dep_hashes=tuple(dep_hashes),
        public_signatures=public_sigs,
        exported_generic_bodies=tuple(sorted(exported_generics.items())),
        public_consts=public_consts,
        coherence_index=coherence_index,
        content_hash=digest,
    )
    return artifact, []


# ---- J47：保留字按编译单元语言版本隔离

KEYWORDS_BY_VERSION: Dict[str, FrozenSet[str]] = {
    "1.0": frozenset({"fun", "class", "val", "var", "borrow", "inout", "own"}),
    "2.0": frozenset({"fun", "class", "val", "var", "borrow", "inout", "own", "view", "flows"}),
}


def check_unit_keywords(unit: CompilationUnit) -> List[str]:
    """J47：新版本保留字仅在声明 >= 该版本的单元内生效，旧版本单元可继续用作普通标识符。"""
    reserved = KEYWORDS_BY_VERSION.get(unit.language_version)
    if reserved is None:
        return [f"J47_UNKNOWN_VERSION: {unit.name} 声明了未知语言版本 {unit.language_version}"]
    return [
        f"J47_RESERVED_KEYWORD: {unit.name}({unit.language_version}) 不能将保留字 {ident!r} 用作标识符"
        for ident in unit.identifiers
        if ident in reserved
    ]
