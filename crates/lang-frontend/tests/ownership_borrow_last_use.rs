//! Restricted straight-line loan last-use; single/unit share each normal source.
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, DropTarget, LoanTarget, OwnershipCheckedFile,
        UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
        check_ownership,
    },
    parser::{ParsedFile, Statement, parse_file},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn checked(
    body: &str,
) -> (
    SourceMap,
    ParsedFile,
    OwnershipCheckedFile,
    CompilationUnitOwnership,
) {
    let text = format!(
        r#"fun view(source: String): borrow String from source = source
fun wrap(source: String): borrow String from source = view(source)
fun consume(own source: String) {{}}
fun touch(first: String, own second: String) {{}}
class Holder(val text: String)
fun viewHolder(source: Holder): borrow Holder from source = source
fun consumeHolder(own source: Holder) {{}}
{body}"#
    );
    let mut sources = SourceMap::new();
    let source = sources.add_source("last-use.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let single = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    let inputs = [SourceUnitInput::new("root", "last-use.ko", source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    (sources, parsed, single, unit)
}

#[test]
fn same_scope_operations_follow_the_last_borrow_use() {
    for body in [
        r#"fun run() { val source = "kept"; borrow val parent = source; borrow val alias = parent; borrow val child = view(alias); println(child); consume(source) }"#,
        r#"fun run() { val source = Holder("kept"); borrow val projected = source.text; borrow val alias = projected; println(alias); consumeHolder(source) }"#,
        r#"fun run() { val source = "kept"; borrow val item = wrap(source); println(item); consume(source) }"#,
        r#"fun run() { val source = "kept"; borrow val unused = view(source); consume(source) }"#,
        r#"fun run() { val source = "kept"; borrow val parent = wrap(source); borrow val alias = parent; borrow val child = view(alias); println(child); consume(source) }"#,
        r#"fun run() { val source = Holder("kept"); borrow val parent = viewHolder(source); borrow val projected = parent.text; println(projected); consumeHolder(source) }"#,
        r#"fun run() { var source = mutableMapOf<String, String>(); source.put("key", "first"); borrow val item = source.requireValue("key"); println(item); source.put("key", "second"); borrow val changed = source.requireValue("key"); println(changed) }"#,
        r#"fun run() { val source = "kept"; borrow val first = view(source); borrow val second = wrap(source); println(first); println(second); consume(source) }"#,
    ] {
        let (_, _, single, unit) = checked(body);
        assert!(
            single.diagnostics().is_empty(),
            "{body}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics().is_empty(),
            "{body}: {:?}",
            unit.diagnostics()
        );
        assert!(single.deferred().is_empty(), "{body}");
        assert!(unit.deferred().is_empty(), "{body}");
    }
}

#[test]
fn unused_result_ends_after_its_initializer() {
    let (sources, parsed, single, unit) = checked(
        r#"fun run() { val source = "kept"; borrow val unused = view(source); consume(source) }"#,
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    let [end] = single.borrow_results().ends() else {
        panic!("one result end")
    };
    let DropPoint::AfterStatement(statement) = end.point() else {
        panic!("statement endpoint")
    };
    assert_eq!(
        sources
            .slice(parsed.ast().statements().get(statement).unwrap().span())
            .unwrap(),
        "borrow val unused = view(source)"
    );
    let [unit_end] = unit.borrow_results().ends() else {
        panic!("one unit result end")
    };
    let UnitDropPoint::AfterStatement(unit_statement) = unit_end.point() else {
        panic!("unit endpoint")
    };
    assert_eq!(unit_statement.statement(), statement);
}

#[test]
fn last_use_ends_children_before_parents_and_source_cleanup() {
    let (sources, parsed, single, unit) = checked(
        r#"fun run() { val source = "kept"; borrow val parent = wrap(source); borrow val alias = parent; borrow val child = view(alias); println(child) }"#,
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    let bindings = single.borrow_results().bindings();
    assert_eq!(bindings.len(), 3);
    assert_eq!(bindings[1].parent(), Some(bindings[0].binding()));
    assert_eq!(bindings[2].parent(), Some(bindings[1].binding()));
    let ends = single.borrow_results().ends();
    assert_eq!(
        ends.iter().map(|end| end.binding()).collect::<Vec<_>>(),
        bindings
            .iter()
            .rev()
            .map(|binding| binding.binding())
            .collect::<Vec<_>>()
    );
    assert!(ends.iter().all(|end| end.point() == ends[0].point()));
    let DropPoint::AfterStatement(statement) = ends[0].point() else {
        panic!("statement endpoint")
    };
    assert_eq!(
        sources
            .slice(parsed.ast().statements().get(statement).unwrap().span())
            .unwrap(),
        "println(child)"
    );
    let LoanTarget::Place(origin) = bindings[0].origin() else {
        panic!("source place")
    };
    let source_drops = single
        .drops()
        .iter()
        .filter(|drop| drop.target() == DropTarget::Named(origin.root()))
        .collect::<Vec<_>>();
    assert_eq!(source_drops.len(), 1);
    assert_eq!(source_drops[0].point(), ends[0].point());
    assert!(
        !single
            .drops()
            .iter()
            .any(|drop| bindings.iter().any(|binding| drop.target()
                == DropTarget::Named(binding.binding())
                || drop.target() == DropTarget::Temporary(binding.initializer())))
    );

    let bindings = unit.borrow_results().bindings();
    let ends = unit.borrow_results().ends();
    assert_eq!(ends.len(), 3);
    assert_eq!(
        ends.iter().map(|end| end.binding()).collect::<Vec<_>>(),
        bindings
            .iter()
            .rev()
            .map(|binding| binding.binding())
            .collect::<Vec<_>>()
    );
    assert!(ends.iter().all(|end| end.point() == ends[0].point()));
    let UnitDropPoint::AfterStatement(unit_statement) = ends[0].point() else {
        panic!("unit endpoint")
    };
    assert_eq!(unit_statement.statement(), statement);
    let UnitLoanTarget::Place(origin) = bindings[0].origin() else {
        panic!("unit source place")
    };
    let source_drops = unit
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(origin.root()))
        .collect::<Vec<_>>();
    assert_eq!(source_drops.len(), 1);
    assert_eq!(source_drops[0].point(), ends[0].point());
}

#[test]
fn independent_results_have_distinct_last_use_points() {
    let (sources, parsed, single, unit) = checked(
        r#"fun run() { val source = "kept"; borrow val first = view(source); borrow val second = wrap(source); println(first); println(second) }"#,
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    for (index, text) in ["println(first)", "println(second)"]
        .into_iter()
        .enumerate()
    {
        let end = single
            .borrow_results()
            .ends()
            .iter()
            .find(|end| end.binding() == single.borrow_results().bindings()[index].binding())
            .unwrap();
        let DropPoint::AfterStatement(statement) = end.point() else {
            panic!("statement endpoint")
        };
        assert_eq!(
            sources
                .slice(parsed.ast().statements().get(statement).unwrap().span())
                .unwrap(),
            text
        );
        let end = unit
            .borrow_results()
            .ends()
            .iter()
            .find(|end| end.binding() == unit.borrow_results().bindings()[index].binding())
            .unwrap();
        let UnitDropPoint::AfterStatement(unit_statement) = end.point() else {
            panic!("unit endpoint")
        };
        assert_eq!(unit_statement.statement(), statement);
    }
}

#[test]
fn future_alias_projection_and_child_uses_keep_the_source_borrowed() {
    for body in [
        r#"fun run() { val source = "kept"; borrow val parent = source; borrow val alias = parent; consume(source); println(alias) }"#,
        r#"fun run() { val source = Holder("kept"); borrow val projected = source.text; borrow val alias = projected; consumeHolder(source); println(alias) }"#,
        r#"fun run() { val source = "kept"; borrow val parent = view(source); borrow val alias = parent; println(parent); consume(source); println(alias) }"#,
        r#"fun run() { val source = Holder("kept"); borrow val parent = viewHolder(source); borrow val projected = parent.text; consumeHolder(source); println(projected) }"#,
        r#"fun run() { val source = "kept"; borrow val parent = view(source); borrow val child = wrap(parent); println(parent); consume(source); println(child) }"#,
        r#"fun run() { val source = "kept"; borrow val parent = view(source); borrow val child = wrap(parent); println(child); consume(source); println(parent) }"#,
        r#"fun run() { val source = "kept"; borrow val parent = view(source); println(parent); consume(source); borrow val child = wrap(parent); println(child) }"#,
        r#"fun run() { val source = "kept"; borrow val first = view(source); borrow val second = view(source); println(first); consume(source); println(second) }"#,
    ] {
        let (sources, _, single, unit) = checked(body);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            let conflict = diagnostics
                .iter()
                .find(|d| d.code().to_string() == "L0135")
                .unwrap_or_else(|| panic!("{body}: {diagnostics:?}"));
            assert_eq!(
                sources.slice(conflict.primary_span()).unwrap(),
                "source",
                "{body}"
            );
            assert!(
                conflict.details().iter().any(|detail| matches!(
                    detail,
                    lang_frontend::diagnostic::DiagnosticDetail::Label(_)
                )),
                "{body}"
            );
        }
        assert!(single.borrow_results().bindings().is_empty());
        assert!(unit.borrow_results().bindings().is_empty());
    }
}

#[test]
fn live_map_payload_keeps_invalidation_blocked() {
    let (_, _, single, unit) = checked(
        r#"fun run() { var source = mutableMapOf<String, String>(); source.put("key", "first"); borrow val item = source.requireValue("key"); source.put("key", "second"); println(item) }"#,
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn call_inputs_remain_borrowed_until_the_call_returns() {
    let (_, _, single, unit) = checked(
        r#"fun run() { val source = "kept"; borrow val item = view(source); touch(item, source) }"#,
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
            "{diagnostics:?}"
        );
    }
    let (_, _, single, unit) = checked(
        r#"fun run() { val source = "kept"; borrow val item = view(source); touch(item, "tail"); consume(source) }"#,
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
}

#[test]
fn control_flow_keeps_the_scope_endpoint_conservative() {
    let (_, parsed, single, unit) = checked(
        r#"fun run(flag: Boolean) { val source = "kept"; borrow val item = view(source); if (flag) { println(item) } }"#,
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    let [end] = single.borrow_results().ends() else {
        panic!("one scope result")
    };
    let DropPoint::AfterStatement(statement) = end.point() else {
        panic!("scope endpoint")
    };
    assert!(matches!(
        parsed.ast().statements().get(statement).unwrap().payload(),
        Statement::Block { .. }
    ));
    let [end] = unit.borrow_results().ends() else {
        panic!("one unit scope result")
    };
    let UnitDropPoint::AfterStatement(unit_statement) = end.point() else {
        panic!("unit scope endpoint")
    };
    assert_eq!(unit_statement.statement(), statement);
}
