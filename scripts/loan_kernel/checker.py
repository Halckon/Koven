"""借用内核演算的参考检查器原型。

非规范、不被编译器引用。演算的语法、规则 K1–K11 与用例清单见
docs/proposals/loan-kernel-calculus.md；本文件只把那页的规则写成可执行形式。
检查器基于用例，不是健全性证明。
"""
from __future__ import annotations

import importlib.util
import re
import sys
from collections import namedtuple
import functools
from dataclasses import dataclass, field
from pathlib import Path


def sibling(name):
    """加载同目录下的另一个原型；各原型都是独立脚本，不组成包。"""
    spec = importlib.util.spec_from_file_location(f"loan_kernel_{name}", Path(__file__).with_name(f"{name}.py"))
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


CAP = sibling("capability")  # 结构能力推导（KB4）：可复制、可转移、可逃逸、可提前析构都向它问
DP = sibling("drop_plan")  # 析构义务（KB1）：本检查器定下的析构交给它的检验器独立检验

Type = namedtuple("Type", "kind name")  # kind: int | unit | cls | view | borrow | inout
INT = Type("int", "Int")
UNIT = Type("unit", "Unit")


class SyntaxProblem(Exception):
    """演算程序本身不合文法；与借用规则无关。"""


class InvalidOpen(Exception):
    """非法拆解没有可供后续语句使用的字段绑定，停止检查当前函数。"""


# ---------------------------------------------------------------- 语法树

@dataclass
class ClassDecl:
    name: str
    fields: list  # (kind, name, type_name)；kind 为 "" | "borrow" | "inout"
    is_view: bool
    deinit: bool
    line: int
    implements: list = field(default_factory=list)
    private: set = field(default_factory=set)  # 只有声明为该类型成员的函数可以访问的字段
    cell: bool = False  # 内部可变：内容只能经受信任函数访问，共享借用下也可能被改写
    local: bool = False  # 绑定在创建它的线程上，不可转移（例如非原子的共享计数）
    carrier: bool = False  # 写作 view class C<view T>：泛型容器。演算不写类型实参，它一律代表以携带来源的类型
    #                        实例化的 C<T>；内容只经受信任函数访问，携带的来源整体记在值名下

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
    flows: list = field(default_factory=list)  # ((param, None | "*" | field), (目标参数, None | 字段路径))
    owner: str = ""  # in C：该函数是类型 C 的成员，可以访问 C 的私有字段
    views: set = field(default_factory=set)  # 写作 view T 的类型参数：可以实例化为携带来源的类型
    suspend: bool = False  # 可以包含挂起点
    parallel: bool = False  # 可能在另一个线程上恢复：跨挂起点存活的值须可转移
    bounds: dict = field(default_factory=dict)  # 类型参数 -> 它声明的能力上界（T: Cap + Cap）
    deinit_of: str = ""  # deinit C { … }：类型 C 的析构函数体，this 拥有各字段


@dataclass
class Place:
    root: str
    fields: list
    line: int


@dataclass
class Expr:
    kind: str  # int | place | move | ref | call | pack | start
    line: int
    site: int
    place: Place = None
    mut: bool = False
    name: str = ""
    args: list = field(default_factory=list)


@dataclass
class Stmt:
    kind: str  # val | assign | store | use | call | block | if | while | suspend | poll | await
    line: int
    name: str = ""
    place: Place = None
    expr: Expr = None
    body: list = field(default_factory=list)
    orelse: list = field(default_factory=list)
    end_line: int = 0
    else_end_line: int = 0
    live_out: frozenset = frozenset()
    names: list = field(default_factory=list)  # open 引入的新绑定，按字段声明顺序


# ---------------------------------------------------------------- 解析

TOKEN = re.compile(r"\s+|//[^\n]*|[A-Za-z_]\w*|\d+|[(){}:,.=&*<>+]")


def tokenize(text):
    """切分为 (文本, 行号)；空白与注释丢弃。"""
    tokens, pos, line = [], 0, 1
    for match in TOKEN.finditer(text):
        if match.start() != pos:
            break
        tok = match.group()
        if tok[0] in " \t\r\n":
            line += tok.count("\n")
        elif not tok.startswith("//"):
            tokens.append((tok, line))
        pos = match.end()
    if pos < len(text):
        raise SyntaxProblem(f"第 {line} 行：无法识别的字符 {text[pos]!r}")
    return tokens


@functools.lru_cache(maxsize=None)
def parse(text):
    """同一段程序只解析一次。检查器不改写语法树（活性标注每次重算，结果只由语法树决定），所以可以共用。"""
    return Parser(text).program()


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
            elif self.peek() == "deinit":  # deinit C { … }：C 的析构函数体
                self.next()
                name = self.next()
                body, _ = self.block()
                funs[f"deinit {name}"] = Fun(f"deinit {name}", [("own", "this", name)], ("", "Unit"), [], False, body,
                                             None, line, owner=name, deinit_of=name)
            elif self.peek() in ("fun", "sig", "suspend", "parallel"):
                mods = set()
                while self.peek() in ("suspend", "parallel"):
                    mods.add(self.next())
                table = funs if self.peek() == "fun" else sigs
                fun = self.fun(self.peek())
                fun.suspend, fun.parallel = bool(mods), "parallel" in mods
                table[fun.name] = fun
            else:
                is_local = self.accept("local")
                is_cell = self.accept("cell")
                is_view = self.accept("view")
                self.expect("class")
                name = self.next()
                is_carrier = self.accept("<")
                if is_carrier:
                    self.expect("view")
                    self.next()
                    self.expect(">")
                self.expect("(")
                fields, private = [], set()
                while not self.accept(")"):
                    hidden = self.accept("private")
                    kind = self.next() if self.peek() in ("borrow", "inout") else ""
                    fname = self.next()
                    if hidden:
                        private.add(fname)
                    self.expect(":")
                    fields.append((kind, fname, self.type_name()))
                    self.accept(",")
                implements = []
                while self.accept("is"):
                    implements.append(self.next())
                has_deinit = self.peek() == "deinit" and self.peek(2) != "{" and self.accept("deinit")
                classes[name] = ClassDecl(name, fields, is_view, has_deinit, line, implements, private, is_cell,
                                          is_local, is_carrier)
        return classes, funs, sigs

    def type_name(self):
        """类型名；fn N 是函数类型，any I 是接口 I 的存在类型。"""
        tok = self.next()
        return f"{tok} {self.next()}" if tok in ("fn", "any") else tok

    def source(self):
        """来源：x、x.f、x.f.g（多级字段路径）或 x.origins（x 携带的全部来源）。"""
        param = self.next()
        path = []
        while self.accept("."):
            path.append(self.next())
        if path == ["origins"]:
            return (param, "*")
        return (param, ".".join(path) or None)

    def fun(self, keyword):
        """函数或抽象签名（sig）；sig 没有函数体，用作函数类型与接口方法的合同。"""
        line = self.line()
        self.expect(keyword)
        tparams, views, bounds = [], set(), {}
        if self.accept("<"):
            while not self.accept(">"):
                may_carry = self.accept("view")
                tparams.append(self.next())
                if may_carry:
                    views.add(tparams[-1])
                if self.accept(":"):
                    bounds[tparams[-1]] = {self.next()}
                    while self.accept("+"):
                        bounds[tparams[-1]].add(self.next())
                self.accept(",")
        name = self.next()
        self.expect("(")
        params = []
        while not self.accept(")"):
            mode = self.next()
            pname = self.next()
            if any(name == pname for _, name, _ in params):
                raise SyntaxProblem(f"第 {self.line()} 行：重复形参 {pname}")
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
                flows.append((src, self.source()))
                if not self.accept(","):
                    break
        owner = self.next() if self.accept("in") else ""
        trusted = self.accept("trusted") or keyword == "sig"
        body, ret = [], None
        if not trusted:
            self.expect("{")
            while self.peek() not in ("return", "}"):
                body.append(self.stmt())
            if self.accept("return"):
                ret = self.expr()
            self.expect("}")
        return Fun(name, params, result, sources, trusted, body, ret, line, tparams, flows, owner, views,
                   bounds=bounds)

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
        if tok == "open" and self.peek(2) == "as":
            self.next()
            source = self.next()
            self.expect("as")
            names = [self.next()]
            while self.accept(","):
                names.append(self.next())
            if not all(re.fullmatch(r"[A-Za-z_]\w*", n) for n in names):
                raise SyntaxProblem(f"第 {line} 行：open 需要字段绑定名")
            return Stmt("open", line, name=source, names=names)
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
        if tok == "suspend":  # 一个挂起点
            self.next()
            return Stmt("suspend", line)
        if tok in ("poll", "await") and self.peek(1) != "(":  # 推进一个任务但不等它结束；等它结束并消费它
            self.next()
            return Stmt(tok, line, name=self.next())
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
        if tok == "start":  # 创建一个任务：实参已求值并存进它的帧，函数体尚未开始执行
            self.next()
            inner = self.expr()
            return Expr("start", line, site, name=inner.name, args=inner.args)
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
    if stmt.kind in ("store", "poll", "await", "open"):
        uses.add(stmt.name)
    return uses


def stmt_defs(stmt):
    if stmt.kind == "open":
        return set(stmt.names)
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
    pinned: bool = False  # 任务已开始执行：帧不可再移动，离开作用域前须等它结束
    gone: dict = field(default_factory=dict)  # 已移出的按值字段：(字段名,) -> M | MM


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
    return {name: Var(var.ty, var.init, copy_loans(var.loans), var.payload, var.pinned, dict(var.gone))
            for name, var in state.items()}


def prefix_related(a, b):
    shorter = min(len(a), len(b))
    return a[:shorter] == b[:shorter]


# ---------------------------------------------------------------- 检查器

class Checker:
    drop_before_suspend = True  # S6；置为 False 仅用于对照

    def __init__(self, classes, funs, sigs, early_drop=False, frames_movable=False):
        self.classes, self.funs, self.sigs = classes, funs, sigs
        self.frames_movable = frames_movable  # 对照用：假设开始执行后的帧仍可移动
        self.frames = {}  # (函数名, 行号) -> (跨这个挂起点留在帧里的变量, 其中被帧内借用指向的变量)
        self.errors = set()
        self.early_drop = early_drop  # 析构不可观察的局部值在不再需要时提前析构（统一析构规则的“许可”）
        self.drops = []  # 提前析构的 (函数名, 变量名, 其后的语句行号)
        self.out = None  # 正在生成的析构义务程序（DP.Stmt 列表）
        self.quiet = False  # 为真时访问不记为使用（整体赋值的目标）
        self.lowered = {}  # 函数名 -> (类型表, 带显式析构的程序, 是否被接受)
        self.caps = CAP.Engine({name: self.cap_decl(decl) for name, decl in classes.items()})
        for line, message in self.caps.check_declarations():
            self.error("CAP", line, message)

    # ---- 能力（KB4）：演算的声明翻译成能力推导的声明，各处的判定都向推导器问

    def cap_ty(self, name):
        if name.startswith("any "):
            return CAP.Ty("Any")
        if self.carrier(name):
            return CAP.Ty(name, (CAP.Ty("inout", (CAP.Ty("Unit"),)),))  # 实参取一个携带来源、不可复制的代表
        return CAP.Ty("Unit" if name.startswith("fn ") else name)

    def carrier(self, name):
        return name in self.classes and self.classes[name].carrier

    def cap_decl(self, decl):
        """内部可变即含有一个 RawCell；绑定在线程上即含有一个非原子的共享计数。两者都不另设标记。"""
        fields = [(name, CAP.Ty(kind, (self.cap_ty(ty),)) if kind else self.cap_ty(ty)) for kind, name, ty in decl.fields]
        if decl.cell:
            fields.append(("cell", CAP.Ty("RawCell", (CAP.Ty("Unit"),))))
        if decl.local:
            fields.append(("count", CAP.Ty("Rc", (CAP.Ty("Unit"),))))
        params = ["T"] if decl.carrier else []
        if decl.carrier:  # 容器按值含有它的元素
            fields.append(("item", CAP.Ty("T")))
        return CAP.Decl(decl.name, params, fields, "view" if decl.is_view else "class", deinit=decl.deinit,
                        bounds={p: set() for p in params}, relaxed={p: {"Escapable"} for p in params}, line=decl.line)

    def has(self, cap, ty):
        """类型 ty 的值是否满足能力 cap。类型参数只凭它声明的上界；任务一律不满足。"""
        if ty.kind in ("int", "unit", "fn"):
            return True
        if ty.kind == "task":
            return False
        if ty.kind == "opaque" or ty.name in self.fun.tparams:
            if ty.kind == "borrow" and cap == "Transferable":  # 共享借用可转移，当且仅当所指可共享
                cap = "Shareable"
            return ty.kind != "inout" and cap in self.fun.bounds.get(ty.name, ())
        inner = self.cap_ty(ty.name)
        asked = CAP.Ty(ty.kind, (inner,)) if ty.kind in ("borrow", "inout") else inner
        return self.caps.requires(asked, cap) is not CAP.NEVER

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
        return Type("view" if decl.is_view and not decl.carrier else "cls", name)

    def type_in(self, fun, name):
        """在 fun 的签名语境里解释类型名：类型参数是不透明类型，fn S 是合同为 S 的函数类型。"""
        if name.startswith("fn "):
            if name[3:] not in self.sigs:
                raise SyntaxProblem(f"未声明的签名 {name[3:]}")
            return Type("fn", name[3:])
        return Type("opaque", name) if name in fun.tparams else self.type_of(name)

    # ---- 析构义务（KB1）：把每个函数写成只含所有权操作、析构显式的程序

    @staticmethod
    def tracked(var):
        """名下有析构义务的变量：拥有的值。借用值、整数、函数值与任务不在内。"""
        return var is not None and var.ty.kind in ("cls", "view", "any", "opaque")

    def emit(self, kind, line, name="", state=None, **tags):
        if self.out is None:
            return
        stmt = DP.Stmt(kind, line, var=name, **tags)
        if state is not None:  # 可能失败的操作：失败时析构此刻仍拥有的变量，自内向外、声明逆序
            stmt.cleanup = [(n, self.flagged(v)) for n, v in reversed(list(state.items())) if self.owes(v)]
        self.out.append(stmt)

    def owes(self, var):
        """变量名下是否还有析构义务：拥有的值，没有被整体移走，字段也没有全部移出。"""
        decl = self.classes.get(var.ty.name)
        emptied = decl is not None and decl.fields and all(var.gone.get((f,)) == "M" for _, f, _ in decl.fields)
        return self.tracked(var) and var.init != "M" and not emptied

    @staticmethod
    def flagged(var):
        """析构时须看运行时的标志：整体或某个字段只在部分路径上被移走。"""
        return var.init == "MM" or "MM" in var.gone.values()

    def dp_type(self, ty):
        """析构义务一侧的类型。普通的类与视图带着字段，可以逐字段移出；其余只有“析构是否可观察”一个标记。"""
        decl = self.classes.get(ty.name)
        if ty.kind in ("cls", "view") and decl is not None and not (decl.cell or decl.local or decl.carrier):
            fields = []
            for kind, name, fty in decl.fields:
                plain = kind or fty in ("Int", "Unit") or fty.startswith("fn ")  # 没有析构义务的字段
                fields.append((name, "&" if plain else self.dp_type(self.type_of(fty))))
            self.dp_types.setdefault("&", DP.TypeDecl("&", []))
            self.dp_types[ty.name] = DP.TypeDecl(ty.name, fields, deinit=decl.deinit)
        else:  # 析构是否可观察取自能力推导
            self.dp_types[ty.name] = DP.TypeDecl(ty.name, [], deinit=not self.has("EarlyDrop", ty))
        return ty.name

    def declare_this(self, line, ty):
        """deinit 函数体里的 this：拥有各字段，deinit 这份义务正在履行，不再另算（析构义务原型 §6.2 的“拥有”）。"""
        decl = self.dp_types[self.dp_type(ty)]
        shadow = f"{ty.name}.fields"
        self.dp_types[shadow] = DP.TypeDecl(shadow, decl.fields)
        self.out.append(DP.Stmt("this", line, var="this", ty=shadow))

    def declare(self, kind, line, name, ty, state):
        """声明或重新赋值一个拥有的变量。"""
        self.emit(kind, line, name, state if kind == "let" else None, ty=self.dp_type(ty))
        if kind == "let":
            self.leave(state, "fail", line)

    def destroy(self, line, name, var, **tags):
        if self.owes(var):
            self.emit("drop", line, name, if_owned=self.flagged(var), **tags)

    # ---- 签名

    def check_signature(self, fun):
        """来源与流向声明须指向参数；字段形式须是视图的借用字段；流向的目标须是 inout 参数。"""
        params = {name: (mode, ty) for mode, name, ty in fun.params}
        named = fun.sources + [src for src, _ in fun.flows] + [target for _, target in fun.flows if target[1]]
        for param, fld in named:
            if param not in params:
                self.error("SIG", fun.line, f"{fun.name}：来源指向不存在的参数 {param}")
                continue
            mode, ty = params[param]
            if fld is None:
                if fun.result[0] == "inout" and mode == "borrow" and (param, fld) in fun.sources:
                    self.error("SIG", fun.line, f"{fun.name}：inout 结果不能依赖 borrow 参数 {param}")
            elif fld != "*":
                decl = self.classes.get(ty)
                paths = [path for _, path in self.ref_paths(ty)] if decl is not None and decl.is_view else []
                if not any(path == fld or path.startswith(fld + ".") for path in paths):
                    self.error("SIG", fun.line, f"{fun.name}：{param}.{fld} 之下没有视图的借用字段")
        kind, ty = fun.result
        if fun.sources and not kind and (ty in ("Int", "Unit") or ty in self.classes and self.escapable_in(fun, ty)):
            self.error("SIG", fun.line, f"{fun.name}：结果类型 {ty} 不能携带来源，不能声明 from")
        for _, (target, path) in fun.flows:
            mode, ty = params.get(target, ("", ""))
            cell = mode == "borrow" and ty in self.classes and self.classes[ty].cell
            # 按值的视图参数：流向只能经它的独占字段落到另一个视图里
            beyond = mode == "own" and ty in self.classes and self.classes[ty].is_view and any(
                self.within(p, path) and self.reaches_exclusively(ty, p.split(".")[:-1]) for _, p in self.ref_paths(ty))
            if ty in self.classes and not self.classes[ty].is_view or ty in fun.tparams and ty not in fun.views:
                self.error("SIG", fun.line, f"{fun.name}：流向的目标 {target} 的类型 {ty} 不能携带来源")
            if mode != "inout" and not cell and not beyond:  # K19：经共享借用流入，只对内部可变类型成立
                self.error("SIG", fun.line, f"{fun.name}：流向的目标 {target} 须是 inout 参数、内部可变类型的 borrow 参数，"
                                            "或经独占字段指向另一个视图的按值参数")

    def result_exclusive(self, fun):
        kind, ty = fun.result
        # 不透明的结果可能被实例化为独占类型，按独占处理
        return kind == "inout" or ty in fun.tparams or (ty in self.classes and self.exclusive(ty))

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
        self.leaving = False  # 正在离开函数（返回或失败）：调用方的持有者不会指向本函数的局部存储（K6）
        self.out, self.dp_types, known = [], {}, len(self.errors)
        state = {}
        for mode, name, ty_name in fun.params:
            ty = self.type_in(fun, ty_name)
            loans = {}
            if mode != "own":
                loans[""] = {self.symbolic(name, None, mode, fun.line)}
            if ty.kind == "view":
                for kind, fld in self.ref_paths(ty_name):
                    loans[fld] = {self.symbolic(name, fld, kind, fun.line)}
            elif ty.kind == "opaque" and ty_name in fun.views or mode == "own" and self.carrier(ty_name):
                loans["*"] = {self.symbolic(name, "*", "inout", fun.line)}  # view T 与容器的值携带一组未知的来源
            # 按 inout / borrow 接收存在类型，得到的是载荷访问（之三 §2.4）
            if fun.deinit_of:
                if not (ty_name in self.classes and self.classes[ty_name].deinit):
                    raise SyntaxProblem(f"第 {fun.line} 行：{ty_name} 没有声明 deinit")
                self.declare_this(fun.line, ty)
            elif mode == "own" and self.tracked(Var(ty, "A", {})):
                self.declare("let", fun.line, name, ty, state)
            state[name] = Var(ty if mode == "own" else Type(mode, ty_name), "A", loans,
                              mode != "own" and ty.kind == "any")
        declared = set(state)
        self.check_unique(fun.body, declared, fun)
        live_in(fun.body, expr_uses(fun.ret) if fun.ret else set())
        try:
            state = self.run(fun.body, state)
        except InvalidOpen:
            self.lowered[fun.name] = (self.dp_types, [], False)
            self.out = None
            return
        if fun.ret is not None:
            self.live = expr_uses(fun.ret)
            # 借用结果按“交出一个借用”求值：返回借用值即对它重借
            kind, wanted = fun.result
            returned = self.argument(fun.ret, kind, state) if kind else self.value(fun.ret, state)
            if not kind and wanted in self.classes and wanted not in fun.tparams and returned.ty != self.type_of(wanted):
                self.error("TYPE", fun.ret.line, f"返回值须是拥有的 {wanted}，不能返回 {returned.ty.kind} {returned.ty.name}")
            self.check_contract(returned, fun)
        self.check_flows(state, fun)
        self.check_tasks(state, list(state), fun.line)
        self.leave(state, "kill", fun.line)
        for name in reversed(list(state)):  # 函数结束：仍拥有的值按声明逆序析构
            self.destroy(fun.line, name, state[name])
        self.lowered[fun.name] = (self.dp_types, self.out, len(self.errors) == known)
        self.out = None

    def check_unique(self, stmts, declared, fun):
        """演算不处理遮蔽：一个函数内每个名字只声明一次。"""
        for stmt in stmts:
            for name in ([stmt.name] if stmt.kind == "val" else stmt.names):
                if name in declared:
                    raise SyntaxProblem(f"第 {stmt.line} 行：{fun.name} 内重复声明 {name}")
                declared.add(name)
            self.check_unique(stmt.body, declared, fun)
            self.check_unique(stmt.orelse, declared, fun)

    def ref_paths(self, class_name):
        """视图的全部借用字段，含按值嵌套的视图里的：(种类, 字段路径)。"""
        return self.paths_within(class_name, ())

    def paths_within(self, class_name, visiting):
        """经共享借用字段走到内层视图时，内层的借用字段一律按共享计。"""
        if class_name in visiting:  # 递归的视图类型：不再展开
            return []
        paths = []
        for kind, name, ty_name in self.classes[class_name].fields:
            if kind:
                paths.append((kind, name))
            elif self.carrier(ty_name):  # 按值的容器字段携带一组未知的来源，按独占计
                paths.append(("inout", name))
            if ty_name in self.classes and self.classes[ty_name].is_view:
                for inner, path in self.paths_within(ty_name, visiting + (class_name,)):
                    paths.append(("borrow" if kind == "borrow" else inner, f"{name}.{path}"))
        return paths

    def escapable(self, ty_name):
        """类型的值是否一定不携带来源：含借用字段的视图、借用值与 view T 不是，其余都是。"""
        if ty_name in self.classes:
            return self.has("Escapable", self.type_of(ty_name))
        return ty_name not in self.fun.views and not ty_name.startswith(("borrow ", "inout "))

    def escapable_in(self, fun, ty_name):
        """签名检查时函数体还没有开始，能力判定里的类型参数取这个签名的。"""
        self.fun = fun
        return self.has("Escapable", self.type_of(ty_name))

    def exclusive(self, class_name):
        return any(kind == "inout" for kind, _ in self.ref_paths(class_name))

    @staticmethod
    def covering(param, fld):
        """能覆盖来源 param.fld 的声明：参数本身、全部来源、该路径及它的每个前缀。"""
        wanted = {(param, None)}
        if fld:
            parts = fld.split(".")
            wanted |= {(param, "*")} | {(param, ".".join(parts[:n])) for n in range(1, len(parts) + 1)}
        return wanted

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
                if not self.covering(param, fld) & sources:
                    missing.append(ancestor[2:-1])
        return missing

    def check_contract(self, value, fun):
        """K6：返回值持有的每个 loan 都要被签名的 from 覆盖。"""
        for name in self.uncovered(value.all_loans(), set(fun.sources)):
            self.error("K6", fun.ret.line, "返回值依赖局部变量的存储" if name is None
                       else f"返回值的来源 {name.replace('.*', '.origins')} 没有在 from 中声明")

    def check_flows(self, state, fun):
        """K6 对流向的延伸：函数体让哪些来源流进了借用参数所指的值，签名的 flows 就要声明哪些。

        """
        for name, key, extra in self.flowed_in(state):
            declared = {src for src, (target, path) in fun.flows if target == name and self.within(key, path)}
            for missing in self.uncovered(extra, declared):
                self.error("K6", fun.line, f"流入 {name} 的值依赖局部变量的存储" if missing is None
                           else f"流入 {name} 的来源 {missing} 没有在 flows 中声明")

    def flowed_in(self, state):
        """流进调用方存储的来源：(参数名, 字段路径, loan)。

        参数名下每个字段路径（类型参数是 *）起初只持有它自己的符号 loan；多出来的都是流入的。
        """
        for mode, name, ty in self.fun.params:
            var = state.get(name)
            if var is None or (var.init == "M" and mode != "own"):
                continue
            for key in sorted(k for k in var.loans if k):
                # 按值参数自己的存储随它消失；只有越过借用字段、落在调用方视图里的才算流出
                if mode == "own" and not self.crosses_borrow_field(ty, key.split(".")[:-1]):
                    continue
                yield name, key, set(var.loans[key]) - {f"P({name}.{key})"}

    @staticmethod
    def within(key, path):
        """字段路径 key 是否落在 path 之下；path 为 None 表示整个值。"""
        return path is None or key == path or key.startswith(path + ".")

    def record_flow(self, state, targets, incoming, replace=False):
        """K20：来源只记一份，记在拥有被写入存储的变量上；incoming 以相对该存储的字段路径为键。

        存储在调用方时记在相应的参数上，由 check_flows 与签名核对。replace 表示旧值确定被整个换掉。
        """
        for root, path in sorted(targets):
            if root in state:
                owner, base = state[root], path
            else:
                name, _, fld = root[1:].partition(".")
                owner, base = state[name], (tuple(fld.split(".")) if fld else ()) + path
            prefix = ".".join(base)
            if replace:
                owner.loans = {k: held for k, held in owner.loans.items() if not k or not self.within(k, prefix or None)}
            for sub, held in incoming.items():
                key = prefix or "*" if sub == "*" else ".".join(filter(None, (prefix, sub)))
                owner.loans.setdefault(key, set()).update(held)

    def held_fields(self, state, targets, carried):
        """借用所指为视图时，它各借用字段当前持有的 loan：以拥有该存储的变量上的记录为准（K20），

        并上建立这份借用时随它带来的副本。所指在调用方时，账记在相应的参数名下。
        """
        loans = copy_loans(carried or {})
        for root, path in targets:
            owner = state.get(root)
            if owner is None and root.startswith("*"):
                name, _, fld = root[1:].partition(".")
                owner, path = state.get(name), (tuple(fld.split(".")) if fld else ()) + path
            elif owner is not None and owner.ty.kind != "view":
                owner = None
            if owner is None:
                continue
            prefix = "".join(f"{name}." for name in path)
            for key, held in owner.loans.items():
                if key.startswith(prefix) and key != "*" and len(key) > len(prefix):
                    loans.setdefault(key[len(prefix):], set()).update(held)
        return loans

    def walk(self, state, targets, held, class_name, through):
        """沿字段路径走：按值字段留在同一处存储，借用字段换到它所指的存储。返回走到的存储与那里的账。"""
        at = set(targets)
        for name in through:
            kind, _, class_name = self.classes[class_name].field(name)
            beneath = {key[len(name) + 1:]: loans for key, loans in held.items() if key.startswith(name + ".")}
            if kind:  # 越过借用字段：接下来的存储是它所指的视图
                at = self.places(held.get(name, ()))
                held = self.held_fields(state, at, beneath)
            else:
                at, held = {(root, where + (name,)) for root, where in at}, beneath
        return at, held

    def current_fields(self, state, targets, carried, class_name):
        """K20 现查：视图 class_name 位于 targets 时各借用字段当前持有的 loan。

        借用字段之外的路径（它所指的另一个视图的字段）在这个视图名下只有建立时的副本，
        账在所指视图的拥有者上，沿路径走过去取。
        """
        fields = self.held_fields(state, targets, carried)
        for _, path in self.ref_paths(class_name) if class_name in self.classes else ():
            *through, last = path.split(".")
            if self.crosses_borrow_field(class_name, through):
                _, held = self.walk(state, targets, fields, class_name, through)
                fields.setdefault(path, set()).update(held.get(last, ()))
        return fields

    def flow_into(self, state, targets, fields, class_name, declared, flowing):
        """K20 调用：来源并入视图 class_name 在 declared 之下的每个借用字段。

        targets 是这个视图所在的存储（按值传入时为空），fields 是它各借用字段持有的 loan。
        字段路径越过借用字段时，账记到那个字段所指的视图的拥有者上。
        """
        for _, path in self.ref_paths(class_name):
            if not self.within(path, declared):
                continue
            *through, last = path.split(".")
            at, _ = self.walk(state, targets, fields, class_name, through)
            self.record_flow(state, at, {last: flowing})

    def reaches_exclusively(self, class_name, fields):
        """字段路径是否越过借用字段，且越过的都是独占字段。"""
        kinds = []
        for name in fields:
            kind, _, class_name = self.classes[class_name].field(name)
            kinds.append(kind)
        return "inout" in kinds and "borrow" not in kinds

    def folded(self, fields, class_name, reached):
        """递归的视图类型：账只展开到第一次回到同一类型为止。

        再往里的借用字段没有自己的记录，它们的来源归到走到这里所凭借的那份借用名下。
        """
        decl = self.classes.get(class_name)
        for kind, name, _ in decl.fields if decl is not None and decl.is_view else ():
            if kind and name not in fields:
                fields[name] = set(reached)
        return fields

    def crosses_borrow_field(self, class_name, fields):
        """字段路径是否越过借用字段，也就是走到了另一个值的存储里。"""
        for name in fields:
            decl = self.classes.get(class_name)
            fld = decl.field(name) if decl else None
            if fld is None:
                return False
            if fld[0]:
                return True
            class_name = fld[2]
        return False

    def refines(self, impl, sig, line):
        """K12：把函数当作合同为 sig 的值使用时，它声明的来源与流向都须被 sig 覆盖。"""
        if [m for m, _, _ in impl.params] != [m for m, _, _ in sig.params]:
            self.error("K12", line, f"{impl.name} 的参数模式与 {sig.name} 不符")
            return
        for (mode, name, actual), (_, _, wanted) in zip(impl.params, sig.params):
            if mode == "own" and actual in self.classes and actual not in impl.tparams and (
                    actual != wanted or wanted in sig.tparams):
                self.error("K12", line, f"{impl.name} 的具体 own 参数 {name}: {actual} 与 {sig.name} 不符")
                return
        rename = {i[1]: s[1] for i, s in zip(impl.params, sig.params)}

        def covered(src, allowed):
            param, fld = rename.get(src[0], src[0]), src[1]
            return bool(self.covering(param, fld) & allowed)

        for src in impl.sources:
            if not covered(src, set(sig.sources)):
                self.error("K12", line, f"{impl.name} 的来源 {src[0]}{'.' + src[1] if src[1] else ''} 超出 {sig.name} 的声明")
        for src, (target, path) in impl.flows:
            allowed = {s for s, (t, p) in sig.flows
                       if t == rename.get(target, target) and (p is None or (path and self.within(path, p)))}
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
        调用方的持有者在本函数返回之后仍然存活，所以流进它的来源一直存活（K20 存活）。
        析构可观察的值活到它被析构的那一刻：它的 deinit 可能经它持有的借用读出去（之二 R1.2）。
        """
        names = {n for n in self.live if n in state} | {
            n for n, v in state.items() if self.tracked(v) and v.init != "M" and not self.has("EarlyDrop", v.ty)}
        loans = set().union(*self.pending) if self.pending else set()
        for _, _, extra in () if self.leaving else self.flowed_in(state):
            loans |= extra
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
        if kind not in ("move", "kill", "fail") and not self.quiet:
            for root, path in sorted(targets):  # 经借用访问也是对 owner 的一次使用；带上用到的是哪个字段
                if self.tracked(state.get(root)):
                    self.emit("use", line, root, fld=path[0] if path else "")
        skip = set(via) | self.ancestors(via)
        for loan in sorted(self.live_loans(state) - skip):
            info = self.loans[loan]
            if info.kind == "shared" and kind in ("read", "shared"):
                continue
            if not any(r == r2 and prefix_related(p, p2) for r, p in targets for r2, p2 in info.places):
                continue
            rule = "K4" if via & self.ancestors({loan}) else {"kill": "K9", "fail": "K22"}.get(kind, "K1")
            self.error(rule, line, f"{kind} 与第 {info.line} 行建立的 {info.kind} loan 冲突")

    def leave(self, state, kind, line):
        """离开函数时的析构：仍拥有的值自内向外、声明逆序析构，此后本函数没有别的代码再运行。

        kind 为 kill 是正常返回（K9），为 fail 是某个操作失败（K22）。此刻还存活的 loan 只剩
        尚未析构的、析构可观察的值所持有的：先析构的若是它们的来源，它们的 deinit 就会读到已析构的存储。
        """
        saved = self.live, self.pending
        self.live, self.pending, self.leaving = set(), [], True
        left = dict(state)
        for name in reversed(list(left)):
            if left[name].init != "M":
                self.access(left, {(name, ())}, set(), kind, line)
            del left[name]
        (self.live, self.pending), self.leaving = saved, False

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
        elif var.gone and (not place.fields or (place.fields[0],) in var.gone):
            self.error("K8", place.line, f"{'.'.join([place.root] + place.fields[:1])} 已被部分移出")
        here = {(place.root, ())}
        if not place.fields:
            held = self.current_fields(state, here, {}, var.ty.name) if var.ty.kind == "view" else var.loans
            return Resolved(here, set(), var.ty, True, held, "")
        targets, via, ty, writable, field_loans = here, set(), var.ty, True, var.loans
        if var.ty.kind in ("borrow", "inout"):
            self.access(state, here, set(), "read", place.line)
            via = set(var.loans.get("", ()))
            targets, ty, writable = self.places(via), self.type_of(var.ty.name), var.ty.kind == "inout"
            field_loans = self.folded(self.current_fields(state, targets, var.loans, var.ty.name), var.ty.name, via)
        crossed = ""
        for name in place.fields:
            decl = self.classes.get(ty.name)
            fld = decl.field(name) if decl else None
            if fld is None:
                raise SyntaxProblem(f"第 {place.line} 行：{ty.name} 没有字段 {name}")
            if decl.cell:
                self.error("K19", place.line, f"{decl.name} 是内部可变类型，它的内容只能经受信任函数访问")
            if name in decl.private and self.fun.owner != decl.name:
                self.error("K15", place.line, f"{decl.name}.{name} 是私有字段，只有 {decl.name} 的成员函数可以访问")
            kind, _, fty = fld
            crossed = kind
            if not kind:
                targets = {(root, path + (name,)) for root, path in targets}
                if field_loans is not None:  # 走进按值嵌套的视图：只留它名下的借用字段
                    field_loans = {key[len(name) + 1:]: held for key, held in field_loans.items()
                                   if key.startswith(name + ".")}
            else:
                self.access(state, targets, via, "read", place.line)
                held = set((field_loans or {}).get(name, ()))
                via = held if kind == "borrow" else via | held
                targets = self.places(held)
                writable = writable and kind == "inout"
                if field_loans is not None:  # 借用字段指向另一个视图：它的借用字段记在同一前缀之下
                    field_loans = self.folded(self.current_fields(state, targets, {
                        key[len(name) + 1:]: held for key, held in field_loans.items() if key.startswith(name + ".")},
                        fty), fty, held)
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
        if expr.kind == "start":
            return self.start(expr, state)
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
        if not self.has("Copyable", res.ty) or place.fields:
            self.error("K8", expr.line, f"{place.root} 不可隐式复制，须写 move")
        return Value(res.ty, copy_loans(res.field_loans or {}))

    def move(self, expr, state, local_binding=False):
        """K8：移动整个变量；移动是一次与任何存活 loan 都不相容的访问。"""
        place = expr.place
        if place.fields:
            return self.take(expr, state)
        res = self.resolve(state, place)
        if self.fun.deinit_of and place.root == "this":
            self.error("K24", expr.line, "deinit 里不能整体移走 this")
        self.access(state, res.targets, set(), "move", expr.line)
        var = state[place.root]
        if var.pinned:
            self.error("S3", expr.line, f"{place.root} 已开始执行，它的帧不能再移动")
        # 移入新局部变量时转移原账；现查展开的字段供边界读出，不存成新的副本。
        held = var.loans if local_binding and var.ty.kind == "view" else res.field_loans
        value = Value(var.ty, copy_loans(held), var.payload)
        if self.tracked(var):
            self.emit("move", expr.line, place.root)
        var.init, var.loans = "M", {}
        return value

    def take(self, expr, state):
        """K23：从拥有的变量里移出一个按值字段。此后这个字段不可再用，变量不可再整体使用，其余字段照常。

        声明了 deinit 的类型不能被移出字段：它的 deinit 要看到完整的值。
        """
        place = expr.place
        if len(place.fields) != 1:
            raise SyntaxProblem(f"第 {expr.line} 行：演算只支持移出一层字段")
        res = self.resolve(state, place)
        var, name = state[place.root], place.fields[0]
        if res.ty.kind == "int":  # 整数没有可移走的东西，只是一次读
            self.access(state, res.targets, res.via, "read", expr.line)
            return Value(INT)
        decl = self.classes.get(var.ty.name)
        if var.ty.kind not in ("cls", "view") or res.crossed:
            self.error("K8", expr.line, f"{place.root}.{name} 不在这个变量自己的存储里，不能移出")
            return Value(res.ty)
        if decl.deinit and not (self.fun.deinit_of and place.root == "this"):
            self.error("K23", expr.line, f"{var.ty.name} 声明了 deinit，不能从它里面移出字段")
        self.access(state, res.targets, set(), "move", expr.line)
        value = Value(res.ty, copy_loans(res.field_loans or {}))
        var.loans = {key: held for key, held in var.loans.items() if not self.within(key, name)}
        var.gone[(name,)] = "M"
        if self.tracked(Var(res.ty, "A", {})):
            taken = f"{place.root}#{name}"
            self.dp_type(res.ty)
            self.emit("take", expr.line, taken, ty=place.root, fld=name)
            self.emit("move", expr.line, taken)
        return value

    # ---- 挂起（内核基底 KB8）

    def start(self, expr, state):
        """创建任务：按被调函数的参数模式求实参，任务值持有全部实参带来的 loan。"""
        fun = self.funs.get(expr.name)
        if fun is None or len(fun.params) != len(expr.args):
            raise SyntaxProblem(f"第 {expr.line} 行：start {expr.name} 的目标或实参个数不符")
        if not fun.suspend:
            self.error("S0", expr.line, f"{expr.name} 不是 suspend 函数，不能作为任务启动")
        held, pushed = set(), 0
        for (mode, _, wanted), arg in zip(fun.params, expr.args):
            value = self.argument(arg, mode, state)
            self.check_owned_argument(fun, mode, wanted, value, arg.line)
            held |= value.all_loans()
            self.pending.append(value.all_loans())
            pushed += 1
        del self.pending[len(self.pending) - pushed:]
        return Value(Type("task", expr.name), {"*": held})

    def suspend_point(self, state, line):
        """S0.3 状态守恒：挂起不是一次访问，loan 集合不变。这里记录并检查留在帧里的东西。"""
        if not self.fun.suspend:
            self.error("S0", line, "挂起点只能出现在 suspend 函数里")
            return
        if self.drop_before_suspend:  # S6：不再需要、析构不可观察的值不带过挂起点
            self.drop_unneeded(state, line, set())
        loans = self.live_loans(state)
        inside = sorted({root for loan in loans for root, _ in self.loans[loan].places if root in state})
        # 留在帧里的：还会用到的、被存活的借用指向的，以及此刻仍拥有的每个值（它们等着在作用域结束时析构）
        owned = [n for n, v in state.items() if v.ty.kind in ("cls", "any", "opaque") and v.init != "M"]
        names = sorted({n for n in self.live if n in state and state[n].init != "M"} | set(inside) | set(owned))
        self.frames[self.fun.name, line] = (names, inside)
        # 帧若在这里被销毁而不再恢复（所在执行器异常终止），按此刻的状态析构：与失败出口是同一份清单
        self.emit("suspend", line, state=state)
        self.leave(state, "fail", line)
        if self.frames_movable and inside:
            self.error("S1", line, f"帧内有指向 {'、'.join(inside)} 的借用跨过挂起点；帧若可移动，这些借用会悬空")
        if self.fun.parallel:
            for name in names:
                if not self.has("Transferable", state[name].ty):
                    self.error("S5", line, f"{name} 跨过挂起点存活，但它不可转移到另一个线程")

    def check_tasks(self, state, names, line):
        """S4：已开始执行的任务在离开作用域前须被等待；正常路径上任务总是运行到自己返回。"""
        for name in names:
            var = state.get(name)
            if var is not None and var.ty.kind == "task" and var.pinned and var.init != "M":
                self.error("S4", line, f"任务 {name} 已开始执行，离开作用域前须 await")

    def borrow(self, expr, place, mut, state, stored_kind=None, reserved=False):
        """K2：在 place 上建立 loan；经持有者到达时，新 loan 是其 loan 的子 loan。

        reserved：接收者位置的独占借用先按共享建立（K21），等其余实参求值完再由 activate 转为独占。
        """
        res = self.resolve(state, place)
        if mut and not res.writable:
            self.error("K1", expr.line, "不能经共享借用建立独占借用")
        self.access(state, res.targets, res.via, "excl" if mut and not reserved else "shared", expr.line)
        kind = "shared" if reserved else stored_kind or ("excl" if mut else "shared")
        loan = self.new_loan(f"L{expr.site}", kind, res.targets, res.via, expr.line)
        self.loans[loan].kind = kind
        loans = {"": {loan}}
        if res.ty.kind in ("view", "opaque"):
            loans.update(copy_loans({k: v for k, v in (res.field_loans or {}).items() if k}))
        # 视图里存放的存在类型借用是载荷访问，经它重借出来的仍是
        payload = bool(res.crossed) and res.ty.kind == "any"
        return Value(Type("inout" if mut else "borrow", res.ty.name), loans, payload)

    def activate(self, value, kind, entry, state, line):
        """K21：其余实参求值完毕，接收者的借用转为独占；此刻与它重叠的存活 loan 都是冲突。"""
        loan = next(iter(value.loans[""]))
        info = self.loans[loan]
        self.pending.remove(entry)
        self.access(state, info.places, info.parents, "excl", line)
        self.pending.append(entry)
        info.kind = kind

    def check_owned_argument(self, fun, mode, wanted, value, line):
        """具体类的 own 参数要取得这个类的值，而不是同名类型的借用。"""
        if mode == "own" and wanted in self.classes and wanted not in fun.tparams and value.ty != self.type_of(wanted):
            self.error("TYPE", line, f"own {wanted} 不能接收 {value.ty.kind} {value.ty.name}")

    def argument(self, expr, mode, state, stored_kind=None, reserved=False):
        """按参数模式求实参：borrow 与 inout 位置上建立（或重借出）一个 loan。"""
        if mode == "own":
            return self.value(expr, state)
        mut = mode == "inout"
        if expr.kind == "ref":
            if expr.mut != mut:
                self.error("TYPE", expr.line, f"{mode} 位置的实参写法不符")
            return self.borrow(expr, expr.place, mut, state, stored_kind, reserved)
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
            loans.update({k: v for k, v in self.current_fields(state, targets, var.loans, var.ty.name).items() if k})
            return Value(Type(mode, var.ty.name), loans, var.payload)
        res = self.resolve(state, expr.place)
        if not res.crossed:
            self.error("TYPE", expr.line, f"{mode} 位置需要 & 表达式或一个借用值")
            return Value(Type(mode, res.ty.name), {"": set()})
        if mut and not res.writable:
            self.error("K1", expr.line, "不能经共享借用建立独占借用")
        self.access(state, res.targets, res.via, "excl" if mut else "shared", expr.line)
        kind = stored_kind or ("excl" if mut else "shared")
        loans = {"": {self.new_loan(f"L{expr.site}", kind, res.targets, res.via, expr.line)}}
        if res.ty.kind == "view":  # 交出的是指向另一个视图的借用：带上那个视图各借用字段的来源
            loans.update(copy_loans({k: v for k, v in (res.field_loans or {}).items() if k}))
        return Value(Type(mode, res.ty.name), loans, res.ty.kind == "any")

    def construct(self, expr, state):
        """K7：视图实例在每个借用字段上持有实参处建立的 loan。"""
        decl = self.classes[expr.name]
        if len(expr.args) != len(decl.fields):
            raise SyntaxProblem(f"第 {expr.line} 行：{expr.name} 的实参个数不符")
        loans, pushed = {}, 0
        for (kind, name, fty), arg in zip(decl.fields, expr.args):
            val = self.argument(arg, kind or "own", state)
            if not kind and fty in self.classes and val.ty != self.type_of(fty):
                self.error("TYPE", arg.line, f"{decl.name}.{name} 要求拥有的 {fty}，不能接收 {val.ty.kind} {val.ty.name}")
            if kind:
                loans[name] = set(val.loans.get("", ()))
            elif self.carrier(fty):  # 按值的容器字段：它携带的来源记在这个字段名下
                loans[name] = set(val.loans.get("*", ()))
            # 按值嵌套或借用另一个视图：内层的借用字段记在 字段名.路径 之下
            loans.update({f"{name}.{key}": set(held) for key, held in val.loans.items() if key and key != "*"})
            self.pending.append(val.all_loans())
            pushed += 1
        del self.pending[len(self.pending) - pushed:]
        return Value(self.type_of(expr.name), loans)

    def bind_types(self, fun, values, line):
        """X2：inout 位置对所指类型不变，载荷访问不能充当槽位。

        形参写作 any I 时，实参所指可以是实现 I 的具体类型或 any I，得到的是载荷访问；
        形参是具体类型或类型参数时，实参须是所指类型恰为该类型的槽位，类型参数在各处的实例须一致。
        载荷访问交给 inout T 时打开存在类型（X4）：T 绑定为这一处独有的未知具体类型，
        所以别处拿不出类型恰为 T 的值，替换无从发生。返回各类型参数的绑定。
        """
        bound = {}
        for mode, name, wanted in fun.params:
            value = values[name]
            if value.ty.kind == "fn":
                continue
            actual = value.ty.name
            if mode == "own" and value.ty.kind in ("borrow", "inout"):  # 借用值本身作为 T 的值
                actual = f"{value.ty.kind} {actual}"
            if wanted in fun.tparams and wanted not in fun.views and not self.escapable(actual):
                self.error("K17", line, f"{name}：{actual} 可能携带来源，不能实例化可逃逸的类型参数 {wanted}")
            if wanted in fun.tparams:
                if mode == "inout" and value.payload:
                    actual = ("opened", actual, name)
                if bound.setdefault(wanted, actual) != actual:
                    shown = [b[1] + "（经 " + b[2] + " 打开的未知类型）" if isinstance(b, tuple) else b
                             for b in (bound[wanted], actual)]
                    self.error("X2", line, f"类型参数 {wanted} 被同时实例化为 {shown[0]} 与 {shown[1]}")
                for cap in sorted(fun.bounds.get(wanted, ())):  # 实参的类型须满足类型参数声明的上界
                    if isinstance(actual, tuple):
                        known = False  # 打开得到的未知类型：没有上界可依据
                    elif actual.startswith(("borrow ", "inout ")):
                        known = self.has(cap, Type(*actual.split(" ", 1)))
                    else:
                        known = self.has(cap, self.type_in(self.fun, actual))
                    if not known:
                        shown = actual[1] if isinstance(actual, tuple) else actual
                        self.error("CAP", line, f"{name}：{shown} 不满足类型参数 {wanted} 的上界 {cap}")
            elif wanted.startswith("any "):
                decl = self.classes.get(actual)
                if actual != wanted and (decl is None or wanted[4:] not in decl.implements):
                    self.error("TYPE", line, f"{name}：{actual} 不能当作 {wanted}")
            elif mode == "inout" and value.payload:
                self.error("X2", line, f"{name}：载荷访问不能充当 {wanted} 的槽位")
        return bound

    def carried(self, value, mode, fld):
        """按来源的三种形式取出实参带来的 loan。"""
        if fld is None:  # 依赖参数本身；按值参数没有“本身”，等同于转发它携带的全部来源
            return value.all_loans() if mode == "own" else set(value.loans.get("", ()))
        if fld == "*":
            if mode == "own":
                return value.all_loans()
            return set().union(*(held for key, held in value.loans.items() if key))
        held = [loans for key, loans in value.loans.items() if key == fld or key.startswith(fld + ".")]
        return set().union(*held)

    def callable_argument(self, arg, sig_name, state):
        """函数类型的实参：只接受具名函数或另一个函数类型的参数，并按 K12 核对合同。"""
        name = arg.place.root if arg.kind == "place" and not arg.place.fields else None
        var = state.get(name)
        impl = self.sigs[var.ty.name] if var is not None and var.ty.kind == "fn" else self.funs.get(name)
        if impl is None:
            raise SyntaxProblem(f"第 {arg.line} 行：函数类型的实参须是函数名")
        self.refines(impl, self.sigs[sig_name], arg.line)
        return Value(Type("fn", sig_name)), impl

    def call(self, expr, state):
        """K5：结果的 loan 完全由被调方的签名决定，不读它的函数体。"""
        var = state.get(expr.name)
        fun = self.sigs[var.ty.name] if var is not None and var.ty.kind == "fn" else self.funs.get(expr.name)
        if fun is None or len(fun.params) != len(expr.args):
            raise SyntaxProblem(f"第 {expr.line} 行：调用 {expr.name} 的目标或实参个数不符")
        suspends = fun.suspend
        modes = {name: mode for mode, name, _ in fun.params}
        # 依赖参数本身的结果或流向使实参处的 loan 在调用之后继续存在：流入独占结果的按独占计，
        # 流入持有者的按参数自己的模式计，其余按共享计
        into_result = {param for param, fld in fun.sources if fld is None and modes.get(param) != "own"}
        into_holder = {param for (param, fld), _ in fun.flows if fld is None and modes.get(param) != "own"}
        exclusive = {name for name in into_result if self.result_exclusive(fun)} | {
            name for name in into_holder if modes[name] == "inout"}
        values, pushed, receiver, results = {}, 0, None, {}
        for (mode, name, ty_name), arg in zip(fun.params, expr.args):
            if ty_name.startswith("fn "):
                values[name], impl = self.callable_argument(arg, ty_name[3:], state)
                # 合同的结果是类型参数、实参函数的结果是具体类型时，这个类型参数就此确定（同名即同一个）
                if self.sigs[ty_name[3:]].result[1] in fun.tparams and impl.result[1] not in impl.tparams:
                    results[self.sigs[ty_name[3:]].result[1]] = " ".join(filter(None, impl.result))
                continue
            kept = ("excl" if name in exclusive else "shared") if name in into_result | into_holder else None
            # 接收者：名为 this 的第一个参数，按 inout 传入且实参是直接的 &mut
            reserved = name == "this" and mode == "inout" and arg is expr.args[0] and arg.kind == "ref"
            values[name] = self.argument(arg, mode, state, kept, reserved)
            self.check_owned_argument(fun, mode, ty_name, values[name], arg.line)
            self.pending.append(values[name].all_loans())
            pushed += 1
            if reserved:
                receiver = (values[name], kept or "excl", self.pending[-1])
        if receiver:
            self.activate(*receiver, state, expr.line)
        if suspends:  # 调用 suspend 函数本身是调用方的一个挂起点；实参的 loan 在此期间保持
            self.suspend_point(state, expr.line)
        del self.pending[len(self.pending) - pushed:]
        self.emit("call", expr.line, state=state)
        bound = {**results, **(self.bind_types(fun, values, expr.line) or {})}
        for (param, fld), (target, path) in fun.flows:  # 来源流入已有的持有者
            index = [name for _, name, _ in fun.params].index(target)
            arg, wanted = expr.args[index], fun.params[index][2]
            given = values[target]
            if modes[target] == "own":  # 按值交出的视图：它自己的存储归被调方，流向只能经它的借用字段落到别处
                targets = set()
            elif arg.kind == "ref" or (arg.kind == "place" and arg.place.fields):
                targets = self.resolve(state, arg.place).targets  # 含经视图的借用字段到达的另一个视图
            elif arg.kind == "place":
                # 借用值：来源流入它所指的每处存储；所指在调用方时，记在相应的参数上
                targets = self.places(state[arg.place.root].loans.get("", ()))
            else:
                raise SyntaxProblem(f"第 {expr.line} 行：流向的目标须是对变量或其字段的借用，或一个借用值")
            flowing = self.carried(values[param], modes[param], fld)
            actual = bound.get(wanted, wanted)
            if isinstance(actual, str) and actual in self.classes and self.type_of(actual).kind == "view":
                # 目标是视图：流入的来源可能落在 path 之下的任何一个借用字段上
                fields = {key: held for key, held in given.loans.items() if key}
                self.flow_into(state, targets, self.current_fields(state, targets, fields, actual), actual, path, flowing)
            else:
                self.record_flow(state, targets, {"*": flowing})
        self.leave(state, "fail", expr.line)  # 被调方可能在流向发生之后才失败
        result = set()
        for param, fld in fun.sources:
            if param in values:
                result |= self.carried(values[param], modes[param], fld)
        kind, ty_name = fun.result
        if kind:
            actual = bound.get(ty_name, ty_name)  # 类型参数换成本次调用的实例
            if isinstance(actual, tuple):  # 打开得到的未知类型：结果仍是载荷访问
                return Value(Type(kind, actual[1]), {"": result}, True)
            return Value(Type(kind, actual), {"": result}, actual.startswith("any "))
        ty = self.type_in(fun, ty_name)
        if ty.kind == "opaque" and isinstance(bound.get(ty_name), str):  # 类型参数换成本次调用的实例
            ty_name = bound[ty_name]
            if ty_name.startswith(("borrow ", "inout ")):
                return Value(Type(*ty_name.split(" ", 1)), {"": result})
            ty = self.type_in(self.fun, ty_name)
        if ty.kind == "opaque":
            return Value(ty, {"*": result})
        if ty.kind == "view":  # K11：含多个借用字段的结果共同持有这些 loan
            return Value(ty, {path: set(result) for _, path in self.ref_paths(ty_name)})
        if self.carrier(ty_name):  # 容器：它携带的来源整体记在值名下
            return Value(ty, {"*": result})
        return Value(ty)

    # ---- 语句

    def run(self, stmts, state):
        for stmt in stmts:
            state = self.step(stmt, state)
            if self.early_drop:
                self.drop_dead(stmt, state)
        return state

    def drop_dead(self, stmt, state):
        """提前析构：变量不再被用到、名下没有存活的 loan、析构不可观察时，在这条语句之后析构。

        存活的 loan 使它所指的 owner 保持存活，所以析构点由“变量与从它借出的一切都不再使用”决定。
        """
        self.live = set(stmt.live_out)
        self.drop_unneeded(state, stmt.line, {name for _, name, _ in self.fun.params})

    def drop_unneeded(self, state, line, keep):
        """析构此刻不再被用到、名下没有存活 loan、析构不可观察的局部值；keep 里的变量不动。"""
        for name in reversed(list(state)):
            var = state[name]
            if name in self.live or name in keep or var.init != "A" or var.ty.kind != "cls":
                continue
            if not self.has("EarlyDrop", var.ty):  # 析构可观察：声明了 deinit，或按值含有这样的字段
                continue
            if self.borrowed_from(name, state):
                continue
            self.access(state, {(name, ())}, set(), "kill", line)
            self.destroy(line, name, var)
            var.init = "M"
            self.drops.append((self.fun.name, name, line))

    def settle(self, then, then_out, other, other_out, stmt):
        """合流前的补偿析构：不可观察的值只在一条分支上被移走或析构、此后不再使用时，在另一条分支末尾析构。

        这样合流后两边一致，不需要运行时的析构标志；可观察的值不在此列，它们留到作用域结束并带标志。
        """
        self.live = set(stmt.live_out)
        params = {name for _, name, _ in self.fun.params}
        for name in reversed([n for n in then if n in other]):
            for state, out, gone in ((then, then_out, other[name].init), (other, other_out, then[name].init)):
                var = state[name]
                if var.init != "A" or gone != "M" or name in self.live or name in params:
                    continue
                if var.ty.kind != "cls" or not self.has("EarlyDrop", var.ty) or self.borrowed_from(name, state):
                    continue
                self.access(state, {(name, ())}, set(), "kill", stmt.line)
                if out is not None:
                    out.append(DP.Stmt("drop", stmt.line, var=name))
                var.init = "M"
                self.drops.append((self.fun.name, name, stmt.line))

    def borrowed_from(self, name, state):
        """是否还有存活的 loan 指向变量 name 的存储。"""
        return any(root == name for loan in self.live_loans(state) for root, _ in self.loans[loan].places)

    def step(self, stmt, state):
        self.live = (set(stmt.live_out) - stmt_defs(stmt)) | stmt_uses(stmt)
        if stmt.kind == "val":
            value = (self.move(stmt.expr, state, local_binding=True) if stmt.expr.kind == "move"
                     else self.value(stmt.expr, state))
            var = Var(value.ty, "A", value.loans, value.payload)
            # 直接构造的独占字段若取自 &mut local，所指视图有确切的账，不保存会过期的字段副本。
            # 调用结果的 from 只指明来源，不一定指明所指存储；不裁剪它的合同记录。
            if value.ty.kind == "view" and stmt.expr.kind == "call" and stmt.expr.name in self.classes:
                for (kind, name, fty), arg in zip(self.classes[stmt.expr.name].fields, stmt.expr.args):
                    if (kind == "inout" and arg.kind == "ref" and arg.mut and not arg.place.fields
                            and state[arg.place.root].ty == Type("view", fty)):
                        # 只省去外层能重新展开的键；递归折叠边界以外的记录不能重建。
                        for _, path in self.ref_paths(value.ty.name):
                            if path.startswith(name + "."):
                                var.loans.pop(path, None)
            if self.tracked(var):
                self.declare("let", stmt.line, stmt.name, value.ty, state)
            state[stmt.name] = var
        elif stmt.kind == "open":
            self.open_receiver(stmt, state)
        elif stmt.kind == "store":
            self.store(stmt, state)
        elif stmt.kind == "suspend":
            self.suspend_point(state, stmt.line)
        elif stmt.kind in ("poll", "await"):
            var = state.get(stmt.name)
            if var is None or var.ty.kind != "task":
                raise SyntaxProblem(f"第 {stmt.line} 行：{stmt.name} 不是任务")
            if var.init != "A":
                self.error("K8", stmt.line, f"{stmt.name} 已被移动或可能已被移动")
            elif stmt.kind == "poll":
                var.pinned = True
            else:  # await：等它结束。等待期间任务及其持有的 loan 都在；结束后任务被消费
                self.suspend_point(state, stmt.line)
                var.init, var.loans = "M", {}
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
            self.emit("block", stmt.line, body=self.inner)
        elif stmt.kind == "if":
            then = self.scope(stmt.body, copy_state(state), stmt.live_out, stmt.end_line)
            body = self.inner
            other = self.scope(stmt.orelse, copy_state(state), stmt.live_out, stmt.else_end_line)
            if self.early_drop:
                self.settle(then, body, other, self.inner, stmt)
            self.emit("if", stmt.line, body=body, orelse=self.inner)
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
            self.emit("loop", stmt.line, body=self.inner)  # 取到达不动点的那一遍
        return state

    def open_receiver(self, stmt, state):
        """K25：只拆类型作者的 own this。单个 open 原子转移义务，不生成 move 加若干可失败的 let。"""
        fun = self.fun
        decl = self.classes.get(fun.owner)
        if fun.deinit_of:
            self.error("K24", stmt.line, "不能在 deinit 中解除 this 的析构")
            raise InvalidOpen
        if (stmt.name != "this" or not fun.params or fun.params[0] != ("own", "this", fun.owner)
                or state[stmt.name].ty != Type("cls", fun.owner)
                or not decl or not decl.deinit or decl.is_view or decl.cell or decl.local or decl.carrier):
            self.error("K25", stmt.line, "open 仅支持普通 deinit 类型自身的 own this 方法")
            raise InvalidOpen
        if len(stmt.names) != len(decl.fields) or any(
                kind or ty not in self.classes or self.classes[ty].is_view or not self.escapable(ty)
                for kind, _, ty in decl.fields):
            self.error("K25", stmt.line, "此切片只支持为每个按值类字段绑定一个名字")
            raise InvalidOpen
        var = state[stmt.name]
        self.resolve(state, Place(stmt.name, [], stmt.line))  # K8：完整、仍拥有的值
        self.access(state, {(stmt.name, ())}, set(), "move", stmt.line)  # K1：仍被借用时不可拆开
        var.init, var.loans = "M", {}
        self.emit("open", stmt.line, stmt.name, names=stmt.names)
        for name, (_, _, ty) in zip(stmt.names, decl.fields):
            state[name] = Var(self.type_of(ty), "A", {})

    def store(self, stmt, state):
        """X1：经独占借用整体替换所指的值，要求槽位访问且新值的类型恰为所指类型。"""
        value = self.value(stmt.expr, state)
        var = state.get(stmt.name)
        if var is None or var.ty.kind != "inout":
            raise SyntaxProblem(f"第 {stmt.line} 行：*{stmt.name} 须是独占借用")
        self.resolve(state, Place(stmt.name, [], stmt.line))
        if var.payload:
            self.error("X1", stmt.line, f"{stmt.name} 是载荷访问，不能整体替换")
        elif value.ty.name != var.ty.name or value.ty.kind in ("borrow", "inout"):
            self.error("X1", stmt.line, f"不能把 {value.ty.kind} {value.ty.name} 写入 {var.ty.name} 的槽位")
        via = set(var.loans.get("", ()))
        self.access(state, self.places(via), via, "write", stmt.line)
        incoming = {key: held for key, held in value.loans.items() if key}
        if incoming:  # K20：新值携带的来源记到所指的存储上
            var.loans = {"": via}
            self.record_flow(state, self.places(via), incoming, replace=len(self.places(via)) == 1)

    def assign(self, stmt, state):
        value = self.value(stmt.expr, state)
        place = stmt.place
        if place.fields:
            res = self.resolve(state, place)
            if not res.crossed and res.ty.kind in ("cls", "view") and res.ty != value.ty:
                self.error("TYPE", stmt.line, f"字段须接收拥有的 {res.ty.name}，不能写入 {value.ty.kind} {value.ty.name}")
                return
            if not res.writable:
                self.error("K1", stmt.line, "不能经共享借用写入")
            self.access(state, res.targets, res.via, "write", stmt.line)
            if value.all_loans():  # 把携带来源的值写进视图的字段
                var = state[place.root]
                prefix = ".".join(place.fields)
                # K20：账记在被写入的存储的拥有者上；经借用或经视图的借用字段到达时，
                # 这条路径上随借用带来的副本作废，之后从拥有者那里现查
                if var.ty.kind in ("borrow", "inout") or self.crosses_borrow_field(var.ty.name, place.fields):
                    var.loans = {key: held for key, held in var.loans.items() if not key.startswith(prefix + ".")}
                incoming = {key: set(held) for key, held in value.loans.items() if key}
                self.record_flow(state, res.targets, incoming, replace=len(res.targets) == 1)
            return
        var = state.get(place.root)
        if var is None:
            raise SyntaxProblem(f"第 {stmt.line} 行：未声明的变量 {place.root}")
        if var.ty.kind in ("cls", "view") and value.ty != var.ty:
            self.error("TYPE", stmt.line, f"不能把 {value.ty.kind} {value.ty.name} 赋给 {var.ty.name} 的拥有槽位")
            return
        # 旧值正被覆盖，它持有的 loan 此刻不再算存活；新值在赋值之后仍被使用时算存活
        self.live = set(stmt.live_out) - {place.root}
        if place.root in stmt.live_out:
            self.pending.append(value.all_loans())
        self.quiet = True
        self.access(state, {(place.root, ())}, set(), "write", stmt.line)
        self.quiet = False
        if place.root in stmt.live_out:
            self.pending.pop()
        if self.tracked(var):  # 先求值新值，再析构旧值，再提交
            self.destroy(stmt.line, place.root, var, replace=True)
            self.declare("set", stmt.line, place.root, value.ty, state)
        if var.ty.kind == "any" and value.ty.name != var.ty.name:  # X3：槽位里只能放同一存在类型的值
            self.error("X1", stmt.line, f"不能把 {value.ty.name} 赋给 {var.ty.name}")
        var.init, var.loans, var.payload, var.gone = "A", value.loans, value.payload, {}

    def scope(self, stmts, state, live_after, end_line):
        """K9：作用域结束时按声明逆序析构；析构与仍存活的 loan 冲突即拒绝。"""
        outer = set(state)
        enclosing, self.out = self.out, [] if self.out is not None else None
        state = self.run(stmts, state)
        self.live = set(live_after)
        self.check_tasks(state, [n for n in state if n not in outer], end_line)
        for name in reversed([n for n in state if n not in outer]):
            if state[name].init != "M":
                self.access(state, {(name, ())}, set(), "kill", end_line)
            self.destroy(end_line, name, state[name])
            del state[name]
        self.inner, self.out = self.out, enclosing
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
            gone = {path: x.gone[path] if x.gone.get(path) == y.gone.get(path) else "MM"
                    for path in sorted(set(x.gone) | set(y.gone))}
            merged[name] = Var(x.ty, x.init if x.init == y.init else "MM", loans, x.payload, x.pinned or y.pinned, gone)
        return merged


def check(text, early_drop=False, drops=None, frames_movable=False, frames=None, lowered=None):
    """检查一段演算程序，返回按行排序的 (规则, 行号, 说明)。空列表表示接受。

    early_drop 打开提前析构；drops 若给出，收到各处提前析构的 (函数名, 变量名, 行号)。
    lowered 若给出，收到各函数的 (类型表, 带显式析构的程序, 是否被接受)，可交给 DP.Verifier。
    """
    classes, funs, sigs = parse(text)
    checker = Checker(classes, funs, sigs, early_drop, frames_movable)
    for fun in funs.values():
        checker.check_fun(fun)
    if drops is not None:
        drops.extend(checker.drops)
    if frames is not None:
        frames.update(checker.frames)
    if lowered is not None:
        lowered.update(checker.lowered)
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
