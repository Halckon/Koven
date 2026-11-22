"""结构能力推导的原型（内核基底 KB4）。

一个以“能力描述”为参数的推导器：每项能力是一张叶子公理表加“对字段取合取”。
对每个声明求出的是一个条件——它的类型参数各须满足哪些能力——而不是逐个实例求真假，
所以递归类型、参数不断增长的递归类型都会收敛。规格见 docs/proposals/capability-derivation-model.md。

用法：python3 scripts/loan_kernel/capability.py 文件.cap ...
"""
from dataclasses import dataclass, field
from pathlib import Path
import re
import sys

NEVER = None  # 条件的一种取值：无论类型实参是什么都不满足


@dataclass(frozen=True)
class Ty:
    """类型表达式：名字加类型实参。可空类型写作 Nullable<T>，借用写作 borrow<T> / inout<T>。"""
    name: str
    args: tuple = ()

    def __str__(self):
        if self.name == "Nullable":
            return f"{self.args[0]}?"
        return self.name + (f"<{', '.join(map(str, self.args))}>" if self.args else "")


@dataclass
class Decl:
    name: str
    params: list
    fields: list  # (字段名, Ty)
    kind: str  # class | value | enum | view
    deinit: bool = False
    unforgettable: bool = False
    must_consume: bool = False
    opaque: bool = False  # 库以 unsafe 实现：能力不由字段推导，未声明的一律不满足
    trusted: dict = field(default_factory=dict)  # 能力 -> 条件（受信任声明，覆盖推导）
    declared: dict = field(default_factory=dict)  # 可推导也可声明的能力：手工 impl Cap [if ...]
    implements: list = field(default_factory=list)  # 实现的接口名列表（is I, J）
    bounds: dict = field(default_factory=dict)  # 类型参数 -> 它须满足的能力或接口
    relaxed: dict = field(default_factory=dict)  # 类型参数 -> 它放弃的默认上界（view T 即放弃 Escapable）
    line: int = 0


# ---- 能力描述：新增一项能力只需在这里加一行

def follows(*caps_of_first_arg):
    """叶子公理的常见形状：当且仅当第一个类型实参满足这些能力。"""
    return frozenset((0, cap) for cap in caps_of_first_arg)


ALWAYS = frozenset()


@dataclass(frozen=True)
class Capability:
    name: str
    leaves: dict  # 内建类型 -> 条件；未列出的内建类型一律不满足
    deinit_ok: bool = True  # 声明了 deinit 的类型是否还能满足
    kinds: frozenset = frozenset({"class", "value", "enum", "view"})  # 哪些声明种类可按字段推导
    declarable: bool = False  # 可推导也可由普通代码安全声明（如 Equatable）；False 表示只能推导或 trusted


SCALARS = ("Int", "Boolean", "Char", "Unit")
CONTAINERS = ("Box", "List", "Array", "Nullable")


def capability(name, always=(), same=(), deinit_ok=True, kinds=("class", "value", "enum", "view"),
               declarable=False, **special):
    """always：无条件满足的内建类型；same：随第一个类型实参的同名能力；special：其余条件。"""
    leaves = {builtin: ALWAYS for builtin in always}
    leaves.update({builtin: follows(name) for builtin in same})
    leaves.update(special)
    return Capability(name, leaves, deinit_ok, frozenset(kinds), declarable)


PLAIN = SCALARS + ("String", "RawPtr", "borrow", "inout")  # 没有析构义务的叶子
OWNERS = CONTAINERS + ("RawCell", "Rc")  # 析构性质随载荷的内建 owner

CAPABILITIES = {cap.name: cap for cap in (
    capability("Copyable", SCALARS + ("borrow", "RawPtr"), ("Nullable",),
               deinit_ok=False, kinds=("value", "enum", "view")),
    capability("Transferable", SCALARS + ("String",), CONTAINERS + ("RawCell",),
               borrow=follows("Shareable"), inout=follows("Transferable")),
    capability("Shareable", SCALARS + ("String",), CONTAINERS,
               borrow=follows("Shareable"), inout=follows("Shareable")),
    # Any 代表载荷被擦除的存在类型：打包时要求载荷满足默认上界，所以这三项恒成立；其余一律未知
    capability("Escapable", SCALARS + ("String", "RawPtr", "Any"), OWNERS + ("Weak",)),
    # 析构的三项性质（评审 R3）：可否提前、可否遗忘、可否隐式析构
    # Weak 的析构只递减弱计数、不运行载荷的 deinit，所以恒可提前析构、可遗忘、可隐式析构
    capability("EarlyDrop", PLAIN + ("Weak",), OWNERS, deinit_ok=False),
    capability("Forgettable", PLAIN + ("Any", "Weak"), OWNERS),
    capability("ImplicitDrop", PLAIN + ("Any", "Weak"), OWNERS),
    # 可推导也可声明的能力：按字段自动推导，也可由用户以 impl 声明自定义等价
    capability("Equatable", SCALARS + ("String",), CONTAINERS + ("Rc", "RawCell"), declarable=True),
)}

# 内建类型对类型实参的要求：共享 owner 可能成环而永不析构，所以载荷必须可遗忘
# 类型参数的默认上界：不写任何东西的 T 是“普通值”——不携带来源、可以遗忘、可以隐式析构。
# 泛型代码要接受其余的类型，须逐项放弃：view T 放弃 Escapable，T: ?Cap 放弃 Cap。
DEFAULT_BOUNDS = ("Escapable", "Forgettable", "ImplicitDrop")
# 内建类型放弃的默认上界。独占 owner 的析构性质随载荷，所以放开后两项；
# Rc 不放开 Forgettable（共享 owner 可能成环而永不析构），也不放开 ImplicitDrop（没有唯一的消费者）。
OWNER_RELAX = {"Forgettable", "ImplicitDrop"}
BUILTIN_RELAX = {"Box": OWNER_RELAX, "List": OWNER_RELAX, "Array": OWNER_RELAX, "Nullable": OWNER_RELAX,
                 "RawCell": OWNER_RELAX, "RawPtr": set(DEFAULT_BOUNDS),
                 "borrow": set(DEFAULT_BOUNDS), "inout": set(DEFAULT_BOUNDS)}


class Problem(Exception):
    pass


# ---- 解析

TOKEN = re.compile(r"\s*(//[^\n]*|[A-Za-z_]\w*|[<>(),:?=+])")


class Parser:
    def __init__(self, text):
        self.lines = text.splitlines()

    def program(self):
        decls, interfaces, checks = {}, {}, []
        for number, raw in enumerate(self.lines, start=1):
            self.toks = [t for t in TOKEN.findall(raw) if not t.startswith("//")]
            self.pos, self.number = 0, number
            if not self.toks:
                continue
            if self.toks[0] == "expect":
                checks.append(self.expectation())
            elif self.toks[0] == "interface":
                self.next()
                iname = self.next()
                ibounds = set()
                if self.accept(":"):
                    while True:
                        ibounds.add(self.next())
                        if not self.accept("+"):
                            break
                interfaces[iname] = (ibounds, self.number)
            else:
                decl = self.decl()
                decls[decl.name] = decl
        self.interfaces = interfaces
        return decls, checks

    def peek(self):
        return self.toks[self.pos] if self.pos < len(self.toks) else None

    def next(self):
        tok = self.peek()
        if tok is None:
            raise Problem(f"第 {self.number} 行：意外的行尾")
        self.pos += 1
        return tok

    def accept(self, tok):
        if self.peek() == tok:
            self.pos += 1
            return True
        return False

    def expect(self, tok):
        if not self.accept(tok):
            raise Problem(f"第 {self.number} 行：期望 {tok!r}，得到 {self.peek()!r}")

    def type(self):
        if self.peek() in ("borrow", "inout"):
            return Ty(self.next(), (self.type(),))
        if self.peek() == "any":
            self.next()
            return Ty("Any", (Ty(self.next()),))
        ty = Ty(self.next())
        if self.accept("<"):
            args = []
            while not self.accept(">"):
                args.append(self.type())
                self.accept(",")
            ty = Ty(ty.name, tuple(args))
        while self.accept("?"):
            ty = Ty("Nullable", (ty,))
        return ty

    def condition(self):
        """yes | no | if T: Cap, U: Cap"""
        word = self.next()
        if word in ("yes", "no"):
            return ALWAYS if word == "yes" else NEVER
        if word != "if":
            raise Problem(f"第 {self.number} 行：期望 yes、no 或 if，得到 {word!r}")
        needs = set()
        while True:
            param = self.next()
            self.expect(":")
            needs.add((param, self.next()))
            if not self.accept(","):
                return frozenset(needs)

    def decl(self):
        kind = "class"
        if self.peek() in ("value", "view", "opaque"):
            kind = self.next()
        if self.accept("enum"):
            kind = "enum"
        else:
            self.expect("class")
        decl = Decl(self.next(), [], [], "class" if kind == "opaque" else kind,
                    opaque=kind == "opaque", line=self.number)
        if self.accept("<"):
            while not self.accept(">"):
                relaxed = {"Escapable"} if self.accept("view") else set()
                decl.params.append(self.next())
                bounds = set()
                if self.accept(":"):  # T: Cap + I + ?Cap
                    while True:
                        (relaxed if self.accept("?") else bounds).add(self.next())
                        if not self.accept("+"):
                            break
                decl.bounds[decl.params[-1]] = bounds
                decl.relaxed[decl.params[-1]] = relaxed
                self.accept(",")
        if self.accept("("):
            index = 0
            while not self.accept(")"):
                name, mode = f"_{index}", None
                if self.peek() in ("borrow", "inout") and self.toks[self.pos + 2:self.pos + 3] == [":"]:
                    mode = self.next()
                if self.toks[self.pos + 1:self.pos + 2] == [":"]:
                    name = self.next()
                    self.expect(":")
                ty = self.type()
                decl.fields.append((name, Ty(mode, (ty,)) if mode else ty))
                index += 1
                self.accept(",")
        while self.peek() is not None:
            word = self.next()
            if word == "is":
                while True:
                    decl.implements.append(self.next())
                    if not self.accept(","):
                        break
            elif word == "deinit":
                decl.deinit = True
            elif word == "unforgettable":
                decl.unforgettable = True
            elif word == "consume":
                decl.must_consume = True
            elif word in ("trusted", "impl"):
                cap = self.next()
                needs = self.condition() if self.peek() in ("yes", "no", "if") else ALWAYS
                target = decl.trusted if word == "trusted" else decl.declared
                target[cap] = needs if needs is NEVER else frozenset(
                    (decl.params.index(p), c) for p, c in needs)
            else:
                raise Problem(f"第 {self.number} 行：不认识的修饰 {word!r}")
        return decl

    def expectation(self):
        """expect Cap Type = 条件；expect wf Type = yes|no；expect why Cap Type = 字段路径；expect judge Cap [<T: ...>] Type = yes|no|unknown|error"""
        self.expect("expect")
        what = self.next()
        if what == "judge":
            cap = self.next()
            params, assumed = [], {}
            if self.accept("<"):
                while not self.accept(">"):
                    p = self.next()
                    params.append(p)
                    b = set()
                    if self.accept(":"):
                        while True:
                            b.add(self.next())
                            if not self.accept("+"):
                                break
                    assumed[p] = b
                    self.accept(",")
            ty = self.type()
            self.expect("=")
            return ("judge", cap, ty, (tuple(params), assumed, self.next()), self.number)
        cap = self.next() if what == "why" else what
        ty = self.type()
        self.expect("=")
        if what == "why":
            return (what, cap, ty, self.lines[self.number - 1].split("=", 1)[1].strip(), self.number)
        return ("cap", cap, ty, self.condition(), self.number)


# ---- 推导

UNARY_BUILTINS = set(CONTAINERS) | {"RawCell", "Rc", "Weak", "RawPtr", "borrow", "inout"}


class Engine:
    def __init__(self, decls, capabilities=None, interfaces=None):
        self.decls = decls
        self.caps = capabilities or CAPABILITIES
        self.interfaces = {name: set(bounds) for name, (bounds, _) in (interfaces or {}).items()}
        self.problems = []
        for decl in decls.values():
            for cap in decl.trusted:
                if cap not in self.caps:
                    raise Problem(f"第 {decl.line} 行：未知的能力 {cap}")
            for cap in decl.declared:
                if cap not in self.caps:
                    raise Problem(f"第 {decl.line} 行：未知的能力 {cap}")
                if not self.caps[cap].declarable:
                    self.problems.append((decl.line, f"{decl.name}：{cap} 只能自动推导或由 trusted 声明，不能手工 impl"))
            if decl.unforgettable and not decl.deinit:
                self.problems.append((decl.line, f"{decl.name}：不可遗忘的类型必须声明 deinit，义务由它履行"))
        # 每个声明、每项能力的条件：frozenset{(参数下标, 能力)} 或 NEVER。
        # 从“无条件满足”出发反复收紧到不动点（最大不动点），递归类型因此按“假设自己满足”处理。
        self.conds = {(name, cap): ALWAYS for name in decls for cap in self.caps}
        changed = True
        while changed:
            changed = False
            for decl in decls.values():
                for cap in self.caps:
                    new = self.derive(decl, cap)
                    if new != self.conds[decl.name, cap]:
                        self.conds[decl.name, cap] = new
                        changed = True

    def derive(self, decl, cap):
        if cap in decl.trusted:
            return decl.trusted[cap]
        if cap in decl.declared and self.caps[cap].declarable:
            return decl.declared[cap]
        spec = self.caps[cap]
        if decl.opaque or decl.kind not in spec.kinds or (decl.deinit and not spec.deinit_ok):
            return NEVER
        if self.declared_out(decl, cap):
            return NEVER
        needs = set()
        for _, ty in decl.fields:
            part = self.requires(ty, cap, decl.params)
            if part is NEVER:
                return NEVER
            needs |= part
        return frozenset(needs)

    @staticmethod
    def declared_out(decl, cap):
        """声明上的标记直接排除的能力。必须显式消费的值没有隐式析构，也就谈不上提前。"""
        return (cap == "Forgettable" and decl.unforgettable) or \
            (cap in ("ImplicitDrop", "EarlyDrop") and decl.must_consume)

    def expand_bounds(self, raw_bounds):
        """将类型参数上的接口上界展开为它蕴含的能力上界。"""
        caps, ifaces = set(), set()
        for b in raw_bounds:
            if b in self.interfaces:
                ifaces.add(b)
                caps |= self.interfaces[b]
            else:
                caps.add(b)
        return caps, ifaces

    def type_error(self, ty, params=()):
        """静态检查类型表达式是否引用未声明类型或实参个数不符（四值判定中的 error）。"""
        if ty.name in params:
            return f"类型参数 {ty.name} 不能带类型实参" if ty.args else None
        if ty.name == "Any" and ty.args:
            iname = ty.args[0].name
            return None if iname in self.interfaces else f"未知的接口 {iname}"
        if ty.name in self.decls:
            wanted = len(self.decls[ty.name].params)
            if len(ty.args) != wanted:
                return f"{ty.name} 期望 {wanted} 个类型实参，得到 {len(ty.args)} 个"
        elif ty.name in self.all_builtins():
            wanted = 1 if ty.name in UNARY_BUILTINS else 0
            if len(ty.args) != wanted:
                return f"{ty.name} 期望 {wanted} 个类型实参，得到 {len(ty.args)} 个"
        else:
            return f"未知的类型 {ty.name}"
        for arg in ty.args:
            err = self.type_error(arg, params)
            if err:
                return err
        return None

    def requires(self, ty, cap, params=()):
        """ty 满足 cap 的条件，以 params 的下标表示；没有类型参数时结果只会是 ALWAYS 或 NEVER。"""
        if ty.name in params:
            return frozenset({(params.index(ty.name), cap)})
        if ty.name == "Any" and ty.args:
            iname = ty.args[0].name
            if iname not in self.interfaces:
                raise Problem(f"未知的接口 {iname}")
            if cap in self.interfaces[iname]:
                return ALWAYS
            return ALWAYS if ty.name in self.caps[cap].leaves else NEVER
        if ty.name in self.decls:
            cond = self.conds[ty.name, cap]
        else:
            leaves = self.caps[cap].leaves
            if ty.name not in leaves:
                if ty.name not in self.all_builtins():
                    raise Problem(f"未知的类型 {ty.name}")
                return NEVER
            cond = leaves[ty.name]
        if cond is NEVER:
            return NEVER
        needs = set()
        for index, wanted in cond:
            part = self.requires(ty.args[index], wanted, params)
            if part is NEVER:
                return NEVER
            needs |= part
        return frozenset(needs)

    def judge(self, ty, cap, params=(), assumed=None):
        """四值判定：yes（满足）| no（不满足）| unknown（依赖未约束类型参数）| error（类型或能力错误）。"""
        if cap not in self.caps or self.type_error(ty, params):
            return "error"
        needs = self.requires(ty, cap, params)
        if needs is NEVER:
            return "no"
        if not needs:
            return "yes"
        assumed = assumed or {}
        for index, wanted in needs:
            caps, _ = self.expand_bounds(assumed.get(params[index], ()))
            if wanted not in caps:
                return "unknown"
        return "yes"

    def all_builtins(self):
        return {name for cap in self.caps.values() for name in cap.leaves} | set(BUILTIN_RELAX) | {"Rc", "Weak"}

    def bounds_of(self, decl):
        """声明的每个类型参数须满足的能力（含接口上界蕴含的能力）加上没有放弃的默认上界。"""
        defaults = {cap for cap in DEFAULT_BOUNDS if cap in self.caps}
        result = {}
        for p in decl.params:
            caps, _ = self.expand_bounds(decl.bounds[p])
            result[p] = caps | (defaults - decl.relaxed[p])
        return result

    def why_not(self, ty, cap, params=(), seen=()):
        """诊断用：从 ty 走到第一个使 cap 不成立的叶子，返回经过的字段路径。"""
        if ty.name in params or self.requires(ty, cap, params) is not NEVER:
            return None
        decl = self.decls.get(ty.name)
        if (ty.name, cap) in seen:  # 回到正在解释的类型：环本身不是原因，继续看别的字段
            return None
        if decl is None or cap in decl.trusted:
            cond = self.caps[cap].leaves.get(ty.name) if decl is None else decl.trusted.get(cap, NEVER)
            for index, wanted in cond or ():
                inner = self.why_not(ty.args[index], wanted, params, seen)
                if inner is not None:
                    return [str(ty)] + inner
            return None if cond else [f"{ty}（不满足 {cap}）"]
        if decl.opaque:
            return [f"{ty}（以 unsafe 实现，未声明 {cap}）"]
        spec = self.caps[cap]
        if decl.kind not in spec.kinds:
            return [f"{ty}（{decl.kind} 声明不满足 {cap}）"]
        if (decl.deinit and not spec.deinit_ok) or self.declared_out(decl, cap):
            return [f"{ty}（声明使它不满足 {cap}）"]
        binding = dict(zip(decl.params, ty.args))
        for name, fty in decl.fields:
            inner = self.why_not(substitute(fty, binding), cap, params, seen + ((ty.name, cap),))
            if inner is not None:
                return [f"{ty}.{name}"] + inner
        return [f"{ty}（不满足 {cap}）"]

    def well_formed(self, ty, params=(), assumed=None, assumed_ifaces=None):
        """类型实参是否满足各声明的能力上界与接口上界；返回第一处不满足的说明，或 None。"""
        assumed = assumed or {}
        assumed_ifaces = assumed_ifaces or {}
        if ty.name in params:
            return None
        err = self.type_error(ty, params)
        if err:
            return err
        if ty.name == "Any":
            return None
        for arg in ty.args:
            inner = self.well_formed(arg, params, assumed, assumed_ifaces)
            if inner:
                return inner
        decl = self.decls.get(ty.name)
        if decl is not None:
            bounds = self.bounds_of(decl)
            wanted = {i: bounds[p] for i, p in enumerate(decl.params)}
            wanted_ifaces = {i: self.expand_bounds(decl.bounds[p])[1] for i, p in enumerate(decl.params)}
        else:
            keep = {cap for cap in DEFAULT_BOUNDS if cap in self.caps} - BUILTIN_RELAX.get(ty.name, set())
            wanted = {i: keep for i in range(len(ty.args))}
            wanted_ifaces = {}
        for index, caps in wanted.items():
            for cap in sorted(caps):
                needs = self.requires(ty.args[index], cap, params)
                if needs is NEVER or any(c not in assumed.get(params[i], ()) for i, c in needs):
                    return f"{ty}：实参 {ty.args[index]} 须满足 {cap}"
        for index, ifaces in wanted_ifaces.items():
            arg = ty.args[index]
            for iface in sorted(ifaces):
                if arg.name in params:
                    if iface not in assumed_ifaces.get(arg.name, ()):
                        return f"{ty}：类型参数 {arg.name} 须实现接口 {iface}"
                elif arg.name == "Any" and arg.args:
                    if arg.args[0].name != iface:
                        return f"{ty}：实参 {arg} 未实现接口 {iface}"
                elif arg.name not in self.decls or iface not in self.decls[arg.name].implements:
                    return f"{ty}：实参 {arg} 未实现接口 {iface}"
        return None

    def check_declarations(self):
        """声明自身的合法性：字段类型良构；接口实现满足其能力上界；各析构维度之间的约束成立。"""
        for decl in self.decls.values():
            assumed = self.bounds_of(decl)
            assumed_ifaces = {p: self.expand_bounds(decl.bounds[p])[1] for p in decl.params}
            for name, ty in decl.fields:
                bad = self.well_formed(ty, tuple(decl.params), assumed, assumed_ifaces)
                if bad:
                    self.problems.append((decl.line, f"{decl.name}.{name}：{bad}"))
            for iface in decl.implements:
                if iface not in self.interfaces:
                    self.problems.append((decl.line, f"{decl.name}：实现了未声明的接口 {iface}"))
                    continue
                for cap in sorted(self.interfaces[iface]):
                    needs = self.conds[decl.name, cap]
                    if needs is NEVER or any(c not in assumed[decl.params[i]] for i, c in needs):
                        self.problems.append((decl.line, f"{decl.name} 声明实现 {iface}，但不满足其能力上界 {cap}"))
            escapable = self.conds[decl.name, "Escapable"] if "Escapable" in self.caps else ALWAYS
            open_params = {i for i, p in enumerate(decl.params) if "Escapable" in decl.relaxed[p]}
            if decl.kind != "view" and not decl.opaque and (
                    escapable is NEVER or any(i in open_params for i, _ in escapable)):
                self.problems.append((decl.line, f"{decl.name}：可能携带来源的类型须声明为 view class"))
            holds = {cap: self.conds[decl.name, cap] is not NEVER for cap in self.caps}
            for premise, conclusion in (("Copyable", "EarlyDrop"), ("Copyable", "Forgettable"),
                                        ("Copyable", "ImplicitDrop"), ("EarlyDrop", "Forgettable"),
                                        ("EarlyDrop", "ImplicitDrop")):
                if premise in holds and holds[premise] and not holds[conclusion]:
                    self.problems.append((decl.line, f"{decl.name}：满足 {premise} 却不满足 {conclusion}"))
        return self.problems


def substitute(ty, binding):
    if ty.name in binding:
        return binding[ty.name]
    return Ty(ty.name, tuple(substitute(arg, binding) for arg in ty.args))


def show(cond, params=()):
    if cond is NEVER:
        return "no"
    if not cond:
        return "yes"
    return "if " + ", ".join(f"{params[i] if isinstance(i, int) else i}: {cap}" for i, cap in sorted(
        cond, key=lambda item: (str(item[0]), item[1])))


def check(text):
    """返回 (行号, 说明) 列表：与预期不符之处。

    声明行上的 `// ERR` 表示预期该声明被拒绝；expect 行给出预期的判定结果。
    """
    parser = Parser(text)
    decls, checks = parser.program()
    engine = Engine(decls, interfaces=parser.interfaces)
    problems = engine.check_declarations()
    marked = {n for n, raw in enumerate(text.splitlines(), start=1) if "// ERR" in raw}
    failures = [item for item in problems if item[0] not in marked]
    failures += [(n, "预期被拒绝的声明通过了") for n in sorted(marked - {line for line, _ in problems})]
    for what, cap, ty, wanted, line in checks:
        if what == "why":
            got = " -> ".join(engine.why_not(ty, cap) or ["满足"])
            if got != wanted:
                failures.append((line, f"why {cap} {ty}：得到 {got}"))
            continue
        if what == "judge":
            jparams, jassumed, expected_val = wanted
            got = engine.judge(ty, cap, jparams, jassumed)
            if got != expected_val:
                failures.append((line, f"judge {cap} {ty}：期望 {expected_val}，得到 {got}"))
            continue
        if cap == "wf":
            got = "yes" if engine.well_formed(ty) is None else "no"
            params = ()
        elif ty.name in decls and not ty.args and decls[ty.name].params:
            params = decls[ty.name].params  # 只写泛型声明的名字：询问它对类型参数的条件
            got = show(engine.conds[ty.name, cap], params)
        else:
            params = ()
            got = show(engine.requires(ty, cap))
        if got != show(wanted):
            failures.append((line, f"{cap} {ty}：期望 {show(wanted)}，得到 {got}"))
    return failures


def main(argv):
    status = 0
    for path in argv[1:]:
        failures = check(Path(path).read_text(encoding="utf-8"))
        print(f"{path}: {'通过' if not failures else '不符'}")
        for line, message in failures:
            print(f"  第 {line} 行：{message}")
        status |= bool(failures)
    return status


if __name__ == "__main__":
    sys.exit(main(sys.argv))
