//! Ordinary nullable Rc source contracts; only real strict Unit products are inspected.

use super::{OwnedProducts, UnitSource};
use crate::{
    native_tests::emit_link_and_run,
    ssa::{
        model::{
            Edge, EntityId, EntityType, Function, Instruction, LoanKind, Operation, PlaceAccess,
            SsaTypeKind, TerminatorKind, ValueId,
        },
        verify::verify_program,
    },
};
use lang_frontend::{
    name_resolution::SymbolKind,
    ownership_checking::RcOwnershipEffectKind,
    parser::Expression,
    source::Span,
    type_checking::{RcOperationKind, UnitExpressionId},
};

const NULLABLE_SOURCE: &str = r#"fun inspect(own owner: Rc<Int>?): Rc<Int>? {
    if (owner != null) {
        val retained = owner.share()
        val copied = owner.value
        val retainedCopy = retained.value
    }
    return owner
}"#;

const PLAIN_SOURCE: &str = r#"fun inspect(own owner: Rc<Int>): Rc<Int> {
    val retained = owner.share()
    val copied = owner.value
    val retainedCopy = retained.value
    return owner
}"#;

fn expression(source: &UnitSource, text: &str) -> (UnitExpressionId, Span) {
    let [unit] = source.names.names().index().source_units() else {
        panic!("one real memory document")
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

fn instruction<'a>(
    source: &UnitSource,
    function: &'a Function,
    text: &str,
    accepts: impl Fn(&Operation) -> bool,
) -> &'a Instruction {
    let (_, span) = expression(source, text);
    let found = function
        .instructions
        .iter()
        .filter(|instruction| instruction.origin.span() == span && accepts(&instruction.operation))
        .collect::<Vec<_>>();
    let [instruction] = found.as_slice() else {
        panic!("one actual SSA operation at {text:?}")
    };
    instruction
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
        panic!("the same nullable owner crosses each edge exactly once")
    };
    value(function.block(edge.target).unwrap().parameters[*column])
}

fn assert_operations(source: &UnitSource, function: &Function, owner: EntityId) -> ValueId {
    let retain = instruction(source, function, "owner.share()", |operation| {
        matches!(operation, Operation::SharedRetain { .. })
    });
    assert_eq!(retain.operation, Operation::SharedRetain { owner });
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::SharedRetain { .. })
            })
            .count(),
        1,
        "only the explicit share creates a second owner"
    );
    let [retained] = retain.results.as_slice() else {
        panic!("share creates exactly one independent owner")
    };
    for (text, expected) in [("owner.value", owner), ("retained.value", *retained)] {
        let payload = instruction(source, function, text, |operation| {
            matches!(operation, Operation::SharedPayloadPlace { .. })
        });
        assert_eq!(
            payload.operation,
            Operation::SharedPayloadPlace { owner: expected }
        );
        let [EntityId::Place(place)] = payload.results.as_slice() else {
            panic!("payload is a read-only place, not another owner")
        };
        let read = instruction(source, function, text, |operation| {
            matches!(operation, Operation::Read { .. })
        });
        assert_eq!(
            read.operation,
            Operation::Read {
                source: PlaceAccess::Place(*place)
            }
        );
        assert_eq!(read.block, payload.block);
    }
    let drops = function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::Drop { owner } => Some(owner),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        drops,
        [value(*retained)],
        "only the retained owner is released here"
    );
    value(*retained)
}

#[test]
fn nullable_rc_if_uses_one_shared_proof_and_returns_the_original_owner() {
    let source = UnitSource::analyze("nullable-rc-proof.ko", NULLABLE_SOURCE)
        .expect("strict final admission and real ownership succeed");
    source.assert_frontend_clean();
    let OwnedProducts::Basic(typed, owned) = &source.products else {
        panic!("this ordinary source has basic capability")
    };
    let typed = typed.types();
    let condition = typed
        .null_comparison(expression(&source, "owner != null").0)
        .expect("real typed null comparison");
    assert!(condition.non_null_when_true());
    for (text, kind, effect_kind) in [
        (
            "owner.share()",
            RcOperationKind::Share,
            RcOwnershipEffectKind::Retain,
        ),
        (
            "owner.value",
            RcOperationKind::Value,
            RcOwnershipEffectKind::BorrowPayload,
        ),
    ] {
        let operation = typed.rc_operation(expression(&source, text).0).unwrap();
        assert_eq!(operation.kind(), kind);
        let proof = typed.non_null_use(operation.receiver()).unwrap();
        assert_eq!(proof.symbol(), condition.symbol());
        assert_eq!(proof.declared_type(), condition.nullable_type());
        let effects = owned
            .ownership()
            .rc_effects()
            .iter()
            .filter(|effect| effect.expression() == operation.expression())
            .collect::<Vec<_>>();
        let [effect] = effects.as_slice() else {
            panic!("one real Rc ownership effect per source operation")
        };
        assert_eq!(effect.receiver(), operation.receiver());
        assert_eq!(effect.kind(), effect_kind);
        assert_eq!(effect.payload_type(), operation.payload_type());
    }
    let (program, entry) = source
        .lower_entry(source.declaration("inspect", SymbolKind::Function))
        .expect("nullable Rc source must lower to verified SSA, not InvalidSsa");
    verify_program(&program).expect("all ownership and proof edges verify");
    let module = program.module(entry.module()).unwrap();
    let function = module.function(entry).unwrap();
    let branches = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::NullableBranch {
                owner,
                when_null,
                when_non_null,
                view,
            } => Some((block.id, *owner, when_null, when_non_null, *view)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(header, owner, null, present, view)] = branches.as_slice() else {
        panic!("one nullable branch creates the actual non-null proof")
    };
    let controls = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            matches!(node.payload(), Expression::If { .. }).then_some(node.span())
        })
        .collect::<Vec<_>>();
    let [control] = controls.as_slice() else {
        panic!("one full original if expression")
    };
    let condition = source
        .parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(_, node)| {
            if let Expression::If { condition, .. } = node.payload() {
                Some(
                    source
                        .parsed
                        .ast()
                        .expressions()
                        .get(*condition)
                        .unwrap()
                        .span(),
                )
            } else {
                None
            }
        });
    let actual_span = function
        .block(*header)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .origin
        .span();
    assert!(actual_span == *control || Some(actual_span) == condition);
    let entry_block = function.block(function.entry_block().unwrap()).unwrap();
    let [input_owner] = entry_block.parameters.as_slice() else {
        panic!("the original function has exactly one owner parameter")
    };
    assert_eq!(EntityId::Value(*owner), *input_owner);
    let owner_type = function
        .entity(EntityId::Value(*owner))
        .unwrap()
        .ty
        .semantic_type();
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(owner_type) else {
        panic!("source owner keeps its nullable storage identity")
    };
    assert!(matches!(
        module.type_kind(*inner),
        Some(SsaTypeKind::SharedOwner { .. })
    ));
    assert_eq!(
        function.entity(EntityId::Loan(*view)).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: *inner
        }
    );
    assert_eq!(
        function.block(present.target).unwrap().parameters.last(),
        Some(&EntityId::Loan(*view))
    );
    let predecessors = function
        .blocks
        .iter()
        .filter(|block| {
            block
                .terminator
                .as_ref()
                .unwrap()
                .kind
                .targets()
                .contains(&present.target)
        })
        .map(|block| block.id)
        .collect::<Vec<_>>();
    assert_eq!(
        predecessors,
        [*header],
        "no ordinary edge can mint this proof"
    );
    let retained = assert_operations(&source, function, EntityId::Loan(*view));
    assert_eq!(
        function.entity(EntityId::Value(retained)).unwrap().ty,
        EntityType::Value(*inner)
    );
    let present_block = function.block(present.target).unwrap();
    let ends = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == *view)
        })
        .collect::<Vec<_>>();
    let [end] = ends.as_slice() else {
        panic!("the proof ends exactly once")
    };
    assert_eq!(end.block, present.target);
    let payload = instruction(&source, function, "owner.value", |operation| {
        matches!(operation, Operation::SharedPayloadPlace { .. })
    });
    assert_eq!(payload.block, present.target);
    let position = |id| {
        present_block
            .instructions
            .iter()
            .position(|actual| *actual == id)
            .unwrap()
    };
    assert!(position(payload.id) < position(end.id));
    let null_owner = forwarded(function, null, *owner);
    let present_owner = forwarded(function, present, *owner);
    let TerminatorKind::Branch(null_join) = &function
        .block(null.target)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
    else {
        panic!("null branch joins normally")
    };
    let TerminatorKind::Branch(present_join) = &present_block.terminator.as_ref().unwrap().kind
    else {
        panic!("non-null branch retires its proof and joins normally")
    };
    assert_eq!(null_join.target, present_join.target);
    let joined = forwarded(function, null_join, null_owner);
    assert_eq!(forwarded(function, present_join, present_owner), joined);
    let returns = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Return { values } => Some(values.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        returns,
        [&[joined][..]],
        "the original owner remains live after the join"
    );
    assert!(
        !function
            .instructions
            .iter()
            .any(|instruction| { matches!(instruction.operation, Operation::NullableTake { .. }) })
    );
}

#[test]
fn non_nullable_rc_keeps_value_operands_without_a_nullable_proof() {
    let source = UnitSource::analyze("plain-rc-neighbor.ko", PLAIN_SOURCE)
        .expect("ordinary non-null Rc source passes strict admission");
    source.assert_frontend_clean();
    let (program, entry) = source
        .lower_entry(source.declaration("inspect", SymbolKind::Function))
        .expect("existing non-null Rc lowering remains accepted");
    verify_program(&program).expect("ordinary Rc ownership verifies");
    let function = program
        .module(entry.module())
        .unwrap()
        .function(entry)
        .unwrap();
    let entry_block = function.block(function.entry_block().unwrap()).unwrap();
    let [owner] = entry_block.parameters.as_slice() else {
        panic!("the ordinary function has exactly one owner parameter")
    };
    assert_operations(&source, function, *owner);
    assert!(!function.blocks.iter().any(|block| matches!(
        block.terminator.as_ref().unwrap().kind,
        TerminatorKind::NullableBranch { .. }
    )));
    assert!(
        !function
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
    );
    let returns = function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Return { values } => Some(values.as_slice()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(returns, [&[value(*owner)][..]]);
}

#[test]
fn nullable_rc_null_and_present_paths_run_and_keep_the_returned_owner_live() {
    let run = emit_link_and_run(
        "nullable-rc-normal.ko",
        r#"fun inspect(own owner: Rc<Int>?): Rc<Int>? {
    var copied = 0
    var retainedCopy = 0
    if (owner != null) {
        val retained = owner.share()
        copied = owner.value
        retainedCopy = retained.value
    }
    if (copied == 41) { println("owner") }
    if (retainedCopy == 41) { println("retained") }
    return owner
}
fun entry(): Unit {
    val present: Rc<Int>? = Rc(41)
    val again = inspect(present)
    val finalPresent = inspect(again)
    println("present-done")
    val absent: Rc<Int>? = null
    val stillAbsent = inspect(absent)
    val finalAbsent = inspect(stillAbsent)
    println("null-done")
}"#,
        "entry",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"owner\nretained\nowner\nretained\npresent-done\nnull-done\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
}
