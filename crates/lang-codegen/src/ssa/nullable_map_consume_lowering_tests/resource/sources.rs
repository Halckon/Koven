//! Frozen four-source Resource baseline; source order is not mapper registration order.

pub(super) struct Case {
    pub id: &'static str,
    pub path: &'static str,
    pub single: &'static str,
    pub provider: &'static str,
    pub consumer: &'static str,
    pub counts: [usize; 6],
    pub has_ordinary: bool,
}

pub(super) const CASES: &[Case] = &[
    Case {
        id: "R0",
        path: "r0_remove_only.ko",
        single: r#"class Resource { deinit() {} }
fun main(): Unit {
  var inner = mutableMapOf<Int, Resource>()
  inner.put(1, Resource())
  var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
  outer.put(1, inner)
  val got = outer.remove(1)!!
  val count = got.size
}
"#,
        provider: r#"package p
class Resource { deinit() {} }
fun main(): Unit {
  var inner = mutableMapOf<Int, Resource>()
  inner.put(1, Resource())
  var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
  outer.put(1, inner)
  val got = outer.remove(1)!!
  val count = got.size
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        counts: [2, 2, 1, 1, 0, 1],
        has_ordinary: false,
    },
    Case {
        id: "R1",
        path: "r1_ordinary_source_before_remove.ko",
        single: r#"class Resource { deinit() {} }
fun main(): Unit {
  val ordinary: MutableMap<Int, Resource>? = null
  var inner = mutableMapOf<Int, Resource>()
  inner.put(1, Resource())
  var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
  outer.put(1, inner)
  val got = outer.remove(1)!!
  val count = got.size
}
"#,
        provider: r#"package p
class Resource { deinit() {} }
fun main(): Unit {
  val ordinary: MutableMap<Int, Resource>? = null
  var inner = mutableMapOf<Int, Resource>()
  inner.put(1, Resource())
  var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
  outer.put(1, inner)
  val got = outer.remove(1)!!
  val count = got.size
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        counts: [2, 2, 1, 1, 0, 1],
        has_ordinary: true,
    },
    Case {
        id: "R2",
        path: "r2_ordinary_source_after_remove.ko",
        single: r#"class Resource { deinit() {} }
fun main(): Unit {
  var inner = mutableMapOf<Int, Resource>()
  inner.put(1, Resource())
  var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
  outer.put(1, inner)
  val got = outer.remove(1)!!
  val count = got.size
  val ordinary: MutableMap<Int, Resource>? = null
}
"#,
        provider: r#"package p
class Resource { deinit() {} }
fun main(): Unit {
  var inner = mutableMapOf<Int, Resource>()
  inner.put(1, Resource())
  var outer = mutableMapOf<Int, MutableMap<Int, Resource>>()
  outer.put(1, inner)
  val got = outer.remove(1)!!
  val count = got.size
  val ordinary: MutableMap<Int, Resource>? = null
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        counts: [2, 2, 1, 1, 0, 1],
        has_ordinary: true,
    },
    Case {
        id: "R3",
        path: "r3_ordinary_null_only.ko",
        single: r#"class Resource { deinit() {} }
fun main(): Unit {
  val ordinary: MutableMap<Int, Resource>? = null
}
"#,
        provider: r#"package p
class Resource { deinit() {} }
fun main(): Unit {
  val ordinary: MutableMap<Int, Resource>? = null
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        counts: [0, 0, 0, 0, 0, 0],
        has_ordinary: true,
    },
];
