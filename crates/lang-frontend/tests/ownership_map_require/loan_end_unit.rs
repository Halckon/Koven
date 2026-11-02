//! Original successful witnesses, with actual source-qualified loan handoff/end facts.
use super::*;
use lang_frontend::{
    name_resolution::UnitReferenceTarget,
    ownership_checking::{
        BorrowBindingStorage, LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
        UnitValueDeliveryKind,
    },
    parser::{Expression, Statement},
    type_checking::{BorrowReturnOrigin, UnitExpressionId, UnitStatementId},
};

fn assert_contract(text: &str, last_use: &str, consume_owner: bool) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("map.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty());
    let expression = |text: &str| {
        let matches = file
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                (matches!(node.payload(), Expression::Call { .. })
                    && sources.slice(node.span()).unwrap() == text)
                    .then_some(id)
            })
            .collect::<Vec<_>>();
        let [id] = matches.as_slice() else {
            panic!("unique original call: {text}")
        };
        *id
    };
    let last_expression = expression(last_use);
    let statements = file.ast().statements().iter().filter_map(|(id, node)| {
        matches!(node.payload(), Statement::Expression { expression } if *expression == last_expression)
            .then_some(id)
    }).collect::<Vec<_>>();
    let [last_statement] = statements.as_slice() else {
        panic!("exact last-use statement")
    };
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::memory_document(source, &file),
        SourceUnitInput::new("root", "map.ko", source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let source_unit = index.source_units()[0].id();
        assert_eq!(index.source_units()[0].source_id(), source);
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(
            typed.diagnostics().is_empty(),
            "{text}: {:?}",
            typed.diagnostics()
        );
        let [require] = typed.map_require_values() else {
            panic!("one original requireValue")
        };
        let require = *require;
        assert_eq!(
            require.expression(),
            UnitExpressionId::new(source_unit, expression("m.requireValue(key)"))
        );
        assert_eq!(
            require.borrow_return().origin(),
            BorrowReturnOrigin::Receiver
        );
        assert_eq!(
            sources
                .slice(require.borrow_return().marker_span())
                .unwrap(),
            "requireValue"
        );
        let receiver_span = file
            .ast()
            .expressions()
            .get(require.receiver().expression())
            .unwrap()
            .span();
        assert_eq!(sources.slice(receiver_span).unwrap(), "m");
        assert_eq!(require.receiver().source_unit(), source_unit);
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(
            owned.diagnostics().is_empty(),
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert!(owned.deferred().is_empty());
        assert!(owned.is_compatible_with(&typed));
        assert!(owned.clone().validate().is_ok());
        let [binding] = owned.borrow_results().bindings() else {
            panic!("one result binding")
        };
        let Expression::Call { arguments, .. } = file
            .ast()
            .expressions()
            .get(last_expression)
            .unwrap()
            .payload()
        else {
            panic!("actual last-use call")
        };
        let [argument] = arguments.as_slice() else {
            panic!("one original item argument")
        };
        let item_span = file.ast().expressions().get(argument.value).unwrap().span();
        assert_eq!(sources.slice(item_span).unwrap(), "item");
        let item_references = names
            .names()
            .references()
            .iter()
            .filter(|reference| reference.span() == item_span)
            .collect::<Vec<_>>();
        let [item_reference] = item_references.as_slice() else {
            panic!("one actual item reference")
        };
        assert_eq!(
            item_reference.target(),
            &UnitReferenceTarget::Symbol(binding.binding())
        );
        // Parameter capabilities live in binding_kind; ordinary borrow locals use borrow_results.
        assert_eq!(owned.binding_kind(binding.binding()), None);
        assert!(!owned.formed_owned_locals().contains(&binding.binding()));
        assert_eq!(binding.initializer(), require.expression());
        assert_eq!(binding.storage(), BorrowBindingStorage::BorrowedStorage);
        assert_eq!(binding.parent(), None);
        let UnitLoanTarget::Place(origin) = binding.origin() else {
            panic!("real receiver place")
        };
        assert!(origin.is_root());
        assert_eq!(origin.root().source_unit(), source_unit);
        let references = names
            .names()
            .references()
            .iter()
            .filter(|reference| reference.span() == receiver_span)
            .collect::<Vec<_>>();
        let [reference] = references.as_slice() else {
            panic!("one receiver reference")
        };
        assert_eq!(
            reference.target(),
            &UnitReferenceTarget::Symbol(origin.root())
        );
        assert_eq!(
            typed.types().expression_type(require.receiver()),
            typed.types().symbol_type(origin.root())
        );
        let source_loan = binding.source_loan().expect("continued receiver loan");
        assert_eq!(source_loan.call(), require.expression());
        assert_eq!(source_loan.argument(), require.receiver());
        let loans = owned
            .loans()
            .iter()
            .filter(|loan| {
                loan.call() == source_loan.call() && loan.argument() == source_loan.argument()
            })
            .collect::<Vec<_>>();
        let [loan] = loans.as_slice() else {
            panic!("one actual receiver loan")
        };
        assert_eq!(loan.kind(), LoanKind::Shared);
        assert_eq!(loan.target(), binding.origin());
        let [end] = owned.borrow_results().ends() else {
            panic!("one actual loan end")
        };
        assert_eq!(end.binding(), binding.binding());
        assert_eq!(
            end.point(),
            UnitDropPoint::AfterStatement(UnitStatementId::new(source_unit, *last_statement))
        );
        assert!(
            file.ast()
                .statements()
                .get(*last_statement)
                .unwrap()
                .span()
                .end()
                < sources
                    .source_text(source)
                    .unwrap()
                    .rfind("println(key)")
                    .unwrap()
        );
        // The result is non-owning; neither it nor its already-moved/borrowed receiver is dropped here.
        assert!(!owned.drops().iter().any(|drop| matches!(drop.target(),
            UnitDropTarget::Named(symbol) if symbol == binding.binding() || symbol == origin.root())));
        assert!(!owned.drops().iter().any(|drop| matches!(drop.target(),
            UnitDropTarget::Temporary(value) if value == require.expression())));
        if consume_owner {
            let consume = UnitExpressionId::new(source_unit, expression("consume(m)"));
            let deliveries = owned
                .value_deliveries()
                .iter()
                .filter(|fact| fact.call() == consume)
                .collect::<Vec<_>>();
            let [delivery] = deliveries.as_slice() else {
                panic!("one owner delivery after end")
            };
            assert_eq!(delivery.kind(), UnitValueDeliveryKind::Move);
            assert_eq!(delivery.place(), Some(origin));
            assert!(
                file.ast()
                    .expressions()
                    .get(consume.expression())
                    .unwrap()
                    .span()
                    .start()
                    > file
                        .ast()
                        .statements()
                        .get(*last_statement)
                        .unwrap()
                        .span()
                        .end()
            );
        }
    }
}

#[test]
fn original_mutable_map_results_end_at_last_use_before_owner_delivery() {
    for value in ["String", "Packet", "Resource", "Token?"] {
        let text = format!(
            "value class Packet(val text: String)\nclass Resource {{ deinit() {{}} }}\nclass Token(val n: Int)\nfun observe(item: {value}) {{}}\nfun consume(own m: MutableMap<String, {value}>) {{}}\nfun run() {{ var m = mutableMapOf<String, {value}>(); val key = \"key\"; {{ borrow val item = m.requireValue(key); observe(item); println(key) }}; consume(m) }}"
        );
        assert_contract(&text, "observe(item)", true);
    }
}

#[test]
fn original_readonly_map_result_ends_before_unrelated_key_use() {
    assert_contract(
        "fun run(m: Map<String, String>, key: String) { borrow val item = m.requireValue(key); println(item); println(key) }",
        "println(item)",
        false,
    );
}
