//! Preserve the caller's Inout slots and transfer old values out of atomic exchanges.
use super::*;
use crate::ssa::{
    model::{EntityId, PlaceAccess},
    verify::{VerifyErrorKind, verify_program},
};

#[test]
fn inout_primitive_returns_old_owners_after_root_field_and_mixed_writeback() {
    for constants in [false, true] {
        run_native(
            r#"
            class Token(val label: String) { deinit() { println(this.label) } }
            class Holder(var item: Token)
            fun change(inout left: Token, inout right: Token, inout holder: Holder): Token {
                swap(&left, &right)
                swap(&holder.item, &left)
                val old = replace(&right, Token("new right"))
                println("changed")
                return old
            }
            fun reset(inout holder: Holder): Token { return replace(&holder.item, Token("new field")) }
            fun entry(): Unit {
                var left = Token("left")
                var right = Token("right")
                var holder = Holder(Token("field"))
                val old = change(&left, &right, &holder)
                val previous = reset(&holder)
                println("caller")
            }
            "#,
            constants,
            6,
            b"changed\ncaller\nright\nleft\nnew field\nnew right\nfield\n",
        );
    }
}

#[test]
fn inout_primitive_replacement_keeps_parent_after_terminating_rhs() {
    for constants in [false, true] {
        run_native(
            r#"
            class Token(val label: String) { deinit() { println(this.label) } }
            fun set(inout value: Token, stop: Boolean): Int {
                val old = replace(&value, if (stop) { return 7 } else { Token("new") })
                println("committed")
                return 8
            }
            fun entry(): Unit {
                var value = Token("old")
                if (set(&value, true) == 7) { println("returned") }
                if (set(&value, false) == 8) { println("caller") }
            }
            "#,
            constants,
            2,
            b"returned\ncommitted\nold\ncaller\nnew\n",
        );
    }
}

#[test]
fn inout_primitive_swap_invalidates_an_element_place_of_the_previous_owner() {
    let (mut program, _) = lower(
        r#"
        fun exchange(inout left: Array<Int>, inout right: Array<Int>): Int {
            val previous = left[0]
            swap(&left, &right)
            return previous
        }
        fun entry(): Unit {
            var left = arrayOf<Int>(10)
            var right = arrayOf<Int>(20)
            if (exchange(&left, &right) == 10 && left[0] == 20 && right[0] == 10) {
                println("changed")
            }
        }
        "#,
        false,
    )
    .unwrap();
    verify_program(&program).unwrap();
    let function = program.modules[0]
        .functions
        .iter_mut()
        .find(|function| function.name.contains("test.exchange."))
        .unwrap();
    let block = &mut function.blocks[0];
    let (read, place) = block
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| {
            match function.instructions[instruction.index()].operation {
                Operation::Read {
                    source: PlaceAccess::Place(place),
                } => Some((index, place)),
                _ => None,
            }
        })
        .unwrap();
    let swap = block
        .instructions
        .iter()
        .position(|instruction| {
            matches!(
                function.instructions[instruction.index()].operation,
                Operation::ProjectedSwap { .. }
            )
        })
        .unwrap();
    assert!(read < swap);
    let old_read = block.instructions.remove(read);
    block.instructions.insert(swap, old_read);
    assert!(
        verify_program(&program)
            .unwrap_err()
            .errors
            .iter()
            .any(|error| matches!(
                error.kind, VerifyErrorKind::PlaceUnavailable { place: stale } if stale == place
            ))
    );
}

#[test]
fn inout_primitive_verifier_rejects_ending_the_suspended_parent() {
    let (mut program, _) = lower(
        r#"
        fun exchange(inout left: Int, inout right: Int): Unit { swap(&left, &right) }
        fun entry(): Unit {
            var left = 10
            var right = 20
            exchange(&left, &right)
        }
        "#,
        false,
    )
    .unwrap();
    verify_program(&program).unwrap();
    let function = program.modules[0]
        .functions
        .iter_mut()
        .find(|function| function.name.contains("test.exchange."))
        .unwrap();
    let EntityId::Loan(parent) = function.blocks[0].parameters[0] else {
        panic!("first parameter is an Inout loan");
    };
    let swap = function
        .instructions
        .iter_mut()
        .find(|instruction| matches!(instruction.operation, Operation::ProjectedSwap { .. }))
        .unwrap();
    swap.operation = Operation::BorrowEnd { loan: parent };
    assert!(verify_program(&program).unwrap_err().errors.iter().any(|error| matches!(
        error.kind, VerifyErrorKind::LoanDependencyActive { parent: failed, .. } if failed == parent
    )));
}
