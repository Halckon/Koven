//! Ordinary original-source capability checkpoint, never synthetic relationship assembly.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropTarget, UnitIterationCleanupAction, UnitLoanAction, UnitLoanActionCoverage,
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

const SOURCE: &str = "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, owner); consume(item) }";

#[test]
fn ordinary_auxiliary_binding_requires_complete_interval() {
    let mut sources = SourceMap::new();
    let source = sources.add_source("auxiliary-binding.ko", SOURCE).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let mut completed = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "auxiliary-binding.ko", source, &parsed),
        ),
        ("memory", SourceUnitInput::memory_document(source, &parsed)),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let on = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        let off = with_loan_observation(false, || {
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap()
        });
        assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
        assert_eq!(
            on.ownership().owner_observations(),
            off.ownership().owner_observations()
        );
        assert!(off.ownership().loan_action_observations().is_empty());
        let [catalog] = on.ownership().loan_action_observations() else {
            panic!("one catalog")
        };
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        assert!(catalog.binding_regions().is_empty());
        let [raw] = on.ownership().borrow_call_handoffs() else {
            panic!("one raw handoff")
        };
        let [aux] = raw.auxiliary_loan_ends() else {
            panic!("one raw auxiliary retirement")
        };
        assert_ne!(raw.source_loan().argument(), aux.argument());
        assert_eq!(raw.source_loan().target(), aux.target());
        assert_eq!(raw.source_loan().call(), aux.call());
        assert!(!raw.auxiliary_loan_ends().contains(raw.source_loan()));
        for loan in [raw.source_loan(), aux] {
            assert_eq!(
                catalog
                    .actions()
                    .iter()
                    .filter(|action| {
                        matches!(action, UnitLoanAction::Begin(actual) if actual == loan)
                    })
                    .count(),
                1
            );
        }
        assert_eq!(catalog.actions().iter().filter(|action| {
            matches!(action, UnitLoanAction::BorrowHandoffCommitted(actual) if actual == raw)
        }).count(), 1);
        let [relation] = catalog.auxiliary_retirements() else {
            panic!("one actual aux relation")
        };
        let [
            UnitLoanAction::Begin(source_begin),
            UnitLoanAction::Begin(aux_begin),
            UnitLoanAction::BorrowHandoffCommitted(committed),
            UnitLoanAction::Begin(consumer_begin),
            UnitLoanAction::BorrowBindingUseReturned(used),
            UnitLoanAction::BorrowBindingEnd(ended),
            UnitLoanAction::BorrowBindingSourceRetired(retired),
            UnitLoanAction::Cleanup {
                point,
                action: UnitIterationCleanupAction::Drop(drop),
            },
        ] = catalog.actions()
        else {
            panic!("entire original eight records, without filtering")
        };
        assert_eq!(source_begin, raw.source_loan());
        assert_eq!(aux_begin, aux);
        assert_eq!(committed, raw);
        assert_eq!(relation.source_begin(), catalog.action_id(0).unwrap());
        assert_eq!(relation.auxiliary_begin(), catalog.action_id(1).unwrap());
        assert_eq!(relation.handoff(), catalog.action_id(2).unwrap());
        assert_eq!(used.source_begin(), catalog.action_id(0));
        assert_eq!(used.handoff(), catalog.action_id(2));
        assert_eq!(used.begin(), catalog.action_id(3));
        assert_eq!(used.loan(), consumer_begin);
        assert_eq!(used.binding(), raw.binding());
        assert_eq!(ended.binding(), raw.binding());
        assert_eq!(retired.source_begin(), catalog.action_id(0));
        assert_eq!(retired.handoff(), catalog.action_id(2));
        assert_eq!(retired.use_return(), catalog.action_id(4));
        assert_eq!(retired.binding_end(), catalog.action_id(5));
        assert_eq!(retired.transaction().source_loan(), source_begin);
        assert_eq!(retired.transaction().binding(), raw.binding());
        assert_eq!(retired.transaction().point(), ended.point());
        assert_eq!(*point, ended.point());
        assert_eq!(drop.point(), *point);
        let lang_frontend::ownership_checking::UnitLoanTarget::Place(root) = source_begin.target()
        else {
            panic!("root")
        };
        assert_eq!(drop.target(), UnitDropTarget::Named(root.root()));
        assert_eq!(source_begin.target(), consumer_begin.target());
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        let callable = lang_frontend::type_checking::UnitItemId::new(run.source_unit(), run.root());
        use lang_frontend::parser::{FunctionBody, FunctionForm, Item, Statement};
        use lang_frontend::type_checking::UnitStatementId;
        let Item::Function {
            form:
                FunctionForm::Explicit {
                    body: FunctionBody::Block(body),
                    ..
                },
            ..
        } = parsed.ast().items().get(run.root()).unwrap().payload()
        else {
            panic!("original explicit Unit caller body")
        };
        let Statement::Block { elements } = parsed.ast().statements().get(*body).unwrap().payload()
        else {
            panic!("original three-statement block")
        };
        assert_eq!(elements.len(), 3);
        let entry_statement = UnitStatementId::new(run.source_unit(), elements[1]);
        let completion_statement = UnitStatementId::new(run.source_unit(), *body);
        assert_eq!(
            ended.point(),
            lang_frontend::ownership_checking::UnitDropPoint::AfterStatement(UnitStatementId::new(
                run.source_unit(),
                elements[2]
            ))
        );
        println!(
            "layout={layout}; old_products_equal=true; raw_records=8; auxiliary_relations=1; new_regions={}",
            catalog.auxiliary_binding_regions().len()
        );
        completed.push((
            layout,
            catalog.clone(),
            callable,
            root.root(),
            raw.binding(),
            source_begin.call(),
            consumer_begin.call(),
            entry_statement,
            completion_statement,
        ));
    }
    // Both layouts complete their original-product oracles before the capability assertion.
    for (layout, catalog, callable, root, binding, source_call, consumer_call, entry, completion) in
        completed
    {
        assert_eq!(
            catalog.auxiliary_binding_regions().len(),
            1,
            "{layout}: independently entered auxiliary binding region required"
        );
        let region = &catalog.auxiliary_binding_regions()[0];
        assert!(catalog.owns_auxiliary_binding_region(region));
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.callable(), callable);
        assert_eq!(region.root(), root);
        assert_eq!(region.binding(), binding);
        assert_eq!(region.source_call(), source_call);
        assert_eq!(region.consumer_call(), consumer_call);
        assert_eq!(region.entry_statement(), entry);
        assert_eq!(region.completion_statement(), completion);
        assert_eq!(
            region.action_ids().collect::<Vec<_>>(),
            (0..8)
                .map(|i| catalog.action_id(i).unwrap())
                .collect::<Vec<_>>()
        );
        assert!(
            catalog.binding_regions().is_empty(),
            "old seven-record gate unchanged"
        );
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
    }
}

#[test]
fn ordinary_auxiliary_region_does_not_cover_extra_owner_use_or_nested_control() {
    for body in [
        "val owner = \"kept\"; borrow val item = view(owner, owner); consume(item); consume(owner)",
        "val owner = \"kept\"; if (true) { borrow val item = view(owner, owner); consume(item) }",
        "val owner = \"kept\"; borrow val item = view(owner, owner); if (true) { consume(item) } else { consume(item) }",
    ] {
        let text = format!(
            "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{ {body} }}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("aux-boundary.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        let inputs = [SourceUnitInput::memory_document(source, &parsed)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let on = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        let off = with_loan_observation(false, || {
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap()
        });
        assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
        assert!(
            on.ownership()
                .loan_action_observations()
                .iter()
                .flat_map(|catalog| catalog.auxiliary_binding_regions())
                .all(|region| region.coverage() != UnitLoanActionCoverage::Complete),
            "{body}"
        );
    }
}

#[test]
fn ordinary_non_auxiliary_binding_keeps_its_original_region() {
    let text = SOURCE
        .replace(", aux: String", "")
        .replace("view(owner, owner)", "view(owner)");
    let mut sources = SourceMap::new();
    let source = sources.add_source("old-binding.ko", &text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let result = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
        .unwrap()
        .validate()
        .unwrap();
    let [catalog] = result.ownership().loan_action_observations() else {
        panic!("one catalog")
    };
    assert!(catalog.auxiliary_binding_regions().is_empty());
    let [region] = catalog.binding_regions() else {
        panic!("original seven-record region")
    };
    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
    assert_eq!(region.action_ids().len(), 7);
}
