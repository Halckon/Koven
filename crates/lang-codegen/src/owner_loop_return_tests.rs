//! Original finite loop-owned Return probe; no injected route or product.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); return held }
    return held
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn loop_owned_return_original_source_probe() {
    let source = UnitSource::analyze("loop-owned-return.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    use crate::ssa::model::{EntityId, Operation, TerminatorKind};
    use lang_frontend::ownership_checking::UnitDropPoint;
    let (program, entry) = source.lower_entry(source.entry()).unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let functions = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .collect::<Vec<_>>();
    let run = *functions
        .iter()
        .find(|f| {
            source
                .sources
                .slice(f.origin.span())
                .unwrap()
                .starts_with("fun run(")
        })
        .unwrap();
    let caller = *functions
        .iter()
        .find(|f| {
            source
                .sources
                .slice(f.origin.span())
                .unwrap()
                .starts_with("fun entry(")
        })
        .unwrap();
    assert_eq!(
        run.blocks.len(),
        3,
        "condition and two terminal paths, no common owner join"
    );
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
        panic!("one real condition");
    };
    let body = run.block(body_edge.target).unwrap();
    let finished = run.block(false_edge.target).unwrap();
    assert_ne!(body.id, finished.id);
    let literals = |bytes: &[u8]| {
        run.instructions.iter().find(|i| matches!(&i.operation, Operation::StringLiteral { bytes: actual, .. } if actual == bytes)).unwrap()
    };
    let initial = literals(b"outer");
    let replacement = literals(b"next");
    assert_eq!(body_edge.arguments, initial.results);
    assert_eq!(false_edge.arguments, initial.results);
    let exchanges = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::RootReplace { .. }))
        .collect::<Vec<_>>();
    let [exchange] = exchanges.as_slice() else {
        panic!("one original root replacement");
    };
    assert_eq!(exchange.block, body.id);
    let Operation::RootReplace {
        owner,
        replacement: rhs,
        ..
    } = exchange.operation
    else {
        unreachable!()
    };
    assert_eq!(body.parameters, [EntityId::Value(owner)]);
    assert_eq!(replacement.results, [EntityId::Value(rhs)]);
    let TerminatorKind::Return {
        values: body_values,
    } = &body.terminator.as_ref().unwrap().kind
    else {
        panic!("body Return must not become Break/backedge");
    };
    let TerminatorKind::Return {
        values: false_values,
    } = &finished.terminator.as_ref().unwrap().kind
    else {
        panic!("false path Return");
    };
    assert_eq!(body_values.len(), 1);
    assert_eq!(false_values.len(), 1);
    assert_eq!(exchange.results[0], EntityId::Value(body_values[0]));
    assert_eq!(finished.parameters, [EntityId::Value(false_values[0])]);
    assert_ne!(
        body.terminator.as_ref().unwrap().origin.span(),
        finished.terminator.as_ref().unwrap().origin.span()
    );
    assert_eq!(
        run.blocks
            .iter()
            .filter(|b| matches!(
                b.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { .. }
            ))
            .count(),
        2
    );
    let drops = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("only detached P is dropped by run");
    };
    let Operation::Drop { owner } = drop.operation else {
        unreachable!()
    };
    assert_eq!(EntityId::Value(owner), exchange.results[1]);
    assert_eq!(drop.block, body.id);
    let position = |id| body.instructions.iter().position(|i| *i == id).unwrap();
    assert!(position(exchange.id) < position(drop.id));
    let run_span = run.origin.span();
    let facts = source
        .ownership()
        .drops()
        .iter()
        .filter(|d| {
            let span = d.value_origin();
            span.source_id() == run_span.source_id()
                && span.start() >= run_span.start()
                && span.end() <= run_span.end()
        })
        .collect::<Vec<_>>();
    let [fact] = facts.as_slice() else {
        panic!("one actual planner cleanup in run");
    };
    assert!(matches!(fact.point(), UnitDropPoint::AfterStatement(_)));
    assert_eq!(
        source.sources.slice(fact.value_origin()).unwrap(),
        "previous"
    );
    assert_eq!(fact.value_origin(), drop.origin.span());
    let received = caller
        .instructions
        .iter()
        .filter(
            |i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == run.id()),
        )
        .collect::<Vec<_>>();
    assert_eq!(received.len(), 2);
    let caller_drops = caller
        .instructions
        .iter()
        .filter_map(|i| match i.operation {
            Operation::Drop { owner } => Some(EntityId::Value(owner)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(caller_drops.len(), 2);
    for call in received {
        assert_eq!(call.results.len(), 1);
        assert_eq!(
            caller_drops
                .iter()
                .filter(|value| **value == call.results[0])
                .count(),
            1
        );
    }
}
