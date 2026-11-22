use super::{ORIGINAL_SHADOW_ARRAYS, short_circuit_unit::with_verified_unit};
use crate::ssa::model::{
    Block, Edge, EntityId, Function, Instruction, LoanKind, Operation, ScalarConstant,
    TerminatorKind,
};

#[test]
fn original_external_shadow_owners_retire_separately_in_unit_and_native() {
    with_verified_unit(
        ORIGINAL_SHADOW_ARRAYS,
        "lowering.ko",
        |sources, program, entry| {
            let function = program
                .module(entry.module())
                .unwrap()
                .function(entry)
                .unwrap();
            assert_eq!(
                sources.slice(function.origin.span()),
                Ok(ORIGINAL_SHADOW_ARRAYS)
            );
            let span = |inst: &Instruction| inst.origin.span();
            let text = |inst: &Instruction| sources.slice(span(inst)).unwrap();
            let start = function.block(function.entry_block().unwrap()).unwrap();
            let constructions: Vec<_> = operations(function, start)
                .into_iter()
                .filter(|i| matches!(i.operation, Operation::ContainerConstruct { .. }))
                .collect();
            let [outer, inner] = constructions.as_slice() else {
                panic!("both distinct source instances are constructed")
            };
            let outer_owner = construct(function, outer, &[9]);
            let inner_owner = construct(function, inner, &[1, 2]);
            assert_ne!(outer_owner, inner_owner);
            assert_eq!(text(outer), "arrayOf(9)");
            assert_eq!(text(inner), "arrayOf(1, 2)");

            let ops = operations(function, start);
            let at = |inst: &Instruction| ops.iter().position(|i| i.id == inst.id).unwrap();
            let [outer_place, outer_loan, outer_len, outer_end, outer_drop] =
                &ops[at(outer) + 1..at(inner) - 2]
            else {
                panic!("the outer owner is used, unborrowed and retired before inner allocation")
            };
            assert_length_then_end(outer_place, outer_loan, outer_len, outer_end, outer_owner);
            assert_eq!(text(outer_len), "keys.size");
            assert_eq!(outer_drop.operation, Operation::Drop { owner: outer_owner });

            let [loop_place, loop_borrow, ..] = &ops[at(inner) + 1..] else {
                panic!("the inner source is borrowed for iteration")
            };
            assert_eq!(
                loop_place.operation,
                Operation::RootPlace { owner: inner_owner }
            );
            let [EntityId::Place(place)] = loop_place.results.as_slice() else {
                panic!("inner place")
            };
            assert_eq!(
                loop_borrow.operation,
                Operation::BorrowBegin {
                    place: *place,
                    kind: LoanKind::Shared
                }
            );
            let [loop_loan @ EntityId::Loan(_)] = loop_borrow.results.as_slice() else {
                panic!("inner source loan")
            };
            let header_edge = branch(start);
            assert!(
                !header_edge
                    .arguments
                    .contains(&EntityId::Value(outer_owner))
            );
            let header_owner = carry(function, header_edge, EntityId::Value(inner_owner));
            let header_loan = carry(function, header_edge, *loop_loan);
            let header = function.block(header_edge.target).unwrap();
            let TerminatorKind::Conditional {
                when_true: body_edge,
                when_false: exit_edge,
                ..
            } = &header.terminator.as_ref().unwrap().kind
            else {
                panic!("real inner loop")
            };
            let body_owner = carry(function, body_edge, header_owner);
            let body_loan = carry(function, body_edge, header_loan);
            let body = function.block(body_edge.target).unwrap();
            let body_ops = operations(function, body);
            assert!(body_ops.iter().any(|i| matches!(i.operation, Operation::ContainerElementPlace { owner, .. } if owner == body_loan)));
            let TerminatorKind::Conditional {
                when_true: skip,
                when_false: next,
                ..
            } = &body.terminator.as_ref().unwrap().kind
            else {
                panic!("continue decision")
            };
            for edge in [skip, next] {
                let backedge = branch(function.block(edge.target).unwrap());
                assert_eq!(backedge.target, header.id);
                assert_eq!(
                    carry(function, backedge, carry(function, edge, body_owner)),
                    header_owner
                );
                assert_eq!(
                    carry(function, backedge, carry(function, edge, body_loan)),
                    header_loan
                );
            }
            let EntityId::Value(exiting_owner) = carry(function, exit_edge, header_owner) else {
                panic!("same inner source on loop exit")
            };
            let EntityId::Loan(exiting_loan) = carry(function, exit_edge, header_loan) else {
                panic!("same inner loan on loop exit")
            };
            let exit = function.block(exit_edge.target).unwrap();
            let [
                loop_end,
                post_place,
                post_loan,
                post_len,
                post_end,
                inner_drop,
            ]: [_; 6] = operations(function, exit)
                .try_into()
                .expect("only the inner source retires after its final read");
            assert_eq!(
                loop_end.operation,
                Operation::BorrowEnd { loan: exiting_loan }
            );
            assert_length_then_end(post_place, post_loan, post_len, post_end, exiting_owner);
            assert_eq!(text(post_len), "keys.size");
            assert!(span(outer_len).end() < span(post_len).start());
            assert_eq!(
                inner_drop.operation,
                Operation::Drop {
                    owner: exiting_owner
                }
            );
            assert_eq!(
                function
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::Drop { .. }))
                    .count(),
                2
            );
            let llvm = crate::llvm::render_verified_program_with_entry(program, entry).unwrap();
            crate::ssa::unit_field_replace_tests::link_and_run(
                &llvm
                    .replace("@malloc(", "@array_malloc(")
                    .replace("@free(", "@array_free("),
                COUNTER,
                b"",
            );
        },
    );
}

fn construct(
    function: &Function,
    inst: &Instruction,
    expected: &[i128],
) -> crate::ssa::model::ValueId {
    let Operation::ContainerConstruct { elements, .. } = &inst.operation else {
        panic!("real construction")
    };
    assert_eq!(elements.len(), expected.len());
    for (element, expected) in elements.iter().zip(expected) {
        let instruction = function
            .instructions
            .iter()
            .find(|i| i.results.as_slice() == [EntityId::Value(*element)])
            .unwrap();
        assert_eq!(
            instruction.operation,
            Operation::Constant(ScalarConstant::Integer(*expected))
        );
    }
    let [EntityId::Value(owner)] = inst.results.as_slice() else {
        panic!("actual resulting owner")
    };
    *owner
}

fn assert_length_then_end(
    place: &Instruction,
    begin: &Instruction,
    length: &Instruction,
    end: &Instruction,
    owner: crate::ssa::model::ValueId,
) {
    assert_eq!(place.operation, Operation::RootPlace { owner });
    let [EntityId::Place(place)] = place.results.as_slice() else {
        panic!("actual owner place")
    };
    assert_eq!(
        begin.operation,
        Operation::BorrowBegin {
            place: *place,
            kind: LoanKind::Shared
        }
    );
    let [EntityId::Loan(loan)] = begin.results.as_slice() else {
        panic!("actual loan")
    };
    assert_eq!(
        length.operation,
        Operation::ContainerLength {
            owner: EntityId::Loan(*loan)
        }
    );
    assert_eq!(end.operation, Operation::BorrowEnd { loan: *loan });
}

fn operations<'a>(function: &'a Function, block: &Block) -> Vec<&'a Instruction> {
    block
        .instructions
        .iter()
        .map(|id| function.instruction(*id).unwrap())
        .collect()
}

fn branch(block: &Block) -> &Edge {
    let TerminatorKind::Branch(edge) = &block.terminator.as_ref().unwrap().kind else {
        panic!("expected loop jump")
    };
    edge
}

fn carry(function: &Function, edge: &Edge, input: EntityId) -> EntityId {
    let target = function.block(edge.target).unwrap();
    let found: Vec<_> = edge
        .arguments
        .iter()
        .zip(&target.parameters)
        .filter_map(|(argument, parameter)| (*argument == input).then_some(*parameter))
        .collect();
    let [output] = found.as_slice() else {
        panic!("one actual transported identity")
    };
    *output
}

const COUNTER: &str = r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
static unsigned sequence;
static void *live;
void *array_malloc(size_t bytes) {
    assert(!live && (sequence == 0 || sequence == 2));
    assert(bytes == (sequence == 0 ? 1 : 2) * sizeof(int32_t));
    live = malloc(bytes);
    assert(live);
    ++sequence;
    return live;
}
void array_free(void *pointer) {
    assert(pointer && pointer == live && (sequence == 1 || sequence == 3));
    int32_t contents[2] = {0, 0};
    memcpy(contents, pointer, (sequence == 1 ? 1 : 2) * sizeof(int32_t));
    if (sequence == 1) assert(contents[0] == 9);
    else assert(contents[0] == 1 && contents[1] == 2);
    free(pointer);
    live = NULL;
    ++sequence;
}
__attribute__((destructor)) static void verify(void) {
    assert(sequence == 4 && !live);
}
"#;
