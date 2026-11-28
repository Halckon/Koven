"""OD3（3A 取消接口三方案）嵌套 scoped + 已取消子任务 join() + 泛型 retry<T, E> 对照原型。

非规范、不被编译器引用。对齐 docs/proposals/kernel-open-decisions-2026-11.md §3.3（OD3 2026-11-27 共识）：
- 比较三套候选取消出口表面（不提前冻结具体类型名）：
  - C-I：Result<T, E> 由业务错误 E 显式承载 Cancelled（仅带 FromCancelled 构造器 vs 额外 IsCancelled 谓词）
  - C-II-a：独立三态枚举 ScopedResult<T, E> = Ok(T) | Err(E) | Cancelled
  - C-II-b：独立嵌套 Result<Result<T, E>, Cancelled>（外层 ? 原生向上传播取消）
- 检验两条 OD3 / OD2 B1 硬约束：
  1. 泛型 retry<T, E> 收到 Cancelled 时必须立刻原样向上传播，绝不重试（且不把取消隐式转成业务错误 E）；
  2. 嵌套 scoped 在两种提交全序下：
     (i)  外部取消先提交：内外层均保持 Cancelled 身份，不经 ? 变成普通业务错误 E；
     (ii) 子任务业务错误 err_A 先提交：即使外层随后观察到取消，主结果仍保留 err_A。
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Generic, List, Optional, Tuple, TypeVar, Union

T = TypeVar("T")
E = TypeVar("E")


# ---- 三种候选结果表示

@dataclass(frozen=True)
class Ok(Generic[T]):
    value: T


@dataclass(frozen=True)
class Err(Generic[E]):
    error: E


@dataclass(frozen=True)
class Cancelled:
    pass


# C-I: Result<T, AppError>，其中 AppError 包含业务错误或 "Cancelled"
Result = Union[Ok[T], Err[E]]
# C-II-a: 独立三态枚举
ScopedResult = Union[Ok[T], Err[E], Cancelled]
# C-II-b: 独立嵌套 Result<Result<T, E>, Cancelled>
NestedResult = Union[Ok[Result[T, E]], Err[Cancelled]]


@dataclass(frozen=True)
class RetryTrace:
    surface: str
    attempts: int
    outcome: object
    retried_on_cancel: bool
    cancel_turned_into_business_err: bool
    native_question_mark_for_cancel: bool
    required_extra_e_trait: bool


# ---- 方案 C-I：E 显式承载 Cancelled

def run_ci_nested_retry(
    commit_order: str,  # cancel_first | err_first
    has_is_cancelled_predicate: bool = False,
    max_attempts: int = 3,
) -> RetryTrace:
    """C-I：泛型 retry<T, E> 只看见 Result<T, E>；若无额外 IsCancelled 谓词，无法区分 Cancelled 与可重试业务错误。"""
    attempts = 0
    retried_on_cancel = False
    cancel_turned_into_err = False

    def inner_scoped() -> Result[str, str]:
        if commit_order == "cancel_first":
            # 外部取消先提交 -> C-I 经 onCancel() 装入 Err("AppError.Cancelled")
            return Err("AppError.Cancelled")
        return Err("AppError.IoTimeout")

    def outer_scoped() -> Result[str, str]:
        # 在 C-I 中，内层 scoped 返回 Result<T, E>，外层直接写 ? 会把 Err("AppError.Cancelled") 按普通 Err 冒泡
        res = inner_scoped()
        if isinstance(res, Err):
            return res
        return res

    last: Result[str, str] = Ok("")
    while attempts < max_attempts:
        attempts += 1
        last = outer_scoped()
        if isinstance(last, Ok):
            break
        if has_is_cancelled_predicate and last.error == "AppError.Cancelled":
            # 需要额外谓词接口才能把取消从 E 中剥离
            break
        if last.error == "AppError.Cancelled":
            # 无 IsCancelled 谓词时，泛型 retry<T, E> 把取消误当成业务错误 E 重试！
            cancel_turned_into_err = True
            if attempts < max_attempts:
                retried_on_cancel = True
                continue
        # 普通业务错误按策略重试
        if attempts < max_attempts:
            continue
        break

    return RetryTrace(
        surface="C-I+IsCancelled" if has_is_cancelled_predicate else "C-I",
        attempts=attempts,
        outcome=last,
        retried_on_cancel=retried_on_cancel,
        cancel_turned_into_business_err=cancel_turned_into_err,
        native_question_mark_for_cancel=True,
        required_extra_e_trait=True,
    )


# ---- 方案 C-II-a：独立三态 ScopedResult<T, E>

def run_ciia_nested_retry(commit_order: str, max_attempts: int = 3) -> RetryTrace:
    """C-II-a：类型层严格正交分离 Cancelled 与 Err(E)，对 E 零额外约束；现行 Guide 09 的 ? 不能直接用于三态枚举。"""
    attempts = 0

    def inner_scoped() -> ScopedResult[str, str]:
        if commit_order == "cancel_first":
            return Cancelled()
        return Err("AppError.IoTimeout")

    def outer_scoped() -> ScopedResult[str, str]:
        # 显式 when 转发（不经 ? 隐式转 E），保持 Cancelled 身份或保留先提交的 err_A
        res = inner_scoped()
        if isinstance(res, Cancelled):
            return Cancelled()
        if isinstance(res, Err):
            return res
        return res

    last: ScopedResult[str, str] = Ok("")
    while attempts < max_attempts:
        attempts += 1
        last = outer_scoped()
        if isinstance(last, (Ok, Cancelled)):
            break
        if attempts < max_attempts:
            continue
        break

    return RetryTrace(
        surface="C-II-a",
        attempts=attempts,
        outcome=last,
        retried_on_cancel=False,
        cancel_turned_into_business_err=False,
        native_question_mark_for_cancel=False,
        required_extra_e_trait=False,
    )


# ---- 方案 C-II-b：独立嵌套 Result<Result<T, E>, Cancelled>

def run_ciib_nested_retry(commit_order: str, max_attempts: int = 3) -> RetryTrace:
    """C-II-b：外层 Result<_, Cancelled> 可直接用 ? 原生传播取消，内层 Result<T, E> 承载纯业务错误。"""
    attempts = 0

    def inner_scoped() -> NestedResult[str, str]:
        if commit_order == "cancel_first":
            return Err(Cancelled())
        return Ok(Err("AppError.IoTimeout"))

    def outer_scoped() -> NestedResult[str, str]:
        # 外层直接对 Cancelled 通道使用 ?（等价于若 Err(Cancelled) 立刻原样返回）
        res = inner_scoped()
        if isinstance(res, Err):
            return res
        inner_business = res.value
        return Ok(inner_business)

    last: NestedResult[str, str] = Ok(Ok(""))
    while attempts < max_attempts:
        attempts += 1
        last = outer_scoped()
        if isinstance(last, Err):
            # 外层 Err 必为 Cancelled：立刻原样向上传播，绝不重试
            break
        if isinstance(last.value, Ok):
            break
        # 仅对内层业务错误 Err(E) 按策略重试
        if attempts < max_attempts:
            continue
        break

    return RetryTrace(
        surface="C-II-b",
        attempts=attempts,
        outcome=last,
        retried_on_cancel=False,
        cancel_turned_into_business_err=False,
        native_question_mark_for_cancel=True,
        required_extra_e_trait=False,
    )


def check_e2_question_mark_conversion(from_type: str, to_type: str) -> Optional[str]:
    """之十一 E2 + OD3 硬约束：? 错误转换绝不得把 Cancelled 隐式转换为普通业务错误 E。"""
    if from_type == "Cancelled" and to_type != "Cancelled":
        return f"E2_FORBIDDEN_CANCEL_TO_ERR: 禁止 ? 将 {from_type} 隐式转为业务错误 {to_type}"
    return None
