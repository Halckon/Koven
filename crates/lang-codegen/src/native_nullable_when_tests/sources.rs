//! Three independent complete ordinary source documents and source-authored output oracles.
//! No source or expectation is extracted from the legacy mixed native whole.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Family {
    Node,
    Box,
    Rc,
}

pub(super) struct Fixture {
    pub(super) family: Family,
    pub(super) name: &'static str,
    pub(super) text: &'static str,
    pub(super) stdout: &'static [u8],
    pub(super) first_when: (usize, usize),
}

pub(super) fn fixtures() -> [Fixture; 3] {
    [
        Fixture {
            family: Family::Node,
            name: "ordinary-when-node.ko",
            text: NODE,
            stdout: NODE_STDOUT,
            first_when: (553, 669),
        },
        Fixture {
            family: Family::Box,
            name: "ordinary-when-box.ko",
            text: BOX,
            stdout: BOX_STDOUT,
            first_when: (597, 713),
        },
        Fixture {
            family: Family::Rc,
            name: "ordinary-when-rc.ko",
            text: RC,
            stdout: RC_STDOUT,
            first_when: (729, 845),
        },
    ]
}

pub(super) const NODE: &str = include_str!("sources/node.ko");

pub(super) const NODE_STDOUT: &[u8] = b"repeat-present\nread\nread\nread\nread\nconsume\nrepeat-null\nnull\nlater-present-start\nlater-present\nlater-use\nlater-null-start\nlater-null\nlater-use\nreturn-present-start\nreturn-present\nprefix-drop\nouter-drop\nreturn-null-start\ninner-call\nprefix-drop\nouter-call\nouter-drop\nreturn-after\nbreak-present-start\nmake\nbreak-present\nprefix-drop\nouter-drop\nbreak-tail\nbreak-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\nbreak-after\nbreak-tail\ncontinue-present-start\nmake\ncontinue-present\nprefix-drop\nouter-drop\ncontinue-tail\ncontinue-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\ncontinue-after\ncontinue-tail\ntemporary-present-start\nmake\ntemporary-present\nouter-call\nouter-drop\ntemporary-after\ntemporary-null-start\nmake\ntemporary-null\nouter-call\nouter-drop\ntemporary-after\ntemporary-return-present-start\nmake\ntemporary-return\nprefix-drop\nouter-drop\ntemporary-return-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\ntemporary-return-after\ntemporary-break-present-start\nmake\ntemporary-break\nprefix-drop\nouter-drop\ntemporary-break-tail\ntemporary-break-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\ntemporary-break-after\ntemporary-break-tail\ntemporary-continue-present-start\nmake\ntemporary-continue\nprefix-drop\nouter-drop\ntemporary-continue-tail\ntemporary-continue-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\ntemporary-continue-after\ntemporary-continue-tail\ndouble-present-start\nread\ndouble-null-start\ndouble-null\ntemporary-double-present-start\nmake\ntemporary-double-present\ntemporary-double-null-start\nmake\ntemporary-double-null\ntaken-return-start\ntaken-commit-start\ntake-commit\ntaken-after\ntaken-null-start\ntaken-after\ndone\n";

pub(super) const BOX: &str = include_str!("sources/box.ko");

pub(super) const BOX_STDOUT: &[u8] = b"repeat-present\nread\nread\nread\nread\nconsume\nrepeat-null\nnull\nlater-present-start\nlater-present\nlater-use\nlater-null-start\nlater-null\nlater-use\nreturn-present-start\nreturn-present\nprefix-drop\nouter-drop\nreturn-null-start\ninner-call\nprefix-drop\nouter-call\nouter-drop\nreturn-after\nbreak-present-start\nmake\nbreak-present\nprefix-drop\nouter-drop\nbreak-tail\nbreak-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\nbreak-after\nbreak-tail\ncontinue-present-start\nmake\ncontinue-present\nprefix-drop\nouter-drop\ncontinue-tail\ncontinue-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\ncontinue-after\ncontinue-tail\ntemporary-present-start\nmake\ntemporary-present\nouter-call\nouter-drop\ntemporary-after\ntemporary-null-start\nmake\ntemporary-null\nouter-call\nouter-drop\ntemporary-after\ndone\n";

pub(super) const RC: &str = include_str!("sources/rc.ko");

pub(super) const RC_STDOUT: &[u8] = b"repeat-present\nread\nread\nread\nread\nvalue\nshared\nconsume\nrepeat-null\nnull\nlater-present-start\nlater-present\nlater-use\nlater-null-start\nlater-null\nlater-use\nreturn-present-start\nreturn-present\nprefix-drop\nouter-drop\nreturn-null-start\ninner-call\nprefix-drop\nouter-call\nouter-drop\nreturn-after\nbreak-present-start\nmake\nbreak-present\nprefix-drop\nouter-drop\nbreak-tail\nbreak-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\nbreak-after\nbreak-tail\ncontinue-present-start\nmake\ncontinue-present\nprefix-drop\nouter-drop\ncontinue-tail\ncontinue-null-start\nmake\ninner-call\nprefix-drop\nouter-call\nouter-drop\ncontinue-after\ncontinue-tail\ntemporary-present-start\nmake\ntemporary-present\nouter-call\nouter-drop\ntemporary-after\ntemporary-null-start\nmake\ntemporary-null\nouter-call\nouter-drop\ntemporary-after\ndone\n";

// Every negative is a separate full source document, with byte-exact stage boundaries.
pub(super) struct Negative {
    pub(super) name: &'static str,
    pub(super) text: &'static str,
    pub(super) stage: &'static str,
    pub(super) code: &'static str,
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
        green_span: (48, 89),
    },
    Negative {
        name: "inout-root",
        text: r#"class Node {}
fun entry(inout subject: Node?): Unit { when (subject) { null -> {}; else -> {} } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
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
        // Ordinary nullable class fields now have layouts; field-When stays unsupported.
        // Reject at the exact whole When before evaluating its field subject.
        green_span: (79, 125),
    },
    Negative {
        name: "container-element",
        text: r#"class Node {}
fun entry(items: List<Node?>): Unit { when (items[0]) { null -> {}; else -> {} } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        green_span: (52, 94),
    },
    Negative {
        name: "inline-nullable",
        text: r#"value class Cell(val value: Int)
fun entry(own subject: Cell?): Unit { when (subject) { null -> {}; else -> {} } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        // Inline-nullable value classes now have layouts (SPEC-0382); inline-nullable When stays unsupported.
        // Reject at the exact whole When before evaluating its subject.
        green_span: (71, 112),
    },
    Negative {
        name: "complex-alternatives",
        text: r#"class Node {}
fun entry(own subject: Node?): Unit { when (subject) { null, Node() -> {}; else -> {} } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        green_span: (52, 101),
    },
    Negative {
        name: "return-extraction",
        text: r#"class Node {}
fun entry(own subject: Node?): Node { when (subject) { null -> { return Node() }; else -> { return subject } } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
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
        green_span: (141, 148),
    },
    Negative {
        name: "tail-extraction",
        text: r#"class Node {}
fun entry(own subject: Node?): Node = when (subject) { null -> Node(); else -> subject }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        green_span: (52, 102),
    },
    Negative {
        name: "receiver",
        text: r#"class Node { fun ping(): Unit {} }
fun entry(own subject: Node?): Unit { when (subject) { null -> {}; else -> { subject.ping() } } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
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
        green_span: (157, 164),
    },
    // Direct owner-parameter T? has an existing layout; this isolates the When-field gate.
    Negative {
        name: "generic-nullable-field",
        text: r#"class Node {}
class Holder<T>(val value: T?)
fun entry(holder: Holder<Node>): Unit { when (holder.value) { null -> {}; else -> {} } }
"#,
        stage: "lower",
        code: "UnsupportedNode",
        green_span: (85, 131),
    },
];
