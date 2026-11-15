"""析构义务的原型（内核基底 KB1）。

两个互相独立的部分：
- 规划器：给一段只有所有权操作的程序插入显式的析构，并为每个可能失败的操作列出失败时的清理。
- 检验器：不看规划器怎么想，只执行带析构的程序，穷举全部路径，检查每份义务恰好履行一次、
  可观察的析构发生在作用域结束处且按声明逆序。

规格见 docs/proposals/drop-obligation-model.md。用法：python3 scripts/loan_kernel/drop_plan.py 文件.dp ...
"""
from dataclasses import dataclass, field
from pathlib import Path
import re
import sys


@dataclass
class TypeDecl:
    name: str
    fields: list  # (字段名, 类型名)
    deinit: bool = False
    consume: bool = False  # 必须显式消费：不得隐式析构
    unforgettable: bool = False


@dataclass
class Stmt:
    kind: str  # let use move call set take open forget return break if loop block | drop（规划器插入）
    line: int
    var: str = ""
    ty: str = ""
    fld: str = ""
    names: list = field(default_factory=list)
    body: list = field(default_factory=list)
    orelse: list = field(default_factory=list)
    if_owned: bool = False  # drop：只在运行时仍拥有时析构（析构标志）
    replace: bool = False  # drop：赋值时析构旧值
    cleanup: list = field(default_factory=list)  # 可能失败的操作：失败时依次析构的 (变量, 是否带标志)


@dataclass(frozen=True)
class Policy:
    early: str = "eligible"  # none：全部留到作用域结束 | eligible：不可观察的在最后一次使用后析构 | all：变异
    maybe_moved: str = "flag"  # flag：析构标志 | branch_end：在没有移动的分支末尾析构 | reject | ignore：变异
    failure_cleanup: bool = True  # False：变异，失败出口上不清理


class Rejected(Exception):
    """程序本身不被接受（相当于编译错误）。"""

    def __init__(self, line, rule, message):
        super().__init__(f"第 {line} 行：{rule} {message}")
        self.line, self.rule = line, rule


# ---- 解析：一行一条语句

def parse(text):
    types, funs, stack = {}, {}, []
    for number, raw in enumerate(text.splitlines(), start=1):
        line = raw.split("//")[0].strip()
        if not line:
            continue
        words = re.findall(r"[A-Za-z_]\w*|[{}().=,:]", line)
        head = words[0]
        if head == "type":
            flds = re.findall(r"(\w+)\s*:\s*(\w+)", line)
            types[words[1]] = TypeDecl(words[1], flds, "deinit" in words[2:], "consume" in words[2:],
                                       "unforgettable" in words[2:])
        elif head == "fun":
            funs[words[1]] = []
            stack = [funs[words[1]]]
        elif head == "}":
            stack.pop()
            if words[1:3] == ["else", "{"]:
                stack.append(stack[-1][-1].orelse)
        elif head in ("if", "loop", "{"):
            stmt = Stmt({"{": "block"}.get(head, head), number)
            stack[-1].append(stmt)
            stack.append(stmt.body)
        elif head in ("let", "set"):
            stack[-1].append(Stmt(head, number, var=words[1], ty=words[3]))
        elif head == "take":  # take y = x.a
            stack[-1].append(Stmt("take", number, var=words[1], ty=words[3], fld=words[5]))
        elif head == "open":  # open x as a, b
            stack[-1].append(Stmt("open", number, var=words[1], names=[w for w in words[3:] if w != ","]))
        elif head in ("use", "move", "forget"):
            stack[-1].append(Stmt(head, number, var=words[1]))
        elif head in ("call", "return", "break"):
            stack[-1].append(Stmt(head, number))
        else:
            raise Rejected(number, "SYNTAX", f"不认识的语句 {head}")
    return types, funs


# ---- 类型的性质

class Types:
    def __init__(self, types):
        self.types = types

    def observable(self, name):
        """析构是否可观察：自己声明了 deinit，或某个字段可观察。"""
        decl = self.types[name]
        return decl.deinit or any(self.observable(ty) for _, ty in decl.fields)

    def units(self, var, name):
        """一个变量名下的义务单元：每个字段一份，声明了 deinit 的类型自身另有一份。"""
        decl = self.types[name]
        units = [f"{var}.{fld}" for fld, _ in decl.fields]
        return units + [var] if decl.deinit or not decl.fields else units


# ---- 活性：语句之后还会用到哪些变量

def uses(stmt):
    if stmt.kind in ("use", "move", "forget", "open"):
        return {stmt.var}
    if stmt.kind == "take":
        return {stmt.ty}
    return set()


def liveness(stmts, after, loop_exit=frozenset(), fun_exit=frozenset()):
    """自后向前。给每条语句记下 live_out；循环体迭代到不动点。"""
    live = set(after)
    for stmt in reversed(stmts):
        stmt.live_out = set(live)
        if stmt.kind == "if":
            stmt.entry = (liveness(stmt.body, live, loop_exit, fun_exit),
                          liveness(stmt.orelse, live, loop_exit, fun_exit))
            live = stmt.entry[0] | stmt.entry[1]
        elif stmt.kind == "block":
            live = liveness(stmt.body, live, loop_exit, fun_exit)
        elif stmt.kind == "loop":
            head = set(live)
            while True:
                grown = head | liveness(stmt.body, head, frozenset(live), fun_exit)
                if grown == head:
                    break
                head = grown
            live = head
        elif stmt.kind == "return":
            live = set(fun_exit)
        elif stmt.kind == "break":
            live = set(loop_exit)
        else:
            defined = {stmt.var} if stmt.kind == "let" else set(stmt.names) if stmt.kind == "open" else set()
            live = (live - defined) | uses(stmt)
    return live


# ---- 规划器

class Planner:
    def __init__(self, types, policy):
        self.t = Types(types)
        self.policy = policy

    def plan(self, body):
        liveness(body, set())
        self.scopes = []  # 每层：[变量名…]，按声明顺序
        self.ty = {}
        self.status = {}  # 义务单元 -> O 拥有 | G 已不归本函数（移走、析构、遗忘、拆开）| ? 可能已移走
        return self.block(body, loop_depth=None)

    # 变量级的查询
    def units(self, var):
        return self.t.units(var, self.ty[var])

    def state(self, var):
        seen = {self.status[u] for u in self.units(var)}
        return seen.pop() if len(seen) == 1 else "P"  # P：部分移出

    def early(self, var):
        return self.policy.early == "all" or (
            self.policy.early == "eligible" and not self.t.observable(self.ty[var]))

    def drop(self, var, line, **tags):
        if self.t.types[self.ty[var]].consume:  # 任何隐式析构都不允许，无论由哪条规则引出
            raise Rejected(line, "D5", f"{var} 必须显式消费，不能被隐式析构")
        flagged = any(self.status[u] == "?" for u in self.units(var))
        for unit in self.units(var):
            self.status[unit] = "G"
        return Stmt("drop", line, var=var, if_owned=flagged, **tags)

    def owned(self, var):
        return any(self.status[u] in ("O", "?") for u in self.units(var))

    def exit_drops(self, scopes, line, commit):
        """离开若干层作用域时的析构：自内向外，每层按声明逆序。"""
        out = []
        saved = dict(self.status)
        for names in reversed(scopes):
            for var in reversed(names):
                if not self.owned(var):
                    continue
                if self.t.types[self.ty[var]].consume:
                    raise Rejected(line, "D5", f"{var} 必须显式消费，不能留到作用域结束")
                out.append(self.drop(var, line))
        if not commit:
            self.status = saved
        return out

    def cleanup(self):
        """可能失败的操作：失败时要析构的变量，自内向外、声明逆序。"""
        if not self.policy.failure_cleanup:
            return []
        return [(var, any(self.status[u] == "?" for u in self.units(var)))
                for names in reversed(self.scopes) for var in reversed(names) if self.owned(var)]

    def block(self, stmts, loop_depth):
        self.scopes.append([])
        out = []
        ended = False
        for stmt in stmts:
            if ended:
                raise Rejected(stmt.line, "SYNTAX", "return / break 之后还有语句")
            out += self.stmt(stmt, loop_depth)
            ended = stmt.kind in ("return", "break")
            if not ended:
                out += self.early_drops(stmt.live_out, stmt.line)
        if not ended:
            out += self.exit_drops(self.scopes[-1:], stmts[-1].line if stmts else 0, commit=True)
        self.scopes.pop()
        return out

    def early_drops(self, live, line):
        out = []
        for names in reversed(self.scopes):
            for var in reversed(names):
                if var not in live and self.early(var) and self.state(var) in ("O", "P") and self.owned(var):
                    if self.t.types[self.ty[var]].consume:
                        continue
                    out.append(self.drop(var, line))
        return out

    def need(self, var, line, what):
        if var not in self.ty:
            raise Rejected(line, "SYNTAX", f"未声明的变量 {var}")
        if self.state(var) != "O":
            raise Rejected(line, "D1", f"{what} {var}：它已被移走、可能已被移走或已部分移出")

    def stmt(self, stmt, loop_depth):
        kind, line = stmt.kind, stmt.line
        if kind == "let":
            stmt.cleanup = self.cleanup()
            self.declare(stmt.var, stmt.ty)
            return [stmt]
        if kind == "call":
            stmt.cleanup = self.cleanup()
            return [stmt]
        if kind == "use":
            self.need(stmt.var, line, "使用")
            return [stmt]
        if kind in ("move", "forget"):
            self.need(stmt.var, line, "移动" if kind == "move" else "遗忘")
            if kind == "forget" and self.unforgettable(self.ty[stmt.var]):
                raise Rejected(line, "D6", f"{stmt.var} 的类型不可遗忘")
            for unit in self.units(stmt.var):
                self.status[unit] = "G"
            return [stmt]
        if kind == "set":  # 先求值新值（可能失败），再析构旧值，再提交
            if stmt.var not in self.ty:
                raise Rejected(line, "SYNTAX", f"未声明的变量 {stmt.var}")
            evaluate = Stmt("call", line, cleanup=self.cleanup())
            out = [evaluate]
            if self.owned(stmt.var):
                if self.t.types[self.ty[stmt.var]].consume:
                    raise Rejected(line, "D5", f"{stmt.var} 必须显式消费，不能被覆盖")
                out.append(self.drop(stmt.var, line, replace=True))
            self.ty[stmt.var] = stmt.ty
            for unit in self.units(stmt.var):
                self.status[unit] = "O"
            return out + [stmt]
        if kind == "take":  # take y = x.a：从 x 移出一个字段
            source, unit = stmt.ty, f"{stmt.ty}.{stmt.fld}"
            if source not in self.ty or unit not in self.status:
                raise Rejected(line, "SYNTAX", f"{source} 没有字段 {stmt.fld}")
            decl = self.t.types[self.ty[source]]
            if decl.deinit:
                raise Rejected(line, "D2", f"{source} 的类型声明了 deinit，不能部分移出；须用 open")
            if self.status[unit] != "O":
                raise Rejected(line, "D1", f"{unit} 已被移走或可能已被移走")
            self.status[unit] = "G"
            self.declare(stmt.var, dict(decl.fields)[stmt.fld])
            return [stmt]
        if kind == "open":  # 消费并解除析构：字段成为局部变量，不运行 deinit
            self.need(stmt.var, line, "拆开")
            decl = self.t.types[self.ty[stmt.var]]
            if len(stmt.names) != len(decl.fields):
                raise Rejected(line, "SYNTAX", f"{decl.name} 有 {len(decl.fields)} 个字段")
            for unit in self.units(stmt.var):
                self.status[unit] = "G"
            for name, (_, ty) in zip(stmt.names, decl.fields):
                self.declare(name, ty)
            return [stmt]
        if kind == "return":
            return self.exit_drops(self.scopes, line, commit=True) + [stmt]
        if kind == "break":
            if loop_depth is None:
                raise Rejected(line, "SYNTAX", "break 不在循环里")
            return self.exit_drops(self.scopes[loop_depth:], line, commit=True) + [stmt]
        if kind == "block":
            stmt.body = self.block(stmt.body, loop_depth)
            return [stmt]
        if kind == "if":
            return self.branch(stmt, loop_depth)
        if kind == "loop":
            return self.loop(stmt)
        raise Rejected(line, "SYNTAX", kind)

    def declare(self, var, ty):
        if var in self.ty and self.owned(var):
            raise Rejected(0, "SYNTAX", f"重复声明 {var}")
        self.ty[var] = ty
        self.scopes[-1].append(var)
        for unit in self.units(var):
            self.status[unit] = "O"

    def unforgettable(self, name):
        decl = self.t.types[name]
        return decl.unforgettable or any(self.unforgettable(ty) for _, ty in decl.fields)

    def visible(self):
        return [var for names in self.scopes for var in names]

    def branch(self, stmt, loop_depth):
        before, types = dict(self.status), dict(self.ty)
        arms = []
        for body, entry in zip((stmt.body, stmt.orelse), stmt.entry):
            self.status, self.ty = dict(before), dict(types)
            # 这条分支里不再用到、可以提前析构的，进入分支时就析构
            lead = self.early_drops(entry, stmt.line)
            planned = lead + self.block(body, loop_depth)
            ends = bool(body) and body[-1].kind in ("return", "break")
            arms.append([planned, dict(self.status), ends])
        open_arms = [arm for arm in arms if not arm[2]]
        merged = dict(before)
        if open_arms:
            for var in self.visible():
                for unit in self.t.units(var, types[var]):
                    seen = {arm[1][unit] for arm in open_arms}
                    merged[unit] = seen.pop() if len(seen) == 1 else "?"
            for var in self.visible():
                units = self.t.units(var, types[var])
                if not any(merged[u] == "?" for u in units):
                    continue
                choice = "branch_end" if self.early(var) and self.policy.maybe_moved != "ignore" \
                    else self.policy.maybe_moved
                if choice == "reject":
                    raise Rejected(stmt.line, "D3", f"{var} 只在一条分支上被移走")
                if choice == "ignore":
                    for unit in units:
                        merged[unit] = "G"
                elif choice == "branch_end":  # 在仍拥有它的分支末尾析构，合流后两边一致
                    for arm in open_arms:
                        if any(arm[1][u] == "O" for u in units):
                            self.status, self.ty = arm[1], types
                            arm[0].append(self.drop(var, stmt.line))
                    for unit in units:
                        merged[unit] = "G"
        self.status, self.ty = merged, types
        stmt.body, stmt.orelse = arms[0][0], arms[1][0]
        return [stmt]

    def loop(self, stmt):
        before = dict(self.status)
        stmt.body = self.block(stmt.body, loop_depth=len(self.scopes))
        for var in self.visible():
            for unit in self.units(var):
                if before[unit] == "O" and self.status[unit] != "O":
                    raise Rejected(stmt.line, "D4", f"{var} 在循环体内被移走或析构，下一次迭代时已不可用")
        self.status = before
        return [stmt]


# ---- 检验器：执行带析构的程序

class Leave(Exception):
    def __init__(self, how):
        self.how = how  # return | break | fail


class Verifier:
    """只认程序里显式的 drop 与各操作的失败清理；自己不推断任何析构。"""

    def __init__(self, types, explore_failure=True):
        self.t = Types(types)
        self.explore_failure = explore_failure

    def run_all(self, body):
        """穷举选择序列：分支走哪边、循环几次、哪个操作失败。返回违反的不变量集合。"""
        found, pending = {}, [[]]
        while pending:
            self.choices, self.taken = pending.pop(), []
            self.unit, self.scopes, self.order, self.ty = {}, [], [], {}
            self.early_observable, self.violations = set(), []
            try:
                self.block(body)
            except Leave:
                pass
            for name in self.violations:
                found.setdefault(name, list(self.taken))
            for index in range(len(self.choices), len(self.taken)):
                value, arity = self.taken[index]
                for other in range(value + 1, arity):
                    pending.append([v for v, _ in self.taken[:index]] + [other])
        return found

    def choose(self, arity):
        index = len(self.taken)
        value = self.choices[index] if index < len(self.choices) else 0
        self.taken.append((value, arity))
        return value

    def block(self, stmts):
        self.scopes.append([])
        try:
            for stmt in stmts:
                self.step(stmt)
        finally:
            for var in self.scopes.pop():
                if any(self.unit[u] == "O" for u in self.t.units(var, self.ty[var])):
                    self.violations.append("V1 义务未履行（泄漏）")
                self.early_observable.discard(var)

    def active(self):
        return [var for names in self.scopes for var in names]

    def declare(self, var, ty):
        self.ty[var] = ty
        self.scopes[-1].append(var)
        self.order.append(var)
        for unit in self.t.units(var, ty):
            self.unit[unit] = "O"

    def destroy(self, var, if_owned, replace=False):
        units = self.t.units(var, self.ty[var])
        states = [self.unit[u] for u in units]
        if all(s != "O" for s in states):
            if not if_owned:
                self.violations.append("V2 析构了已不拥有的值（重复析构）")
            return
        for unit in units:
            if self.unit[unit] == "O":
                self.unit[unit] = "D"
        if self.t.observable(self.ty[var]) and not replace:
            later = self.order[self.order.index(var) + 1:]
            if any(w in self.active() and self.t.observable(self.ty[w])
                   and any(self.unit[u] == "O" for u in self.t.units(w, self.ty[w])) for w in later):
                self.violations.append("V4 可观察的析构没有按声明逆序")
            if var in self.active():
                self.early_observable.add(var)

    def fail(self, stmt):
        for var, flagged in stmt.cleanup:
            self.destroy(var, flagged)
        raise Leave("fail")

    def step(self, stmt):
        kind = stmt.kind
        if kind == "drop":
            self.destroy(stmt.var, stmt.if_owned, stmt.replace)
            return
        if kind in ("return", "break"):
            raise Leave(kind)
        if self.early_observable:  # 有可观察的值已析构而它的作用域还在继续执行别的语句
            self.violations.append("V3 可观察的值在作用域结束前被析构")
            self.early_observable.clear()
        if kind in ("let", "call") and self.explore_failure and self.choose(2):
            self.fail(stmt)
        if kind == "let":
            self.declare(stmt.var, stmt.ty)
        elif kind == "set":
            self.ty[stmt.var] = stmt.ty
            for unit in self.t.units(stmt.var, stmt.ty):
                self.unit[unit] = "O"
        elif kind in ("use", "move", "forget", "open"):
            units = self.t.units(stmt.var, self.ty[stmt.var])
            if any(self.unit[u] != "O" for u in units):
                self.violations.append("V5 使用了已析构或已移走的值")
            if kind != "use":
                for unit in units:
                    self.unit[unit] = "M"
            if kind == "open":
                for name, (_, ty) in zip(stmt.names, self.t.types[self.ty[stmt.var]].fields):
                    self.declare(name, ty)
        elif kind == "take":
            unit = f"{stmt.ty}.{stmt.fld}"
            if self.unit[unit] != "O":
                self.violations.append("V5 使用了已析构或已移走的值")
            self.unit[unit] = "M"
            self.declare(stmt.var, dict(self.t.types[self.ty[stmt.ty]].fields)[stmt.fld])
        elif kind == "block":
            self.block(stmt.body)
        elif kind == "if":
            self.block(stmt.orelse if self.choose(2) else stmt.body)
        elif kind == "loop":
            for _ in range(self.choose(3)):
                try:
                    self.block(stmt.body)
                except Leave as leave:
                    if leave.how != "break":
                        raise
                    break


def show(stmts, depth=1):
    lines = []
    for stmt in stmts:
        pad = "  " * depth
        if stmt.kind in ("if", "loop", "block"):
            lines.append(f"{pad}{stmt.kind if stmt.kind != 'block' else ''} {{".replace("  {", " {"))
            lines += show(stmt.body, depth + 1)
            if stmt.orelse:
                lines.append(f"{pad}}} else {{")
                lines += show(stmt.orelse, depth + 1)
            lines.append(f"{pad}}}")
        elif stmt.kind == "drop":
            lines.append(f"{pad}drop{'?' if stmt.if_owned else ''} {stmt.var}")
        else:
            text = " ".join(part for part in (stmt.kind, stmt.var, stmt.ty) if part)
            if stmt.cleanup:
                text += "    失败时: " + ", ".join(var + ("?" if flagged else "") for var, flagged in stmt.cleanup)
            lines.append(pad + text)
    return lines


def check(text, policy=Policy()):
    """对每个函数：返回 (函数名, 拒绝的规则 | None, 检验器发现的违反集合, 规划后的程序)。"""
    types, funs = parse(text)
    results = []
    for name, body in funs.items():
        try:
            planned = Planner(types, policy).plan(body)
        except Rejected as rejected:
            results.append((name, rejected.rule, set(), [str(rejected)]))
            continue
        results.append((name, None, set(Verifier(types).run_all(planned)), show(planned)))
    return results


def main(argv):
    for path in argv[1:]:
        for name, rule, violations, listing in check(Path(path).read_text(encoding="utf-8")):
            verdict = f"拒绝（{rule}）" if rule else "违反：" + "；".join(sorted(violations)) if violations else "通过"
            print(f"{path} {name}: {verdict}")
            print("\n".join(listing))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
