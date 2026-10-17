//! Exact approved eight sources; no synthetic pre-registration or source rewriting.

use lang_frontend::type_checking::BuiltinType;

pub(super) struct Case {
    pub id: &'static str,
    pub label: &'static str,
    pub path: &'static str,
    pub single: &'static str,
    pub provider: &'static str,
    pub consumer: &'static str,
    pub counts: [usize; 3],
    pub non_null_when_true: Option<bool>,
    pub scalar: BuiltinType,
    pub roots: &'static [(&'static str, bool)],
}

// A root's bool selects p/provider.ko (true) or q/consumer.ko (false) in unit mode.
// Single mode resolves every listed root in its one actual source.
pub(super) const CASES: &[Case] = &[
    Case {
        id: "A1",
        label: "null_decl",
        path: "a1_null_decl.ko",
        single: r#"fun main(): Unit {
  val source: MutableMap<Int, Int>? = null
  println("null done")
}
"#,
        provider: r#"package p
fun main(): Unit {
  val source: MutableMap<Int, Int>? = null
  println("null done")
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        counts: [0, 0, 0],
        non_null_when_true: None,
        scalar: BuiltinType::Int,
        roots: &[("source", true)],
    },
    Case {
        id: "A2",
        label: "present_decl",
        path: "a2_present_decl.ko",
        single: r#"fun main(): Unit {
  val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  println("present done")
}
"#,
        provider: r#"package p
fun main(): Unit {
  val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  println("present done")
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        counts: [0, 0, 0],
        non_null_when_true: None,
        scalar: BuiltinType::Int,
        roots: &[("source", true)],
    },
    Case {
        id: "A3",
        label: "remove_canonical",
        path: "a3_remove_canonical.ko",
        single: r#"fun main(): Unit {
  val directNull: MutableMap<Int, Int>? = null
  val directPresent: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  var outer = mutableMapOf<Int, MutableMap<Int, Int>>()
  outer.put(1, mutableMapOf<Int, Int>())
  val removed: MutableMap<Int, Int>? = outer.remove(1)
  println("canonical done")
}
"#,
        provider: r#"package p
fun main(): Unit {
  val directNull: MutableMap<Int, Int>? = null
  val directPresent: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  var outer = mutableMapOf<Int, MutableMap<Int, Int>>()
  outer.put(1, mutableMapOf<Int, Int>())
  val removed: MutableMap<Int, Int>? = outer.remove(1)
  println("canonical done")
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        counts: [0, 1, 0],
        non_null_when_true: None,
        scalar: BuiltinType::Int,
        roots: &[
            ("directNull", true),
            ("directPresent", true),
            ("removed", true),
        ],
    },
    Case {
        id: "B1",
        label: "if_group_size",
        path: "b1_if_group_size.ko",
        single: r#"fun main(): Unit {
  val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  if ((source) != null) { val count = ((source)).size }
  println("read done")
}
"#,
        provider: r#"package p
fun main(): Unit {
  val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  if ((source) != null) { val count = ((source)).size }
  println("read done")
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        counts: [0, 0, 1],
        non_null_when_true: Some(true),
        scalar: BuiltinType::Int,
        roots: &[("source", true)],
    },
    Case {
        id: "B2",
        label: "if_else_var_size",
        path: "b2_if_else_var_size.ko",
        single: r#"fun main(): Unit {
  var source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  if (source == null) {} else { val count = source.size }
  println("else read done")
}
"#,
        provider: r#"package p
fun main(): Unit {
  var source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  if (source == null) {} else { val count = source.size }
  println("else read done")
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        counts: [0, 0, 1],
        non_null_when_true: Some(false),
        scalar: BuiltinType::Int,
        roots: &[("source", true)],
    },
    Case {
        id: "C1",
        label: "val_empty",
        path: "c1_val_empty.ko",
        single: r#"fun main(): Unit {
  val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  if (source != null) {
    val result = source.consume()
    if (result.size == 0) { println("consumed empty") }
  }
}
"#,
        provider: r#"package p
fun main(): Unit {
  val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  if (source != null) {
    val result = source.consume()
    if (result.size == 0) { println("consumed empty") }
  }
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        counts: [1, 0, 1],
        non_null_when_true: Some(true),
        scalar: BuiltinType::Int,
        roots: &[("source", true)],
    },
    Case {
        id: "C2",
        label: "var_string_group_discard",
        path: "c2_var_string_group_discard.ko",
        single: r#"fun main(): Unit {
  var seed = mutableMapOf<String, String>()
  seed.put("key".clone(), "old".clone())
  seed.put("key".clone(), "value".clone())
  var source: MutableMap<String, String>? = seed
  if (source != null) { ((source)).consume() }
  println("consumed strings")
}
"#,
        provider: r#"package p
fun main(): Unit {
  var seed = mutableMapOf<String, String>()
  seed.put("key".clone(), "old".clone())
  seed.put("key".clone(), "value".clone())
  var source: MutableMap<String, String>? = seed
  if (source != null) { ((source)).consume() }
  println("consumed strings")
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        counts: [1, 0, 0],
        non_null_when_true: Some(true),
        scalar: BuiltinType::String,
        roots: &[("source", true)],
    },
    Case {
        id: "C3",
        label: "nullable_input_both_edges",
        path: "c3_nullable_input_both_edges.ko",
        single: r#"fun run(own input: MutableMap<Int, Int>?): Unit {
  val source: MutableMap<Int, Int>? = input
  if (source == null) {} else {
    val result = source.consume()
    if (result.size == 0) { println("input consumed") }
  }
}
fun main(): Unit {
  val present: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  run(present)
  val absent: MutableMap<Int, Int>? = null
  run(absent)
}
"#,
        provider: r#"package p
fun run(own input: MutableMap<Int, Int>?): Unit {
  val source: MutableMap<Int, Int>? = input
  if (source == null) {} else {
    val result = source.consume()
    if (result.size == 0) { println("input consumed") }
  }
}
"#,
        consumer: r#"package q
fun entry(): Unit {
  val present: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  p.run(present)
  val absent: MutableMap<Int, Int>? = null
  p.run(absent)
}
"#,
        counts: [1, 0, 1],
        non_null_when_true: Some(false),
        scalar: BuiltinType::Int,
        roots: &[
            ("input", true),
            ("source", true),
            ("present", false),
            ("absent", false),
        ],
    },
];
