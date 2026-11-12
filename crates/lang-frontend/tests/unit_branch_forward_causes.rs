//! Ordinary dynamic source: per-exit causes are not Complete coverage or receipts.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint, UnitLoanAction, UnitLoanActionCoverage, check_compilation_unit_ownership,
        legacy_ownership_eq, with_loan_observation,
    },
    parser::{Expression, FunctionBody, FunctionForm, Item, Statement, parse_file},
    source::SourceMap,
    type_checking::{UnitItemId, check_compilation_unit_types, standard_environments},
};
const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun wrap(source: String, flag: Boolean): borrow String from source { if (flag) { return view(source) }; return view(source) }";

#[test]
fn ordinary_dynamic_branch_requires_independent_path_coverage() {
    // Reuse the complete original AST/actual-cause oracle before asking for new coverage.
    ordinary_dynamic_branch_has_two_distinct_actual_acquisition_causes();
    let mut sources = SourceMap::new();
    let source = sources.add_source("branch-coverage.ko", SOURCE).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let mut results = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "branch-coverage.ko", source, &parsed),
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
            panic!("catalog")
        };
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        assert!(
            catalog.forward_regions().is_empty(),
            "old expression receipt input stays empty"
        );
        let handoffs = catalog
            .actions()
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::BorrowForwardHandoff(handoff) => Some(handoff),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(handoffs.len(), 2);
        assert_eq!(on.ownership().borrow_forward_transports().len(), 2);
        for (handoff, raw) in handoffs
            .iter()
            .zip(on.ownership().borrow_forward_transports())
        {
            assert_eq!(handoff.transaction(), raw);
            assert_eq!(
                handoff.callable(),
                Some(UnitItemId::new(
                    raw.returned_expression().source_unit(),
                    parsed.roots()[1]
                ))
            );
            assert_eq!(
                catalog.action(&handoff.source_begin().unwrap()),
                Some(&UnitLoanAction::Begin(raw.source_loan().clone()))
            );
            assert!(handoff.branch().is_some());
        }
        assert_ne!(handoffs[0].source_begin(), handoffs[1].source_begin());
        assert_ne!(
            handoffs[0].branch().unwrap().then_arm(),
            handoffs[1].branch().unwrap().then_arm()
        );
        let coverage = catalog
            .branch_forward_regions()
            .iter()
            .map(|region| region.coverage())
            .collect::<Vec<_>>();
        for region in catalog.branch_forward_regions() {
            assert!(catalog.owns_branch_forward_region(region));
            assert_eq!(region.callable(), handoffs[0].callable().unwrap());
            let first = handoffs[0].branch().unwrap();
            assert_eq!(region.split(), first.split());
            assert_eq!(region.condition(), first.condition());
            let lang_frontend::ownership_checking::UnitLoanTarget::Place(place) =
                handoffs[0].transaction().source_loan().target()
            else {
                panic!("root source")
            };
            assert_eq!(region.source_parameter(), place.root());
            assert_eq!(
                region.paths().len(),
                2,
                "both declared exits must be certified"
            );
            let mut all = Vec::new();
            for path in region.paths() {
                let matching = handoffs
                    .iter()
                    .filter(|h| h.branch() == Some(path.branch()))
                    .collect::<Vec<_>>();
                assert_eq!(matching.len(), 1);
                let handoff = matching[0];
                assert_eq!(path.source_begin(), handoff.source_begin().unwrap());
                assert_eq!(
                    catalog.action(&path.live_handoff()),
                    Some(&UnitLoanAction::BorrowForwardHandoff((**handoff).clone()))
                );
                let ids = path.action_ids().collect::<Vec<_>>();
                assert_eq!(ids, vec![path.source_begin(), path.live_handoff()]);
                for id in ids {
                    assert!(
                        !all.contains(&id),
                        "mutually exclusive paths must not share record identities"
                    );
                    assert!(catalog.action(&id).is_some());
                    all.push(id);
                }
            }
            assert_ne!(
                region.paths()[0].branch().then_arm(),
                region.paths()[1].branch().then_arm()
            );
            assert_eq!(
                all.len(),
                catalog.actions().len(),
                "this original fixture has no hidden condition or path records"
            );
        }
        println!(
            "layout={layout}; both_modes_ownership_succeeded=true; actual_causes=2; source=Partial; branch_regions={}",
            coverage.len()
        );
        results.push((layout, coverage));
    }
    // Both layouts and all true-off preconditions must finish before the capability red.
    for (layout, coverage) in results {
        assert_eq!(
            coverage.len(),
            1,
            "{layout}: missing independent two-path region"
        );
        assert_eq!(coverage[0], UnitLoanActionCoverage::Complete);
    }
}

#[test]
fn ordinary_dynamic_branch_has_two_distinct_actual_acquisition_causes() {
    let mut sources = SourceMap::new();
    let source = sources.add_source("branch-causes.ko", SOURCE).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let Item::Function {
        form:
            FunctionForm::Explicit {
                body: FunctionBody::Block(block),
                ..
            },
        ..
    } = parsed
        .ast()
        .items()
        .get(parsed.roots()[1])
        .unwrap()
        .payload()
    else {
        panic!("wrapper block")
    };
    let Statement::Block { elements } = parsed.ast().statements().get(*block).unwrap().payload()
    else {
        panic!("block")
    };
    let [split_statement, tail_statement] = elements.as_slice() else {
        panic!("two original statements")
    };
    let statement_expression = |id| {
        let Statement::Expression { expression } =
            parsed.ast().statements().get(id).unwrap().payload()
        else {
            panic!("expression statement")
        };
        *expression
    };
    let split = statement_expression(*split_statement);
    let tail = statement_expression(*tail_statement);
    let Expression::If {
        condition,
        then_branch,
        else_branch: None,
        ..
    } = parsed.ast().expressions().get(split).unwrap().payload()
    else {
        panic!("dynamic if")
    };
    let (Statement::Block { elements } | Statement::ControlBody { elements }) = parsed
        .ast()
        .statements()
        .get(*then_branch)
        .unwrap()
        .payload()
    else {
        panic!("true body")
    };
    let [then_statement] = elements.as_slice() else {
        panic!("one true return")
    };
    let then_control = statement_expression(*then_statement);
    let (environment, types) = standard_environments();
    let mut results = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "branch-causes.ko", source, &parsed),
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
        assert!(off.ownership().loan_action_observations().is_empty());
        assert_eq!(
            on.ownership().owner_observations(),
            off.ownership().owner_observations()
        );
        let [catalog] = on.ownership().loan_action_observations() else {
            panic!("one source catalog")
        };
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        assert!(
            catalog.forward_regions().is_empty(),
            "no new Complete region"
        );
        let handoffs = catalog
            .actions()
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::BorrowForwardHandoff(handoff) => Some(handoff),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(handoffs.len(), 2);
        assert_eq!(on.ownership().borrow_forward_transports().len(), 2);
        let mut links = Vec::new();
        for (handoff, raw) in handoffs
            .iter()
            .zip(on.ownership().borrow_forward_transports())
        {
            assert_eq!(handoff.transaction(), raw);
            if let Some(callable) = handoff.callable() {
                assert_eq!(
                    callable,
                    UnitItemId::new(raw.returned_expression().source_unit(), parsed.roots()[1])
                );
            }
            let UnitDropPoint::ControlTransfer(control) = raw.point() else {
                panic!("return control")
            };
            let Expression::Return {
                value: Some(value), ..
            } = parsed
                .ast()
                .expressions()
                .get(control.expression())
                .unwrap()
                .payload()
            else {
                panic!("explicit return")
            };
            assert_eq!(raw.returned_expression().expression(), *value);
            assert_ne!(
                control,
                raw.returned_expression(),
                "control is not call identity"
            );
            assert_eq!(raw.source_loan().call(), raw.returned_expression());
            let linked = handoff.source_begin().inspect(|id| {
                assert_eq!(
                    catalog.action(id),
                    Some(&UnitLoanAction::Begin(raw.source_loan().clone()))
                );
            });
            if let Some(branch) = handoff.branch() {
                assert_eq!(branch.callable(), handoff.callable().unwrap());
                assert_eq!(branch.control(), control);
                assert_eq!(branch.value(), raw.returned_expression());
                assert_eq!(branch.split().source_unit(), control.source_unit());
                assert_eq!(branch.condition().source_unit(), control.source_unit());
                assert_eq!(branch.split().expression(), split);
                assert_eq!(branch.condition().expression(), *condition);
                assert_eq!(
                    branch.control().expression(),
                    if branch.then_arm() {
                        then_control
                    } else {
                        tail
                    }
                );
            }
            links.push(linked);
        }
        assert_ne!(
            handoffs[0].transaction().point(),
            handoffs[1].transaction().point()
        );
        assert_ne!(
            handoffs[0].transaction().source_loan().call(),
            handoffs[1].transaction().source_loan().call()
        );
        assert_eq!(
            handoffs[0].transaction().source_loan().target(),
            handoffs[1].transaction().source_loan().target()
        );
        let callables = handoffs
            .iter()
            .map(|handoff| handoff.callable())
            .collect::<Vec<_>>();
        println!(
            "layout={layout}; ownership_phase_succeeded=true; handoffs=2; linked={}; callable_links={}; coverage=Partial",
            links.iter().filter(|x| x.is_some()).count(),
            callables.iter().filter(|x| x.is_some()).count()
        );
        let branches = handoffs
            .iter()
            .map(|handoff| handoff.branch())
            .collect::<Vec<_>>();
        results.push((layout, links, callables, branches));
    }
    for (layout, links, callables, branches) in results {
        assert!(
            branches.iter().all(Option::is_some),
            "both real branch entries must be proved"
        );
        assert_ne!(
            branches[0].unwrap().then_arm(),
            branches[1].unwrap().then_arm()
        );
        assert!(
            links.iter().all(Option::is_some),
            "{layout}: both actual acquisitions need causes"
        );
        assert_ne!(
            links[0], links[1],
            "mutually exclusive calls never share acquisition"
        );
        assert!(
            callables.iter().all(Option::is_some),
            "original wrapper identity"
        );
        assert_eq!(callables[0], callables[1]);
    }
}

#[test]
fn ordinary_unadmitted_exit_shapes_keep_raw_partial_projection() {
    for (body, expected) in [
        ("{ return view(source) }", 1),
        (
            "{ if (flag) { return view(source) }; return ((view(source))) }",
            2,
        ),
    ] {
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun wrap(source: String, flag: Boolean): borrow String from source {body}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("branch-neighbor.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "branch-neighbor.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
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
                panic!("catalog")
            };
            assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
            assert!(catalog.forward_regions().is_empty());
            let handoffs = catalog
                .actions()
                .iter()
                .filter_map(|action| match action {
                    UnitLoanAction::BorrowForwardHandoff(handoff) => Some(handoff),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(handoffs.len(), expected);
            assert_eq!(on.ownership().borrow_forward_transports().len(), expected);
            for (handoff, raw) in handoffs
                .into_iter()
                .zip(on.ownership().borrow_forward_transports())
            {
                assert_eq!(handoff.transaction(), raw);
                assert!(handoff.source_begin().is_none());
                assert!(handoff.callable().is_none());
                assert!(handoff.branch().is_none());
            }
        }
    }
}
