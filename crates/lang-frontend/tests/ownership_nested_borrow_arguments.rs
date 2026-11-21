//! 同步借用结果的双 Unit 合同及仍独立比较的 Single nullable 对照。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitId, SourceUnitInput, SourceUnitKey, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipCheckedFile, check_compilation_unit_ownership,
        check_ownership,
    },
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        standard_environments,
    },
};

#[path = "ownership_nested_borrow_arguments/distinct_roots.rs"]
mod distinct_roots;
#[path = "ownership_nested_borrow_arguments/generic_transport.rs"]
mod generic_transport;
#[path = "ownership_nested_borrow_arguments/grouped_transport.rs"]
mod grouped_transport;
#[path = "ownership_nested_borrow_arguments/later_scalar.rs"]
mod later_scalar;
#[path = "ownership_nested_borrow_arguments/owned_auxiliary.rs"]
mod owned_auxiliary;
#[path = "ownership_nested_borrow_arguments/parent_transport.rs"]
mod parent_transport;

struct Checked {
    single: OwnershipCheckedFile,
    unit: CompilationUnitOwnership,
}

fn checked(text: &str, rooted: bool) -> Checked {
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
    let inputs = [if rooted {
        SourceUnitInput::new("root", "nested_borrow.ko", source, &parsed)
    } else {
        SourceUnitInput::memory_document(source, &parsed)
    }];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let [indexed] = index.source_units() else {
        panic!("one original source")
    };
    assert_eq!(indexed.source_id(), source);
    match (rooted, indexed.key()) {
        (false, lang_frontend::name_resolution::SourceUnitKey::MemoryDocument) => {}
        (true, lang_frontend::name_resolution::SourceUnitKey::Rooted { root, logical_path }) => {
            assert_eq!(root.as_str(), "root");
            assert_eq!(logical_path.as_str(), "nested_borrow.ko");
        }
        _ => panic!("requested entry identity"),
    }
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    Checked { single, unit }
}

// Unit-only cases use the original source in both layouts. Other positive cases
// retain independent Single event checks until Unit equivalents are proved.
struct UnitChecked {
    sources: SourceMap,
    parsed: ParsedFile,
    source_units: [SourceUnitId; 2],
    unit_names: [ValidatedCompilationUnitNames; 2],
    memory: CompilationUnitOwnership,
    rooted: CompilationUnitOwnership,
}

fn ordinary_unit(body: &str) -> UnitChecked {
    let text = format!(
        "fun view(source: String): borrow String from source = source\nfun observe(item: String) {{ println(item) }}\nfun consume(own item: String) {{}}\n{body}"
    );
    unit_checked(&text)
}

fn unit_checked(text: &str) -> UnitChecked {
    let mut sources = SourceMap::new();
    let source = sources.add_source("nested_borrow.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let parsed = parse_file(&sources, &lexed).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let [
        (memory, memory_id, memory_names),
        (rooted, rooted_id, rooted_names),
    ] = [
        (false, SourceUnitInput::memory_document(source, &parsed)),
        (
            true,
            SourceUnitInput::new("root", "nested_borrow.ko", source, &parsed),
        ),
    ]
    .map(|(rooted, input)| {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
        let [indexed] = index.source_units() else {
            panic!("one original source");
        };
        assert_eq!(indexed.source_id(), source);
        match (rooted, indexed.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "nested_borrow.ko");
            }
            _ => panic!("expected requested Unit layout"),
        }
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(complete.is_same_analysis(&typed));
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        (owned, indexed.id(), names)
    });
    UnitChecked {
        sources,
        parsed,
        source_units: [memory_id, rooted_id],
        unit_names: [memory_names, rooted_names],
        memory,
        rooted,
    }
}

#[test]
fn direct_and_grouped_results_are_synchronous_borrow_operands() {
    use lang_frontend::{
        ownership_checking::{LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget},
        parser::Expression,
        type_checking::UnitExpressionId,
    };
    for (operand, inner, inner_argument) in [
        ("view(source)", "view(source)", "source"),
        ("((view((source))))", "view((source))", "(source)"),
    ] {
        let checked = ordinary_unit(&format!(
            "fun run() {{ val source = \"kept\"; observe({operand}); consume(source) }}"
        ));
        for unit in [checked.memory, checked.rooted] {
            assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
            assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
            let [usage] = unit.borrow_results().call_uses() else {
                panic!("one verified synchronous borrow operand");
            };
            let [transport] = unit.borrow_call_transports() else {
                panic!("one actual inner-to-outer continuation");
            };
            let [consumer, source] = transport.ended_loans();
            let source_unit = usage.call().source_unit();
            let text = |id: UnitExpressionId| {
                assert_eq!(id.source_unit(), source_unit);
                let span = checked
                    .parsed
                    .ast()
                    .expressions()
                    .get(id.expression())
                    .unwrap()
                    .span();
                assert_eq!(span.source_id(), checked.parsed.source_id());
                checked.sources.slice(span).unwrap()
            };
            assert_eq!(text(source.call()), inner);
            assert_eq!(text(source.argument()), inner_argument);
            assert_eq!(text(consumer.call()), format!("observe({operand})"));
            assert_eq!(text(consumer.argument()), operand);
            assert_eq!(consumer.kind(), LoanKind::Shared);
            assert_eq!(source.kind(), LoanKind::Shared);
            assert_eq!(usage.expression(), source.call());
            assert_eq!(usage.source_loan().call(), source.call());
            assert_eq!(usage.source_loan().argument(), source.argument());
            assert_eq!(usage.call(), consumer.call());
            assert_eq!(usage.argument(), consumer.argument());
            let node = checked
                .parsed
                .ast()
                .expressions()
                .get(usage.call().expression())
                .unwrap();
            let Expression::Call { arguments, .. } = node.payload() else {
                panic!("actual outer call");
            };
            assert!(
                arguments
                    .iter()
                    .any(|arg| arg.value == usage.argument().expression())
            );
            for loan in [source, consumer] {
                assert_eq!(loan.target(), usage.origin());
                assert!(unit.loans().contains(loan));
            }
            let UnitLoanTarget::Place(root) = usage.origin() else {
                panic!("actual named source");
            };
            assert_eq!(
                transport.handoff_point(),
                UnitDropPoint::CallReturn(source.call())
            );
            assert_eq!(
                transport.point(),
                UnitDropPoint::CallReturn(consumer.call())
            );
            assert_ne!(transport.handoff_point(), transport.point());
            assert_eq!(usage.end_point(), transport.point());
            assert!(
                !unit
                    .direct_call_loan_ends()
                    .iter()
                    .any(|end| end.loan() == source || end.loan() == consumer)
            );
            assert!(!unit.drops().iter().any(|drop| drop.target()
                == UnitDropTarget::Temporary(usage.expression())
                || drop.target() == UnitDropTarget::Temporary(usage.argument())));
            assert!(
                !unit
                    .drops()
                    .iter()
                    .any(|drop| drop.target() == UnitDropTarget::Named(root.root())
                        && drop.point() == transport.point())
            );
            assert!(unit.validate().is_ok());
        }
    }
}

#[test]
fn reversed_named_same_owner_chains_end_before_consume() {
    use lang_frontend::{
        ownership_checking::{
            LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, UnitValueDeliveryKind,
        },
        parser::Expression,
        type_checking::UnitExpressionId,
    };

    let checked = ordinary_unit(
        "fun pair(first: String, second: String) {}\nfun run() { val source = \"kept\"; pair(second = view(source), first = view(source)); consume(source) }",
    );
    let expressions = checked.parsed.ast().expressions();
    let node = |id| expressions.get(id).unwrap();
    let text = |span| checked.sources.slice(span).unwrap();
    let call_id = |spelling| {
        let matches: Vec<_> = expressions
            .iter()
            .filter_map(|(id, node)| (text(node.span()) == spelling).then_some(id))
            .collect();
        let [id] = matches.as_slice() else {
            panic!("expected one expression spelling {spelling}");
        };
        *id
    };
    let pair_id = call_id("pair(second = view(source), first = view(source))");
    let consume_id = call_id("consume(source)");
    let arguments = |id| {
        let Expression::Call { arguments, .. } = node(id).payload() else {
            panic!("expected an ordinary call");
        };
        arguments.as_slice()
    };
    let [second, first] = arguments(pair_id) else {
        panic!("expected two named arguments in source order");
    };
    let [consume] = arguments(consume_id) else {
        panic!("expected the original consuming argument");
    };
    assert_eq!(text(second.named_prefix.unwrap().name_span), "second");
    assert_eq!(text(first.named_prefix.unwrap().name_span), "first");
    assert_ne!(second.value, first.value);
    assert!(node(pair_id).span().end() < node(consume_id).span().start());

    for (unit, source_unit) in [checked.memory, checked.rooted]
        .into_iter()
        .zip(checked.source_units)
    {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        let [second_use, first_use] = unit.borrow_results().call_uses() else {
            panic!("expected two borrow uses in source order");
        };
        let [end_a, end_b] = unit.borrow_call_transports() else {
            panic!("expected each same-root chain to retire at the consumer");
        };
        let [delivery] = unit.value_deliveries() else {
            panic!("expected the later ownership transfer");
        };
        let qualified = |id| UnitExpressionId::new(source_unit, id);
        let pair = qualified(pair_id);
        let root = delivery.place().expect("consume must move the named owner");
        assert_eq!(delivery.call(), qualified(consume_id));
        assert_eq!(delivery.argument(), qualified(consume.value));
        assert_eq!(delivery.kind(), UnitValueDeliveryKind::Move);
        assert_eq!(delivery.span(), node(consume.value).span());
        assert_eq!(text(delivery.span()), "source");

        // Match by exact operand identity, not just the shared owner root.
        // The API proves consumer-before-source in each separate transaction.
        for (argument, usage) in [(second, second_use), (first, first_use)] {
            let [source_argument] = arguments(argument.value) else {
                panic!("expected one source operand to view");
            };
            let inner = qualified(argument.value);
            let operand = qualified(source_argument.value);
            assert_eq!(text(node(argument.value).span()), "view(source)");
            assert_eq!(text(node(source_argument.value).span()), "source");
            let end = [end_a, end_b]
                .into_iter()
                .find(|end| end.ended_loans()[0].argument() == inner)
                .expect("unique real transport for this source operand");
            let [consumer, source] = end.ended_loans();
            assert_eq!((consumer.call(), consumer.argument()), (pair, inner));
            assert_eq!((source.call(), source.argument()), (inner, operand));
            assert_eq!((usage.call(), usage.argument()), (pair, inner));
            assert_eq!(usage.expression(), inner);
            assert_eq!(usage.source_loan().call(), inner);
            assert_eq!(usage.source_loan().argument(), operand);
            assert_eq!(usage.origin(), &UnitLoanTarget::Place(root.clone()));
            for loan in [consumer, source] {
                assert_eq!(loan.kind(), LoanKind::Shared);
                assert_eq!(loan.target(), usage.origin());
                assert!(unit.loans().contains(loan));
                assert!(
                    !unit
                        .direct_call_loan_ends()
                        .iter()
                        .any(|direct| direct.loan() == loan)
                );
            }
            assert_eq!(end.handoff_point(), UnitDropPoint::CallReturn(inner));
            assert_eq!(end.point(), UnitDropPoint::CallReturn(pair));
            assert_eq!(usage.end_point(), end.point());
            assert!(
                !unit
                    .drops()
                    .iter()
                    .any(|drop| drop.target() == UnitDropTarget::Temporary(inner))
            );
        }
        assert!(
            !unit
                .drops()
                .iter()
                .any(|drop| drop.target() == UnitDropTarget::Named(root.root()))
        );
        assert!(unit.validate().is_ok());
    }
}

#[test]
fn generic_synchronous_borrow_has_the_exact_unit_transport() {
    generic_transport::assert_transport(
        include_str!("ownership_nested_borrow_arguments/fixtures/generic-transport.ko"),
        "genericView",
        "source: T",
    );
}

#[test]
fn wrapper_synchronous_borrow_has_the_exact_unit_transport() {
    generic_transport::assert_transport(
        include_str!("ownership_nested_borrow_arguments/fixtures/wrapper-transport.ko"),
        "wrap",
        "source: String",
    );
}

#[test]
fn field_borrow_ends_before_releasing_the_exact_record_owner() {
    generic_transport::assert_root_drop_transport(
        include_str!("ownership_nested_borrow_arguments/fixtures/field-transport.ko"),
        "field",
        "source: Record",
    );
}

#[test]
fn scalar_synchronous_borrow_ends_before_moving_the_original_owner() {
    generic_transport::assert_scalar_transport(include_str!(
        "ownership_nested_borrow_arguments/fixtures/scalar-transport.ko"
    ));
}

#[test]
fn later_move_and_exclusive_access_conflict_before_outer_return() {
    for (body, code) in [
        (
            "fun pair(first: String, own second: String) {}\nfun run() { val source = \"kept\"; pair(view(source), source) }",
            "L0135",
        ),
        (
            "fun pair(first: String, own n: Int) {}\nfun take(own source: String): Int = 1\nfun run() { val source = \"kept\"; pair(view(source), take(source)) }",
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
        let checked = ordinary_unit(body);
        for diagnostics in [checked.memory.diagnostics(), checked.rooted.diagnostics()] {
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

fn assert_atomic_empty(checked: &UnitChecked) {
    for owned in [&checked.memory, &checked.rooted] {
        assert!(owned.borrow_results().call_uses().is_empty());
        assert!(owned.borrow_results().bindings().is_empty());
        assert!(owned.borrow_results().ends().is_empty());
    }
}

#[test]
fn grouped_named_operands_and_same_root_chains_are_not_collapsed() {
    grouped_transport::assert_grouped_same_root_transport();
}

#[test]
fn existing_parent_survives_call_and_memory_owner_drops_at_outer_return() {
    // Keep the historical test identity; the direct no-parent case now runs separately.
    parent_transport::assert_parent_drop_transport();
}

#[test]
fn direct_named_owner_drops_at_the_exact_outer_return_without_a_move() {
    generic_transport::assert_root_drop_transport(
        include_str!("ownership_nested_borrow_arguments/fixtures/root-drop-transport.ko"),
        "view",
        "source: String",
    );
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
        let checked = ordinary_unit(body);
        for diagnostics in [checked.memory.diagnostics(), checked.rooted.diagnostics()] {
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
        let checked = ordinary_unit(body);
        for diagnostics in [checked.memory.diagnostics(), checked.rooted.diagnostics()] {
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
    let checked = ordinary_unit(
        "fun run() { val source = \"kept\"; observe(view(source)); var callback: () -> Int = { 1 }; val old = replace(&callback, { 2 }) }",
    );
    assert!(
        checked.memory.diagnostics().is_empty(),
        "{:?}",
        checked.memory.diagnostics()
    );
    assert!(
        checked.rooted.diagnostics().is_empty(),
        "{:?}",
        checked.rooted.diagnostics()
    );
    assert!(!checked.memory.deferred().is_empty());
    assert!(!checked.rooted.deferred().is_empty());
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
        let checked = ordinary_unit(body);
        for diagnostics in [checked.memory.diagnostics(), checked.rooted.diagnostics()] {
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

#[test]
fn nullable_copy_does_not_create_ownership_of_a_move_only_result() {
    let text = "fun view(source: String): borrow String from source = source\nfun observe(value: String?): Unit {}\nfun run() { val n = \"resource\"; observe(view(n)) }";
    for rooted in [false, true] {
        let Checked { single, unit, .. } = checked(text, rooted);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            let [diagnostic] = diagnostics else {
                panic!("{diagnostics:?}")
            };
            assert_eq!(diagnostic.code().to_string(), "L0164");
        }
        assert!(single.borrow_results().call_uses().is_empty());
        assert!(unit.borrow_results().call_uses().is_empty());
    }
}

#[test]
fn nullable_copy_preserves_a_live_parent_and_ends_a_last_use_source() {
    use lang_frontend::ownership_checking::{
        DropPoint, DropTarget, LoanTarget, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
    };

    let overlapping = "fun view(source: Int): borrow Int from source = source\nfun observe(value: Int?, inout changed: Int): Unit { changed = 9 }\nfun keep(value: Int): Unit {}\nfun run() { var n: Int = 7; borrow val parent = view(n); observe(view(parent), &n); keep(parent) }";
    let last_use = "class Boxed(val value: Int)\nfun view(source: Boxed, aux: Boxed): borrow Int from source = source.value\nfun observe(value: Int?): Unit {}\nfun run() { val boxed = Boxed(7); val aux = Boxed(9); observe(view(boxed, aux)) }";

    for rooted in [false, true] {
        let Checked { single, unit, .. } = checked(overlapping, rooted);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            let [diagnostic] = diagnostics else {
                panic!("{diagnostics:?}")
            };
            assert_eq!(diagnostic.code().to_string(), "L0135");
        }

        let accepted = checked(last_use, rooted);
        assert!(
            accepted.single.diagnostics().is_empty(),
            "{:?}",
            accepted.single.diagnostics()
        );
        assert!(
            accepted.unit.diagnostics().is_empty(),
            "{:?}",
            accepted.unit.diagnostics()
        );
        let single = &accepted.single;
        let [usage] = single.borrow_results().call_uses() else {
            panic!("expected one copy")
        };
        let LoanTarget::Place(place) = usage.origin() else {
            panic!("expected owned source place")
        };
        let drops = single
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), DropTarget::Named(_)))
            .collect::<Vec<_>>();
        let [aux, drop] = drops.as_slice() else {
            panic!("{drops:?}")
        };
        assert_eq!(aux.point(), DropPoint::CallReturn(usage.expression()));
        assert_eq!(drop.target(), DropTarget::Named(place.root()));
        assert_eq!(drop.point(), DropPoint::AfterExpression(usage.argument()));

        let unit = &accepted.unit;
        let [usage] = unit.borrow_results().call_uses() else {
            panic!("expected one copy")
        };
        let UnitLoanTarget::Place(place) = usage.origin() else {
            panic!("expected owned source place")
        };
        let drops = unit
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), UnitDropTarget::Named(_)))
            .collect::<Vec<_>>();
        let [aux, drop] = drops.as_slice() else {
            panic!("{drops:?}")
        };
        assert_eq!(aux.point(), UnitDropPoint::CallReturn(usage.expression()));
        assert_eq!(drop.target(), UnitDropTarget::Named(place.root()));
        assert_eq!(
            drop.point(),
            UnitDropPoint::AfterExpression(usage.argument())
        );
    }
}

#[test]
fn nullable_copy_protects_the_source_from_recursive_call_return_cleanup() {
    use lang_frontend::ownership_checking::{
        DropPoint, DropTarget, LoanTarget, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
    };
    let text = "class Boxed(val value: Int)\nfun view(source: Boxed, aux: () -> Int): borrow Int from source = source.value\nfun observe(value: Int?): Unit {}\nfun run() { val boxed = Boxed(7); val aux: () -> Int = { boxed.value }; observe(view(boxed, aux)) }";
    for rooted in [false, true] {
        let Checked { single, unit, .. } = checked(text, rooted);
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        let [usage] = single.borrow_results().call_uses() else {
            panic!("expected one copy")
        };
        let LoanTarget::Place(place) = usage.origin() else {
            panic!("expected a source place")
        };
        let points: Vec<_> = single
            .drops()
            .iter()
            .filter(|fact| fact.target() == DropTarget::Named(place.root()))
            .map(|fact| fact.point())
            .collect();
        assert_eq!(points, [DropPoint::AfterExpression(usage.argument())]);

        let [usage] = unit.borrow_results().call_uses() else {
            panic!("expected one copy")
        };
        let UnitLoanTarget::Place(place) = usage.origin() else {
            panic!("expected a source place")
        };
        let points: Vec<_> = unit
            .drops()
            .iter()
            .filter(|fact| fact.target() == UnitDropTarget::Named(place.root()))
            .map(|fact| fact.point())
            .collect();
        assert_eq!(points, [UnitDropPoint::AfterExpression(usage.argument())]);
    }
}
