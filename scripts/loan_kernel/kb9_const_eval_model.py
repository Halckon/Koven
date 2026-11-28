"""P2-2：KB9 图上封闭编译期求值器原型（之六 J6/J8 / G0/G2）。

非规范、不被编译器引用。对齐 docs/proposals/global-and-static-state.md 与演进总纲之八 §2.5（IR5/KB9）：
- 在控制流图（CFG）上解释执行封闭的编译期求值指令集（不引入 const fun 语法）；
- 支持 32 位有符号整数带检查算术（溢出报 KB9_ARITH_OVERFLOW，除零报 KB9_DIV_BY_ZERO）；
- 支持纯内存 value class 静态构造与字段读取（支撑 G2 静态不可变存储）；
- 带确定性步数上限（max_steps）防死循环（KB9_STEP_LIMIT_EXCEEDED）；
- 严禁运行时副作用/堆分配/IO（KB9_FORBIDDEN_RUNTIME_EFFECT）。
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple, Union

I32_MIN = -(1 << 31)
I32_MAX = (1 << 31) - 1


@dataclass(frozen=True)
class ValueRecord:
    type_name: str
    fields: Tuple[Tuple[str, "ConstVal"], ...]

    def get(self, field_name: str) -> Optional["ConstVal"]:
        for k, v in self.fields:
            if k == field_name:
                return v
        return None


ConstVal = Union[int, bool, str, ValueRecord]


@dataclass(frozen=True)
class ConstInstr:
    kind: str  # ConstInt | ConstStr | AddChecked | SubChecked | MulChecked | DivChecked | CmpLt | ConstructValueClass | FieldGet | RuntimeEffect
    dst: str = ""
    args: Tuple[Union[str, int], ...] = ()
    type_name: str = ""
    field_names: Tuple[str, ...] = ()


@dataclass(frozen=True)
class ConstTerm:
    kind: str  # Goto | Branch | Return
    target: str = ""
    cond_reg: str = ""
    then_bb: str = ""
    else_bb: str = ""
    ret_reg: str = ""


@dataclass(frozen=True)
class ConstBlock:
    block_id: str
    instrs: Tuple[ConstInstr, ...]
    term: ConstTerm


@dataclass(frozen=True)
class ConstGraph:
    name: str
    entry: str
    blocks: Dict[str, ConstBlock]


def _check_i32(val: int) -> Optional[str]:
    if val < I32_MIN or val > I32_MAX:
        return f"KB9_ARITH_OVERFLOW: 结果 {val} 超出 Int32 范围 [{I32_MIN}, {I32_MAX}]"
    return None


def eval_const_graph(
    graph: ConstGraph,
    max_steps: int = 1000,
) -> Tuple[Optional[ConstVal], Optional[str], int]:
    """在封闭 CFG 上执行编译期求值，返回 (value, error, steps_used)。"""
    regs: Dict[str, ConstVal] = {}
    cur_bb_id = graph.entry
    steps = 0

    while True:
        bb = graph.blocks.get(cur_bb_id)
        if bb is None:
            return None, f"KB9_BAD_BLOCK: 找不到基本块 {cur_bb_id}", steps
        for ins in bb.instrs:
            steps += 1
            if steps > max_steps:
                return None, f"KB9_STEP_LIMIT_EXCEEDED: 求值步数超过上限 {max_steps}", steps
            if ins.kind == "RuntimeEffect":
                return None, f"KB9_FORBIDDEN_RUNTIME_EFFECT: 编译期求值禁止运行时副作用 {ins.type_name}", steps
            if ins.kind == "ConstInt":
                val = int(ins.args[0])
                err = _check_i32(val)
                if err is not None:
                    return None, err, steps
                regs[ins.dst] = val
            elif ins.kind == "ConstStr":
                regs[ins.dst] = str(ins.args[0])
            elif ins.kind in ("AddChecked", "SubChecked", "MulChecked", "DivChecked"):
                a = regs[str(ins.args[0])]
                b = regs[str(ins.args[1])]
                if not isinstance(a, int) or not isinstance(b, int):
                    return None, f"KB9_TYPE_ERROR: {ins.kind} 需要整数操作数", steps
                if ins.kind == "AddChecked":
                    res = a + b
                elif ins.kind == "SubChecked":
                    res = a - b
                elif ins.kind == "MulChecked":
                    res = a * b
                else:
                    if b == 0:
                        return None, "KB9_DIV_BY_ZERO: 编译期整数除零", steps
                    if a == I32_MIN and b == -1:
                        return None, "KB9_ARITH_OVERFLOW: I32_MIN / -1 溢出", steps
                    res = int(a / b)
                err = _check_i32(res)
                if err is not None:
                    return None, err, steps
                regs[ins.dst] = res
            elif ins.kind == "CmpLt":
                a = regs[str(ins.args[0])]
                b = regs[str(ins.args[1])]
                regs[ins.dst] = bool(a < b)
            elif ins.kind == "ConstructValueClass":
                fields = tuple((fname, regs[str(reg)]) for fname, reg in zip(ins.field_names, ins.args))
                regs[ins.dst] = ValueRecord(ins.type_name, fields)
            elif ins.kind == "FieldGet":
                rec = regs[str(ins.args[0])]
                if not isinstance(rec, ValueRecord):
                    return None, f"KB9_TYPE_ERROR: FieldGet 目标不是 ValueRecord", steps
                got = rec.get(str(ins.args[1]))
                if got is None:
                    return None, f"KB9_MISSING_FIELD: {rec.type_name} 不存在字段 {ins.args[1]}", steps
                regs[ins.dst] = got
            else:
                return None, f"KB9_UNKNOWN_INSTR: 未知指令 {ins.kind}", steps

        steps += 1
        if steps > max_steps:
            return None, f"KB9_STEP_LIMIT_EXCEEDED: 求值步数超过上限 {max_steps}", steps
        term = bb.term
        if term.kind == "Return":
            return regs.get(term.ret_reg), None, steps
        if term.kind == "Goto":
            cur_bb_id = term.target
        elif term.kind == "Branch":
            cond = regs.get(term.cond_reg)
            cur_bb_id = term.then_bb if bool(cond) else term.else_bb
        else:
            return None, f"KB9_UNKNOWN_TERM: 未知终结符 {term.kind}", steps
