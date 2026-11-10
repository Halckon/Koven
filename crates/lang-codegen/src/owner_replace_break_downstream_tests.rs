//! Ordinary original B source through existing lowering, not a new carrier consumer.
use super::{OwnedProducts, UnitSource};
use crate::ssa::{
    model::{Edge, EntityId, Function, Operation, TerminatorKind, ValueId},
    verify::verify_program,
};
use lang_frontend::name_resolution::SymbolKind;

const SOURCE: &str = r#"fun run(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; val retired = held }
fun entry(): Unit { run(false); run(true) }"#;

#[test]
fn bounded_replace_break_original_source_reaches_existing_ssa_and_llvm() {
    let source = UnitSource::analyze("bounded-owner-b.ko", SOURCE).expect("ordinary B source");
    source.assert_frontend_clean();
    let OwnedProducts::Basic(_, owned) = &source.products else {
        panic!("basic Unit products")
    };
    let boundaries = owned
        .ownership()
        .owner_observations()
        .iter()
        .filter_map(|o| o.replace_break_observation_v1())
        .collect::<Vec<_>>();
    assert_eq!(boundaries.len(), 1);
    assert!(boundaries[0].suffix_consumer_v1().is_some());
    let (program, entry) = source
        .lower_entry(source.declaration("entry", SymbolKind::Function))
        .expect("existing Unit lowering reaches original B source");
    verify_program(&program).expect("original source SSA verifies");
    let functions = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| {
            source
                .sources
                .slice(function.origin.span())
                .is_ok_and(|text| text.starts_with("fun run("))
        })
        .collect::<Vec<_>>();
    let [function] = functions.as_slice() else {
        panic!("exact original callable")
    };
    assert_selected_owner(&source, function);
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("original source LLVM verifies");
    assert!(llvm.contains("define i32 @main("));
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("owned SSA value")
    };
    value
}

fn forward(function: &Function, edge: &Edge, owner: ValueId) -> ValueId {
    assert_eq!(edge.arguments, [EntityId::Value(owner)]);
    let parameters = &function.block(edge.target).unwrap().parameters;
    assert_eq!(parameters.len(), 1);
    value(parameters[0])
}

fn branch(function: &Function, block: crate::ssa::model::BlockId) -> &Edge {
    let TerminatorKind::Branch(edge) = &function
        .block(block)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
    else {
        panic!("unconditional exit edge")
    };
    edge
}

fn assert_selected_owner(source: &UnitSource, function: &Function) {
    let literal = |bytes: &[u8]| {
        let found = function.instructions.iter().filter(|instruction|
            matches!(&instruction.operation, Operation::StringLiteral { bytes: actual, .. } if actual == bytes)
        ).collect::<Vec<_>>();
        assert_eq!(found.len(), 1);
        found[0]
    };
    let initial = literal(b"outer");
    let replacement = literal(b"next");
    assert_eq!(source.sources.slice(initial.origin.span()), Ok("\"outer\""));
    assert_eq!(
        source.sources.slice(replacement.origin.span()),
        Ok("\"next\"")
    );
    let conditionals = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.as_ref()?.kind {
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => Some((when_true, when_false)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(when_true, when_false)] = conditionals.as_slice() else {
        panic!("one actual Boolean condition")
    };
    let body_owner = forward(function, when_true, value(initial.results[0]));
    let false_owner = forward(function, when_false, value(initial.results[0]));
    let exchanges = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::RootReplace { .. }))
        .collect::<Vec<_>>();
    let [exchange] = exchanges.as_slice() else {
        panic!("one actual Replace")
    };
    assert_eq!(
        source.sources.slice(exchange.origin.span()),
        Ok("replace(&held, \"next\")")
    );
    let Operation::RootReplace {
        owner,
        replacement: rhs,
        ..
    } = exchange.operation
    else {
        unreachable!()
    };
    assert_eq!(owner, body_owner);
    assert_eq!(rhs, value(replacement.results[0]));
    assert_eq!(exchange.block, when_true.target);
    assert_eq!(exchange.results.len(), 2);
    let break_edge = branch(function, exchange.block);
    let false_edge = branch(function, when_false.target);
    assert_eq!(break_edge.target, false_edge.target);
    let selected = forward(function, break_edge, value(exchange.results[0]));
    assert_eq!(forward(function, false_edge, false_owner), selected);
    assert_ne!(break_edge.arguments, false_edge.arguments);
    let drops = function
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::Drop { owner } = instruction.operation {
                Some((instruction, owner))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        2,
        "one detached old value and one selected exit value"
    );
    let old = drops
        .iter()
        .find(|(_, owner)| *owner == value(exchange.results[1]))
        .unwrap()
        .0;
    let final_drop = drops
        .iter()
        .find(|(_, owner)| *owner == selected)
        .unwrap()
        .0;
    assert_eq!(old.block, exchange.block);
    assert_eq!(final_drop.block, break_edge.target);
    assert_eq!(source.sources.slice(old.origin.span()), Ok("previous"));
    assert_eq!(
        source.sources.slice(final_drop.origin.span()),
        Ok("retired")
    );
    let body = function.block(exchange.block).unwrap();
    assert!(
        body.instructions
            .iter()
            .position(|id| *id == exchange.id)
            .unwrap()
            < body
                .instructions
                .iter()
                .position(|id| *id == old.id)
                .unwrap()
    );
    assert!(matches!(
        function
            .block(break_edge.target)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind,
        TerminatorKind::Return { .. }
    ));
}
