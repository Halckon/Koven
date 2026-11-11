//! One original explicit-return wrapper; ordinary analysis and lowering only.
use super::*;
use lang_frontend::ownership_checking::{LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget};

const FORWARD_SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source { return view(source) }\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = wrap(owner); consume(item) }";

#[test]
fn original_single_wrapper_records_live_handoff_and_actual_lowering() {
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "forward-lifecycle.ko", FORWARD_SOURCE);
    let (environment, types) = standard_environments();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "forward-lifecycle.ko", source, &parsed),
        ),
        ("memory", SourceUnitInput::memory_document(source, &parsed)),
    ] {
        let inputs = [input];
        let (names, typed, owned) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let ownership = owned.ownership();
        let [forwarded] = ownership.borrow_forward_transports() else {
            panic!("one actual wrapper handoff");
        };
        let loan = forwarded.source_loan();
        let text = |id: lang_frontend::type_checking::UnitExpressionId| {
            let [indexed] = names.names().index().source_units() else {
                panic!("one source in this real layout");
            };
            assert_eq!(id.source_unit(), indexed.id());
            let span = parsed
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
                .span();
            assert_eq!(span.source_id(), source);
            sources.slice(span).unwrap()
        };
        assert_eq!(text(loan.call()), "view(source)");
        assert_eq!(text(loan.argument()), "source");
        assert_eq!(loan.kind(), LoanKind::Shared);
        assert!(ownership.loans().contains(loan));
        assert_eq!(
            forwarded.handoff_point(),
            UnitDropPoint::CallReturn(loan.call())
        );
        assert_eq!(text(forwarded.returned_expression()), "view(source)");
        let UnitDropPoint::ControlTransfer(returned) = forwarded.point() else {
            panic!("the actual explicit Return, not an inferred source end");
        };
        assert_eq!(text(returned), "return view(source)");
        let UnitLoanTarget::Place(place) = loan.target() else {
            panic!("shared parameter place");
        };
        assert!(place.is_root());
        assert!(!ownership.formed_owned_locals().contains(&place.root()));
        assert!(
            !ownership
                .drops()
                .iter()
                .any(|drop| { drop.target() == UnitDropTarget::Named(place.root()) })
        );
        let [caller] = ownership.borrow_binding_transports() else {
            panic!("independent caller binding transport");
        };
        assert_eq!(text(caller.source_loan().call()), "wrap(owner)");
        assert_ne!(caller.source_loan().target(), loan.target());
        let [observation] = ownership.loan_action_observations() else {
            panic!("one source catalog");
        };
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
        assert_eq!(
            observation
                .actions()
                .iter()
                .filter(|action| {
                    matches!(action, UnitLoanAction::Begin(actual) if actual == loan)
                })
                .count(),
            1,
            "the actually registered wrapper source token"
        );
        assert!(!observation.actions().iter().any(|action| {
            matches!(action, UnitLoanAction::Cleanup {
                action: lang_frontend::ownership_checking::UnitIterationCleanupAction::EndCallLoan(actual),
                ..
            } if actual == loan)
        }), "a live forwarded obligation is not a source End");
        println!("layout={layout}; forward={forwarded:#?}");
        println!("layout={layout}; actions={:#?}", observation.actions());
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "run")
            .unwrap();
        let (lowered, routes) = crate::ssa::refine::observe_routing(|| {
            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                &owned,
                run.id(),
            )
        });
        let (program, entry) = lowered.expect("the original ordinary wrapper source lowers");
        verify_program(&program).unwrap();
        let llvm = crate::llvm::render_verified_program(&program).unwrap();
        println!(
            "layout={layout}; routes={routes:?}; entry={entry:?}; llvm_bytes={}",
            llvm.len()
        );
        println!(
            "layout={layout}; ssa=\n{}",
            crate::ssa::render_program(&program)
        );
    }
}
