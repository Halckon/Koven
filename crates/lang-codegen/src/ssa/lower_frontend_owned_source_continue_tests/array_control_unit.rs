//! Same source as the three Single rejection rows, through actual Unit facts and SSA.
use super::{short_circuit_unit::with_verified_unit, structure};
use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{
        BinaryOperator, BlockId, EntityId, Function, Instruction, Operation, ScalarConstant,
        TerminatorKind, ValueId,
    },
};
use lang_frontend::{
    ownership_checking::{UnitIterationCleanupAction, UnitIterationExitKind},
    source::SourceMap,
};

const INDEX_WITHOUT_LOOP: &str = "fun main(): Unit { var keys = arrayOf(1, 2, 3); keys[0] = 9 }";
const TWO_CONTINUE: &str = "fun main(): Unit { val keys = arrayOf(1, 2, 3); for (key in keys) { if (key == 1) { continue }; if (key == 2) { continue } }; keys.size }";
const BREAK: &str = "fun main(): Unit { val keys = arrayOf(1, 2, 3); for (key in keys) { if (key == 2) { break } }; keys.size }";

#[test]
fn original_index_assignment_replaces_the_real_array_without_a_provider() {
    let unit = UnitSource::analyze("lowering.ko", INDEX_WITHOUT_LOOP).unwrap();
    assert!(unit.ownership().iterations().is_empty());
    with_verified_unit(
        INDEX_WITHOUT_LOOP,
        "lowering.ko",
        |sources, program, entry| {
            let function = program
                .module(entry.module())
                .unwrap()
                .function(entry)
                .unwrap();
            let (owner, _) = original_array(function, sources);
            let writes: Vec<_> = function
                .instructions
                .iter()
                .filter_map(|i| {
                    let Operation::ContainerReplace {
                        owner: written,
                        index,
                        value,
                    } = i.operation
                    else {
                        return None;
                    };
                    Some((i, written, index, value))
                })
                .collect();
            let [(write, written, index, value)] = writes.as_slice() else {
                panic!("one actual element update");
            };
            assert!(structure::forwards(
                function,
                EntityId::Value(*written),
                EntityId::Value(owner),
                0
            ));
            assert_eq!(sources.slice(write.origin.span()), Ok("keys[0] = 9"));
            assert_integer(function, *index, 0);
            assert_integer(function, *value, 9);
            assert!(
                !function
                    .instructions
                    .iter()
                    .any(|i| matches!(i.operation, Operation::ContainerLength { .. }))
            );
            let drop = original_drop(function, owner);
            assert_eq!(write.block, drop.block);
            assert!(write.id.index() < drop.id.index());
            crate::llvm::render_verified_program(program).unwrap();
        },
    );
}

#[test]
fn original_two_continues_preserve_both_iteration_edges_and_the_array_owner() {
    assert_iteration_exits(TWO_CONTINUE, 2, false);
    with_verified_unit(TWO_CONTINUE, "lowering.ko", |sources, program, entry| {
        let function = program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap();
        let (_, seed) = original_array(function, sources);
        // Existing strong source/element/cursor/cleanup contract, applied to both
        // explicit continues plus the original normal backedge.
        structure::check_with_backedges(program, true, seed.origin.span(), 3);
        let advances = function
            .instructions
            .iter()
            .filter(|i| {
                sources.slice(i.origin.span()) == Ok("continue")
                    && matches!(
                        i.operation,
                        Operation::Binary {
                            operator: BinaryOperator::Add,
                            ..
                        }
                    )
            })
            .collect::<Vec<_>>();
        let [first, second] = advances.as_slice() else {
            panic!("both original Continue sites advance the real loop cursor");
        };
        assert_ne!(first.origin.span(), second.origin.span());
        assert_ne!(first.block, second.block);
        crate::llvm::render_verified_program(program).unwrap();
    });
}

#[test]
fn original_break_keeps_the_array_until_its_post_loop_size_read() {
    assert_iteration_exits(BREAK, 0, true);
    with_verified_unit(BREAK, "lowering.ko", |sources, program, entry| {
        let function = program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap();
        let (owner, _) = original_array(function, sources);
        let drop = original_drop(function, owner);
        let reads = function
            .instructions
            .iter()
            .filter(|i| {
                sources.slice(i.origin.span()) == Ok("keys.size")
                    && matches!(i.operation, Operation::ContainerLength { .. })
            })
            .collect::<Vec<_>>();
        let [size] = reads.as_slice() else {
            panic!("one actual post-loop length read");
        };
        let Operation::ContainerLength {
            owner: EntityId::Loan(loan),
        } = size.operation
        else {
            panic!("post-loop query uses its real read loan");
        };
        let Operation::BorrowBegin { place, .. } =
            producer(function, EntityId::Loan(loan)).operation
        else {
            panic!("query loan begins after the loop");
        };
        let Operation::RootPlace { owner: queried } =
            producer(function, EntityId::Place(place)).operation
        else {
            panic!("query borrows the original owner");
        };
        assert!(structure::forwards(
            function,
            EntityId::Value(queried),
            EntityId::Value(owner),
            0
        ));
        assert_eq!(size.block, drop.block);
        assert!(size.id.index() < drop.id.index());
        let broken = function
            .blocks
            .iter()
            .filter_map(|block| {
                let term = block.terminator.as_ref().unwrap();
                (sources.slice(term.origin.span()) == Ok("break")).then_some(&term.kind)
            })
            .collect::<Vec<_>>();
        let [TerminatorKind::Branch(edge)] = broken.as_slice() else {
            panic!("one original source-qualified Break edge");
        };
        assert!(straight_path_reaches(function, edge.target, size.block));
        crate::llvm::render_verified_program(program).unwrap();
    });
}

fn assert_iteration_exits(text: &str, continue_count: usize, has_break: bool) {
    let unit = UnitSource::analyze("lowering.ko", text).unwrap();
    let [plan] = unit.ownership().iterations() else {
        panic!("one actual provider");
    };
    let mut transfers = Vec::new();
    let mut exhausting = 0;
    for exit in plan.exits() {
        let finishes = exit
            .actions()
            .iter()
            .filter(|action| {
                matches!(action, UnitIterationCleanupAction::FinishProvider(statement)
                if *statement == plan.descriptor().statement())
            })
            .count();
        match exit.kind() {
            UnitIterationExitKind::Exhaustion => {
                exhausting += 1;
                assert_eq!(finishes, 1);
            }
            UnitIterationExitKind::Continue(expression)
            | UnitIterationExitKind::Break(expression) => {
                let span = unit
                    .parsed()
                    .ast()
                    .expressions()
                    .get(expression.expression())
                    .unwrap()
                    .span();
                let breaking = matches!(exit.kind(), UnitIterationExitKind::Break(_));
                assert_eq!(
                    unit.sources().slice(span),
                    Ok(if breaking { "break" } else { "continue" })
                );
                assert_eq!(finishes, usize::from(breaking));
                transfers.push((span.start(), breaking));
            }
            _ => {}
        }
    }
    assert_eq!(exhausting, 1);
    transfers.sort_unstable();
    transfers.dedup();
    assert_eq!(
        transfers.iter().filter(|(_, breaking)| !breaking).count(),
        continue_count
    );
    assert_eq!(
        transfers.iter().filter(|(_, breaking)| *breaking).count(),
        usize::from(has_break)
    );
}

fn original_array<'a>(function: &'a Function, sources: &SourceMap) -> (ValueId, &'a Instruction) {
    let arrays = function
        .instructions
        .iter()
        .filter(|i| {
            matches!(i.operation, Operation::ContainerConstruct { .. })
                && sources.slice(i.origin.span()) == Ok("arrayOf(1, 2, 3)")
        })
        .collect::<Vec<_>>();
    let [array] = arrays.as_slice() else {
        panic!("one actual original array");
    };
    let [EntityId::Value(owner)] = array.results.as_slice() else {
        panic!("actual array owner");
    };
    (*owner, array)
}

fn original_drop(function: &Function, seed: ValueId) -> &Instruction {
    let drops = function
        .instructions
        .iter()
        .filter(|i| {
            matches!(i.operation, Operation::Drop { owner }
            if structure::forwards(function, EntityId::Value(owner), EntityId::Value(seed), 0))
        })
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("the array's only actual Drop");
    };
    drop
}

fn producer(function: &Function, value: EntityId) -> &Instruction {
    function
        .instructions
        .iter()
        .find(|i| i.results.contains(&value))
        .unwrap()
}

fn assert_integer(function: &Function, value: ValueId, expected: i128) {
    assert_eq!(
        producer(function, EntityId::Value(value)).operation,
        Operation::Constant(ScalarConstant::Integer(expected))
    );
}

fn straight_path_reaches(function: &Function, mut from: BlockId, to: BlockId) -> bool {
    for _ in 0..function.blocks.len() {
        if from == to {
            return true;
        }
        let Some(TerminatorKind::Branch(edge)) = function
            .block(from)
            .and_then(|block| block.terminator.as_ref())
            .map(|term| &term.kind)
        else {
            return false;
        };
        from = edge.target;
    }
    false
}
