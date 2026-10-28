//! A borrowed closure held outside a lexical scope must not borrow a binding of that scope
//! (SPEC-0363): rejects with L0135 at the capture, legal neighbors stay clean. Unit pipeline only.

#[path = "support/closure_safety.rs"]
mod support;
use support::{Case, observe};

fn case(id: usize) -> Case {
    let (body, rejected) = match id {
        0 => (
            r#"fun run(): Unit {
  var saved: () -> Unit = {}
  loop {
    val local = "x"
    saved = { read(local) }
    break
  }
  saved()
}"#,
            Some(("L0135", "{ read(local) }", "local")),
        ),
        1 => (
            r#"fun run(flag: Boolean): Unit {
  var saved: () -> Unit = {}
  if (flag) {
    val local = "x"
    saved = { read(local) }
  }
  saved()
}"#,
            Some(("L0135", "{ read(local) }", "local")),
        ),
        2 => (
            r#"fun run(flag: Boolean): Unit {
  val saved: () -> Unit = if (flag) {
    val local = "x"
    val inner: () -> Unit = { read(local) }
    inner
  } else {
    val other: () -> Unit = {}
    other
  }
  saved()
}"#,
            Some(("L0135", "{ read(local) }", "local")),
        ),
        3 => (
            r#"fun run(flag: Boolean): Unit {
  var saved: () -> Unit = {}
  loop {
    val local = "x"
    saved = { read(local) }
    if (flag) { continue }
    break
  }
  saved()
}"#,
            Some(("L0135", "{ read(local) }", "local")),
        ),
        4 => (
            r#"fun run(flag: Boolean): Unit {
  if (flag) {
    val local = "x"
    val inner: () -> Unit = { read(local) }
    inner()
  }
}"#,
            None,
        ),
        5 => (
            r#"fun run(source: String, flag: Boolean): Unit {
  var saved: () -> Unit = {}
  if (flag) { saved = { read(source) } }
  saved()
}"#,
            None,
        ),
        6 => (
            r#"fun run(flag: Boolean): Unit {
  var saved: () -> Unit = {}
  if (flag) {
    val local = "x"
    saved = move { read(local) }
  }
  saved()
}"#,
            None,
        ),
        7 => (
            r#"fun run(flag: Boolean): Unit {
  var saved: () -> Unit = {}
  if (flag) {
    val local = "x"
    saved = { read(local) }
    saved()
    saved = {}
  }
  saved()
}"#,
            None,
        ),
        8 => (
            r#"fun run(flag: Boolean): Unit {
  var saved: () -> Unit = {}
  if (flag) {
    val local = "x"
    val inner: () -> Unit = { read(local) }
    saved = { inner() }
  }
  saved()
}"#,
            Some(("L0135", "{ inner() }", "inner")),
        ),
        9 => (
            r#"fun run(flag: Boolean): Unit {
  var saved: () -> Unit = {}
  if (flag) {
    val local = "x"
    val inner: () -> Unit = { read(local) }
    saved = move { inner() }
  }
  saved()
}"#,
            Some(("L0135", "{ read(local) }", "local")),
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
    let observation = observe(&case, true);
    assert!(
        matches!(observation, Ok(true)),
        "scope escape case {id}: {observation:#?}"
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
matrix_case!(reject_loop_break_keeps_closure_of_loop_local, 0);
matrix_case!(reject_if_branch_keeps_closure_of_branch_local, 1);
matrix_case!(reject_block_result_closure_of_block_local, 2);
matrix_case!(reject_loop_continue_keeps_closure_of_loop_local, 3);
matrix_case!(legal_closure_used_inside_its_scope, 4);
matrix_case!(legal_outer_source_captured_in_inner_scope, 5);
matrix_case!(legal_move_capture_of_scope_local, 6);
matrix_case!(legal_held_closure_replaced_before_scope_end, 7);
matrix_case!(reject_outer_closure_borrowing_scope_closure, 8);
matrix_case!(reject_move_closure_carrying_scope_borrow, 9);
