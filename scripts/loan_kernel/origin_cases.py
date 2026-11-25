"""来源记账的用例生成器。

非规范、不被编译器引用。借用原型至今的漏洞都是“来源漏记”，集中在三件事上：值被存进某处、
借用被再传一手、所指不在本函数里（docs/proposals/loan-kernel-calculus.md §22.4）。
这里按这三条轴枚举，每个组合生成一段“读出借用后动它的来源”的程序（须被拒绝）和一段
来源无关的对照程序（须被接受）。预期由规则推出，写成与手写用例相同的 `// ERR 规则` 标记。

用法：python3 origin_cases.py            列出全部组合与不适用的组合
      python3 origin_cases.py <用例名>   打印一段生成的程序
"""
from __future__ import annotations

import sys

PRELUDE = """\
class Arr(len: Int)
view class V(borrow a: Arr)
view class W(inner: V, borrow c: Arr)
view class P(borrow target: V)
view class H(inout target: V)
view class Env(borrow a: Arr)
view class Bag<view T>(n: Int)
cell view class Slot<view T>(n: Int)
sig <view F> Body(borrow f: F): borrow Int from f.origins
fun lenOf(borrow a: Arr): borrow Int from a { return &a.len }
fun readV(borrow v: V): borrow Int from v.a { return &v.a.len }
fun readW(borrow w: W): borrow Int from w.inner.a { return &w.inner.a.len }
fun readP(borrow p: P): borrow Int from p.target.a { return &p.target.a.len }
fun readH(borrow h: H): borrow Int from h.target.a { return &h.target.a.len }
fun envBody(borrow env: Env): borrow Int from env.a { return &env.a.len }
fun <view F> call(borrow f: F, own body: fn Body): borrow Int from f.origins { return body(f) }
fun pass(inout v: V): inout V from v { return v }
fun setV(inout v: V, borrow s: Arr): Unit flows s into v { *v = V(s) }
fun passEnv(inout env: Env): inout Env from env { return env }
fun setEnv(inout v: Env, borrow s: Arr): Unit flows s into v { *v = Env(s) }
fun passBag(inout bag: Bag): inout Bag from bag { return bag }
fun <view T> put(inout bag: Bag, own x: T): Unit flows x into bag trusted
fun peekBag(borrow bag: Bag): borrow Int from bag trusted
fun <view T> set(borrow slot: Slot, own x: T): Unit flows x into slot trusted
fun peekSlot(borrow slot: Slot): borrow Int from slot trusted
view class BoxBag(inner: Bag)
view class BoxSlot(inner: Slot)
fun <T> id(own x: T): T { return move x }
fun keepBag(own x: Bag): Bag from x trusted
fun keepSlot(own x: Slot): Slot from x trusted
"""

# 轴一：来源存在哪里。持有者一律叫 h；{s} 是要跟踪的来源，{o} 是另一个来源（旧值或旁支）。
#   direct  构造时就持有 {s}
#   seed    先持有 {o}，再由 write 把 {s} 写进 target（flow：直接写；alias：经局部借用值 w 写）
#   assign  先持有 {o}，再在函数体内直接改写（所指唯一，旧来源解除）
#   tail    写入之后才建立的外层持有者
#   path    从 h 到那份借用的字段路径；没有 path 的是不透明的持有者，只能经受信任函数读
#   args    “经视图字段作实参”的各种写法
#   caller  持有者也在调用方时：它的参数模式、流向的目标、函数体内改写的写法
STORAGES = {
    "局部视图的借用字段": dict(
        cls="V", path="a", reader="readV({h})", direct=["val h = V({s})"], seed=["val h = V({o})"],
        target="&mut h", write="setV({t}, {s})", assign=["val w = &mut h", "*w = V({s})"],
        args={"字段": "lenOf(h.a)"}, caller=dict(mode="inout", target="h", reborrow="pass(h)", assign=["*h = V({s})"])),
    "按值嵌套的视图": dict(
        cls="W", path="inner.a", reader="readW({h})", direct=["val h = W(V({s}), {o})"],
        seed=["val h = W(V({o}), {o})"], target="&mut h.inner", write="setV({t}, {s})",
        assign=["h.inner = V({s})"], args={"字段": "lenOf(h.inner.a)", "内层视图": "readV(&h.inner)"},
        caller=dict(mode="inout", target="&mut h.inner", reborrow="pass(&mut h.inner)", assign=["h.inner = V({s})"])),
    "共享字段所指的另一个视图": dict(
        cls="P", path="target.a", reader="readP({h})", direct=["val v = V({s})", "val h = P(&v)"],
        seed=["val v = V({o})"], tail=["val h = P(&v)"], target="&mut v", write="setV({t}, {s})",
        assign=["val w = &mut v", "*w = V({s})"],
        args={"字段": "lenOf(h.target.a)", "所指视图": "readV(h.target)", "拥有者": "readV(&v)"}),
    "独占字段所指的另一个视图": dict(
        cls="H", path="target.a", reader="readH({h})", direct=["val v = V({s})", "val h = H(&mut v)"],
        seed=["val v = V({o})", "val h = H(&mut v)"], target="h.target", alias_target="&mut h.target",
        write="setV({t}, {s})", assign=["h.target = V({s})"],
        args={"字段": "lenOf(h.target.a)", "所指视图": "readV(h.target)", "拥有者": "readV(&v)"},
        caller=dict(mode="inout", target="h.target", reborrow="pass(h.target)", assign=["h.target = V({s})"])),
    "闭包环境": dict(
        cls="Env", path="a", reader="call({h}, envBody)", direct=["val h = Env({s})"], seed=["val h = Env({o})"],
        target="&mut h", write="setEnv({t}, {s})", assign=["val w = &mut h", "*w = Env({s})"],
        args={"字段": "lenOf(h.a)"}, caller=dict(mode="inout", target="h", reborrow="passEnv(h)", assign=["*h = Env({s})"])),
    "类型参数的持有者": dict(
        cls="Bag", path=None, reader="peekBag({h})", seed=["val h = Bag(0)"], target="&mut h",
        write="put({t}, V({s}))", caller=dict(mode="inout", target="h", reborrow="passBag(h)")),
    "内部可变的槽": dict(
        cls="Slot", path=None, reader="peekSlot({h})", seed=["val h = Slot(0)"], target="&h",
        write="set({t}, V({s}))", caller=dict(mode="borrow", target="h")),
}
BUILDS = ("构造", "流向", "经借用值的流向", "函数体内改写")
MERGING = ("流向", "经借用值的流向")  # 调用方只能并入：旧来源 {o} 仍在持有者名下
HOPS = ("直接读", "经借用值", "经调用返回")
HELD_BY_CALLER = "，持有者也在调用方"
LOCATIONS = ("局部变量", "borrow 参数", "inout 参数", "borrow 参数" + HELD_BY_CALLER, "inout 参数" + HELD_BY_CALLER,
             "按值的视图参数")


def build(spec, kind, s, o, held):
    """轴一与“经流向写入后再读”：把来源 s 放进持有者的语句；不适用时返回说明。

    held 为真时持有者是参数 h，本函数不创建它。
    """
    fill = lambda lines: [line.format(s=s, o=o) for line in lines]
    caller = spec.get("caller", {})
    if kind == "构造":
        if held:
            return "持有者由调用方交入，不在本函数里构造"
        return fill(spec["direct"]) if "direct" in spec else "不透明的持有者只能经流向放入"
    if kind == "函数体内改写":
        if "assign" not in spec:
            return "不透明的持有者的内容只能经受信任函数访问"
        return fill(caller["assign"] if held else spec["seed"] + spec["assign"] + spec.get("tail", []))
    target = caller["target"] if held else spec["target"]
    if kind == "流向":
        lines = [spec["write"].format(t=target, s="{s}")]
    elif held and "reborrow" in caller:
        lines = [f"val w = {caller['reborrow']}", spec["write"].format(t="w", s="{s}")]
    elif held and caller["mode"] == "inout":
        return "对 inout 参数再取借用是借用的借用，演算未覆盖"
    else:
        lines = [f"val w = {target if held else spec.get('alias_target', target)}", spec["write"].format(t="w", s="{s}")]
    return fill(([] if held else spec["seed"]) + lines + ([] if held else spec.get("tail", [])))


def hops(spec):
    return HOPS + tuple(f"经视图字段作实参（{name}）" for name in spec.get("args", {}))


def read(spec, hop, held, own_param=False):
    """轴二：经几手读出借用 e。"""
    href, path = ("h" if held else "&h"), spec["path"]
    if hop == "直接读":
        return [f"val e = &h.{path}.len"] if path else "不透明的持有者没有可直接读的字段"
    if hop == "经借用值":
        if held:
            return "持有者参数本身就是借用值，与“经调用返回”同一段程序"
        return ["val r = &h", f"val e = &r.{path}.len" if path else f"val e = {spec['reader'].format(h='r')}"]
    if hop == "经调用返回":
        return [f"val e = {spec['reader'].format(h=href)}"]
    name = hop[len("经视图字段作实参（"):-1]
    if (held or own_param) and name == "拥有者":
        return "所指视图的拥有者在调用方"
    return [f"val e = {spec['args'][name]}"]


def function(head, body, tail):
    return "\n".join([head] + [f"  {line}" for line in body + tail] + ["}"]) + "\n"


def variants(spec, kind, hop, location):
    """轴三：所指在哪里。返回 {结论: 函数文本}，或不适用的说明。预期都由规则推出：

    局部变量：读出后写来源须被拒绝（K1）；写无关的变量、或借用不再使用后写来源，须被接受。
    borrow / inout 参数：读出的借用返回给调用方，签名声明了别的来源须被拒绝（K6）；inout 参数另有
    一段“读出后经参数写来源”，须被拒绝（K4：参数的 loan 正被子借用暂停）。
    持有者也在调用方：来源流进了调用方的持有者，流向声明了别的来源须被拒绝（K6）；读出的借用同时
    依赖持有者原有的来源与新来源，漏掉新来源须被拒绝（K6）；调用方的持有者在返回之后仍然存活，
    所以放入之后经 inout 参数写来源须被拒绝（K4），与读出的借用是否还在使用无关。
    按值的视图参数：持有者连同来源一起由调用方交入；不声明来源就返回须被拒绝（K6）。
    """
    opaque, path = spec["path"] is None, spec["path"]
    if location == "按值的视图参数":
        if opaque:
            return "读出的借用依赖按值参数自己的存储，返回不了调用方"
        if kind != "构造":
            return "持有者由调用方交入，本函数不放入来源"
        got = read(spec, hop, False, True)
        if isinstance(got, str):
            return got
        return {
            "接受": function(f"fun f(own h: {spec['cls']}): borrow Int from h.{path} {{", got, ["return e"]),
            "拒绝-未声明来源": function(f"fun f(own h: {spec['cls']}): borrow Int {{", got, ["return e  // ERR K6"]),
        }
    local, held = location == "局部变量", location.endswith(HELD_BY_CALLER)
    if held and "caller" not in spec:
        return "共享字段所指的视图不能经它改写（SIG），来源无从放入"
    if opaque and not local and not held:
        return "读出的借用依赖局部的持有者，返回不了调用方"
    placed = build(spec, kind, "&s" if local else "s", "&o" if local else "o", held)
    got = read(spec, hop, held)
    for problem in (placed, got):
        if isinstance(problem, str):
            return problem
    body = placed + got
    if local:
        setup = ["val s = Arr(1)", "val o = Arr(2)", "val x = Arr(3)"]
        return {
            "接受": function("fun f(): Unit {", setup + body, ["x.len = 9", "use(e)", "s.len = 9"]),
            "拒绝-写来源": function("fun f(): Unit {", setup + body, ["s.len = 9  // ERR K1", "use(e)"]),
        }
    mode = location.split()[0]
    params = f"{mode} s: Arr, {mode} o: Arr"
    merged = kind in MERGING
    if held:
        params = f"{spec['caller']['mode']} h: {spec['cls']}, {params}"
        kept = f"h.{path}" if path else "h"  # 持有者原有的来源
        sources = kept if opaque else (f"{kept}, s" if merged else "s")
        good = f"fun f({params}): borrow Int from {sources} flows s into h {{"
        found = {"接受": function(good, body, ["return e"]),
                 "拒绝-未声明流向": function(f"fun f({params}): borrow Int from {sources} flows o into h {{  // ERR K6",
                                      body, ["return e"])}
        if not opaque:
            found["拒绝-未声明来源"] = function(f"fun f({params}): borrow Int from {kept} flows s into h {{",
                                         body, ["return e  // ERR K6"])
    else:
        good = f"fun f({params}): borrow Int from {'s, o' if merged else 's'} {{"
        found = {"接受": function(good, body, ["return e"]),
                 "拒绝-未声明来源": function(f"fun f({params}): borrow Int from o {{", body, ["return e  // ERR K6"])}
    if mode == "inout":
        found["拒绝-写来源"] = function(good, body, ["s.len = 9  // ERR K4", "return e"])
        if held:  # 读出的借用已不再使用，调用方的持有者仍借着来源
            found["拒绝-放入后写来源"] = function(good, body, ["use(e)", "s.len = 9  // ERR K4", "return lenOf(o)"]
                                           ).replace(f"from {sources} ", "from o ")
    return found


def enumerate_axes():
    """全部组合：(名字, {结论: 完整程序} 或不适用的说明)。"""
    for storage, spec in STORAGES.items():
        for kind in BUILDS:
            for hop in hops(spec):
                for location in LOCATIONS:
                    found = variants(spec, kind, hop, location)
                    if not isinstance(found, str):
                        found = {verdict: PRELUDE + text for verdict, text in found.items()}
                    yield f"{storage}/{kind}/{hop}/{location}", found


def escapes():
    """容器（Bag、Slot）代表以携带来源的类型实例化的泛型类型：类型上不可逃逸，携带的来源随值走。

    交给可逃逸的类型参数须被拒绝（K17），空容器也一样，因为这是类型上的判断；按值转交、存进视图的
    按值字段、声明来源后返回，之后读出的借用仍保护来源（K1）；不声明就转交或返回须被拒绝（K6）。
    """
    for storage in ("类型参数的持有者", "内部可变的槽"):
        spec = STORAGES[storage]
        cls = spec["cls"]
        peek = lambda h: f"val e = {spec['reader'].format(h=h)}"
        setup = ["val s = Arr(1)"] + spec["seed"]
        placed = setup + [spec["write"].format(t=spec["target"], s="&s")]
        named = lambda name, text: (f"{storage}/{name}", PRELUDE + text)
        yield named("交给可逃逸的类型参数/拒绝", function("fun f(): Unit {", placed, ["val g = id(move h)  // ERR K17"]))
        yield named("空容器交给可逃逸的类型参数/拒绝", function("fun f(): Unit {", setup, ["val g = id(move h)  // ERR K17"]))
        moves = {"按值转交": f"val g = keep{cls}(move h)", "存进视图的按值字段": f"val g = Box{cls}(move h)"}
        for name, line in moves.items():
            body = placed + [line, peek("&g.inner" if "Box" in line else "&g")]
            yield named(f"{name}/接受", function("fun f(): Unit {", body, ["use(e)", "s.len = 9"]))
            yield named(f"{name}/拒绝", function("fun f(): Unit {", body, ["s.len = 9  // ERR K1", "use(e)"]))
        yield named("转交的函数体/接受", function(f"fun f(own x: {cls}): {cls} from x {{", [], ["return move x"]))
        yield named("转交的函数体/拒绝", function(f"fun f(own x: {cls}): {cls} {{", [], ["return move x  // ERR K6"]))
        yield named("存进普通类的字段/拒绝", f"class Plain(inner: {cls})  // ERR CAP\n")
        make = [spec["write"].format(t=spec["target"], s="s")]
        yield named("未声明来源就返回/拒绝", function(
            f"fun f(borrow s: Arr): {cls} {{", spec["seed"] + make, ["return move h  // ERR K6"]))
        declared = function(f"fun make(borrow s: Arr): {cls} from s {{", spec["seed"] + make, ["return move h"])
        caller = ["val s = Arr(1)", "val g = make(&s)", peek("&g")]
        yield named("声明来源后返回/接受", declared + function("fun f(): Unit {", caller, ["use(e)", "s.len = 9"]))
        yield named("声明来源后返回/拒绝", declared + function("fun f(): Unit {", caller, ["s.len = 9  // ERR K1", "use(e)"]))
    # 裁决（2026-11-16）：流向的目标与带来源的结果，类型上须能携带来源
    yield "签名/类型上不能携带来源/拒绝", PRELUDE + """\
class Plain(n: Int)
fun <view T> intoPlain(inout p: Plain, own x: T): Unit flows x into p trusted  // ERR SIG
fun <T, view U> intoEscapable(inout out: T, own x: U): Unit flows x into out trusted  // ERR SIG
fun plainResult(borrow s: Arr): Plain from s trusted  // ERR SIG
"""


def branch_joins():
    """局部直接赋值只在写入分支解除旧来源，使合流的两条边都可单独观测。"""
    body = ["val o = Arr(1)", "val s = Arr(2)", "val x = Arr(3)", "val h = V(&o)",
            "if (*) {", "  h = V(&s)", "}", "val e = readV(&h)"]
    for name, tail in (
        ("写旧来源", ["o.len = 9  // ERR K1", "use(e)"]),
        ("写新来源", ["s.len = 9  // ERR K1", "use(e)"]),
        ("写无关来源", ["x.len = 9", "use(e)"]),
        ("最后使用后写", ["use(e)", "o.len = 9", "s.len = 9"]),
    ):
        yield f"分支合流/局部视图/{name}", PRELUDE + function("fun f(): Unit {", body, tail)


def conditional_caller_flows():
    """只有 then 放入，且之后不读 h：存活的是调用方持有者名下的来源。"""
    head = "fun f(inout h: V, inout s: Arr, inout x: Arr): Unit"
    placed = ["if (*) {", "  setV(h, s)", "}"]
    for name, signature, body in (
        ("拒绝-放入后写来源", " flows s into h {", placed + ["s.len = 9  // ERR K4"]),
        ("拒绝-未声明流向", " {  // ERR K6", placed),
        ("写无关来源", " flows s into h {", placed + ["x.len = 9"]),
        ("未放入分支写来源", " flows s into h {", placed[:-1] + ["} else {", "  s.len = 9", "}"]),
    ):
        yield f"分支合流/调用方视图/不读回/inout 参数{HELD_BY_CALLER}/{name}", PRELUDE + function(head + signature, body, [])


DEEP_PRELUDE = PRELUDE + """view class Outer(inout link: H)
fun readOuter(borrow q: Outer): borrow Int from q.link.target.a { return &q.link.target.a.len }
"""


def deep_view_reads():
    """先建完整链，再替换唯一所指的 V；读回须认新来源而非构造时的旧来源。"""
    setup = ["val o = Arr(0)", "val s = Arr(1)", "val v = V(&o)", "val h = H(&mut v)",
             "val q = Outer(&mut h)", "q.link.target = V(&s)"]
    for hop, expr in (("直接读", "&q.link.target.a.len"), ("经调用返回", "readOuter(&q)")):
        for access, tail in (("写新来源", ["s.len = 9  // ERR K1", "use(e)"]),
                             ("写旧来源", ["o.len = 9", "use(e)"]),
                             ("最后使用后写", ["use(e)", "s.len = 9"])):
            yield f"三层借用字段/{hop}/{access}", DEEP_PRELUDE + function("fun f(): Unit {", setup + [f"val e = {expr}"], tail)


def owner_pair(extra):
    return ["val l = Arr(1)", "val r = Arr(2)", extra, "val left = V(&l)", "val right = V(&r)",
            "val h = H(&mut left)"]


def joined_owner_setup(extra):
    return owner_pair(extra) + ["if (*) {", "  h = H(&mut right)", "}", "val q = Outer(&mut h)"]


def joined_owner_reads():
    """合流改变 h.target 所指的视图，不只是改变一个视图字段里的来源。"""
    setup = joined_owner_setup("val x = Arr(3)")
    for hop, expr in (("直接读", "&q.link.target.a.len"), ("经调用返回", "readOuter(&q)")):
        for access, tail in (("写左来源", ["l.len = 9  // ERR K1", "use(e)"]),
                             ("写右来源", ["r.len = 9  // ERR K1", "use(e)"]),
                             ("写无关来源", ["x.len = 9", "use(e)"]),
                             ("最后使用后写", ["use(e)", "l.len = 9", "r.len = 9"])):
            yield f"多拥有者读回/{hop}/{access}", DEEP_PRELUDE + function("fun f(): Unit {", setup + [f"val e = {expr}"], tail)


def joined_owner_writes(group, write):
    """合流未消除时，本地赋值与 flows 调用都须考虑本侧未被写入的路径。"""
    setup = joined_owner_setup("val s = Arr(3)") + [write]
    yield from owner_readbacks(group, setup, ("left", "right"))


def branch_local_flow():
    """只有仍选 left 的分支执行流向调用，不能把 s 计入 right。"""
    setup = owner_pair("val s = Arr(3)") + ["if (*) {", "  val q = Outer(&mut h)",
            "  setV(q.link.target, &s)", "} else {", "  h = H(&mut right)", "}"]
    yield from owner_readbacks("分支内流向", setup, ("left",))


def distinct_arm_flows():
    """两臂各有一次流向调用；合流后从具体拥有者读，不能把另一臂的来源混进来。"""
    setup = owner_pair("val s = Arr(3)") + ["val t = Arr(4)", "if (*) {", "  val a = Outer(&mut h)",
            "  setV(a.link.target, &s)", "} else {", "  h = H(&mut right)", "  val b = Outer(&mut h)",
            "  setV(b.link.target, &t)", "}"]
    for owner, protected in (("left", ("l", "s")), ("right", ("r", "t"))):
        body = setup + [f"val e = readV(&{owner})"]
        for source in ("l", "r", "s", "t"):
            mark = "  // ERR K1" if source in protected else ""
            yield f"双臂不同来源/{owner}/{source}", DEEP_PRELUDE + function(
                "fun f(): Unit {", body, [f"{source}.len = 9" + mark, "use(e)"])
        tail = ["use(e)"] + [f"{source}.len = 9" for source in protected]
        yield f"双臂不同来源/{owner}/最后使用后写", DEEP_PRELUDE + function("fun f(): Unit {", body, tail)


def owner_readbacks(group, setup, written):
    for owner, old, other in (("left", "l", "r"), ("right", "r", "l")):
        body = setup + [f"val e = readV(&{owner})"]
        mark = "  // ERR K1" if owner in written else ""
        for access, tail in (("写新来源", ["s.len = 9" + mark, "use(e)"]),
                             ("写本侧旧来源", [f"{old}.len = 9  // ERR K1", "use(e)"]),
                             ("写另一侧旧来源", [f"{other}.len = 9", "use(e)"]),
                             ("最后使用后写", ["use(e)", "s.len = 9", f"{old}.len = 9"])):
            yield f"{group}/{owner}/{access}", DEEP_PRELUDE + function("fun f(): Unit {", body, tail)


def contract_view_reads():
    """返回视图按 from x 保守建账：x 是 W，不是 target 实际指向的 V。"""
    decls = PRELUDE + "fun expose(borrow x: W): P from x { return P(&x.inner) }\n"
    setup = ["val s = Arr(1)", "val o = Arr(2)", "val w = W(V(&s), &o)",
             "val p = expose(&w)", "val e = readP(&p)"]
    for name, tail in (("写来源", ["s.len = 9  // ERR K1", "use(e)"]),
                       ("最后使用后写", ["use(e)", "s.len = 9"])):
        yield f"合同来源/返回子视图/{name}", decls + function("fun f(): Unit {", setup, tail)


def guard_failure_points():
    """所有正常路径都移走 g；只改变清理顺序，以及 mayFail 所在的路径。"""
    declarations = """class Arr(len: Int)
view class Guard(borrow a: Arr) deinit
deinit Guard { use(this.a.len) }
fun setGuard(inout g: Guard, borrow s: Arr): Unit flows s into g trusted
fun release(own g: Guard): Unit trusted
fun mayFail(): Unit trusted
"""
    for late in (False, True):
        order = "来源后声明" if late else "来源先声明"
        source, holder = "val s = Arr(1)", "val g = Guard(&o)"
        setup = ["val o = Arr(0)"] + ([holder, source] if late else [source, holder])
        mark = "  // ERR K22" if late else ""
        for point in ("写入分支", "未写分支", "合流后", "移走后"):
            body = setup + ["if (*) {", "  setGuard(&mut g, &s)" + mark]
            if point == "写入分支":
                body.append("  mayFail()" + mark)
            elif point == "未写分支":
                body.extend(["} else {", "  mayFail()"])
            body.append("}")
            if point == "合流后":
                body.append("mayFail()" + mark)
            body.append("release(move g)")
            if point == "移走后":
                body.append("mayFail()")
            yield f"失败清理/分支放入/{order}/{point}", declarations + function("fun f(): Unit {", body, [])


def loop_backedges():
    """先把中间视图交给 h，再替换中间视图；本轮的新来源只能在下轮到达 h。"""
    body = ["val o = Arr(1)", "val m = Arr(2)", "val s = Arr(3)", "val x = Arr(4)",
            "val h = V(&o)", "val mid = V(&m)", "while (*) {", "  h = mid", "  mid = V(&s)",
            "}", "val e = readV(&h)"]
    for name, tail in (
        ("写零轮来源", ["o.len = 9  // ERR K1", "use(e)"]),
        ("写一轮来源", ["m.len = 9  // ERR K1", "use(e)"]),
        ("写二轮来源", ["s.len = 9  // ERR K1", "use(e)"]),
        ("写无关来源", ["x.len = 9", "use(e)"]),
        ("最后使用后写", ["use(e)", "o.len = 9", "m.len = 9", "s.len = 9"]),
    ):
        yield f"循环回边/局部视图/{name}", PRELUDE + function("fun f(): Unit {", body, tail)


GENERIC_PRELUDE = PRELUDE + """fun <view T> readGeneric(borrow h: T): borrow Int from h.origins trusted
fun <view T> passGeneric(inout h: T): inout T from h { return h }
fun <view T> flowGeneric(inout h: T, borrow s: Arr): Unit flows s into h trusted
fun <view T> keepGeneric(own x: T): T from x { return move x }
fun <view T> dropGeneric(own x: T): Unit trusted
fun <view T> genFlow(inout h: T, borrow s: Arr): Unit flows s into h { flowGeneric(h, s) }
fun <view T> genFlowReadReborrow(inout h: T, borrow s: Arr): borrow Int from h.origins, s flows s into h {
  val w = passGeneric(h)
  flowGeneric(w, s)
  return readGeneric(w)
}
suspend fun <view T> suspendGenFlow(inout h: T, borrow s: Arr): Unit flows s into h trusted
"""


def generic_view_holders():
    """泛型函数体内以 view T 为持有者：按值/局部/借用参数、重借流向、分支/循环流向及具体调用方实例化。"""
    named = lambda name, text: (f"泛型持有者/{name}", GENERIC_PRELUDE + text)

    # 1. 按值 view T 参数：读回、整值转发、逃逸拦截、装入容器、流向调用方容器、参数来源流入
    yield named("按值参数/读回/接受", function(
        "fun <view T> f(own h: T): borrow Int from h {", ["val e = readGeneric(&h)"], ["return e"]))
    yield named("按值参数/读回/拒绝-未声明来源", function(
        "fun <view T> f(own h: T): borrow Int {", ["val e = readGeneric(&h)"], ["return e  // ERR K6"]))
    yield named("按值参数/整值转交/接受", function(
        "fun <view T> f(own h: T): T from h {", [], ["return move h"]))
    yield named("按值参数/整值转交/拒绝-未声明来源", function(
        "fun <view T> f(own h: T): T {", [], ["return move h  // ERR K6"]))
    yield named("按值参数/交给可逃逸类型参数/拒绝", function(
        "fun <view T> f(own h: T): Unit {", [], ["val g = id(move h)  // ERR K17"]))
    yield named("按值参数/装入容器返回/接受", function(
        "fun <view T> f(own h: T): Bag from h {", ["val b = Bag(0)", "put(&mut b, move h)"], ["return move b"]))
    yield named("按值参数/装入容器返回/拒绝-未声明来源", function(
        "fun <view T> f(own h: T): Bag {", ["val b = Bag(0)", "put(&mut b, move h)"], ["return move b  // ERR K6"]))
    yield named("按值参数/流向调用方容器/接受", function(
        "fun <view T> f(inout bag: Bag, own h: T): Unit flows h into bag {", [], ["put(bag, move h)"]))
    yield named("按值参数/流向调用方容器/拒绝-未声明流向", function(
        "fun <view T> f(inout bag: Bag, own h: T): Unit {  // ERR K6", [], ["put(bag, move h)"]))
    yield named("按值参数/流入参数来源/接受", function(
        "fun <view T> f(own h: T, borrow s: Arr): borrow Int from h, s {",
        ["flowGeneric(&mut h, s)", "val e = readGeneric(&h)"], ["return e"]))
    yield named("按值参数/流入参数来源/拒绝-漏掉新来源", function(
        "fun <view T> f(own h: T, borrow s: Arr): borrow Int from h {",
        ["flowGeneric(&mut h, s)", "val e = readGeneric(&h)"], ["return e  // ERR K6"]))
    yield named("按值参数/流入参数来源/拒绝-漏掉原有来源", function(
        "fun <view T> f(own h: T, borrow s: Arr): borrow Int from s {",
        ["flowGeneric(&mut h, s)", "val e = readGeneric(&h)"], ["return e  // ERR K6"]))

    # 2. 局部 view T 持有者：无 EarlyDrop 上界，按 K3 可观察活到析构或显式移走；声明晚于持有者报 K22/K9
    local_body = ["val s = Arr(1)", "val x = Arr(2)", "val h = keepGeneric(move init)",
                  "flowGeneric(&mut h, &s)", "val e = readGeneric(&h)"]
    for label, tail in (
        ("读前写来源", ["s.len = 9  // ERR K1", "use(e)"]),
        ("读后未释放写来源", ["use(e)", "s.len = 9  // ERR K1"]),
        ("释放后写来源", ["use(e)", "dropGeneric(move h)", "s.len = 9"]),
        ("写无关来源", ["x.len = 9", "use(e)"]),
    ):
        yield named(f"局部/先声明来源后放入/{label}", function(
            "fun <view T> f(own init: T): Unit {", local_body, tail))
    yield named("局部/先声明持有者后放入/拒绝-失败与退出清理", function(
        "fun <view T> f(own init: T): Unit {  // ERR K9",
        ["val h = keepGeneric(move init)", "val s = Arr(1)",
         "flowGeneric(&mut h, &s)  // ERR K22", "val e = readGeneric(&h)  // ERR K22"], ["use(e)"]))
    yield named("局部/按值参数放入局部来源/拒绝-失败与退出清理", function(
        "fun <view T> f(own h: T): Unit {  // ERR K9",
        ["val s = Arr(1)", "flowGeneric(&mut h, &s)  // ERR K22",
         "val e = readGeneric(&h)  // ERR K22"], ["use(e)"]))

    # 3. 借用 view T 参数：只读转发、直线/重借流向、分支整体替换、分支/循环流向
    yield named("借用参数/只读转发/接受", function(
        "fun <view T> f(borrow h: T, borrow o: Arr): borrow Int from h.origins {",
        ["val e = readGeneric(h)"], ["return e"]))
    yield named("借用参数/只读转发/拒绝-未声明来源", function(
        "fun <view T> f(borrow h: T, borrow o: Arr): borrow Int from o {",
        ["val e = readGeneric(h)"], ["return e  // ERR K6"]))

    for kind, lines in (
        ("流向", ["flowGeneric(h, s)", "val e = readGeneric(h)"]),
        ("经借用值的流向", ["val w = passGeneric(h)", "flowGeneric(w, s)", "val e = readGeneric(h)"]),
        ("经同一重借读回", ["val w = passGeneric(h)", "flowGeneric(w, s)", "val e = readGeneric(w)"]),
    ):
        for mode in ("borrow", "inout"):
            params = f"inout h: T, {mode} s: Arr, {mode} o: Arr"
            loc = f"{mode} 参数{HELD_BY_CALLER}"
            good = f"fun <view T> f({params}): borrow Int from h.origins, s flows s into h {{"
            yield named(f"独占参数/{kind}/{loc}/接受", function(good, lines, ["return e"]))
            yield named(f"独占参数/{kind}/{loc}/拒绝-未声明流向", function(
                f"fun <view T> f({params}): borrow Int from h.origins, s flows o into h {{  // ERR K6",
                lines, ["return e"]))
            yield named(f"独占参数/{kind}/{loc}/拒绝-未声明来源", function(
                f"fun <view T> f({params}): borrow Int from h.origins flows s into h {{",
                lines, ["return e  // ERR K6"]))
            if mode == "inout":
                yield named(f"独占参数/{kind}/{loc}/拒绝-写来源", function(
                    good, lines, ["s.len = 9  // ERR K4", "return e"]))
                yield named(f"独占参数/{kind}/{loc}/拒绝-放入后写来源", function(
                    f"fun <view T> f({params}): borrow Int from o flows s into h {{",
                    lines, ["use(e)", "s.len = 9  // ERR K4", "return lenOf(o)"]))

    swap_body = ["if (*) {", "  *h = move v", "}", "val e = readGeneric(h)"]
    yield named("独占参数/分支替换/接受", function(
        "fun <view T> f(inout h: T, own v: T): borrow Int from h.origins, v flows v into h {",
        swap_body, ["return e"]))
    yield named("独占参数/分支替换/拒绝-漏掉新值来源", function(
        "fun <view T> f(inout h: T, own v: T): borrow Int from h.origins flows v into h {",
        swap_body, ["return e  // ERR K6"]))
    yield named("独占参数/分支替换/拒绝-漏掉原有来源", function(
        "fun <view T> f(inout h: T, own v: T): borrow Int from v flows v into h {",
        swap_body, ["return e  // ERR K6"]))
    yield named("独占参数/分支替换/拒绝-未声明流向", function(
        "fun <view T> f(inout h: T, own v: T): borrow Int from h.origins, v {  // ERR K6",
        swap_body, ["return e"]))

    branch_head = "fun <view T> f(inout h: T, inout s: Arr, inout t: Arr, inout x: Arr): Unit"
    branch_body = ["if (*) {", "  flowGeneric(h, s)", "} else {", "  flowGeneric(h, t)", "}"]
    for label, sig, tail in (
        ("接受", " flows s into h, t into h {", ["x.len = 9"]),
        ("拒绝-漏掉右臂流向", " flows s into h {  // ERR K6", []),
        ("拒绝-合流后写左臂来源", " flows s into h, t into h {", ["s.len = 9  // ERR K4"]),
        ("拒绝-合流后写右臂来源", " flows s into h, t into h {", ["t.len = 9  // ERR K4"]),
    ):
        yield named(f"独占参数/分支双臂流向/inout 参数{HELD_BY_CALLER}/{label}",
                    function(branch_head + sig, branch_body, tail))

    loop_head = "fun <view T> f(inout h: T, inout s: Arr, inout x: Arr): Unit"
    loop_body = ["while (*) {", "  flowGeneric(h, s)", "}"]
    for label, sig, tail in (
        ("接受", " flows s into h {", ["x.len = 9"]),
        ("拒绝-未声明流向", " {  // ERR K6", []),
        ("拒绝-循环后写来源", " flows s into h {", ["s.len = 9  // ERR K4"]),
    ):
        yield named(f"独占参数/循环流向/inout 参数{HELD_BY_CALLER}/{label}",
                    function(loop_head + sig, loop_body, tail))

    # 4. 具体调用方实例化 view T 泛型流向函数（V、Bag、Slot，含经同一重借读回与不可携带来源类型 Arr 对照）
    for label, seed, reader in (
        ("局部视图", ["val h = V(&o)"], "readV(&h)"),
        ("类型参数的持有者", ["val h = Bag(0)", "put(&mut h, V(&o))"], "peekBag(&h)"),
        ("内部可变的槽", ["val h = Slot(0)", "set(&h, V(&o))"], "peekSlot(&h)"),
    ):
        body = ["val o = Arr(1)", "val s = Arr(2)", "val x = Arr(3)"] + seed + [
            "genFlow(&mut h, &s)", f"val e = {reader}"]
        reborrow_body = ["val o = Arr(1)", "val s = Arr(2)", "val x = Arr(3)"] + seed + [
            "val e = genFlowReadReborrow(&mut h, &s)"]
        for sub, cur_body in (("", body), ("经同一重借读回/", reborrow_body)):
            for access, tail in (
                ("写新来源", ["s.len = 9  // ERR K1", "use(e)"]),
                ("写旧来源", ["o.len = 9  // ERR K1", "use(e)"]),
                ("写无关来源", ["x.len = 9", "use(e)"]),
                ("最后使用后写", ["use(e)", "o.len = 9", "s.len = 9"]),
            ):
                yield named(f"调用方实例化/{sub}{label}/{access}", function("fun f(): Unit {", cur_body, tail))

    yield named("调用方实例化/不可携带来源类型/局部 Arr", function(
        "fun f(): Unit {", ["val h = Arr(1)", "val s = Arr(2)"], ["genFlow(&mut h, &s)  // ERR SIG"]))
    yield named("调用方实例化/不可携带来源类型/参数 Arr 未声明流向", function(
        "fun f(inout h: Arr, borrow s: Arr): Unit {", [], ["genFlow(h, s)  // ERR SIG"]))
    yield named("调用方实例化/不可携带来源类型/参数 Arr 声明流向", function(
        "fun f(inout h: Arr, borrow s: Arr): Unit flows s into h {  // ERR SIG", [],
        ["genFlow(h, s)  // ERR SIG"]))
    yield named("调用方实例化/不可携带来源类型/可逃逸泛型参数", function(
        "fun <T> f(inout h: T, borrow s: Arr): Unit {", [], ["genFlow(h, s)  // ERR SIG"]))
    yield named("调用方实例化/不可携带来源类型/无捕获闭包", function(
        "fun f(): Unit {", ["val h = closure()", "val s = Arr(1)"], ["genFlow(&mut h, &s)  // ERR SIG"]))
    yield named("调用方实例化/不可携带来源类型/函数类型", function(
        "fun f(own g: fn Body, borrow s: Arr): Unit {", [], ["genFlow(&mut g, s)  // ERR SIG"]))
    yield named("调用方实例化/不可携带来源类型/start 局部 Arr", function(
        "suspend fun f(): Unit {", ["val h = Arr(1)", "val s = Arr(2)",
                                    "val t = start suspendGenFlow(&mut h, &s)  // ERR SIG"], ["await t"]))
    yield named("调用方实例化/不可携带来源类型/start 无捕获闭包", function(
        "suspend fun f(): Unit {", ["val h = closure()", "val s = Arr(2)",
                                    "val t = start suspendGenFlow(&mut h, &s)  // ERR SIG"], ["await t"]))

    # 5. 借用容器形参经泛型读取转发（borrow/inout Bag 与 Slot、嵌套容器重借读回与整值替换清旧来源）
    for label, cls, seed in (
        ("类型参数的持有者", "Bag", ["val h = Bag(0)", "put(&mut h, V(&o))"]),
        ("内部可变的槽", "Slot", ["val h = Slot(0)", "set(&h, V(&o))"]),
    ):
        for mode in ("borrow", "inout"):
            yield named(f"借用容器经泛型转发/{label}/{mode}/接受", function(
                f"fun wrap({mode} h: {cls}): borrow Int from h.origins {{", [], ["return readGeneric(h)"]))
            yield named(f"借用容器经泛型转发/{label}/{mode}/拒绝-未声明来源", function(
                f"fun wrap({mode} h: {cls}): borrow Int {{", [], ["return readGeneric(h)  // ERR K6"]))
        wrap = function(f"fun wrap(borrow h: {cls}): borrow Int from h.origins {{", [], ["return readGeneric(h)"])
        call_body = ["val o = Arr(1)"] + seed + ["val e = wrap(&h)"]
        yield named(f"借用容器经泛型转发/{label}/调用方写来源", wrap + function(
            "fun f(): Unit {", call_body, ["o.len = 9  // ERR K1", "use(e)"]))
        yield named(f"借用容器经泛型转发/{label}/调用方最后使用后写", wrap + function(
            "fun f(): Unit {", call_body, ["use(e)", "o.len = 9"]))
    yield named("借用容器经泛型转发/嵌套容器重借读回/接受", function(
        "fun f(inout h: BoxBag, borrow s: Arr): borrow Int from h.inner, s flows s into h.inner {",
        ["val w = passGeneric(&mut h.inner)", "flowGeneric(w, s)"], ["return readGeneric(w)"]))
    yield named("借用容器经泛型转发/嵌套容器重借读回/拒绝-漏掉新来源", function(
        "fun f(inout h: BoxBag, borrow s: Arr): borrow Int from h.inner flows s into h.inner {",
        ["val w = passGeneric(&mut h.inner)", "flowGeneric(w, s)"], ["return readGeneric(w)  // ERR K6"]))
    yield named("借用容器经泛型转发/嵌套容器重借读回/局部写来源", function(
        "fun f(): Unit {", ["val s = Arr(1)", "val h = BoxBag(Bag(0))", "val w = passGeneric(&mut h.inner)",
                            "flowGeneric(w, &s)", "val e = readGeneric(w)"], ["s.len = 9  // ERR K1", "use(e)"]))
    yield named("借用容器经泛型转发/整值替换清空旧来源/接受", function(
        "fun f(inout h: Bag): borrow Int {", ["*h = Bag(0)"], ["return readGeneric(h)"]))
    yield named("借用容器经泛型转发/整值替换清空旧来源/单臂仍保留旧来源", function(
        "fun f(inout h: Bag): borrow Int {", ["if (*) {", "  *h = Bag(0)", "}"], ["return readGeneric(h)  // ERR K6"]))


def branch_and_loop_flows():
    """循环与分支里的 flows 放入：跨视图、不透明容器与内部可变槽，覆盖单臂、双臂、循环与二轮回边。"""
    holders = (
        dict(name="局部视图", cls="V", mode="inout",
             seed=["val h = V(&o)"],
             local_flow=lambda s: f"setV(&mut h, &{s})",
             local_mid_flow="setV(&mut h, mid.a)",
             caller_flow=lambda s: f"setV(h, {s})",
             reader="readV(&h)"),
        dict(name="类型参数的持有者", cls="Bag", mode="inout",
             seed=["val h = Bag(0)", "put(&mut h, V(&o))"],
             local_flow=lambda s: f"put(&mut h, V(&{s}))",
             local_mid_flow="put(&mut h, mid)",
             caller_flow=lambda s: f"put(h, V({s}))",
             reader="peekBag(&h)"),
        dict(name="内部可变的槽", cls="Slot", mode="borrow",
             seed=["val h = Slot(0)", "set(&h, V(&o))"],
             local_flow=lambda s: f"set(&h, V(&{s}))",
             local_mid_flow="set(&h, mid)",
             caller_flow=lambda s: f"set(h, V({s}))",
             reader="peekSlot(&h)"),
    )
    for h in holders:
        name, reader = h["name"], h["reader"]
        base = ["val o = Arr(1)", "val s = Arr(2)", "val x = Arr(3)"] + h["seed"]

        # 1. 局部持有者 + 分支单臂 flows
        one_arm = base + ["if (*) {", f"  {h['local_flow']('s')}", "}", f"val e = {reader}"]
        for access, tail in (
            ("写旧来源", ["o.len = 9  // ERR K1", "use(e)"]),
            ("写新来源", ["s.len = 9  // ERR K1", "use(e)"]),
            ("写无关来源", ["x.len = 9", "use(e)"]),
            ("最后使用后写", ["use(e)", "o.len = 9", "s.len = 9"]),
        ):
            yield f"分支流向/单臂/{name}/{access}", PRELUDE + function("fun f(): Unit {", one_arm, tail)

        # 2. 局部持有者 + 分支双臂不同来源 flows（两臂都写入仍须保留旧来源 o）
        two_arm = ["val o = Arr(1)", "val s = Arr(2)", "val t = Arr(3)", "val x = Arr(4)"] + h["seed"] + [
            "if (*) {", f"  {h['local_flow']('s')}", "} else {", f"  {h['local_flow']('t')}", "}",
            f"val e = {reader}"]
        for access, tail in (
            ("写旧来源", ["o.len = 9  // ERR K1", "use(e)"]),
            ("写左臂来源", ["s.len = 9  // ERR K1", "use(e)"]),
            ("写右臂来源", ["t.len = 9  // ERR K1", "use(e)"]),
            ("写无关来源", ["x.len = 9", "use(e)"]),
            ("最后使用后写", ["use(e)", "o.len = 9", "s.len = 9", "t.len = 9"]),
        ):
            yield f"分支流向/双臂/{name}/{access}", PRELUDE + function("fun f(): Unit {", two_arm, tail)

        # 3. 局部持有者 + 循环内 flows
        loop_flow = base + ["while (*) {", f"  {h['local_flow']('s')}", "}", f"val e = {reader}"]
        for access, tail in (
            ("写旧来源", ["o.len = 9  // ERR K1", "use(e)"]),
            ("写新来源", ["s.len = 9  // ERR K1", "use(e)"]),
            ("写无关来源", ["x.len = 9", "use(e)"]),
            ("最后使用后写", ["use(e)", "o.len = 9", "s.len = 9"]),
        ):
            yield f"循环流向/单步/{name}/{access}", PRELUDE + function("fun f(): Unit {", loop_flow, tail)

        # 4. 局部持有者 + 循环二轮回边 flows（s 仅在第二轮经 mid 流入 h）
        loop_two = ["val o = Arr(1)", "val m = Arr(2)", "val s = Arr(3)", "val x = Arr(4)"] + h["seed"] + [
            "val mid = V(&m)", "while (*) {", f"  {h['local_mid_flow']}", "  mid = V(&s)", "}",
            f"val e = {reader}"]
        for access, tail in (
            ("写零轮来源", ["o.len = 9  // ERR K1", "use(e)"]),
            ("写一轮来源", ["m.len = 9  // ERR K1", "use(e)"]),
            ("写二轮来源", ["s.len = 9  // ERR K1", "use(e)"]),
            ("写无关来源", ["x.len = 9", "use(e)"]),
            ("最后使用后写", ["use(e)", "o.len = 9", "m.len = 9", "s.len = 9"]),
        ):
            yield f"循环流向/二轮回边/{name}/{access}", PRELUDE + function("fun f(): Unit {", loop_two, tail)

        # 5. 调用方持有者 + 分支双臂 / 循环 flows（不读回，验证 K4/K6/K20 存活）
        c_two_head = f"fun f({h['mode']} h: {h['cls']}, inout s: Arr, inout t: Arr, inout x: Arr): Unit"
        c_two_body = ["if (*) {", f"  {h['caller_flow']('s')}", "} else {", f"  {h['caller_flow']('t')}", "}"]
        for label, sig, tail in (
            ("写无关来源", " flows s into h, t into h {", ["x.len = 9"]),
            ("拒绝-漏掉右臂流向", " flows s into h {  // ERR K6", []),
            ("拒绝-合流后写左臂来源", " flows s into h, t into h {", ["s.len = 9  // ERR K4"]),
            ("拒绝-合流后写右臂来源", " flows s into h, t into h {", ["t.len = 9  // ERR K4"]),
        ):
            yield f"分支流向/调用方双臂/{name}/inout 参数{HELD_BY_CALLER}/{label}", PRELUDE + function(
                c_two_head + sig, c_two_body, tail)

        c_loop_head = f"fun f({h['mode']} h: {h['cls']}, inout s: Arr, inout x: Arr): Unit"
        c_loop_body = ["while (*) {", f"  {h['caller_flow']('s')}", "}"]
        for label, sig, tail in (
            ("写无关来源", " flows s into h {", ["x.len = 9"]),
            ("拒绝-未声明流向", " {  // ERR K6", []),
            ("拒绝-循环后写来源", " flows s into h {", ["s.len = 9  // ERR K4"]),
        ):
            yield f"循环流向/调用方/{name}/inout 参数{HELD_BY_CALLER}/{label}", PRELUDE + function(
                c_loop_head + sig, c_loop_body, tail)


def cases():
    """生成的程序：(名字, 文本)。"""
    for name, found in enumerate_axes():
        if not isinstance(found, str):
            for verdict, text in found.items():
                yield f"{name}/{verdict}", text
    yield from escapes()
    yield from branch_joins()
    yield from conditional_caller_flows()
    yield from guard_failure_points()
    yield from deep_view_reads()
    yield from joined_owner_reads()
    yield from joined_owner_writes("多目标写入", "q.link.target = V(&s)")
    yield from joined_owner_writes("多目标流向", "setV(q.link.target, &s)")
    yield from branch_local_flow()
    yield from distinct_arm_flows()
    yield from contract_view_reads()
    yield from loop_backedges()
    yield from generic_view_holders()
    yield from branch_and_loop_flows()


def skipped():
    """不适用的组合：(名字, 说明)。"""
    return [(name, found) for name, found in enumerate_axes() if isinstance(found, str)]


def main(argv):
    table = dict(cases())
    if len(argv) > 1:
        print(table[argv[1]], end="")
        return 0
    for name in table:
        print(name)
    for name, reason in skipped():
        print(f"不适用 {name}：{reason}")
    print(f"{len(table)} 段程序，{len(skipped())} 个组合不适用")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
