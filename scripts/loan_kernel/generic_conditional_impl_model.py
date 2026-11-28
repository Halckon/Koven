"""P1-1：J38 条件实现（where 多约束 + Self）、L5 泛型放开与 J37 重叠检查联动原型。

非规范、不被编译器引用。对齐 docs/proposals/evolution-03-route-existential.md §6（J37–J38）与
docs/proposals/evolution-13-implementation-roadmap.md §4.3–§4.4（阶段二条件实现、阶段三 L5/U2）：
- 支持带 where 多约束的条件接口/能力实现（如 List<T> : Transferable where T : Transferable）；
- 递归求解目标类型是否满足能力/接口约束（带访问栈防环）；
- 联动 coherence_unit_model.unify 做条件实现重叠检查（两个头部可合一的条件实现在 J37 下仍判重叠）；
- 建模 L5 泛型放开：未标 view 的类型参数默认要求 Escapable，标 view T 后允许实例化为携带来源的视图类型。
"""
from __future__ import annotations

from dataclasses import dataclass
import importlib.util
from pathlib import Path
import sys
from typing import Dict, FrozenSet, List, Optional, Set, Tuple

ROOT = Path(__file__).resolve().parent


def _load_coherence():
    name = "loan_kernel_coherence_unit_model"
    if name in sys.modules:
        return sys.modules[name]
    spec = importlib.util.spec_from_file_location(name, ROOT / "coherence_unit_model.py")
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod


COH = _load_coherence()
TyVar = COH.TyVar
TyCon = COH.TyCon
TyTerm = COH.TyTerm


@dataclass(frozen=True)
class ConditionalImpl:
    unit: str
    interface: str
    interface_unit: str
    target: TyTerm
    where_bounds: Tuple[Tuple[str, str], ...] = ()  # ((type_var_name, required_bound),)
    at_type_decl: bool = False


def _match_pattern(pattern: TyTerm, concrete: TyTerm, env: Dict[str, TyTerm]) -> bool:
    if isinstance(pattern, TyVar):
        bound = env.get(pattern.name)
        if bound is None:
            env[pattern.name] = concrete
            return True
        return bound == concrete
    if not isinstance(concrete, TyCon):
        return False
    if pattern.name != concrete.name or pattern.unit != concrete.unit or len(pattern.args) != len(concrete.args):
        return False
    return all(_match_pattern(pa, ca, env) for pa, ca in zip(pattern.args, concrete.args))


def satisfies_bound(
    ty: TyTerm,
    bound: str,
    impls: Tuple[ConditionalImpl, ...],
    _visiting: Optional[FrozenSet[Tuple[TyTerm, str]]] = None,
    max_depth: int = 16,
) -> bool:
    """递归求解具体类型 ty 是否经条件实现满足接口或能力 bound。"""
    visiting = _visiting or frozenset()
    goal = (ty, bound)
    if goal in visiting or len(visiting) >= max_depth:
        return False
    next_visiting = visiting | {goal}
    for impl in impls:
        if impl.interface != bound:
            continue
        env: Dict[str, TyTerm] = {}
        if not _match_pattern(impl.target, ty, env):
            continue
        if all(
            var_name in env and satisfies_bound(env[var_name], req_bound, impls, next_visiting, max_depth)
            for var_name, req_bound in impl.where_bounds
        ):
            return True
    return False


def check_where_clause(
    type_args: Dict[str, TyTerm],
    where_bounds: Tuple[Tuple[str, str], ...],
    impls: Tuple[ConditionalImpl, ...],
) -> List[str]:
    """校验调用点/实例化点的全部 where 多约束，返回每条未满足的上界。"""
    errors: List[str] = []
    for var_name, req_bound in where_bounds:
        actual = type_args.get(var_name)
        if actual is None:
            errors.append(f"J38_MISSING_TYPE_ARG: 缺少类型实参 {var_name}")
            continue
        if not satisfies_bound(actual, req_bound, impls):
            errors.append(f"J38_UNSATISFIED_BOUND: {var_name}={actual} 不满足约束 {req_bound}")
    return errors


def check_conditional_coherence(impls: Tuple[ConditionalImpl, ...]) -> List[str]:
    """J37 + J38：检查条件实现的孤儿规则与头部一阶合一重叠（即便 where 条件不同，头部可合一仍判重叠）。"""
    errors: List[str] = []
    for impl in impls:
        if isinstance(impl.target, TyVar):
            errors.append(f"J37_BLANKET_FORBIDDEN: {impl.unit} 禁止一揽子条件实现 {impl.interface}")
            continue
        is_at_type = impl.at_type_decl and impl.unit == impl.target.unit
        is_in_iface = impl.unit == impl.interface_unit
        if not (is_at_type or is_in_iface):
            errors.append(f"J37_ORPHAN: {impl.unit} 不能为 ({impl.target}, {impl.interface}) 声明孤儿条件实现")

    by_iface: Dict[Tuple[str, str], List[ConditionalImpl]] = {}
    for impl in impls:
        if isinstance(impl.target, TyCon):
            by_iface.setdefault((impl.interface_unit, impl.interface), []).append(impl)

    for (_, iface), group in sorted(by_iface.items()):
        for i in range(len(group)):
            for j in range(i + 1, len(group)):
                a, b = group[i], group[j]
                if COH.unify(a.target, b.target) is not None:
                    errors.append(
                        f"J37_CONDITIONAL_OVERLAP: {iface} 的条件实现头部 {a.target} 与 {b.target} 可合一，存在重叠"
                    )
    return errors


def check_l5_view_generic_instantiation(
    param_allows_view: bool,
    actual_is_escapable: bool,
    param_name: str = "T",
) -> Optional[str]:
    """L5：默认泛型参数要求 Escapable；仅声明 view T 后放开接受携带来源的非逃逸视图类型。"""
    if not param_allows_view and not actual_is_escapable:
        return f"L5_ESCAPABLE_REQUIRED: 非 view 类型参数 {param_name} 拒绝非逃逸视图实参"
    return None
