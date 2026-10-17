//! B3 first observation: the original nullable owner remains live after its If.
//! This independent source does not change the frozen A/B/C source partitions.
//! Full raw ownership/SSA observations precede independent owner/proof identity review.

use super::*;

const CROSS_JOIN: Case = Case {
    id: "B3",
    label: "if_size_keep_nullable_owner",
    path: "b3_if_size_keep_nullable_owner.ko",
    single: r#"fun main(): Unit {
  val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  if (source != null) { val count = source.size }
  val kept = source
  return
}
"#,
    provider: r#"package p
fun main(): Unit {
  val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  if (source != null) { val count = source.size }
  val kept = source
  return
}
"#,
    consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
    counts: [0, 0, 1],
    non_null_when_true: Some(true),
    scalar: lang_frontend::type_checking::BuiltinType::Int,
    roots: &[("source", true), ("kept", true)],
};

#[test]
fn nullable_map_if_size_keeps_owner_across_join() {
    collect_layer(std::slice::from_ref(&CROSS_JOIN), 2);
}
