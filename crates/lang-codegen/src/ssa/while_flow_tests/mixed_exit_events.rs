//! Phase 3 and SSA witnesses for the ordinary CLI loop-exit fixture.
use lang_frontend::{
    name_resolution::SymbolKind,
    ownership_checking::{LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget},
    type_checking::UnitExpressionId,
};

use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::{
        model::{Definition, EntityId, Operation},
        verify::verify_program,
    },
};

#[test]
fn short_circuit_exit_fixture_preserves_loan_end_and_resource_drop_order() {
    let text = include_str!("../../../tests/fixtures/short-circuit-loop-exits.ko");
    let analysis = UnitSource::analyze("loop-exits.ko", text).expect("ordinary source is admitted");
    analysis.assert_frontend_clean();
    let sources = analysis.sources();
    let ast = analysis.parsed().ast();
    let unit = analysis.names().names().index().source_units()[0].id();
    let expr = |text| {
        let mut found = ast
            .expressions()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()).unwrap() == text);
        let (id, _) = found.next().unwrap_or_else(|| panic!("missing {text}"));
        assert!(found.next().is_none(), "ambiguous {text}");
        UnitExpressionId::new(unit, id)
    };

    let ownership = analysis.ownership();
    for (exit, expected) in [
        ("continue", &["second", "first"][..]),
        ("break", &["second", "first"][..]),
        (
            r#"return result(Guard("drop:result-arg"))"#,
            &["second", "first", "outer"][..],
        ),
    ] {
        let point = UnitDropPoint::ControlTransfer(expr(exit));
        let drops = ownership
            .drops()
            .iter()
            .filter(|fact| fact.point() == point)
            .map(|fact| {
                assert!(matches!(fact.target(), UnitDropTarget::Named(_)), "{exit}");
                sources.slice(fact.value_origin()).unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(drops, expected, "{exit}");
    }

    let (program, _) = analysis
        .lower_entry(analysis.declaration("main", SymbolKind::Function))
        .expect("ordinary Unit lowers");
    verify_program(&program).expect("SSA verifies");
    let exercise = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains(".exercise.d"))
        .expect("reachable exercise routine");

    for (call_text, guard_text) in [
        (
            r#"observed(Guard("drop:continue-arg"))"#,
            r#"Guard("drop:continue-arg")"#,
        ),
        (
            r#"observed(Guard("drop:break-arg"))"#,
            r#"Guard("drop:break-arg")"#,
        ),
        (
            r#"observed(Guard("drop:return-arg"))"#,
            r#"Guard("drop:return-arg")"#,
        ),
        (
            r#"result(Guard("drop:result-arg"))"#,
            r#"Guard("drop:result-arg")"#,
        ),
    ] {
        let call = expr(call_text);
        let argument = expr(guard_text);
        let point = UnitDropPoint::CallReturn(call);
        let ends = ownership
            .direct_call_loan_ends()
            .iter()
            .filter(|end| end.loan().call() == call)
            .collect::<Vec<_>>();
        let [end] = ends.as_slice() else {
            panic!("{call_text}: expected one published loan end: {ends:?}");
        };
        assert_eq!(end.loan().kind(), LoanKind::Shared);
        assert_eq!(end.loan().argument(), argument);
        assert_eq!(end.loan().target(), &UnitLoanTarget::Temporary(argument));
        assert_eq!(end.point(), point);
        let drops = ownership
            .drops()
            .iter()
            .filter(|drop| drop.point() == point)
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1, "{call_text}: {drops:?}");
        assert_eq!(drops[0].target(), UnitDropTarget::Temporary(argument));
        assert_eq!(end.preceding_drops(), 0, "{call_text}");

        let call = exercise
            .instructions
            .iter()
            .find(|instruction| {
                matches!(instruction.operation, Operation::DirectCall { .. })
                    && sources.slice(instruction.origin.span()).unwrap() == call_text
            })
            .expect("the original call is emitted");
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        let [EntityId::Loan(loan)] = arguments.as_slice() else {
            panic!("{call_text}: shared argument")
        };
        let definition = |entity| {
            let Definition::InstructionResult { instruction, .. } =
                exercise.entity(entity).unwrap().definition
            else {
                panic!("{call_text}: instruction result")
            };
            exercise.instruction(instruction).unwrap()
        };
        let Operation::BorrowBegin { place, .. } = definition(EntityId::Loan(*loan)).operation
        else {
            panic!("{call_text}: loan begins at its temporary place")
        };
        let Operation::RootPlace { owner } = definition(EntityId::Place(place)).operation else {
            panic!("{call_text}: expected the temporary owner")
        };
        let owner_span = exercise
            .entity(EntityId::Value(owner))
            .unwrap()
            .origin
            .span();
        assert_eq!(
            sources.slice(owner_span).unwrap(),
            guard_text,
            "{call_text}"
        );
        let block = exercise.block(call.block).unwrap();
        let call_index = block
            .instructions
            .iter()
            .position(|&id| id == call.id)
            .unwrap();
        let after = block.instructions[call_index + 1..]
            .iter()
            .map(|&id| exercise.instruction(id).unwrap())
            .collect::<Vec<_>>();
        let end_index = after
            .iter()
            .position(|i| i.operation == Operation::BorrowEnd { loan: *loan });
        let drop_index = after
            .iter()
            .position(|i| i.operation == Operation::Drop { owner });
        assert!(
            matches!((end_index, drop_index), (Some(end), Some(drop)) if end < drop),
            "{call_text}: same loan must end before its owner drops: {after:?}"
        );
    }
}
