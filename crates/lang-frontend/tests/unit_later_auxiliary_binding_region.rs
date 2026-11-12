//! Ordinary original-source capability checkpoint, never synthetic relationship assembly.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitLoanAction, UnitLoanActionCoverage, check_compilation_unit_ownership,
        legacy_ownership_eq, with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn ordinary_later_auxiliary_requires_complete_actual_return() {
    for second in [false, true] {
        let (parameters, arguments) = if second {
            ("aux: String, source: String", "aux, owner")
        } else {
            ("source: String, aux: String", "owner, aux")
        };
        let text = format!(
            "fun view({parameters}): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; val aux = \"aux\"; borrow val item = view({arguments}); consume(item); consume(aux) }}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("later-aux.ko", &text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for (layout, input) in [
            (
                "rooted",
                SourceUnitInput::new("root", "later-aux.ko", source, &parsed),
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
            assert!(catalog.distinct_auxiliary_binding_regions().is_empty());
            let [relation] = catalog.distinct_auxiliary_retirements() else {
                panic!("relation")
            };
            assert!(relation.auxiliary_drop().is_none());
            assert_eq!(relation.source_parameter(), usize::from(second));
            assert_eq!(
                catalog.later_auxiliary_binding_regions().len(),
                1,
                "{layout}/source-second={second}: independently complete later-use region"
            );
            let region = &catalog.later_auxiliary_binding_regions()[0];
            assert!(catalog.owns_later_auxiliary_binding_region(region));
            assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
            assert_eq!(region.source_parameter(), Some(usize::from(second)));
            assert_eq!(region.source_owner(), Some(relation.source_owner()));
            assert_eq!(region.auxiliary_owner(), Some(relation.auxiliary_owner()));
            let ids = region.action_ids().collect::<Vec<_>>();
            assert_eq!(ids.len(), 11);
            assert_eq!(ids[usize::from(second)], relation.source_begin());
            assert_eq!(ids[1 - usize::from(second)], relation.auxiliary_begin());
            assert_eq!(ids[2], relation.handoff());
            assert_eq!(region.later_begin().as_ref(), ids.get(8));
            let actions = ids
                .iter()
                .map(|id| catalog.action(id).unwrap())
                .collect::<Vec<_>>();
            let UnitLoanAction::Begin(later) = actions[8] else {
                panic!("later acquisition")
            };
            assert_eq!(Some(later.call()), region.later_consumer_call());
            assert_eq!(
                later.target(),
                on.ownership().borrow_call_handoffs()[0].auxiliary_loan_ends()[0].target()
            );
            assert!(
                matches!(actions[9], UnitLoanAction::Cleanup { point, action: lang_frontend::ownership_checking::UnitIterationCleanupAction::EndCallLoan(end) } if end == later && *point == lang_frontend::ownership_checking::UnitDropPoint::CallReturn(later.call()))
            );
            assert!(
                matches!(actions[7], UnitLoanAction::Cleanup { action: lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop), .. } if drop.target() == lang_frontend::ownership_checking::UnitDropTarget::Named(region.root()))
            );
            assert!(
                matches!(actions[10], UnitLoanAction::Cleanup { action: lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop), .. } if Some(drop.target()) == region.auxiliary_root().map(lang_frontend::ownership_checking::UnitDropTarget::Named))
            );
        }
    }
}
