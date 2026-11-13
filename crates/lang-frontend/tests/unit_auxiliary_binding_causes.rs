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

const SOURCE: &str = "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, owner); consume(item) }";

#[test]
fn ordinary_auxiliary_binding_requires_actual_use_and_retirement_causes() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("auxiliary-binding-causes.ko", SOURCE)
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let mut completed = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "auxiliary-binding-causes.ko", source, &parsed),
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
        assert_eq!(catalog.auxiliary_retirements().len(), 1);
        let relation = &catalog.auxiliary_retirements()[0];
        assert_eq!(
            catalog.action(&relation.source_begin()),
            Some(&UnitLoanAction::Begin(raw.source_loan().clone()))
        );
        assert_eq!(
            catalog.action(&relation.auxiliary_begin()),
            Some(&UnitLoanAction::Begin(aux.clone()))
        );
        assert_eq!(
            catalog.action(&relation.handoff()),
            Some(&UnitLoanAction::BorrowHandoffCommitted(raw.clone()))
        );
        let uses = catalog
            .actions()
            .iter()
            .filter(|a| matches!(a, UnitLoanAction::BorrowBindingUseReturned(_)))
            .count();
        let retirements = catalog
            .actions()
            .iter()
            .filter(|a| matches!(a, UnitLoanAction::BorrowBindingSourceRetired(_)))
            .count();
        println!(
            "layout={layout}; old_products_equal=true; raw_aux=1; auxiliary_relation=1; actual_use_returns={uses}; actual_source_retirements={retirements}; action_count={}",
            catalog.actions().len()
        );
        completed.push((layout, catalog.clone(), raw.clone(), uses, retirements));
    }
    // Observe both layouts before the missing Partial capability assertions, not an eight-record gate.
    for (layout, catalog, raw, uses, retirements) in completed {
        assert_eq!(
            uses, 1,
            "{layout}: actual auxiliary binding use-return required"
        );
        assert_eq!(
            retirements, 1,
            "{layout}: actual auxiliary source retirement required"
        );
        let relation = &catalog.auxiliary_retirements()[0];
        let (use_index, used) = catalog
            .actions()
            .iter()
            .enumerate()
            .find_map(|(index, a)| match a {
                UnitLoanAction::BorrowBindingUseReturned(used) => Some((index, used)),
                _ => None,
            })
            .unwrap();
        let (retire_index, retired) = catalog
            .actions()
            .iter()
            .enumerate()
            .find_map(|(index, a)| match a {
                UnitLoanAction::BorrowBindingSourceRetired(retired) => Some((index, retired)),
                _ => None,
            })
            .unwrap();
        assert_eq!(used.binding(), raw.binding());
        assert_eq!(used.source_begin(), Some(relation.source_begin()));
        assert_eq!(used.handoff(), Some(relation.handoff()));
        assert_eq!(
            catalog.action(&used.begin().unwrap()),
            Some(&UnitLoanAction::Begin(used.loan().clone()))
        );
        assert_eq!(used.loan().target(), raw.source_loan().target());
        assert_ne!(used.loan().call(), raw.source_loan().call());
        assert_eq!(retired.source_begin(), Some(relation.source_begin()));
        assert_eq!(retired.handoff(), Some(relation.handoff()));
        assert_eq!(retired.use_return(), catalog.action_id(use_index));
        assert!(use_index < retire_index);
        assert_eq!(retired.transaction().source_loan(), raw.source_loan());
        assert_eq!(retired.transaction().binding(), raw.binding());
        let Some(UnitLoanAction::BorrowBindingEnd(end)) =
            catalog.action(&retired.binding_end().unwrap())
        else {
            panic!("real binding end")
        };
        assert_eq!(end.binding(), raw.binding());
        assert_eq!(end.point(), retired.transaction().point());
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        assert!(catalog.binding_regions().is_empty());
    }
}
