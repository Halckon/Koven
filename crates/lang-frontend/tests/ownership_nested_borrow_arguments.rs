//! 普通借用结果直接交付同步 Borrow 实参；只用正常源码构建两入口事实。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipCheckedFile, check_compilation_unit_ownership,
        check_ownership,
    },
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

struct Checked {
    sources: SourceMap,
    parsed: ParsedFile,
    single: OwnershipCheckedFile,
    unit: CompilationUnitOwnership,
}

fn checked(text: &str) -> Checked {
    let mut sources = SourceMap::new();
    let source = sources.add_source("nested_borrow.ko", text).unwrap();
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
    let inputs = [SourceUnitInput::new(
        "root",
        "nested_borrow.ko",
        source,
        &parsed,
    )];
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
    Checked {
        sources,
        parsed,
        single,
        unit,
    }
}

fn ordinary(body: &str) -> Checked {
    checked(&format!(
        "fun view(source: String): borrow String from source = source\nfun observe(item: String) {{ println(item) }}\nfun consume(own item: String) {{}}\n{body}"
    ))
}

fn assert_accepted(checked: &Checked) {
    assert!(
        checked.single.diagnostics().is_empty(),
        "{:?}",
        checked.single.diagnostics()
    );
    assert!(
        checked.unit.diagnostics().is_empty(),
        "{:?}",
        checked.unit.diagnostics()
    );
    assert!(checked.single.deferred().is_empty());
    assert!(checked.unit.deferred().is_empty());
    assert_call_use_contracts(checked);
}

#[test]
fn direct_and_grouped_results_are_synchronous_borrow_operands() {
    for operand in ["view(source)", "((view((source))))"] {
        assert_accepted(&ordinary(&format!(
            "fun run() {{ val source = \"kept\"; observe({operand}); consume(source) }}"
        )));
    }
}

#[test]
fn named_mixed_arguments_and_multiple_result_chains_follow_source_order() {
    for body in [
        "fun select(aux: String, source: String): borrow String from source = source\nfun run() { val source = \"kept\"; val aux = \"aux\"; observe(select(source = source, aux = aux)); consume(source) }",
        "fun select(own aux: Int, source: String): borrow String from source = source\nfun run() { val source = \"kept\"; observe(select(source = source, aux = 7)); consume(source) }",
        "class Resource { deinit() {} }\nfun select(source: String, own aux: Resource): borrow String from source = source\nfun run() { val source = \"kept\"; observe(select(source, Resource())); consume(source) }",
        "fun pair(first: String, second: String) {}\nfun run() { val source = \"kept\"; pair(second = view(source), first = view(source)); consume(source) }",
        "fun pair(first: String, second: String) {}\nfun run() { val first = \"one\"; val second = \"two\"; pair(view(first), view(second)); consume(first); consume(second) }",
        "fun pair(first: String, own count: Int) {}\nfun length(source: String): Int = 1\nfun run() { val source = \"kept\"; pair(view(source), length(source)); consume(source) }",
    ] {
        assert_accepted(&ordinary(body));
    }
}

#[test]
fn generic_field_wrapper_and_existing_parent_sources_keep_their_identity() {
    for body in [
        "fun <T> genericView(source: T): borrow T from source = source\nfun run() { val source = \"kept\"; observe(genericView(source)); consume(source) }",
        "class Record(val text: String)\nfun field(source: Record): borrow String from source = source.text\nfun run() { val source = Record(\"kept\"); observe(field(source)) }",
        "fun wrap(source: String): borrow String from source = view(source)\nfun run() { val source = \"kept\"; observe(wrap(source)); consume(source) }",
        "fun run() { val source = \"kept\"; borrow val parent = view(source); observe(view(parent)); println(parent); consume(source) }",
        "fun measure(source: String): Int = 1\nfun run() { val source = \"kept\"; val count = measure(view(source)); consume(source) }",
    ] {
        assert_accepted(&ordinary(body));
    }
}

#[test]
fn later_move_and_exclusive_access_conflict_before_outer_return() {
    for (body, code) in [
        (
            "fun pair(first: String, own second: String) {}\nfun run() { val source = \"kept\"; pair(view(source), source) }",
            "L0135",
        ),
        (
            "fun pair(own first: String, second: String) {}\nfun run() { val source = \"kept\"; pair(source, view(source)) }",
            "L0131",
        ),
        (
            "fun pair(first: String, inout second: String) {}\nfun run() { var source = \"kept\"; pair(view(source), &source) }",
            "L0135",
        ),
    ] {
        let checked = ordinary(body);
        for diagnostics in [checked.single.diagnostics(), checked.unit.diagnostics()] {
            let diagnostic = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code().to_string() == code)
                .unwrap_or_else(|| panic!("{body}: {diagnostics:?}"));
            assert!(
                !checked
                    .sources
                    .slice(diagnostic.primary_span())
                    .unwrap()
                    .is_empty()
            );
            assert!(!diagnostic.details().is_empty());
        }
        assert_atomic_empty(&checked);
    }
}

fn assert_call_use_contracts(checked: &Checked) {
    use lang_frontend::{
        ownership_checking::{DropPoint, DropTarget, LoanEndPoint, UnitDropPoint, UnitDropTarget},
        parser::Expression,
    };
    let single = checked.single.borrow_results().call_uses();
    let unit = checked.unit.borrow_results().call_uses();
    assert_eq!(single.len(), unit.len());
    for (fact, unit_fact) in single.iter().zip(unit) {
        assert_eq!(fact.expression(), unit_fact.expression().expression());
        assert_eq!(fact.call(), unit_fact.call().expression());
        assert_eq!(fact.argument(), unit_fact.argument().expression());
        assert_eq!(fact.end_point(), DropPoint::CallReturn(fact.call()));
        assert_eq!(
            unit_fact.end_point(),
            UnitDropPoint::CallReturn(unit_fact.call())
        );
        let Expression::Call { arguments, .. } = checked
            .parsed
            .ast()
            .expressions()
            .get(fact.call())
            .unwrap()
            .payload()
        else {
            panic!("outer call");
        };
        assert!(
            arguments
                .iter()
                .any(|argument| argument.value == fact.argument())
        );
        let source = fact.source_loan();
        assert_eq!(source.call(), fact.expression());
        assert_eq!(source.call(), unit_fact.source_loan().call().expression());
        assert_eq!(
            source.argument(),
            unit_fact.source_loan().argument().expression()
        );
        let source_loan = checked
            .single
            .loans()
            .iter()
            .find(|loan| loan.call() == source.call() && loan.argument() == source.argument())
            .unwrap();
        assert_eq!(source_loan.target(), fact.origin());
        let outer_loan = checked
            .single
            .loans()
            .iter()
            .find(|loan| loan.call() == fact.call() && loan.argument() == fact.argument())
            .unwrap();
        assert_eq!(outer_loan.target(), fact.origin());
        let source_ends: Vec<_> = checked
            .single
            .loan_ends()
            .iter()
            .filter(|end| end.call() == source.call() && end.argument() == source.argument())
            .collect();
        assert_eq!(source_ends.len(), 1);
        assert_eq!(
            source_ends[0].point(),
            LoanEndPoint::CallReturn(fact.call())
        );
        let ends = checked.single.loan_ends();
        let outer_end = ends
            .iter()
            .position(|end| end.call() == fact.call() && end.argument() == fact.argument())
            .unwrap();
        let source_end = ends
            .iter()
            .position(|end| end.call() == source.call() && end.argument() == source.argument())
            .unwrap();
        assert!(
            outer_end < source_end,
            "result must end before its source ancestor"
        );
        assert!(
            !checked
                .single
                .loans_ending_at(source.call())
                .any(|loan| loan.argument() == source.argument())
        );
        assert!(
            checked
                .single
                .loans_ending_at(fact.call())
                .any(|loan| loan.argument() == fact.argument())
        );
        let source_loan = checked
            .unit
            .loans()
            .iter()
            .find(|loan| {
                loan.call() == unit_fact.source_loan().call()
                    && loan.argument() == unit_fact.source_loan().argument()
            })
            .unwrap();
        assert_eq!(source_loan.target(), unit_fact.origin());
        let outer_loan = checked
            .unit
            .loans()
            .iter()
            .find(|loan| loan.call() == unit_fact.call() && loan.argument() == unit_fact.argument())
            .unwrap();
        assert_eq!(outer_loan.target(), unit_fact.origin());
        assert!(!checked.single.drops().iter().any(|drop| drop.target()
            == DropTarget::Temporary(fact.expression())
            || drop.target() == DropTarget::Temporary(fact.argument())));
        assert!(!checked.unit.drops().iter().any(|drop| drop.target()
            == UnitDropTarget::Temporary(unit_fact.expression())
            || drop.target() == UnitDropTarget::Temporary(unit_fact.argument())));
    }
}

fn assert_atomic_empty(checked: &Checked) {
    assert!(checked.single.borrow_results().call_uses().is_empty());
    assert!(checked.unit.borrow_results().call_uses().is_empty());
    assert!(checked.single.borrow_results().bindings().is_empty());
    assert!(checked.unit.borrow_results().bindings().is_empty());
    assert!(checked.single.borrow_results().ends().is_empty());
    assert!(checked.unit.borrow_results().ends().is_empty());
}

#[test]
fn grouped_named_operands_and_same_root_chains_are_not_collapsed() {
    let checked = ordinary(
        "fun pair(first: String, second: String) {}\nfun run() { val source = \"kept\"; pair(second = ((view((source)))), first = view(source)) }",
    );
    assert_accepted(&checked);
    let facts = checked.single.borrow_results().call_uses();
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].call(), facts[1].call());
    assert_ne!(facts[0].expression(), facts[1].expression());
    assert_ne!(facts[0].argument(), facts[1].argument());
    assert_eq!(facts[0].origin(), facts[1].origin());
    assert_ne!(facts[0].argument(), facts[0].expression());
    assert_eq!(facts[1].argument(), facts[1].expression());
    let text = |expression| {
        checked
            .sources
            .slice(
                checked
                    .parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .unwrap()
                    .span(),
            )
            .unwrap()
    };
    assert_eq!(text(facts[0].argument()), "((view((source))))");
    assert_eq!(text(facts[0].expression()), "view((source))");
    assert_eq!(text(facts[0].source_loan().argument()), "(source)");
    assert_eq!(text(facts[1].source_loan().argument()), "source");
}

#[test]
fn existing_parent_survives_call_and_memory_owner_drops_at_outer_return() {
    use lang_frontend::ownership_checking::{
        DropPoint, DropTarget, LoanTarget, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
    };
    let checked = ordinary(
        "fun run() { val source = \"kept\"; borrow val parent = view(source); observe(view(parent)); println(parent) }",
    );
    assert_accepted(&checked);
    let fact = &checked.single.borrow_results().call_uses()[0];
    let parent = &checked.single.borrow_results().bindings()[0];
    assert_eq!(fact.parent(), Some(parent.binding()));
    assert_eq!(fact.origin(), parent.origin());
    assert_ne!(
        checked.single.borrow_results().ends()[0].point(),
        fact.end_point()
    );
    let fact = &checked.unit.borrow_results().call_uses()[0];
    assert_eq!(
        fact.parent(),
        Some(checked.unit.borrow_results().bindings()[0].binding())
    );
    assert_ne!(
        checked.unit.borrow_results().ends()[0].point(),
        fact.end_point()
    );

    let checked = ordinary("fun run() { val source = \"kept\"; observe(view(source)) }");
    assert_accepted(&checked);
    let fact = &checked.single.borrow_results().call_uses()[0];
    let LoanTarget::Place(place) = fact.origin() else {
        panic!("named source");
    };
    let drops: Vec<_> = checked
        .single
        .drops()
        .iter()
        .filter(|drop| drop.target() == DropTarget::Named(place.root()))
        .collect();
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].point(), DropPoint::CallReturn(fact.call()));
    let fact = &checked.unit.borrow_results().call_uses()[0];
    let UnitLoanTarget::Place(place) = fact.origin() else {
        panic!("named source");
    };
    let drops: Vec<_> = checked
        .unit
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(place.root()))
        .collect();
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].point(), UnitDropPoint::CallReturn(fact.call()));
}

#[test]
fn owned_and_unproved_continuations_clear_call_uses_atomically() {
    for (body, code, primary) in [
        (
            "fun run() { val source = \"kept\"; observe(view(source)); consume(view(source)) }",
            "L0163",
            "view(source)",
        ),
        (
            "fun intView(source: Int): borrow Int from source = source\nfun ownInt(own value: Int) {}\nfun run() { val source = 7; ownInt(intView(source)) }",
            "L0163",
            "intView(source)",
        ),
        (
            "fun run() { observe(view(\"temporary\")) }",
            "L0164",
            "view(\"temporary\")",
        ),
        (
            "fun run() { val source = \"kept\"; borrow val result = view(view(source)); println(result) }",
            "L0164",
            "view(source)",
        ),
        (
            "fun wrap(source: String): borrow String from source = view(view(source))",
            "L0164",
            "view(source)",
        ),
        (
            "fun wrong(source: String, other: String): borrow String from source = other\nfun run() { val source = \"kept\"; observe(view(source)) }",
            "L0162",
            "other",
        ),
    ] {
        let checked = ordinary(body);
        for diagnostics in [checked.single.diagnostics(), checked.unit.diagnostics()] {
            let diagnostic = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code().to_string() == code)
                .unwrap_or_else(|| panic!("{body}: {diagnostics:?}"));
            assert_eq!(
                checked.sources.slice(diagnostic.primary_span()).unwrap(),
                primary
            );
            assert!(!diagnostic.details().is_empty());
        }
        assert_atomic_empty(&checked);
    }
}

#[test]
fn cfg_loop_and_indirect_result_uses_remain_unproved() {
    for body in [
        "fun run(flag: Boolean) { val source = \"kept\"; if (flag) { observe(view(source)) } }",
        "fun run() { val source = \"kept\"; loop { observe(view(source)); break } }",
        "fun run() { val source = \"kept\"; val indirect: (String) -> Unit = observe; indirect(view(source)) }",
    ] {
        let checked = ordinary(body);
        for diagnostics in [checked.single.diagnostics(), checked.unit.diagnostics()] {
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code().to_string() == "L0164"),
                "{diagnostics:?}"
            );
        }
        assert_atomic_empty(&checked);
    }
}

#[test]
fn later_deferred_work_clears_successful_call_uses() {
    let checked = ordinary(
        "fun run() { val source = \"kept\"; observe(view(source)); var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }",
    );
    assert!(
        checked.single.diagnostics().is_empty(),
        "{:?}",
        checked.single.diagnostics()
    );
    assert!(
        checked.unit.diagnostics().is_empty(),
        "{:?}",
        checked.unit.diagnostics()
    );
    assert!(!checked.single.deferred().is_empty());
    assert!(!checked.unit.deferred().is_empty());
    assert_atomic_empty(&checked);
}

#[test]
fn inout_receiver_and_nonreturning_sources_do_not_publish_call_uses() {
    for (body, primary) in [
        (
            "fun run(inout source: String) { observe(view(source)) }",
            "view(source)",
        ),
        (
            "fun run(inout source: String) { borrow val parent = source; observe(view(parent)) }",
            "view(parent)",
        ),
        (
            "fun <T> genericView(source: T): borrow T from source = source\nfun observeRecord(source: Record) {}\nclass Record(val text: String) { fun inspect() { observeRecord(genericView(this)) } }",
            "genericView(this)",
        ),
        (
            "class Record(val text: String) { fun inspect() { observe(view(this.text)) } }",
            "view(this.text)",
        ),
        (
            "class Record(val text: String) { fun inspect() { observe(view(text)) } }",
            "view(text)",
        ),
        (
            "fun run() { val source = \"kept\"; println(view(source)) }",
            "view(source)",
        ),
        (
            "fun pair(first: String, second: String) {}\nfun run() { val source = \"kept\"; pair(view(source), error(\"stopped\")) }",
            "view(source)",
        ),
        (
            "fun pair(first: String, second: String) {}\nfun run() { val source = \"kept\"; pair(view(source), return) }",
            "view(source)",
        ),
    ] {
        let checked = ordinary(body);
        for diagnostics in [checked.single.diagnostics(), checked.unit.diagnostics()] {
            let diagnostic = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code().to_string() == "L0164")
                .unwrap_or_else(|| panic!("{body}: {diagnostics:?}"));
            assert_eq!(
                checked.sources.slice(diagnostic.primary_span()).unwrap(),
                primary
            );
        }
        assert_atomic_empty(&checked);
    }
}
