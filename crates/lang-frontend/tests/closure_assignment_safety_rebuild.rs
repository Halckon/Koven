//! Newly rebuilt ordinary regression matrix: ten rejects × two entries and five legal neighbors × two.
//! Source inputs and assertions are new; this does not restore the unavailable original active20/10.

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        check_compilation_unit_ownership, check_ownership, validate_iteration_facts,
    },
    parser::parse_file,
    source::{SourceMap, Span},
    type_checking::{
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

const PRELUDE: &str = r#"
class Resource { deinit() {} }
class Holder(val action: () -> Unit)
fun read(item: String): Unit {}
fun readResource(item: Resource): Unit {}
fun take(own item: String): Unit {}
fun hold(own action: () -> Unit): Unit {}
"#;

struct Case {
    body: &'static str,
    rejected: Option<(&'static str, &'static str, &'static str)>,
}

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
    Case { body, rejected }
}

fn clean(items: &[Diagnostic], stage: &str) -> Result<(), String> {
    if items.is_empty() {
        Ok(())
    } else {
        Err(format!("{stage}: {items:#?}"))
    }
}

fn assess(
    case: &Case,
    diagnostics: &[Diagnostic],
    expected_span: Option<Span>,
    captures: usize,
    facts_empty: bool,
    qualified: bool,
) -> bool {
    match case.rejected {
        Some((code, _, _)) => {
            matches!(diagnostics, [d] if d.code().to_string() == code && Some(d.primary_span()) == expected_span)
                && facts_empty
                && !qualified
        }
        None => diagnostics.is_empty() && captures > 0 && qualified,
    }
}

fn observe(case: &Case, unit: bool) -> Result<bool, String> {
    let text = format!("{PRELUDE}{}\n", case.body);
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("safety-rebuild.ko", &text)
        .map_err(|e| format!("source {e:?}"))?;
    let expected_span = case.rejected.map(|(_, needle, primary)| {
        let at = text.rfind(needle).expect("fixed offending expression")
            + needle
                .rfind(primary)
                .expect("fixed primary within expression");
        sources
            .span(source, at, at + primary.len())
            .expect("original source span")
    });
    let lexed = lex(&sources, source).map_err(|e| format!("lex {e:?}"))?;
    clean(lexed.diagnostics(), "lex")?;
    let file = parse_file(&sources, &lexed).map_err(|e| format!("parse {e:?}"))?;
    clean(file.diagnostics(), "parse")?;
    let (name_environment, environment) = standard_environments();
    if !unit {
        let names = resolve_names(&sources, &file, &name_environment)
            .map_err(|e| format!("names {e:?}"))?;
        clean(names.diagnostics(), "names")?;
        let typed =
            check_types(&sources, &file, &names, &environment).map_err(|e| format!("raw {e:?}"))?;
        clean(typed.diagnostics(), "raw")?;
        let complete = complete_file_types(&sources, &file, &names, &environment, &typed)
            .map_err(|e| format!("complete {e:?}"))?;
        if !complete.is_same_analysis(&typed) {
            return Err("complete identity".into());
        }
        let owned = check_ownership(&sources, &file, &names, complete.types())
            .map_err(|e| format!("ownership {e:?}"))?;
        if !owned.deferred().is_empty() {
            return Err(format!("ownership deferred {:?}", owned.deferred()));
        }
        let qualified = owned.diagnostics().is_empty()
            && validate_iteration_facts(&file, &names, &typed, &owned).is_ok();
        let facts_empty = owned.captures().is_empty()
            && owned.drops().is_empty()
            && owned.loans().is_empty()
            && owned.map_iterations().is_empty();
        let matched = assess(
            case,
            owned.diagnostics(),
            expected_span,
            owned.captures().len(),
            facts_empty,
            qualified,
        );
        println!(
            "REBUILT single expected_span={expected_span:?} matched={matched} captures={} facts_empty={facts_empty} qualified={qualified} diagnostics={:#?}",
            owned.captures().len(),
            owned.diagnostics()
        );
        Ok(matched)
    } else {
        let inputs = [SourceUnitInput::new(
            "root",
            "safety-rebuild.ko",
            source,
            &file,
        )];
        let index =
            index_compilation_unit(&sources, &inputs).map_err(|e| format!("index {e:?}"))?;
        clean(index.diagnostics(), "index")?;
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .map_err(|e| format!("names {e:?}"))?
            .validate()
            .map_err(|e| format!("names gate {e:?}"))?;
        let raw = check_compilation_unit_types(&sources, &inputs, &names, &environment)
            .map_err(|e| format!("raw {e:?}"))?;
        clean(raw.diagnostics(), "raw")?;
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &environment, &raw)
                .map_err(|e| format!("complete {e:?}"))?;
        if !complete.is_same_analysis(&raw) {
            return Err("complete identity".into());
        }
        let typed = raw.validate().map_err(|e| format!("type gate {e:?}"))?;
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                .map_err(|e| format!("ownership {e:?}"))?;
        if !owned.is_compatible_with(&typed) || !owned.deferred().is_empty() {
            return Err(format!(
                "ownership identity/deferred {:?}",
                owned.deferred()
            ));
        }
        let facts_empty = owned.captures().is_empty()
            && owned.drops().is_empty()
            && owned.loans().is_empty()
            && owned.map_iterations().is_empty();
        let diagnostics = owned.diagnostics().to_vec();
        let captures = owned.captures().len();
        let qualified = owned.validate().is_ok();
        let matched = assess(
            case,
            &diagnostics,
            expected_span,
            captures,
            facts_empty,
            qualified,
        );
        println!(
            "REBUILT unit expected_span={expected_span:?} matched={matched} captures={captures} facts_empty={facts_empty} qualified={qualified} diagnostics={diagnostics:#?}"
        );
        Ok(matched)
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
