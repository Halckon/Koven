//! One original expression-body wrapper. All normal on/off prerequisites precede the new claim.
use super::*;
use lang_frontend::{
    name_resolution::ValidatedCompilationUnitNames,
    ownership_checking::{
        CompilationUnitOwnership, LoanKind as FrontendLoanKind, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction, UnitLoanTarget, check_compilation_unit_ownership,
        legacy_ownership_eq, with_loan_observation,
    },
    parser::ParsedFile,
    type_checking::UnitItemId,
};

const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = wrap(owner); consume(item) }";

#[test]
fn expression_wrapper_requires_live_source_causal_link_with_true_off() {
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "forward-links.ko", SOURCE);
    let (environment, types) = standard_environments();
    let mut completed = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "forward-links.ko", source, &parsed),
        ),
        ("memory", SourceUnitInput::memory_document(source, &parsed)),
    ] {
        let inputs = [input];
        let (names, typed, on) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
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
        for owned in [&on, &off] {
            assert_original_forward(&sources, &parsed, &names, owned.ownership());
        }
        let [observation] = on.ownership().loan_action_observations() else {
            panic!("one actual source catalog")
        };
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
        let loan = on.ownership().borrow_forward_transports()[0].source_loan();
        assert_eq!(
            observation
                .actions()
                .iter()
                .filter(|action| {
                    matches!(action, UnitLoanAction::Begin(actual) if actual == loan)
                })
                .count(),
            1
        );
        assert!(
            !observation.actions().iter().any(|action| {
                matches!(action, UnitLoanAction::Cleanup {
                action: UnitIterationCleanupAction::EndCallLoan(actual), ..
            } if actual == loan)
            }),
            "live wrapper source has no End"
        );
        let declarations = names.names().index().declarations();
        let run = declarations
            .iter()
            .find(|declaration| declaration.name() == "run")
            .unwrap();
        let wrap = declarations
            .iter()
            .find(|declaration| declaration.name() == "wrap")
            .unwrap();
        let callable = UnitItemId::new(wrap.source_unit(), wrap.root());
        let mut outputs = Vec::new();
        for (setting, owned) in [("on", &on), ("off", &off)] {
            let (lowered, routes) = crate::ssa::refine::observe_routing(|| {
                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources,
                    &inputs,
                    &names,
                    &types,
                    &typed,
                    owned,
                    run.id(),
                )
            });
            let (program, entry) = lowered.unwrap();
            verify_program(&program).unwrap();
            assert_original_ssa(&program, entry, &routes);
            let llvm = crate::llvm::render_verified_program(&program).unwrap();
            println!(
                "layout={layout}; observer={setting}; original_ssa_verified=true; routes={routes:?}; llvm_bytes={}",
                llvm.len()
            );
            outputs.push((crate::ssa::render_program(&program), llvm));
        }
        assert_eq!(outputs[0], outputs[1]);
        assert!(off.ownership().loan_action_observations().is_empty());
        println!(
            "layout={layout}; original_forward_live=true; on_off_products_ssa_llvm_equal=true"
        );
        completed.push((layout, on, callable));
    }
    assert_eq!(completed.len(), 2);
    println!("all_original_prerequisites_complete=2_layouts_4_lowerings");
    // Only now require the new observation. A missing producer must not mask either layout.
    for (layout, on, callable) in completed {
        let [observation] = on.ownership().loan_action_observations() else {
            panic!("one actual source catalog")
        };
        let handoffs = observation
            .actions()
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                if let UnitLoanAction::BorrowForwardHandoff(handoff) = action {
                    Some((index, handoff))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            handoffs.len(),
            1,
            "{layout}: actual live forward handoff record is required"
        );
        let (index, handoff) = handoffs[0];
        assert_eq!(
            handoff.transaction(),
            &on.ownership().borrow_forward_transports()[0]
        );
        assert_eq!(handoff.callable(), Some(callable));
        let begin = handoff
            .source_begin()
            .expect("actual moved source acquisition");
        assert!(
            matches!(observation.action(&begin), Some(UnitLoanAction::Begin(actual))
            if actual == handoff.transaction().source_loan())
        );
        assert!((0..index).any(|earlier| observation.action_id(earlier).as_ref() == Some(&begin)));
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
    }
}

fn assert_original_forward(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    ownership: &CompilationUnitOwnership,
) {
    let [forwarded] = ownership.borrow_forward_transports() else {
        panic!("one original wrapper transport")
    };
    let [indexed] = names.names().index().source_units() else {
        panic!("one source in either real layout")
    };
    let text = |id: lang_frontend::type_checking::UnitExpressionId| {
        assert_eq!(id.source_unit(), indexed.id());
        let span = parsed
            .ast()
            .expressions()
            .get(id.expression())
            .unwrap()
            .span();
        assert_eq!(span.source_id(), parsed.source_id());
        sources.slice(span).unwrap()
    };
    let source = forwarded.source_loan();
    assert_eq!(text(source.call()), "view(source)");
    assert_eq!(text(source.argument()), "source");
    assert_eq!(source.kind(), FrontendLoanKind::Shared);
    assert!(ownership.loans().contains(source));
    assert_eq!(
        forwarded.handoff_point(),
        UnitDropPoint::CallReturn(source.call())
    );
    assert_eq!(forwarded.returned_expression(), source.call());
    assert_eq!(
        forwarded.point(),
        UnitDropPoint::ControlTransfer(source.call())
    );
    let UnitLoanTarget::Place(place) = source.target() else {
        panic!("original shared parameter root")
    };
    assert!(place.is_root());
    assert!(!ownership.formed_owned_locals().contains(&place.root()));
    assert!(
        !ownership
            .drops()
            .iter()
            .any(|drop| drop.target() == UnitDropTarget::Named(place.root()))
    );
    assert!(
        !ownership
            .direct_call_loan_ends()
            .iter()
            .any(|end| end.loan() == source)
    );
    let [caller] = ownership.borrow_binding_transports() else {
        panic!("independent original caller binding")
    };
    assert_eq!(text(caller.source_loan().call()), "wrap(owner)");
    assert_ne!(caller.source_loan().target(), source.target());
}

fn assert_original_ssa(program: &Program, entry: FunctionId, routes: &[String]) {
    let module = &program.modules[0];
    let caller = module.function(entry).unwrap();
    let [
        literal,
        root,
        begin,
        handoff,
        used,
        result_end,
        source_end,
        drop,
    ] = caller.instructions.as_slice()
    else {
        panic!("eight unchanged caller operations")
    };
    let Operation::StringLiteral { bytes, .. } = &literal.operation else {
        panic!("original literal")
    };
    assert_eq!(bytes, b"kept");
    let [EntityId::Value(owner)] = literal.results.as_slice() else {
        panic!("owner")
    };
    let [EntityId::Place(place)] = root.results.as_slice() else {
        panic!("root place")
    };
    let [EntityId::Loan(source)] = begin.results.as_slice() else {
        panic!("caller source")
    };
    let [EntityId::Loan(result)] = handoff.results.as_slice() else {
        panic!("caller result")
    };
    assert_ne!(source, result);
    assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
    assert_eq!(
        begin.operation,
        Operation::BorrowBegin {
            place: *place,
            kind: LoanKind::Shared
        }
    );
    let Operation::BorrowCall {
        callee: wrap,
        arguments,
        source: actual,
    } = &handoff.operation
    else {
        panic!("ordinary call to wrapper")
    };
    assert_eq!(*actual, *source);
    assert_eq!(arguments, &[EntityId::Loan(*source)]);
    let Operation::DirectCall {
        receiver,
        arguments,
        ..
    } = &used.operation
    else {
        panic!("ordinary consumer")
    };
    assert!(receiver.is_none());
    assert_eq!(arguments, &[EntityId::Loan(*result)]);
    assert!(used.results.is_empty());
    assert_eq!(result_end.operation, Operation::BorrowEnd { loan: *result });
    assert_eq!(source_end.operation, Operation::BorrowEnd { loan: *source });
    assert_eq!(drop.operation, Operation::Drop { owner: *owner });
    let wrapper = module.function(*wrap).unwrap();
    let [EntityId::Loan(parameter)] = wrapper.blocks[0].parameters.as_slice() else {
        panic!("wrapper entry loan")
    };
    assert_ne!(parameter.function(), source.function());
    let [forward] = wrapper.instructions.as_slice() else {
        panic!("one original BorrowCall, no extra wrapper acquisition, End or Drop")
    };
    let [EntityId::Loan(returned)] = forward.results.as_slice() else {
        panic!("wrapper result")
    };
    let Operation::BorrowCall {
        callee: view,
        arguments,
        source: actual,
    } = &forward.operation
    else {
        panic!("original wrapper BorrowCall")
    };
    assert_eq!(*actual, *parameter);
    assert_eq!(arguments, &[EntityId::Loan(*parameter)]);
    assert_ne!(parameter, returned);
    assert_eq!(wrapper.blocks.len(), 1);
    assert_eq!(
        wrapper.blocks[0].terminator.as_ref().unwrap().kind,
        TerminatorKind::BorrowReturn { loan: *returned }
    );
    let view = module.function(*view).unwrap();
    let [EntityId::Loan(view_parameter)] = view.blocks[0].parameters.as_slice() else {
        panic!("view entry loan")
    };
    assert_ne!(view_parameter.function(), parameter.function());
    assert!(view.instructions.is_empty());
    assert_eq!(view.blocks.len(), 1);
    assert_eq!(
        view.blocks[0].terminator.as_ref().unwrap().kind,
        TerminatorKind::BorrowReturn {
            loan: *view_parameter
        }
    );
    assert_eq!(
        routes,
        &[view.name.clone(), wrapper.name.clone(), caller.name.clone()]
    );
}
