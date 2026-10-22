//! Three independent complete ordinary source documents and source-authored output oracles.
//! No source or expectation is extracted from the legacy mixed native whole.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Family { Node, Box, Rc }

pub(super) struct Fixture {
    pub(super) family: Family,
    pub(super) name: &'static str,
    pub(super) text: &'static str,
    pub(super) stdout: &'static [u8],
    pub(super) first_when: (usize, usize),
}

pub(super) fn fixtures() -> [Fixture; 3] {
    [
        Fixture { family: Family::Node, name: "ordinary-when-node.ko", text: NODE, stdout: NODE_STDOUT, first_when: (553, 669) },
        Fixture { family: Family::Box, name: "ordinary-when-box.ko", text: BOX, stdout: BOX_STDOUT, first_when: (597, 713) },
        Fixture { family: Family::Rc, name: "ordinary-when-rc.ko", text: RC, stdout: RC_STDOUT, first_when: (729, 845) },
    ]
}

pub(super) const NODE: &str = r#"class Node {}
class Prefix { deinit() { println("prefix-drop") } }
class Outer { deinit() { println("outer-drop") } }
fun touch(value: Node): Unit {}
fun touchNullable(value: Node?): Unit {}
fun observe(value: Node): Unit { println("read") }
fun consume(own value: Node): Unit { println("consume"); touch(value) }
fun accept(prefix: Prefix, own result: Int): Int {
    println("inner-call")
    return result
}
fun enclose(prefix: Outer, own result: Int): Int {
    println("outer-call")
    return result
}
fun repeated(own subject: Node?): Unit {
    when (subject) {
        null -> { println("null") };
        else -> { observe(subject); observe((subject)) }
    }
    var onceMore = true
    var again = true
    while (again) {
        again = onceMore
        onceMore = false
        when (subject) { null -> {}; else -> { observe(subject) } }
    }
    when (subject) { null -> {}; else -> { consume((subject)) } }
}
fun later(own subject: Node?): Unit {
    when (subject) {
        null -> { println("later-null") };
        else -> { println("later-present") }
    }
    println("later-use")
    touchNullable(subject)
}
fun earlyReturn(own subject: Node?): Unit {
    val result = enclose(Outer(), accept(Prefix(), when (subject) {
        null -> 7;
        else -> { println("return-present"); return }
    }))
    println("return-after")
}
fun earlyBreak(own present: Boolean): Unit {
    var once = true
    while (once) {
        once = false
        val subject = make(present)
        val result = enclose(Outer(), accept(Prefix(), when (subject) {
            null -> 7;
            else -> { println("break-present"); break }
        }))
        println("break-after")
    }
    println("break-tail")
}
fun earlyContinue(own present: Boolean): Unit {
    var once = true
    while (once) {
        once = false
        val subject = make(present)
        val result = enclose(Outer(), accept(Prefix(), when (subject) {
            null -> 7;
            else -> { println("continue-present"); continue }
        }))
        println("continue-after")
    }
    println("continue-tail")
}
fun make(own present: Boolean): Node? {
    println("make")
    if (present) {
        val result: Node? = Node()
        return result
    }
    return null
}
fun temporary(own present: Boolean): Unit {
    val result = enclose(Outer(), when (make(present)) {
        null -> { println("temporary-null"); 7 };
        else -> { println("temporary-present"); 8 }
    })
    println("temporary-after")
}
fun temporaryReturn(own present: Boolean): Unit {
    val result = enclose(Outer(), accept(Prefix(), when (make(present)) {
        null -> 7;
        else -> { println("temporary-return"); return }
    }))
    println("temporary-return-after")
}
fun temporaryBreak(own present: Boolean): Unit {
    var once = true
    while (once) {
        once = false
        val result = enclose(Outer(), accept(Prefix(), when (make(present)) {
            null -> 7;
            else -> { println("temporary-break"); break }
        }))
        println("temporary-break-after")
    }
    println("temporary-break-tail")
}
fun temporaryContinue(own present: Boolean): Unit {
    var once = true
    while (once) {
        once = false
        val result = enclose(Outer(), accept(Prefix(), when (make(present)) {
            null -> 7;
            else -> { println("temporary-continue"); continue }
        }))
        println("temporary-continue-after")
    }
    println("temporary-continue-tail")
}
fun doubleReturn(own subject: Node?): Unit {
    when (subject) {
        null -> { println("double-null"); return };
        else -> { observe(subject); return }
    }
}
fun temporaryDoubleReturn(own present: Boolean): Unit {
    when (make(present)) {
        null -> { println("temporary-double-null"); return };
        else -> { println("temporary-double-present"); return }
    }
}
fun delayed(own value: Node, own result: Int): Unit {
    println("take-commit")
    touch(value)
}
fun takenReturn(own subject: Node?, own stop: Boolean): Unit {
    when (subject) {
        null -> {};
        else -> { delayed((subject), when (stop) { true -> return; else -> 7 }) }
    }
    println("taken-after")
}
fun entry(): Unit {
    println("repeat-present")
    val repeatedPresent: Node? = Node()
    repeated(repeatedPresent)
    println("repeat-null")
    repeated(null)
    println("later-present-start")
    val laterPresent: Node? = Node()
    later(laterPresent)
    println("later-null-start")
    later(null)
    println("return-present-start")
    val returnedPresent: Node? = Node()
    earlyReturn(returnedPresent)
    println("return-null-start")
    earlyReturn(null)
    println("break-present-start")
    earlyBreak(true)
    println("break-null-start")
    earlyBreak(false)
    println("continue-present-start")
    earlyContinue(true)
    println("continue-null-start")
    earlyContinue(false)
    println("temporary-present-start")
    temporary(true)
    println("temporary-null-start")
    temporary(false)
    println("temporary-return-present-start")
    temporaryReturn(true)
    println("temporary-return-null-start")
    temporaryReturn(false)
    println("temporary-break-present-start")
    temporaryBreak(true)
    println("temporary-break-null-start")
    temporaryBreak(false)
    println("temporary-continue-present-start")
    temporaryContinue(true)
    println("temporary-continue-null-start")
    temporaryContinue(false)
    println("double-present-start")
    val doublePresent: Node? = Node()
    doubleReturn(doublePresent)
    println("double-null-start")
    doubleReturn(null)
    println("temporary-double-present-start")
    temporaryDoubleReturn(true)
    println("temporary-double-null-start")
    temporaryDoubleReturn(false)
    println("taken-return-start")
    val takenCancelled: Node? = Node()
    takenReturn(takenCancelled, true)
    println("taken-commit-start")
    val takenCommitted: Node? = Node()
    takenReturn(takenCommitted, false)
    println("taken-null-start")
    takenReturn(null, true)
    println("done")
}
"#;

pub(super) const NODE_STDOUT: &[u8] = b"repeat-present\nread\nread\nread\nread\nconsume\nrepeat-null\nnull\nlater-present-start\nlater-present\nlater-use\nlater-null-start\nlater-null\nlater-use\nreturn-present-start\nreturn-present\nprefix-drop\nouter-drop\nreturn-null-start\ninner-call\nprefix-drop\nouter-call\nouter-drop\nreturn-after\nbreak-present-start\nmake\nbreak-present\nprefix-drop\nouter-drop\nbreak-tail\nbreak-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\nbreak-after\nbreak-tail\ncontinue-present-start\nmake\ncontinue-present\nprefix-drop\nouter-drop\ncontinue-tail\ncontinue-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\ncontinue-after\ncontinue-tail\ntemporary-present-start\nmake\ntemporary-present\nouter-call\nouter-drop\ntemporary-after\ntemporary-null-start\nmake\ntemporary-null\nouter-call\nouter-drop\ntemporary-after\ntemporary-return-present-start\nmake\ntemporary-return\nprefix-drop\nouter-drop\ntemporary-return-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\ntemporary-return-after\ntemporary-break-present-start\nmake\ntemporary-break\nprefix-drop\nouter-drop\ntemporary-break-tail\ntemporary-break-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\ntemporary-break-after\ntemporary-break-tail\ntemporary-continue-present-start\nmake\ntemporary-continue\nprefix-drop\nouter-drop\ntemporary-continue-tail\ntemporary-continue-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\ntemporary-continue-after\ntemporary-continue-tail\ndouble-present-start\nread\ndouble-null-start\ndouble-null\ntemporary-double-present-start\nmake\ntemporary-double-present\ntemporary-double-null-start\nmake\ntemporary-double-null\ntaken-return-start\ntaken-commit-start\ntake-commit\ntaken-after\ntaken-null-start\ntaken-after\ndone\n";

pub(super) const BOX: &str = r#"value class Cell(val value: Int)
class Prefix { deinit() { println("prefix-drop") } }
class Outer { deinit() { println("outer-drop") } }
fun touch(value: Box<Cell>): Unit {}
fun touchNullable(value: Box<Cell>?): Unit {}
fun observe(value: Box<Cell>): Unit { println("read") }
fun consume(own value: Box<Cell>): Unit { println("consume"); touch(value) }
fun accept(prefix: Prefix, own result: Int): Int {
    println("inner-call")
    return result
}
fun enclose(prefix: Outer, own result: Int): Int {
    println("outer-call")
    return result
}
fun repeated(own subject: Box<Cell>?): Unit {
    when (subject) {
        null -> { println("null") };
        else -> { observe(subject); observe((subject)) }
    }
    var onceMore = true
    var again = true
    while (again) {
        again = onceMore
        onceMore = false
        when (subject) { null -> {}; else -> { observe(subject) } }
    }
    when (subject) { null -> {}; else -> { consume((subject)) } }
}
fun later(own subject: Box<Cell>?): Unit {
    when (subject) {
        null -> { println("later-null") };
        else -> { println("later-present") }
    }
    println("later-use")
    touchNullable(subject)
}
fun earlyReturn(own subject: Box<Cell>?): Unit {
    val result = enclose(Outer(), accept(Prefix(), when (subject) {
        null -> 7;
        else -> { println("return-present"); return }
    }))
    println("return-after")
}
fun earlyBreak(own present: Boolean): Unit {
    var once = true
    while (once) {
        once = false
        val subject = make(present)
        val result = enclose(Outer(), accept(Prefix(), when (subject) {
            null -> 7;
            else -> { println("break-present"); break }
        }))
        println("break-after")
    }
    println("break-tail")
}
fun earlyContinue(own present: Boolean): Unit {
    var once = true
    while (once) {
        once = false
        val subject = make(present)
        val result = enclose(Outer(), accept(Prefix(), when (subject) {
            null -> 7;
            else -> { println("continue-present"); continue }
        }))
        println("continue-after")
    }
    println("continue-tail")
}
fun make(own present: Boolean): Box<Cell>? {
    println("make")
    if (present) {
        val result: Box<Cell>? = Box(Cell(7))
        return result
    }
    return null
}
fun temporary(own present: Boolean): Unit {
    val result = enclose(Outer(), when (make(present)) {
        null -> { println("temporary-null"); 7 };
        else -> { println("temporary-present"); 8 }
    })
    println("temporary-after")
}
fun entry(): Unit {
    println("repeat-present")
    val repeatedPresent: Box<Cell>? = Box(Cell(7))
    repeated(repeatedPresent)
    println("repeat-null")
    repeated(null)
    println("later-present-start")
    val laterPresent: Box<Cell>? = Box(Cell(7))
    later(laterPresent)
    println("later-null-start")
    later(null)
    println("return-present-start")
    val returnedPresent: Box<Cell>? = Box(Cell(7))
    earlyReturn(returnedPresent)
    println("return-null-start")
    earlyReturn(null)
    println("break-present-start")
    earlyBreak(true)
    println("break-null-start")
    earlyBreak(false)
    println("continue-present-start")
    earlyContinue(true)
    println("continue-null-start")
    earlyContinue(false)
    println("temporary-present-start")
    temporary(true)
    println("temporary-null-start")
    temporary(false)
    println("done")
}
"#;

pub(super) const BOX_STDOUT: &[u8] = b"repeat-present\nread\nread\nread\nread\nconsume\nrepeat-null\nnull\nlater-present-start\nlater-present\nlater-use\nlater-null-start\nlater-null\nlater-use\nreturn-present-start\nreturn-present\nprefix-drop\nouter-drop\nreturn-null-start\ninner-call\nprefix-drop\nouter-call\nouter-drop\nreturn-after\nbreak-present-start\nmake\nbreak-present\nprefix-drop\nouter-drop\nbreak-tail\nbreak-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\nbreak-after\nbreak-tail\ncontinue-present-start\nmake\ncontinue-present\nprefix-drop\nouter-drop\ncontinue-tail\ncontinue-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\ncontinue-after\ncontinue-tail\ntemporary-present-start\nmake\ntemporary-present\nouter-call\nouter-drop\ntemporary-after\ntemporary-null-start\nmake\ntemporary-null\nouter-call\nouter-drop\ntemporary-after\ndone\n";

pub(super) const RC: &str = r#"class Prefix { deinit() { println("prefix-drop") } }
class Outer { deinit() { println("outer-drop") } }
fun touch(value: Rc<Int>): Unit {}
fun touchNullable(value: Rc<Int>?): Unit {}
fun observe(value: Rc<Int>): Unit { println("read") }
fun consume(own value: Rc<Int>): Unit {
    val copied = value.value
    val shared = value.share()
    val sharedCopy = shared.value
    if (copied == 41) { println("value") }
    if (sharedCopy == 41) { println("shared") }
    println("consume")
}
fun accept(prefix: Prefix, own result: Int): Int {
    println("inner-call")
    return result
}
fun enclose(prefix: Outer, own result: Int): Int {
    println("outer-call")
    return result
}
fun repeated(own subject: Rc<Int>?): Unit {
    when (subject) {
        null -> { println("null") };
        else -> { observe(subject); observe((subject)) }
    }
    var onceMore = true
    var again = true
    while (again) {
        again = onceMore
        onceMore = false
        when (subject) { null -> {}; else -> { observe(subject) } }
    }
    when (subject) { null -> {}; else -> { consume((subject)) } }
}
fun later(own subject: Rc<Int>?): Unit {
    when (subject) {
        null -> { println("later-null") };
        else -> { println("later-present") }
    }
    println("later-use")
    touchNullable(subject)
}
fun earlyReturn(own subject: Rc<Int>?): Unit {
    val result = enclose(Outer(), accept(Prefix(), when (subject) {
        null -> 7;
        else -> { println("return-present"); return }
    }))
    println("return-after")
}
fun earlyBreak(own present: Boolean): Unit {
    var once = true
    while (once) {
        once = false
        val subject = make(present)
        val result = enclose(Outer(), accept(Prefix(), when (subject) {
            null -> 7;
            else -> { println("break-present"); break }
        }))
        println("break-after")
    }
    println("break-tail")
}
fun earlyContinue(own present: Boolean): Unit {
    var once = true
    while (once) {
        once = false
        val subject = make(present)
        val result = enclose(Outer(), accept(Prefix(), when (subject) {
            null -> 7;
            else -> { println("continue-present"); continue }
        }))
        println("continue-after")
    }
    println("continue-tail")
}
fun make(own present: Boolean): Rc<Int>? {
    println("make")
    if (present) {
        val result: Rc<Int>? = Rc(41)
        return result
    }
    return null
}
fun temporary(own present: Boolean): Unit {
    val result = enclose(Outer(), when (make(present)) {
        null -> { println("temporary-null"); 7 };
        else -> { println("temporary-present"); 8 }
    })
    println("temporary-after")
}
fun entry(): Unit {
    println("repeat-present")
    val repeatedPresent: Rc<Int>? = Rc(41)
    repeated(repeatedPresent)
    println("repeat-null")
    repeated(null)
    println("later-present-start")
    val laterPresent: Rc<Int>? = Rc(41)
    later(laterPresent)
    println("later-null-start")
    later(null)
    println("return-present-start")
    val returnedPresent: Rc<Int>? = Rc(41)
    earlyReturn(returnedPresent)
    println("return-null-start")
    earlyReturn(null)
    println("break-present-start")
    earlyBreak(true)
    println("break-null-start")
    earlyBreak(false)
    println("continue-present-start")
    earlyContinue(true)
    println("continue-null-start")
    earlyContinue(false)
    println("temporary-present-start")
    temporary(true)
    println("temporary-null-start")
    temporary(false)
    println("done")
}
"#;

pub(super) const RC_STDOUT: &[u8] = b"repeat-present\nread\nread\nread\nread\nvalue\nshared\nconsume\nrepeat-null\nnull\nlater-present-start\nlater-present\nlater-use\nlater-null-start\nlater-null\nlater-use\nreturn-present-start\nreturn-present\nprefix-drop\nouter-drop\nreturn-null-start\ninner-call\nprefix-drop\nouter-call\nouter-drop\nreturn-after\nbreak-present-start\nmake\nbreak-present\nprefix-drop\nouter-drop\nbreak-tail\nbreak-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\nbreak-after\nbreak-tail\ncontinue-present-start\nmake\ncontinue-present\nprefix-drop\nouter-drop\ncontinue-tail\ncontinue-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\ncontinue-after\ncontinue-tail\ntemporary-present-start\nmake\ntemporary-present\nouter-call\nouter-drop\ntemporary-after\ntemporary-null-start\nmake\ntemporary-null\nouter-call\nouter-drop\ntemporary-after\ndone\n";


// Every negative is a separate full source document, with byte-exact stage boundaries.
pub(super) struct Negative {
    pub(super) name: &'static str,
    pub(super) text: &'static str,
    pub(super) stage: &'static str,
    pub(super) code: &'static str,
    pub(super) red_span: (usize, usize),
    pub(super) green_span: (usize, usize),
}

pub(super) const NEGATIVES: &[Negative] = &[
    Negative {
        name: "borrow-root",
        text: r#"class Node {}
fun entry(subject: Node?): Unit { when (subject) { null -> {}; else -> {} } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        red_span: (48, 89),
        green_span: (48, 89),
    },
    Negative {
        name: "inout-root",
        text: r#"class Node {}
fun entry(inout subject: Node?): Unit { when (subject) { null -> {}; else -> {} } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        red_span: (24, 44),
        green_span: (24, 44),
    },
    Negative {
        name: "ordinary-field",
        text: r#"class Node {}
class Holder(val value: Node?)
fun entry(holder: Holder): Unit { when (holder.value) { null -> {}; else -> {} } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        red_span: (79, 125),
        green_span: (79, 125),
    },
    Negative {
        name: "container-element",
        text: r#"class Node {}
fun entry(items: List<Node?>): Unit { when (items[0]) { null -> {}; else -> {} } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        red_span: (52, 94),
        green_span: (52, 94),
    },
    Negative {
        name: "inline-nullable",
        text: r#"value class Cell(val value: Int)
fun entry(own subject: Cell?): Unit { when (subject) { null -> {}; else -> {} } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        red_span: (43, 61),
        green_span: (43, 61),
    },
    Negative {
        name: "complex-alternatives",
        text: r#"class Node {}
fun entry(own subject: Node?): Unit { when (subject) { null, Node() -> {}; else -> {} } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        red_span: (52, 101),
        green_span: (52, 101),
    },
    Negative {
        name: "return-extraction",
        text: r#"class Node {}
fun entry(own subject: Node?): Node { when (subject) { null -> { return Node() }; else -> { return subject } } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        red_span: (52, 124),
        green_span: (106, 120),
    },
    Negative {
        name: "local-extraction",
        text: r#"class Node {}
fun touch(value: Node): Unit {}
fun entry(own subject: Node?): Unit { when (subject) { null -> {}; else -> { val inner: Node = subject; touch(inner) } } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        red_span: (84, 166),
        green_span: (141, 148),
    },
    Negative {
        name: "tail-extraction",
        text: r#"class Node {}
fun entry(own subject: Node?): Node = when (subject) { null -> Node(); else -> subject }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        red_span: (52, 102),
        green_span: (52, 102),
    },
    Negative {
        name: "receiver",
        text: r#"class Node { fun ping(): Unit {} }
fun entry(own subject: Node?): Unit { when (subject) { null -> {}; else -> { subject.ping() } } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        red_span: (73, 130),
        green_span: (112, 126),
    },
    Negative {
        name: "active-proof-loop",
        text: r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own subject: Node?): Unit { when (subject) { null -> {}; else -> { while (false) {}; observe(subject) } } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        red_span: (86, 163),
        green_span: (125, 141),
    },
    Negative {
        name: "replaced-generation",
        text: r#"class Node {}
fun observe(value: Node): Unit {}
fun entry(own input: Node?, own replacement: Node?): Unit { var x = input; when (x) { null -> {}; else -> { x = replacement; if (x == null) {} else { observe(x) } } } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        red_span: (123, 214),
        green_span: (156, 171),
    },
    Negative {
        name: "borrow-consume",
        text: r#"class Node {}
fun take(own value: Node): Unit {}
fun entry(subject: Node?): Unit { when (subject) { null -> {}; else -> { take(subject) } } }
"#,
        stage: "ownership",
        code: "L0133",
        red_span: (127, 134),
        green_span: (127, 134),
    },
    Negative {
        name: "inout-consume",
        text: r#"class Node {}
fun take(own value: Node): Unit {}
fun entry(inout subject: Node?): Unit { when (subject) { null -> {}; else -> { take(subject) } } }
"#,
        stage: "ownership",
        code: "L0133",
        red_span: (133, 140),
        green_span: (133, 140),
    },
    Negative {
        name: "use-after-move",
        text: r#"class Node {}
fun take(own value: Node): Unit {}
fun touch(value: Node?): Unit {}
fun entry(own subject: Node?): Unit { when (subject) { null -> {}; else -> { take(subject) } }; touch(subject) }
"#,
        stage: "ownership",
        code: "L0131",
        red_span: (184, 191),
        green_span: (184, 191),
    },
    Negative {
        name: "conflicting-borrow",
        text: r#"class Node {}
fun pair(first: Node?, own second: Node): Unit {}
fun entry(own subject: Node?): Unit { pair(subject, when (subject) { null -> Node(); else -> subject }) }
"#,
        stage: "ownership",
        code: "L0135",
        red_span: (157, 164),
        green_span: (157, 164),
    },
];
