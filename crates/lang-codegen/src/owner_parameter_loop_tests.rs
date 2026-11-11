//! Ordinary finite Parameter -> mutable root probe; no injected route or identity queries.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(flag: Boolean, own incoming: String): String {
    var held = incoming
    while(flag) { val previous = replace(&held, "next"); return held }
    return held
}
fun entry(): Unit { val first = run(false, "outer"); val second = run(true, "outer") }
"#;

#[test]
fn parameter_open_loop_original_source_probe() {
    use crate::ssa::model::{EntityId, Operation, TerminatorKind};
    use lang_frontend::ownership_checking::UnitDropPoint;
    let source = UnitSource::analyze("parameter-open-loop.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (program, entry) = source.lower_entry(source.entry()).unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    eprintln!("OWNERSHIP {:#?}", source.ownership());
    eprintln!("SSA {}", crate::ssa::render_program(&program));
    let functions = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .collect::<Vec<_>>();
    let named = |prefix: &str| {
        *functions
            .iter()
            .find(|f| {
                source
                    .sources
                    .slice(f.origin.span())
                    .unwrap()
                    .starts_with(prefix)
            })
            .unwrap()
    };
    let run = named("fun run(");
    let caller = named("fun entry(");
    assert_eq!(
        run.blocks.len(),
        3,
        "entry and two terminal paths, no common join"
    );
    let parameters = &run.blocks[0].parameters;
    assert_eq!(
        parameters.len(),
        2,
        "Boolean loan and owned String retain their actual entry ABI"
    );
    assert!(matches!(parameters[0], EntityId::Loan(_)));
    assert!(matches!(parameters[1], EntityId::Value(_)));
    let branches = run
        .blocks
        .iter()
        .filter_map(|b| match &b.terminator.as_ref()?.kind {
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => Some((when_true, when_false)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(body_edge, false_edge)] = branches.as_slice() else {
        panic!("one real condition")
    };
    assert_eq!(body_edge.arguments, [parameters[1]]);
    assert_eq!(false_edge.arguments, [parameters[1]]);
    let body = run.block(body_edge.target).unwrap();
    let finished = run.block(false_edge.target).unwrap();
    assert_ne!(body.id, finished.id);
    let exchanges = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::RootReplace { .. }))
        .collect::<Vec<_>>();
    let [exchange] = exchanges.as_slice() else {
        panic!("one actual replacement")
    };
    let Operation::RootReplace {
        owner, replacement, ..
    } = exchange.operation
    else {
        unreachable!()
    };
    assert_eq!(exchange.block, body.id);
    assert_eq!(body.parameters, [EntityId::Value(owner)]);
    let literals = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
        .collect::<Vec<_>>();
    let [literal] = literals.as_slice() else {
        panic!("only Q is constructed in run; P is the actual parameter")
    };
    assert!(
        matches!(&literal.operation, Operation::StringLiteral { bytes, .. } if bytes == b"next")
    );
    assert_eq!(literal.results, [EntityId::Value(replacement)]);
    let TerminatorKind::Return {
        values: body_values,
    } = &body.terminator.as_ref().unwrap().kind
    else {
        panic!("body Return")
    };
    let TerminatorKind::Return {
        values: false_values,
    } = &finished.terminator.as_ref().unwrap().kind
    else {
        panic!("false Return")
    };
    assert_eq!(body_values.len(), 1);
    assert_eq!(false_values.len(), 1);
    assert_eq!(exchange.results[0], EntityId::Value(body_values[0]));
    assert_eq!(finished.parameters, [EntityId::Value(false_values[0])]);
    let drops = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("only detached P is dropped by run")
    };
    let Operation::Drop { owner } = drop.operation else {
        unreachable!()
    };
    assert_eq!(EntityId::Value(owner), exchange.results[1]);
    assert_eq!(drop.block, body.id);
    let position = |id| body.instructions.iter().position(|i| *i == id).unwrap();
    assert!(position(exchange.id) < position(drop.id));
    let span = run.origin.span();
    let facts = source
        .ownership()
        .drops()
        .iter()
        .filter(|d| {
            let origin = d.value_origin();
            origin.source_id() == span.source_id()
                && origin.start() >= span.start()
                && origin.end() <= span.end()
        })
        .collect::<Vec<_>>();
    let [fact] = facts.as_slice() else {
        panic!("one original planner cleanup in run")
    };
    assert!(matches!(fact.point(), UnitDropPoint::AfterStatement(_)));
    assert_eq!(
        source.sources.slice(fact.value_origin()).unwrap(),
        "previous"
    );
    assert_eq!(drop.origin.span(), fact.value_origin());
    let calls = caller
        .instructions
        .iter()
        .filter(
            |i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == run.id()),
        )
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    let caller_drops = caller
        .instructions
        .iter()
        .filter_map(|i| match i.operation {
            Operation::Drop { owner } => Some(EntityId::Value(owner)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(caller_drops.len(), 2);
    for call in calls {
        assert_eq!(call.results.len(), 1);
        assert_eq!(
            caller_drops
                .iter()
                .filter(|e| **e == call.results[0])
                .count(),
            1
        );
    }
}

#[test]
fn parameter_open_loop_requires_actual_entry_owner_and_return_receipts() {
    let source = UnitSource::analyze("parameter-open-loop-receipts.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [body, condition_false] = receipts.as_slice() else {
        panic!("two actual Return endpoints")
    };
    assert_ne!(body.0, condition_false.0);
    assert!(
        body.2.is_some(),
        "Parameter P must enter the body Q return authority chain"
    );
    assert!(
        condition_false.2.is_some(),
        "Parameter P must independently authorize false Return"
    );
    assert_eq!(body.1, body.2);
    assert_eq!(condition_false.1, condition_false.2);
}
