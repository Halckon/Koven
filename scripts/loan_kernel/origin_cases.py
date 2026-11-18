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
    yield from loop_backedges()


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
