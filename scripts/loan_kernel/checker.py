"""借用内核演算的参考检查器原型。

非规范、不被编译器引用。演算的语法、规则 K1–K11 与用例清单见
docs/proposals/loan-kernel-calculus.md；本文件只把那页的规则写成可执行形式。
检查器基于用例，不是健全性证明。
"""
from __future__ import annotations

import re
import sys
from collections import namedtuple
from dataclasses import dataclass, field
from pathlib import Path

Type = namedtuple("Type", "kind name")  # kind: int | unit | cls | view | borrow | inout
INT = Type("int", "Int")
UNIT = Type("unit", "Unit")


class SyntaxProblem(Exception):
    """演算程序本身不合文法；与借用规则无关。"""


# ---------------------------------------------------------------- 语法树

@dataclass
class ClassDecl:
    name: str
    fields: list  # (kind, name, type_name)；kind 为 "" | "borrow" | "inout"
    is_view: bool
    deinit: bool
    line: int
    implements: list = field(default_factory=list)

    def field(self, name):
        return next((f for f in self.fields if f[1] == name), None)

    def ref_fields(self):
        return [f for f in self.fields if f[0]]

    def exclusive(self):
        return any(f[0] == "inout" for f in self.fields)


@dataclass
class Fun:
    name: str
    params: list  # (mode, name, type_name)
    result: tuple  # (kind, type_name)；kind 为 "" | "borrow" | "inout"
    sources: list  # (param, None | "*" | field)：依赖参数本身、转发它携带的全部来源、转发某个借用字段
    trusted: bool
    body: list
    ret: object
    line: int
    tparams: list = field(default_factory=list)
    flows: list = field(default_factory=list)  # ((param, None | "*" | field), 目标参数)


@dataclass
class Place:
    root: str
    fields: list
    line: int


@dataclass
class Expr:
    kind: str  # int | place | move | ref | call | pack
    line: int
    site: int
    place: Place = None
    mut: bool = False
    name: str = ""
    args: list = field(default_factory=list)


@dataclass
class Stmt:
    kind: str  # val | assign | store | use | call | block | if | while
    line: int
    name: str = ""
    place: Place = None
    expr: Expr = None
    body: list = field(default_factory=list)
    orelse: list = field(default_factory=list)
    end_line: int = 0
    else_end_line: int = 0
    live_out: frozenset = frozenset()


# ---------------------------------------------------------------- 解析

TOKEN = re.compile(r"\s+|//[^\n]*|[A-Za-z_]\w*|\d+|[(){}:,.=&*<>]")


def tokenize(text):
    """切分为 (文本, 行号)；空白与注释丢弃。"""
    tokens, pos, line = [], 0, 1
    while pos < len(text):
        match = TOKEN.match(text, pos)
        if not match:
            raise SyntaxProblem(f"第 {line} 行：无法识别的字符 {text[pos]!r}")
        tok = match.group()
        if not tok.isspace() and not tok.startswith("//"):
            tokens.append((tok, line))
        line += tok.count("\n")
        pos = match.end()
    return tokens


class Parser:
    def __init__(self, text):
        self.toks = tokenize(text)
        self.i = 0
        self.sites = 0

    def peek(self, offset=0):
        j = self.i + offset
        return self.toks[j][0] if j < len(self.toks) else None

    def line(self):
        return self.toks[min(self.i, len(self.toks) - 1)][1]

    def next(self):
        tok = self.peek()
        if tok is None:
            raise SyntaxProblem("程序意外结束")
        self.i += 1
        return tok

    def expect(self, tok):
        line = self.line()
        got = self.next()
        if got != tok:
            raise SyntaxProblem(f"第 {line} 行：期望 {tok!r}，得到 {got!r}")

    def accept(self, tok):
        if self.peek() == tok:
            self.i += 1
            return True
        return False

    def program(self):
        classes, funs, sigs = {}, {}, {}
        self.interfaces = set()
        while self.peek() is not None:
            line = self.line()
            if self.accept("interface"):
                self.interfaces.add(self.next())
            elif self.peek() in ("fun", "sig"):
                table = funs if self.peek() == "fun" else sigs
                fun = self.fun(self.peek())
                table[fun.name] = fun
            else:
                is_view = self.accept("view")
                self.expect("class")
                name = self.next()
                self.expect("(")
                fields = []
                while not self.accept(")"):
                    kind = self.next() if self.peek() in ("borrow", "inout") else ""
                    fname = self.next()
                    self.expect(":")
                    fields.append((kind, fname, self.type_name()))
                    self.accept(",")
                implements = []
                while self.accept("is"):
                    implements.append(self.next())
                classes[name] = ClassDecl(name, fields, is_view, self.accept("deinit"), line, implements)
        return classes, funs, sigs

    def type_name(self):
        """类型名；fn N 是函数类型，any I 是接口 I 的存在类型。"""
        tok = self.next()
        return f"{tok} {self.next()}" if tok in ("fn", "any") else tok

    def source(self):
        param = self.next()
        return (param, self.next() if self.accept(".") else None)

    def fun(self, keyword):
        """函数或抽象签名（sig）；sig 没有函数体，用作函数类型与接口方法的合同。"""
        line = self.line()
        self.expect(keyword)
        tparams = []
        if self.accept("<"):
            while not self.accept(">"):
                tparams.append(self.next())
                self.accept(",")
        name = self.next()
        self.expect("(")
        params = []
        while not self.accept(")"):
            mode = self.next()
            pname = self.next()
            self.expect(":")
            params.append((mode, pname, self.type_name()))
            self.accept(",")
        self.expect(":")
        kind = self.next() if self.peek() in ("borrow", "inout") else ""
        result = (kind, self.type_name())
        sources, flows = [], []
        if self.accept("from"):
            while True:
                sources.append(self.source())
                if not self.accept(","):
                    break
        if self.accept("flows"):
            while True:
                src = self.source()
                self.expect("into")
                flows.append((src, self.next()))
                if not self.accept(","):
                    break
        trusted = self.accept("trusted") or keyword == "sig"
        body, ret = [], None
        if not trusted:
            self.expect("{")
            while self.peek() not in ("return", "}"):
                body.append(self.stmt())
            if self.accept("return"):
                ret = self.expr()
            self.expect("}")
        return Fun(name, params, result, sources, trusted, body, ret, line, tparams, flows)

    def block(self):
        self.expect("{")
        body = []
        while self.peek() != "}":
            body.append(self.stmt())
        end = self.line()
        self.expect("}")
        return body, end

    def star_condition(self):
        self.expect("(")
        self.expect("*")
        self.expect(")")

    def stmt(self):
        line, tok = self.line(), self.peek()
        if tok == "val":
            self.next()
            name = self.next()
            self.expect("=")
            return Stmt("val", line, name=name, expr=self.expr())
        if tok == "use":
            self.next()
            self.expect("(")
            place = self.place()
            self.expect(")")
            return Stmt("use", line, place=place)
        if tok == "{":
            body, end = self.block()
            return Stmt("block", line, body=body, end_line=end)
        if tok == "if":
            self.next()
            self.star_condition()
            body, end = self.block()
            orelse, else_end = ([], end)
            if self.accept("else"):
                orelse, else_end = self.block()
            return Stmt("if", line, body=body, orelse=orelse, end_line=end, else_end_line=else_end)
        if tok == "while":
            self.next()
            self.star_condition()
            body, end = self.block()
            return Stmt("while", line, body=body, end_line=end)
        if tok == "*":  # 经独占借用整体替换它所指的值
            self.next()
            name = self.next()
            self.expect("=")
            return Stmt("store", line, name=name, expr=self.expr())
        if self.peek(1) == "(":  # 只为副作用而调用
            return Stmt("call", line, expr=self.expr())
        place = self.place()
        self.expect("=")
        return Stmt("assign", line, place=place, expr=self.expr())

    def place(self):
        line = self.line()
        root, fields = self.next(), []
        while self.accept("."):
            fields.append(self.next())
        return Place(root, fields, line)

    def expr(self):
        line, tok = self.line(), self.peek()
        self.sites += 1
        site = self.sites
        if tok.isdigit():
            self.next()
            return Expr("int", line, site)
        if tok == "move":
            self.next()
            return Expr("move", line, site, place=self.place())
        if tok == "pack":  # 把具体类型的值打包为存在类型
            self.next()
            name = self.next()
            self.expect("(")
            inner = self.expr()
            self.expect(")")
            return Expr("pack", line, site, name=name, args=[inner])
        if tok == "&":
            self.next()
            mut = self.accept("mut")
            return Expr("ref", line, site, place=self.place(), mut=mut)
        if self.peek(1) == "(":
            name = self.next()
            self.expect("(")
            args = []
            while not self.accept(")"):
                args.append(self.expr())
                self.accept(",")
            return Expr("call", line, site, name=name, args=args)
        return Expr("place", line, site, place=self.place())


# ---------------------------------------------------------------- 活性（K3 的“还会被使用”）

def expr_uses(expr):
    if expr.kind in ("place", "move", "ref"):
        return {expr.place.root}
    return set().union(*(expr_uses(a) for a in expr.args)) if expr.args else set()


def stmt_uses(stmt):
    """语句自身读取的变量；整变量赋值的目标是定义，不算使用。"""
    uses = expr_uses(stmt.expr) if stmt.expr else set()
    if stmt.kind == "use" or (stmt.kind == "assign" and stmt.place.fields):
        uses.add(stmt.place.root)
    if stmt.kind == "store":
        uses.add(stmt.name)
    return uses


def stmt_defs(stmt):
    if stmt.kind == "val":
        return {stmt.name}
    if stmt.kind == "assign" and not stmt.place.fields:
        return {stmt.place.root}
    return set()


def live_in(stmts, out):
    """后向计算一串语句入口处的活变量，并把每条语句出口处的结果记在 live_out 上。"""
    live = set(out)
    for stmt in reversed(stmts):
        stmt.live_out = frozenset(live)
        if stmt.kind == "block":
            live = live_in(stmt.body, live)
        elif stmt.kind == "if":
            live = live_in(stmt.body, live) | live_in(stmt.orelse, live)
        elif stmt.kind == "while":
            head = set(live)
            while True:
                grown = head | live_in(stmt.body, head)
                if grown == head:
                    break
                head = grown
            live = head
        else:
            live = (live - stmt_defs(stmt)) | stmt_uses(stmt)
    return live


# ---------------------------------------------------------------- 状态

@dataclass
class Loan:
    kind: str  # shared | excl
    places: set
    parents: set
    line: int


@dataclass
class Var:
    ty: Type
    init: str  # A | M | MM
    loans: dict  # "" 为借用值自身持有的 loan；其余键为视图的借用字段
    payload: bool = False  # 对存在类型的载荷访问：只能调用方法，不能整体替换


@dataclass
class Value:
    ty: Type
    loans: dict = field(default_factory=dict)
    payload: bool = False

    def all_loans(self):
        return set().union(*self.loans.values()) if self.loans else set()


@dataclass
class Resolved:
    targets: set
    via: set
    ty: Type
    writable: bool
    field_loans: dict  # 所指为视图时，它各借用字段持有的 loan
    crossed: str  # 最后一步若恰好越过借用字段，为该字段的种类


def copy_loans(loans):
    return {key: set(value) for key, value in loans.items()}


def copy_state(state):
    return {name: Var(var.ty, var.init, copy_loans(var.loans), var.payload) for name, var in state.items()}


def prefix_related(a, b):
    shorter = min(len(a), len(b))
    return a[:shorter] == b[:shorter]


# ---------------------------------------------------------------- 检查器

class Checker:
    def __init__(self, classes, funs, sigs):
        self.classes, self.funs, self.sigs = classes, funs, sigs
        self.errors = set()

    def error(self, rule, line, message):
        self.errors.add((rule, line, message))

    def type_of(self, name):
        if name == "Int":
            return INT
        if name == "Unit":
            return UNIT
        if name.startswith("any "):
            return Type("any", name)
        decl = self.classes.get(name)
        if decl is None:
            raise SyntaxProblem(f"未声明的类型 {name}")
        return Type("view" if decl.is_view else "cls", name)

    def type_in(self, fun, name):
        """在 fun 的签名语境里解释类型名：类型参数是不透明类型，fn S 是合同为 S 的函数类型。"""
        if name.startswith("fn "):
            if name[3:] not in self.sigs:
                raise SyntaxProblem(f"未声明的签名 {name[3:]}")
            return Type("fn", name[3:])
        return Type("opaque", name) if name in fun.tparams else self.type_of(name)

    # ---- 签名

    def check_signature(self, fun):
        """来源与流向声明须指向参数；字段形式须是视图的借用字段；流向的目标须是 inout 参数。"""
        params = {name: (mode, ty) for mode, name, ty in fun.params}
        for param, fld in fun.sources + [src for src, _ in fun.flows]:
            if param not in params:
                self.error("SIG", fun.line, f"{fun.name}：来源指向不存在的参数 {param}")
                continue
            mode, ty = params[param]
            if fld is None:
                if fun.result[0] == "inout" and mode == "borrow" and (param, fld) in fun.sources:
                    self.error("SIG", fun.line, f"{fun.name}：inout 结果不能依赖 borrow 参数 {param}")
            elif fld != "*":
                decl = self.classes.get(ty)
                if decl is None or not decl.is_view or not (decl.field(fld) or ("",))[0]:
                    self.error("SIG", fun.line, f"{fun.name}：{param}.{fld} 不是视图的借用字段")
        for _, target in fun.flows:
            if params.get(target, ("",))[0] != "inout":
                self.error("SIG", fun.line, f"{fun.name}：流向的目标 {target} 须是 inout 参数")

    def result_exclusive(self, fun):
        kind, ty = fun.result
        # 不透明的结果可能被实例化为独占类型，按独占处理
        return kind == "inout" or ty in fun.tparams or (ty in self.classes and self.classes[ty].exclusive())

    # ---- 函数体

    def check_fun(self, fun):
        self.check_signature(fun)
        if fun.trusted:
            return
        self.fun = fun
        self.loans = {}
        self.version = 0
        self.pending = []
        self.live = set()
        state = {}
        for mode, name, ty_name in fun.params:
            ty = self.type_in(fun, ty_name)
            loans = {}
            if mode != "own":
                loans[""] = {self.symbolic(name, None, mode, fun.line)}
            if ty.kind == "view":
                for kind, fld, _ in self.classes[ty_name].ref_fields():
                    loans[fld] = {self.symbolic(name, fld, kind, fun.line)}
            elif ty.kind == "opaque":  # 类型参数的值携带一组未知的来源
                loans["*"] = {self.symbolic(name, "*", "inout", fun.line)}
            # 按 inout / borrow 接收存在类型，得到的是载荷访问（之三 §2.4）
            state[name] = Var(ty if mode == "own" else Type(mode, ty_name), "A", loans,
                              mode != "own" and ty.kind == "any")
        declared = set(state)
        self.check_unique(fun.body, declared, fun)
        live_in(fun.body, expr_uses(fun.ret) if fun.ret else set())
        state = self.run(fun.body, state)
        if fun.ret is not None:
            self.live = expr_uses(fun.ret)
            # 借用结果按“交出一个借用”求值：返回借用值即对它重借
            kind = fun.result[0]
            returned = self.argument(fun.ret, kind, state) if kind else self.value(fun.ret, state)
            self.check_contract(returned, fun)
        self.check_flows(state, fun)

    def check_unique(self, stmts, declared, fun):
        """演算不处理遮蔽：一个函数内每个名字只声明一次。"""
        for stmt in stmts:
            if stmt.kind == "val":
                if stmt.name in declared:
                    raise SyntaxProblem(f"第 {stmt.line} 行：{fun.name} 内重复声明 {stmt.name}")
                declared.add(stmt.name)
            self.check_unique(stmt.body, declared, fun)
            self.check_unique(stmt.orelse, declared, fun)

    def symbolic(self, param, fld, kind, line):
        """参数带来的符号 loan，代表调用方那边的来源。"""
        key = f"P({param}{'.' + fld if fld else ''})"
        root = f"*{param}{'.' + fld if fld else ''}"
        self.loans[key] = Loan("excl" if kind == "inout" else "shared", {(root, ())}, set(), line)
        return key

    def uncovered(self, loans, sources):
        """K6 的核心：列出 loans 中没有被 sources 覆盖的来源；落在局部存储上的记为 None。"""
        missing = []
        for loan in sorted(loans):
            if any(not root.startswith("*") for root, _ in self.loans[loan].places):
                missing.append(None)
                continue
            for ancestor in sorted(self.ancestors({loan}) | {loan}):
                if not ancestor.startswith("P("):
                    continue
                param, _, fld = ancestor[2:-1].partition(".")
                wanted = {(param, None)} | ({(param, "*"), (param, fld)} if fld else set())
                if not wanted & sources:
                    missing.append(ancestor[2:-1])
        return missing

    def check_contract(self, value, fun):
        """K6：返回值持有的每个 loan 都要被签名的 from 覆盖。"""
        for name in self.uncovered(value.all_loans(), set(fun.sources)):
            self.error("K6", fun.ret.line, "返回值依赖局部变量的存储" if name is None
                       else f"返回值的来源 {name} 没有在 from 中声明")

    def check_flows(self, state, fun):
        """K6 对流向的延伸：函数体让哪些来源流进了 inout 参数，签名的 flows 就要声明哪些。"""
        for mode, name, _ in fun.params:
            var = state.get(name)
            if mode != "inout" or var is None or var.init == "M":
                continue
            extra = set(var.loans.get("*", ())) - {f"P({name}.*)"}
            declared = {src for src, target in fun.flows if target == name}
            for missing in self.uncovered(extra, declared):
                self.error("K6", fun.line, f"流入 {name} 的值依赖局部变量的存储" if missing is None
                           else f"流入 {name} 的来源 {missing} 没有在 flows 中声明")

    def refines(self, impl, sig, line):
        """K12：把函数当作合同为 sig 的值使用时，它声明的来源与流向都须被 sig 覆盖。"""
        if [m for m, _, _ in impl.params] != [m for m, _, _ in sig.params]:
            self.error("K12", line, f"{impl.name} 的参数模式与 {sig.name} 不符")
            return
        rename = {i[1]: s[1] for i, s in zip(impl.params, sig.params)}

        def covered(src, allowed):
            param, fld = rename.get(src[0], src[0]), src[1]
            wanted = {(param, None)} | ({(param, "*"), (param, fld)} if fld else set())
            return bool(wanted & allowed)

        for src in impl.sources:
            if not covered(src, set(sig.sources)):
                self.error("K12", line, f"{impl.name} 的来源 {src[0]}{'.' + src[1] if src[1] else ''} 超出 {sig.name} 的声明")
        for src, target in impl.flows:
            allowed = {s for s, t in sig.flows if t == rename.get(target, target)}
            if not covered(src, allowed):
                self.error("K12", line, f"{impl.name} 的流向超出 {sig.name} 的声明")

    # ---- loan 的集合运算

    def places(self, loans):
        return set().union(*(self.loans[l].places for l in loans)) if loans else set()

    def ancestors(self, loans):
        seen, todo = set(), list(loans)
        while todo:
            for parent in self.loans[todo.pop()].parents:
                if parent not in seen:
                    seen.add(parent)
                    todo.append(parent)
        return seen

    def live_loans(self, state):
        """K3：loan 存活当且仅当某个存活的持有者持有它。

        实现原型时补充的一条：存活 loan 所指的存储若是某个持有者变量，该变量随之存活，
        否则“依赖视图本身”的结果无法保住视图所持有的来源。
        """
        names = {n for n in self.live if n in state}
        loans = set().union(*self.pending) if self.pending else set()
        while True:
            size = (len(names), len(loans))
            for name in names:
                held = state[name].loans
                loans |= set().union(*held.values()) if held else set()
            for loan in loans:
                names |= {root for root, _ in self.loans[loan].places if root in state}
            if size == (len(names), len(loans)):
                return loans

    def access(self, state, targets, via, kind, line):
        """K1 与 K4：对 targets 做一次访问，与存活 loan 逐个比对。

        via 是这次访问所凭借的 loan；它们及其祖先不算冲突。冲突的 loan 若是 via 的后代，
        说明父持有者正被子借用暂停，记为 K4；作用域结束时的析构冲突记为 K9；其余记为 K1。
        """
        skip = set(via) | self.ancestors(via)
        for loan in sorted(self.live_loans(state) - skip):
            info = self.loans[loan]
            if info.kind == "shared" and kind in ("read", "shared"):
                continue
            if not any(r == r2 and prefix_related(p, p2) for r, p in targets for r2, p2 in info.places):
                continue
            rule = "K4" if via & self.ancestors({loan}) else ("K9" if kind == "kill" else "K1")
            self.error(rule, line, f"{kind} 与第 {info.line} 行建立的 {info.kind} loan 冲突")

    def new_loan(self, site, kind, targets, parents, line):
        """K2：loan 以建立它的语法位置为身份，循环中重复执行同一位置得到同一个 loan。"""
        loan = self.loans.get(site)
        if loan is None:
            self.loans[site] = Loan(kind, set(targets), set(parents), line)
            self.version += 1
        elif not (targets <= loan.places and parents <= loan.parents):
            loan.places |= targets
            loan.parents |= parents
            self.version += 1
        return site

    # ---- place

    def resolve(self, state, place):
        """把 place 解析为它所指的存储、凭借的 loan 与可写性。

        没有字段时指变量自身的存储；有字段时先经借用值或视图的借用字段走到所指的存储。
        越过 borrow 字段后，凭借的 loan 只剩该字段的；越过 inout 字段则累加，
        所以独占视图交出的结果必然还依赖视图本身（之二 R4.5）。
        """
        var = state.get(place.root)
        if var is None:
            raise SyntaxProblem(f"第 {place.line} 行：未声明的变量 {place.root}")
        if var.init != "A":
            self.error("K8", place.line, f"{place.root} 已被移动或可能已被移动")
        here = {(place.root, ())}
        if not place.fields:
            return Resolved(here, set(), var.ty, True, var.loans, "")
        targets, via, ty, writable, field_loans = here, set(), var.ty, True, var.loans
        if var.ty.kind in ("borrow", "inout"):
            self.access(state, here, set(), "read", place.line)
            via = set(var.loans.get("", ()))
            targets, ty, writable = self.places(via), self.type_of(var.ty.name), var.ty.kind == "inout"
        crossed = ""
        for name in place.fields:
            decl = self.classes.get(ty.name)
            fld = decl.field(name) if decl else None
            if fld is None:
                raise SyntaxProblem(f"第 {place.line} 行：{ty.name} 没有字段 {name}")
            kind, _, fty = fld
            crossed = kind
            if not kind:
                targets = {(root, path + (name,)) for root, path in targets}
            else:
                self.access(state, targets, via, "read", place.line)
                held = set((field_loans or {}).get(name, ()))
                via = held if kind == "borrow" else via | held
                targets = self.places(held)
                writable = writable and kind == "inout"
                field_loans = None
            ty = self.type_of(fty)
            if not kind and ty.kind != "view":
                field_loans = None
        return Resolved(targets, via, ty, writable, field_loans, crossed)

    # ---- 表达式

    def value(self, expr, state):
        """在按值位置求值：得到一个值及其持有的 loan。"""
        if expr.kind == "int":
            return Value(INT)
        if expr.kind == "move":
            return self.move(expr, state)
        if expr.kind == "ref":
            return self.borrow(expr, expr.place, expr.mut, state)
        if expr.kind == "call":
            return self.construct(expr, state) if expr.name in self.classes else self.call(expr, state)
        if expr.kind == "pack":
            inner = self.value(expr.args[0], state)
            decl = self.classes.get(inner.ty.name)
            if decl is None or expr.name not in decl.implements:
                self.error("TYPE", expr.line, f"{inner.ty.name} 没有实现 {expr.name}")
            return Value(Type("any", f"any {expr.name}"), inner.loans)
        place = expr.place
        res = self.resolve(state, place)
        if res.crossed:  # 读出视图的借用字段：复制这份借用
            if res.crossed == "inout":
                self.error("K8", expr.line, "独占字段不能按值复制，须重借")
            return Value(Type("borrow", res.ty.name), {"": set(res.via)}, res.ty.kind == "any")
        self.access(state, res.targets, res.via, "read", expr.line)
        if res.ty.kind == "int":
            return Value(INT)
        movable_only = res.ty.kind in ("cls", "inout", "opaque", "any") or (
            res.ty.kind == "view" and self.classes[res.ty.name].exclusive())
        if movable_only or place.fields:
            self.error("K8", expr.line, f"{place.root} 不可隐式复制，须写 move")
        return Value(res.ty, copy_loans(res.field_loans or {}))

    def move(self, expr, state):
        """K8：移动整个变量；移动是一次与任何存活 loan 都不相容的访问。"""
        place = expr.place
        if place.fields:
            raise SyntaxProblem(f"第 {expr.line} 行：演算不支持部分移动")
        res = self.resolve(state, place)
        self.access(state, res.targets, set(), "move", expr.line)
        var = state[place.root]
        value = Value(var.ty, copy_loans(var.loans), var.payload)
        var.init, var.loans = "M", {}
        return value

    def borrow(self, expr, place, mut, state, stored_kind=None):
        """K2：在 place 上建立 loan；经持有者到达时，新 loan 是其 loan 的子 loan。"""
        res = self.resolve(state, place)
        if mut and not res.writable:
            self.error("K1", expr.line, "不能经共享借用建立独占借用")
        self.access(state, res.targets, res.via, "excl" if mut else "shared", expr.line)
        kind = stored_kind or ("excl" if mut else "shared")
        loan = self.new_loan(f"L{expr.site}", kind, res.targets, res.via, expr.line)
        loans = {"": {loan}}
        if res.ty.kind in ("view", "opaque"):
            loans.update(copy_loans({k: v for k, v in (res.field_loans or {}).items() if k}))
        # 视图里存放的存在类型借用是载荷访问，经它重借出来的仍是
        payload = bool(res.crossed) and res.ty.kind == "any"
        return Value(Type("inout" if mut else "borrow", res.ty.name), loans, payload)

    def argument(self, expr, mode, state, stored_kind=None):
        """按参数模式求实参：borrow 与 inout 位置上建立（或重借出）一个 loan。"""
        if mode == "own":
            return self.value(expr, state)
        mut = mode == "inout"
        if expr.kind == "ref":
            if expr.mut != mut:
                self.error("TYPE", expr.line, f"{mode} 位置的实参写法不符")
            return self.borrow(expr, expr.place, mut, state, stored_kind)
        if expr.kind in ("call", "move"):  # 调用结果或移入的借用值本身就是持有者
            return self.value(expr, state)
        if expr.kind != "place":
            self.error("TYPE", expr.line, f"{mode} 位置需要 & 表达式或一个借用值")
            return Value(Type(mode, "Int"), {"": set()})
        var = state.get(expr.place.root)
        if not expr.place.fields and var is not None and var.ty.kind in ("borrow", "inout"):
            # 直接交出一个借用值：对它所指的存储重借
            res = self.resolve(state, expr.place)
            self.access(state, res.targets, set(), "read", expr.line)
            via = set(var.loans.get("", ()))
            if mut and var.ty.kind != "inout":
                self.error("K1", expr.line, "不能把共享借用交给 inout 位置")
            targets = self.places(via)
            self.access(state, targets, via, "excl" if mut else "shared", expr.line)
            kind = stored_kind or ("excl" if mut else "shared")
            loan = self.new_loan(f"L{expr.site}", kind, targets, via, expr.line)
            loans = {"": {loan}}
            loans.update(copy_loans({k: v for k, v in var.loans.items() if k}))
            return Value(Type(mode, var.ty.name), loans, var.payload)
        res = self.resolve(state, expr.place)
        if not res.crossed:
            self.error("TYPE", expr.line, f"{mode} 位置需要 & 表达式或一个借用值")
            return Value(Type(mode, res.ty.name), {"": set()})
        if mut and not res.writable:
            self.error("K1", expr.line, "不能经共享借用建立独占借用")
        self.access(state, res.targets, res.via, "excl" if mut else "shared", expr.line)
        kind = stored_kind or ("excl" if mut else "shared")
        return Value(Type(mode, res.ty.name),
                     {"": {self.new_loan(f"L{expr.site}", kind, res.targets, res.via, expr.line)}})

    def construct(self, expr, state):
        """K7：视图实例在每个借用字段上持有实参处建立的 loan。"""
        decl = self.classes[expr.name]
        if len(expr.args) != len(decl.fields):
            raise SyntaxProblem(f"第 {expr.line} 行：{expr.name} 的实参个数不符")
        loans, pushed = {}, 0
        for (kind, name, _), arg in zip(decl.fields, expr.args):
            val = self.argument(arg, kind or "own", state)
            if kind:
                loans[name] = set(val.loans.get("", ()))
            self.pending.append(val.all_loans())
            pushed += 1
        del self.pending[len(self.pending) - pushed:]
        return Value(self.type_of(expr.name), loans)

    def bind_types(self, fun, values, line):
        """X2：inout 位置对所指类型不变，载荷访问不能充当槽位。

        形参写作 any I 时，实参所指可以是实现 I 的具体类型或 any I，得到的是载荷访问；
        形参是具体类型或类型参数时，实参须是所指类型恰为该类型的槽位，类型参数在各处的实例须一致。
        """
        bound = {}
        for mode, name, wanted in fun.params:
            value = values[name]
            if value.ty.kind == "fn":
                continue
            actual = value.ty.name
            if wanted in fun.tparams:
                if mode == "inout" and value.payload:
                    self.error("X2", line, f"{name}：载荷访问不能交给泛型的 inout 位置")
                elif bound.setdefault(wanted, actual) != actual:
                    self.error("X2", line, f"类型参数 {wanted} 被同时实例化为 {bound[wanted]} 与 {actual}")
            elif wanted.startswith("any "):
                decl = self.classes.get(actual)
                if actual != wanted and (decl is None or wanted[4:] not in decl.implements):
                    self.error("TYPE", line, f"{name}：{actual} 不能当作 {wanted}")
            elif mode == "inout" and value.payload:
                self.error("X2", line, f"{name}：载荷访问不能充当 {wanted} 的槽位")

    def carried(self, value, mode, fld):
        """按来源的三种形式取出实参带来的 loan。"""
        if fld is None:  # 依赖参数本身；按值参数没有“本身”，等同于转发它携带的全部来源
            return value.all_loans() if mode == "own" else set(value.loans.get("", ()))
        if fld == "*":
            if mode == "own":
                return value.all_loans()
            return set().union(*(held for key, held in value.loans.items() if key))
        return set(value.loans.get(fld, ()))

    def callable_argument(self, arg, sig_name, state):
        """函数类型的实参：只接受具名函数或另一个函数类型的参数，并按 K12 核对合同。"""
        name = arg.place.root if arg.kind == "place" and not arg.place.fields else None
        var = state.get(name)
        impl = self.sigs[var.ty.name] if var is not None and var.ty.kind == "fn" else self.funs.get(name)
        if impl is None:
            raise SyntaxProblem(f"第 {arg.line} 行：函数类型的实参须是函数名")
        self.refines(impl, self.sigs[sig_name], arg.line)
        return Value(Type("fn", sig_name))

    def call(self, expr, state):
        """K5：结果的 loan 完全由被调方的签名决定，不读它的函数体。"""
        var = state.get(expr.name)
        fun = self.sigs[var.ty.name] if var is not None and var.ty.kind == "fn" else self.funs.get(expr.name)
        if fun is None or len(fun.params) != len(expr.args):
            raise SyntaxProblem(f"第 {expr.line} 行：调用 {expr.name} 的目标或实参个数不符")
        modes = {name: mode for mode, name, _ in fun.params}
        sources = fun.sources + [src for src, _ in fun.flows]
        depends = {param for param, fld in sources if fld is None and modes.get(param) != "own"}
        flowed = "excl" if self.result_exclusive(fun) or fun.flows else "shared"
        values, pushed = {}, 0
        for (mode, name, ty_name), arg in zip(fun.params, expr.args):
            if ty_name.startswith("fn "):
                values[name] = self.callable_argument(arg, ty_name[3:], state)
                continue
            values[name] = self.argument(arg, mode, state, flowed if name in depends else None)
            self.pending.append(values[name].all_loans())
            pushed += 1
        del self.pending[len(self.pending) - pushed:]
        self.bind_types(fun, values, expr.line)
        for (param, fld), target in fun.flows:  # 来源流入已有的持有者
            arg = expr.args[[name for _, name, _ in fun.params].index(target)]
            holder = arg.place.root if arg.kind in ("ref", "place") and not arg.place.fields else None
            own_param = holder in {name for _, name, _ in self.fun.params}
            if holder is None or (arg.kind == "place" and not own_param):
                raise SyntaxProblem(f"第 {expr.line} 行：流向的目标须是 &mut 变量，或本函数的 inout 参数")
            state[holder].loans.setdefault("*", set()).update(self.carried(values[param], modes[param], fld))
        result = set()
        for param, fld in fun.sources:
            if param in values:
                result |= self.carried(values[param], modes[param], fld)
        kind, ty_name = fun.result
        if kind:
            return Value(Type(kind, ty_name), {"": result}, ty_name.startswith("any "))
        ty = self.type_in(fun, ty_name)
        if ty.kind == "opaque":
            return Value(ty, {"*": result})
        if ty.kind == "view":  # K11：含多个借用字段的结果共同持有这些 loan
            return Value(ty, {f[1]: set(result) for f in self.classes[ty_name].ref_fields()})
        return Value(ty)

    # ---- 语句

    def run(self, stmts, state):
        for stmt in stmts:
            state = self.step(stmt, state)
        return state

    def step(self, stmt, state):
        self.live = (set(stmt.live_out) - stmt_defs(stmt)) | stmt_uses(stmt)
        if stmt.kind == "val":
            value = self.value(stmt.expr, state)
            state[stmt.name] = Var(value.ty, "A", value.loans, value.payload)
        elif stmt.kind == "store":
            self.store(stmt, state)
        elif stmt.kind == "call":
            self.value(stmt.expr, state)
        elif stmt.kind == "use":
            res = self.resolve(state, stmt.place)
            self.access(state, res.targets, res.via, "read", stmt.line)
            if not stmt.place.fields and res.ty.kind in ("borrow", "inout", "opaque"):
                key = "*" if res.ty.kind == "opaque" else ""
                via = set(state[stmt.place.root].loans.get(key, ()))
                self.access(state, self.places(via), via, "read", stmt.line)
        elif stmt.kind == "assign":
            self.assign(stmt, state)
        elif stmt.kind == "block":
            state = self.scope(stmt.body, state, stmt.live_out, stmt.end_line)
        elif stmt.kind == "if":
            then = self.scope(stmt.body, copy_state(state), stmt.live_out, stmt.end_line)
            other = self.scope(stmt.orelse, copy_state(state), stmt.live_out, stmt.else_end_line)
            state = self.join(then, other)
        elif stmt.kind == "while":
            head = set(stmt.live_out)
            while True:  # 循环入口处的活变量；最后一次调用把循环体内各语句的出口活性重新记好
                grown = head | live_in(stmt.body, head)
                if grown == head:
                    break
                head = grown
            while True:  # K10：迭代到不动点
                before = self.version
                after = self.scope(stmt.body, copy_state(state), head, stmt.end_line)
                merged = self.join(state, after)
                if merged == state and before == self.version:
                    break
                state = merged
        return state

    def store(self, stmt, state):
        """X1：经独占借用整体替换所指的值，要求槽位访问且新值的类型恰为所指类型。"""
        value = self.value(stmt.expr, state)
        var = state.get(stmt.name)
        if var is None or var.ty.kind != "inout":
            raise SyntaxProblem(f"第 {stmt.line} 行：*{stmt.name} 须是独占借用")
        self.resolve(state, Place(stmt.name, [], stmt.line))
        if var.payload:
            self.error("X1", stmt.line, f"{stmt.name} 是载荷访问，不能整体替换")
        elif value.ty.name != var.ty.name:
            self.error("X1", stmt.line, f"不能把 {value.ty.name} 写入 {var.ty.name} 的槽位")
        via = set(var.loans.get("", ()))
        self.access(state, self.places(via), via, "write", stmt.line)

    def assign(self, stmt, state):
        value = self.value(stmt.expr, state)
        place = stmt.place
        if place.fields:
            res = self.resolve(state, place)
            if not res.writable:
                self.error("K1", stmt.line, "不能经共享借用写入")
            self.access(state, res.targets, res.via, "write", stmt.line)
            return
        var = state.get(place.root)
        if var is None:
            raise SyntaxProblem(f"第 {stmt.line} 行：未声明的变量 {place.root}")
        # 旧值正被覆盖，它持有的 loan 此刻不再算存活；新值在赋值之后仍被使用时算存活
        self.live = set(stmt.live_out) - {place.root}
        if place.root in stmt.live_out:
            self.pending.append(value.all_loans())
        self.access(state, {(place.root, ())}, set(), "write", stmt.line)
        if place.root in stmt.live_out:
            self.pending.pop()
        if var.ty.kind == "any" and value.ty.name != var.ty.name:  # X3：槽位里只能放同一存在类型的值
            self.error("X1", stmt.line, f"不能把 {value.ty.name} 赋给 {var.ty.name}")
        var.init, var.loans, var.payload = "A", value.loans, value.payload

    def scope(self, stmts, state, live_after, end_line):
        """K9：作用域结束时按声明逆序析构；析构与仍存活的 loan 冲突即拒绝。"""
        outer = set(state)
        state = self.run(stmts, state)
        self.live = set(live_after)
        for name in reversed([n for n in state if n not in outer]):
            if state[name].init != "M":
                self.access(state, {(name, ())}, set(), "kill", end_line)
            del state[name]
        return state

    def join(self, a, b):
        """K10：合流处 loan 取并集；任一路径上已移动即为可能已移动。"""
        merged = {}
        for name in a:
            if name not in b:
                continue
            x, y = a[name], b[name]
            loans = copy_loans(x.loans)
            for key, held in y.loans.items():
                loans.setdefault(key, set()).update(held)
            merged[name] = Var(x.ty, x.init if x.init == y.init else "MM", loans, x.payload)
        return merged


def check(text):
    """检查一段演算程序，返回按行排序的 (规则, 行号, 说明)。空列表表示接受。"""
    classes, funs, sigs = Parser(text).program()
    checker = Checker(classes, funs, sigs)
    for fun in funs.values():
        checker.check_fun(fun)
    return sorted(checker.errors, key=lambda e: (e[1], e[0], e[2]))


def main(argv):
    status = 0
    for path in argv[1:]:
        errors = check(Path(path).read_text(encoding="utf-8"))
        print(f"{path}: {'接受' if not errors else '拒绝'}")
        for rule, line, message in errors:
            print(f"  {rule} 第 {line} 行：{message}")
            status = 1
    return status


if __name__ == "__main__":
    sys.exit(main(sys.argv))
