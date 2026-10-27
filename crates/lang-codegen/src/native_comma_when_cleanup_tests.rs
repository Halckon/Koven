//! Ordinary comma-when source contracts, using only real Unit facts and verified SSA.
//! Pure owner cleanup and a common resource's lexical lifetime are separate oracles.

use super::{OwnedProducts, UnitSource};
use crate::{
    native_tests::emit_link_and_run,
    ssa::{
        model::{
            BlockId, Edge, EntityId, Function, Instruction, Operation, Origin, TerminatorKind,
            ValueId,
        },
        verify::verify_program,
    },
};
use lang_frontend::{
    name_resolution::{SymbolKind, UnitSymbolId},
    ownership_checking::{CompilationUnitOwnership, UnitDropTarget, UnitValueDeliveryKind},
    parser::Expression,
    source::Span,
    type_checking::{Copyability, UnitExpressionId},
};

const OWNER_SOURCE: &str = r#"class Node {}
fun probe(own node: Node): Boolean = false
fun consumeCommon(own common: Node): Unit {}
fun choose(flag: Boolean, own node: Node, own common: Node): Int {
    val result = when { flag, probe(node) -> 7; else -> 8 }
    consumeCommon(common)
    return result
}"#;

fn expression(source: &UnitSource, text: &str) -> (UnitExpressionId, Span) {
    let [unit] = source.names.names().index().source_units() else {
        panic!("one real MemoryDocument")
    };
    assert_eq!(unit.source_id(), source.parsed.source_id());
    let found = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| source.sources.slice(node.span()) == Ok(text))
        .collect::<Vec<_>>();
    let [(id, node)] = found.as_slice() else {
        panic!("one actual expression {text:?}")
    };
    (UnitExpressionId::new(unit.id(), *id), node.span())
}

fn moved_root(
    source: &UnitSource,
    owned: &CompilationUnitOwnership,
    call: &str,
    argument: &str,
) -> (UnitSymbolId, Span) {
    let call = expression(source, call).0;
    let (argument, argument_span) = expression(source, argument);
    let deliveries = owned
        .value_deliveries()
        .iter()
        .filter(|delivery| delivery.call() == call && delivery.argument() == argument)
        .collect::<Vec<_>>();
    let [delivery] = deliveries.as_slice() else {
        panic!("one source-produced Value delivery")
    };
    assert_eq!(delivery.kind(), UnitValueDeliveryKind::Move);
    assert_eq!(delivery.span(), argument_span);
    let place = delivery.place().expect("real named owner source");
    assert!(place.is_root());
    let root = place.root();
    let local = source
        .names
        .names()
        .source_units()
        .iter()
        .find(|local| local.source_unit() == root.source_unit())
        .expect("the root belongs to the same actual source unit");
    let symbol = local
        .resolution()
        .symbols()
        .iter()
        .find(|symbol| symbol.id() == root.symbol())
        .expect("real resolved declaration of the moved owner");
    assert_eq!(symbol.span().source_id(), argument_span.source_id());
    (root, symbol.span())
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("actual owned value")
    };
    value
}

fn forwarded(function: &Function, edge: &Edge, owner: ValueId) -> ValueId {
    let columns = edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(column, entity)| (*entity == EntityId::Value(owner)).then_some(column))
        .collect::<Vec<_>>();
    let [column] = columns.as_slice() else {
        panic!("this exact owner crosses the actual edge once")
    };
    value(function.block(edge.target).unwrap().parameters[*column])
}

fn branch(function: &Function, block: BlockId) -> &Edge {
    let TerminatorKind::Branch(edge) = &function
        .block(block)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
    else {
        panic!("actual normal merge edge")
    };
    edge
}

fn call_at(function: &Function, span: Span) -> &Instruction {
    let calls = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(span)
                && matches!(instruction.operation, Operation::DirectCall { .. })
        })
        .collect::<Vec<_>>();
    let [call] = calls.as_slice() else {
        panic!("one actual source call")
    };
    call
}

fn assert_unused_callee_owner(function: &Function) {
    let entry = function.block(function.entry_block().unwrap()).unwrap();
    let [owner] = entry.parameters.as_slice() else {
        panic!("one real Value parameter")
    };
    let drops = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("the transferred, unused pure owner is dropped exactly once")
    };
    assert_eq!(drop.block, entry.id);
    assert_eq!(
        drop.operation,
        Operation::Drop {
            owner: value(*owner)
        }
    );
}

#[test]
fn comma_when_real_facts_and_ssa_preserve_common_owner_and_clean_first_match() {
    let source = UnitSource::analyze("comma-when-owners.ko", OWNER_SOURCE)
        .expect("ordinary source passes strict Unit admission and ownership");
    source.assert_frontend_clean();
    let OwnedProducts::Basic(typed, owned) = &source.products else {
        panic!("ordinary non-constant source has Basic capability")
    };
    for argument in ["node", "common"] {
        let ty = typed
            .types()
            .expression_type(expression(&source, argument).0)
            .expect("real typed owner expression");
        assert_eq!(typed.types().copyability(ty), Copyability::MoveOnly);
        assert_eq!(typed.types().is_resource_type(ty), Some(false));
    }
    let owned = owned.ownership();
    let (node, node_origin) = moved_root(&source, owned, "probe(node)", "node");
    let (common, _) = moved_root(&source, owned, "consumeCommon(common)", "common");
    assert_ne!(node, common);
    // The old Unit planner loses this obligation by evaluating every alternative first.
    // Do not name the new drop-point variant: this test must compile against old production.
    let node_drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == UnitDropTarget::Named(node))
        .collect::<Vec<_>>();
    let [node_drop] = node_drops.as_slice() else {
        panic!("one real cleanup obligation for the skipped consuming alternative: {node_drops:?}")
    };
    assert_eq!(node_drop.value_origin(), node_origin);
    assert!(
        owned
            .drops()
            .iter()
            .all(|fact| fact.target() != UnitDropTarget::Named(common)),
        "the common owner remains live until its explicit delivery after the when"
    );
    let controls = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Expression::When { entries, .. } => Some(entries),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [entries] = controls.as_slice() else {
        panic!("one actual when")
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].conditions.len(), 2);
    let match_span = entries[0].span;
    assert_eq!(
        source.sources.slice(match_span),
        Ok("flag, probe(node) -> 7")
    );

    let (program, entry) = source
        .lower_entry(source.declaration("choose", SymbolKind::Function))
        .expect("real Unit products lower successfully, including alternative-edge cleanup");
    verify_program(&program).expect("all real owner paths verify");
    let module = program.module(entry.module()).unwrap();
    let function = module.function(entry).unwrap();
    let entry_block = function.block(function.entry_block().unwrap()).unwrap();
    let [_, node, common] = entry_block.parameters.as_slice() else {
        panic!("original flag, node and common parameters in source order")
    };
    let (node, common) = (value(*node), value(*common));
    let TerminatorKind::Conditional {
        when_true: first_match,
        when_false: fallthrough,
        ..
    } = &entry_block.terminator.as_ref().unwrap().kind
    else {
        panic!("the first condition branches before evaluating probe")
    };
    // Each alternative's test branches at its own condition (SPEC-0356 refinement layer).
    assert_eq!(
        entry_block.terminator.as_ref().unwrap().origin,
        Origin::Source(expression(&source, "flag").1)
    );
    let matched_node = forwarded(function, first_match, node);
    let matched_common = forwarded(function, first_match, common);
    let unmatched_node = forwarded(function, fallthrough, node);
    let unmatched_common = forwarded(function, fallthrough, common);
    let drops = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("choose has exactly one drop, not a drop on the consuming path")
    };
    assert_eq!(drop.block, first_match.target);
    assert_eq!(
        drop.operation,
        Operation::Drop {
            owner: matched_node
        }
    );
    assert_eq!(drop.origin, Origin::Source(node_origin));

    let probe = call_at(function, expression(&source, "probe(node)").1);
    assert_eq!(probe.block, fallthrough.target);
    let Operation::DirectCall {
        callee,
        receiver,
        arguments,
    } = &probe.operation
    else {
        unreachable!()
    };
    assert_eq!(*receiver, None);
    assert_eq!(arguments, &[EntityId::Value(unmatched_node)]);
    assert_unused_callee_owner(module.function(*callee).unwrap());
    let TerminatorKind::Conditional {
        condition,
        when_true: second_match,
        when_false: otherwise,
    } = &function
        .block(probe.block)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
    else {
        panic!("only the actual probe result chooses the next edge")
    };
    assert_eq!(probe.results, [EntityId::Value(*condition)]);
    assert_eq!(
        function
            .block(probe.block)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .origin,
        Origin::Source(expression(&source, "probe(node)").1)
    );

    let first_to_body = branch(function, first_match.target);
    let second_to_body = branch(function, second_match.target);
    assert_eq!(first_to_body.target, second_to_body.target);
    assert!(
        !first_to_body
            .arguments
            .contains(&EntityId::Value(matched_node))
    );
    let body_common = forwarded(function, first_to_body, matched_common);
    assert_eq!(
        body_common,
        forwarded(
            function,
            second_to_body,
            forwarded(function, second_match, unmatched_common),
        )
    );
    let body_to_end = branch(function, first_to_body.target);
    let otherwise_to_end = branch(function, otherwise.target);
    assert_eq!(body_to_end.target, otherwise_to_end.target);
    let final_common = forwarded(function, body_to_end, body_common);
    assert_eq!(
        final_common,
        forwarded(
            function,
            otherwise_to_end,
            forwarded(function, otherwise, unmatched_common),
        )
    );
    let consume = call_at(function, expression(&source, "consumeCommon(common)").1);
    assert_eq!(consume.block, body_to_end.target);
    let Operation::DirectCall {
        callee,
        receiver,
        arguments,
    } = &consume.operation
    else {
        unreachable!()
    };
    assert_eq!(*receiver, None);
    assert_eq!(arguments, &[EntityId::Value(final_common)]);
    assert_unused_callee_owner(module.function(*callee).unwrap());
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
            .count(),
        2
    );
}

#[test]
fn comma_when_native_short_circuits_and_keeps_common_resource_until_return() {
    let text = r#"class Node {}
class Guard { deinit() { println("guard") } }
fun probe(own node: Node): Boolean { println("probe"); return false }
fun choose(flag: Boolean, own node: Node): Int {
    val guard = Guard()
    val result = when {
        flag, probe(node) -> { println("chosen"); 7 }
        else -> { println("fallback"); 8 }
    }
    println("after when")
    return result
}
fun main(): Unit {
    val first = choose(true, Node())
    if (first == 7) { println("first=7") } else { println("wrong first") }
    val second = choose(false, Node())
    if (second == 8) { println("second=8") } else { println("wrong second") }
}"#;
    // Node is pure memory; it deliberately has no deinit. Guard is not conditionally moved.
    let output = emit_link_and_run("comma-when-resource.ko", text, "main");
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"chosen\nafter when\nguard\nfirst=7\nprobe\nfallback\nafter when\nguard\nsecond=8\n"
    );
}
