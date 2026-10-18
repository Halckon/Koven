//! Newly rebuilt ordinary regression matrix: ten rejects × two entries and five legal neighbors × two.
//! Source inputs and assertions are new; this does not restore the unavailable original active20/10.

#[path = "support/closure_safety.rs"]
mod support;
use support::{Case, observe};

fn case(id: usize) -> Case {
    let (body, rejected) = match id {
        0 => (
            r#"fun run(own first: String, own second: String): Unit {
  var saved: () -> Unit = { read(first) }
  saved = ({ read(second) })
  take(second)
  saved()
}"#,
            Some(("L0135", "take(second)", "second")),
        ),
        1 => (
            r#"fun run(own first: String, own second: String): Unit {
  var saved: () -> Unit = { read(first) }
  val replacement: () -> Unit = { read(second) }
  saved = (replacement)
  take(second)
  saved()
}"#,
            Some(("L0135", "take(second)", "second")),
        ),
        2 => (
            r#"fun run(own source: String): Unit {
  var saved: () -> Unit = { read(source) }
  saved = (saved)
  take(source)
  saved()
}"#,
            Some(("L0135", "take(source)", "source")),
        ),
        3 => (
            r#"fun run(): Unit {
  var source = "first"
  var saved: () -> Unit = {}
  saved = ({ read(source) })
  source = "second"
  saved()
}"#,
            Some(("L0135", "source = \"second\"", "source")),
        ),
        4 => (
            r#"fun run(source: Map<String, String>): Unit {
  for ((key, value) in source) {
    var saved: () -> Unit = { read(key) }
    saved = ({ read(value) })
    take(key)
    saved()
  }
}"#,
            Some(("L0133", "take(key)", "key")),
        ),
        5 => (
            r#"fun run(source: Map<String, String>): Unit {
  for ((key, value) in source) {
    var saved: () -> Unit = { read(key) }
    saved = ({ read(value) })
    take(value)
    saved()
  }
}"#,
            Some(("L0133", "take(value)", "value")),
        ),
        6 => (
            r#"fun run(source: Map<String, String>): Unit {
  for ((key, value) in source) {
    var saved: () -> Unit = { read(key) }
    saved = ({ read(value) })
    hold(saved)
  }
}"#,
            Some(("L0137", "hold(saved)", "saved")),
        ),
        7 => (
            r#"fun run(source: Map<String, String>): Unit {
  for ((key, value) in source) {
    var saved: () -> Unit = { read(key) }
    saved = ({ read(value) })
    val box = Holder(saved)
  }
}"#,
            Some(("L0137", "Holder(saved)", "saved")),
        ),
        8 => (
            r#"fun run(source: Map<String, String>): Unit {
  for ((key, value) in source) {
    val saved: move () -> Unit = move { read(key) }
    saved()
  }
}"#,
            Some(("L0138", "move { read(key) }", "key")),
        ),
        9 => (
            r#"fun run(source: Map<String, String>): Unit {
  for ((key, value) in source) {
    val saved: move () -> Unit = move { read(value) }
    saved()
  }
}"#,
            Some(("L0138", "move { read(value) }", "value")),
        ),
        10 => (
            r#"fun run(own first: String, own second: String): Unit {
  var saved: () -> Unit = { read(first) }
  saved = ({ read(second) })
  take(first)
  saved()
  take(second)
}"#,
            None,
        ),
        11 => (
            r#"fun run(own first: String, own second: String): Unit {
  var saved: () -> Unit = { read(first) }
  val replacement: () -> Unit = { read(second) }
  saved = (replacement)
  take(first)
  saved()
  take(second)
}"#,
            None,
        ),
        12 => (
            r#"fun run(own source: String): Unit {
  var saved: () -> Unit = { read(source) }
  saved = (saved)
  saved()
  take(source)
}"#,
            None,
        ),
        13 => (
            r#"fun run(source: Map<String, String>, flag: Boolean): Unit {
  for ((key, value) in source) {
    var saved: () -> Unit = { read(key) }
    saved = ({ read(value) })
    if (flag) { return }
    saved()
  }
}"#,
            None,
        ),
        14 => (
            r#"fun run(own source: String, own resource: Resource): Unit {
  var saved: () -> Unit = move { readResource(resource) }
  saved = ({ read(source) })
  saved()
  take(source)
}"#,
            None,
        ),
        _ => unreachable!("fixed matrix case"),
    };
    Case {
        body: body.into(),
        rejected,
    }
}

fn check(id: usize) {
    let case = case(id);
    let observations = [false, true].map(|unit| observe(&case, unit));
    assert!(
        observations.iter().all(|result| matches!(result, Ok(true))),
        "rebuilt case {id} must match both original real-entry pipelines: {observations:#?}"
    );
}

macro_rules! matrix_case {
    ($name:ident, $id:expr) => {
        #[test]
        fn $name() {
            check($id);
        }
    };
}
matrix_case!(reject_replacement_source_move, 0);
matrix_case!(reject_alias_source_move, 1);
matrix_case!(reject_self_source_move, 2);
matrix_case!(reject_replacement_source_mutation, 3);
matrix_case!(reject_map_key_owned_delivery, 4);
matrix_case!(reject_map_value_owned_delivery, 5);
matrix_case!(reject_map_replacement_closure_owned_argument, 6);
matrix_case!(reject_map_replacement_closure_field_storage, 7);
matrix_case!(reject_map_key_move_capture, 8);
matrix_case!(reject_map_value_move_capture, 9);
matrix_case!(legal_replacement_restores_old_then_new_source, 10);
matrix_case!(legal_alias_restores_old_then_new_source, 11);
matrix_case!(legal_self_assignment_then_source_move, 12);
matrix_case!(legal_map_replacement_capture_and_return, 13);
matrix_case!(legal_resource_replacement_then_source_move, 14);
