//! Direct sequential element exchange and its ownership contracts.
use super::*;

#[test]
fn unit_element_swap_native_preserves_distinct_elements_and_resources() {
    for constants in [false, true] {
        run_native(
            r#"
            class Token(val name: String) { deinit() { println(this.name) } }
            fun entry(): Unit {
                val owners = arrayOf<Token>(Token("first"), Token("second"))
                swap(&owners[1], &owners[-((0))])
                println("swapped")
            }
            "#,
            constants,
            3,
            b"swapped\nfirst\nsecond\n",
        );
    }
}

#[test]
fn unit_element_swap_native_evaluates_indices_in_order_across_containers() {
    for constants in [false, true] {
        run_native(
            r#"
            fun leftIndex(): Int { println("first index"); return 0 }
            fun rightIndex(): Int { println("second index"); return 0 }
            fun entry(): Unit {
                val left = arrayOf<Int>(1)
                val right = mutableListOf<Int>(2)
                swap(&left[leftIndex()], &right[rightIndex()])
                if (left[0] == 2 && right[0] == 1) { println("swapped") }
            }
            "#,
            constants,
            2,
            b"first index\nsecond index\nswapped\n",
        );
    }
}

#[test]
fn unit_element_swap_native_stops_on_early_index_return_without_committing() {
    for constants in [false, true] {
        run_native(
            r#"
            class Token(val name: String) { deinit() { println(this.name) } }
            fun pass(): Int { println("first index"); return 0 }
            fun fail(): Int { println("unreachable"); return 1 }
            fun first(): Int {
                val owners = arrayOf<Token>(Token("first"), Token("second"))
                swap(&owners[return 7], &owners[fail()])
            }
            fun second(): Int {
                val owners = arrayOf<Token>(Token("first"), Token("second"))
                swap(&owners[pass()], &owners[return 8])
            }
            fun entry(): Unit {
                if (first() == 7) { println("returned first") }
                if (second() == 8) { println("returned second") }
            }
            "#,
            constants,
            6,
            b"second\nfirst\nreturned first\nfirst index\nsecond\nfirst\nreturned second\n",
        );
    }
}

#[test]
fn unit_element_swap_native_reads_disjoint_sibling_when_second_index_returns() {
    for constants in [false, true] {
        run_native(
            r#"
            fun read(): Int {
                val values = arrayOf<Int>(10, 20)
                swap(&values[0], &values[return values[1]])
            }
            fun entry(): Unit { if (read() == 20) { println("sibling") } }
            "#,
            constants,
            1,
            b"sibling\n",
        );
    }
}

#[test]
fn unit_element_swap_verifier_rejects_repeated_loan_and_same_index() {
    use crate::ssa::{model::EntityId, verify::verify_program};
    let source = r#"
        fun entry(): Unit {
            val values = arrayOf<Int>(1, 2)
            swap(&values[0], &values[1])
        }
        "#;
    let (mut repeated, entry_id) = lower(source, false).unwrap();
    verify_program(&repeated).unwrap();
    let function = &mut repeated.modules[0].functions[entry_id.index()];
    for instruction in &mut function.instructions {
        if let Operation::ProjectedSwap { loans, .. } = &mut instruction.operation {
            loans[1] = loans[0];
        }
    }
    assert!(
        verify_program(&repeated)
            .unwrap_err()
            .errors
            .iter()
            .any(|error| matches!(
                error.kind,
                crate::ssa::verify::VerifyErrorKind::OperationContract { reason }
                    if reason == "projected swap requires disjoint direct mutable places"
            ))
    );

    let (mut overlapped, entry_id) = lower(source, false).unwrap();
    let function = &mut overlapped.modules[0].functions[entry_id.index()];
    let mut first = None;
    for instruction in &mut function.instructions {
        if let Operation::ContainerElementPlace {
            owner: EntityId::Value(_),
            index,
        } = &mut instruction.operation
        {
            *index = *first.get_or_insert(*index);
        }
    }
    assert!(
        verify_program(&overlapped)
            .unwrap_err()
            .errors
            .iter()
            .any(|error| matches!(
                error.kind,
                crate::ssa::verify::VerifyErrorKind::BorrowConflict { .. }
                    | crate::ssa::verify::VerifyErrorKind::OwnerLoanConflict { .. }
            ))
    );
}

#[test]
fn unit_element_swap_native_keeps_zero_size_places_valid_through_index_branch() {
    for constants in [false, true] {
        let (program, entry) = lower(
            r#"
            fun unit(index: Int): Unit {}
            fun entry(): Unit {
                val values = Array<Unit>(2, unit)
                swap(&values[1], &values[-(0)])
                println("unit swapped")
            }
            "#,
            constants,
        )
        .unwrap();
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        assert!(
            !llvm.contains("swap.left"),
            "zero-sized swap must not load the sentinel"
        );
        link_and_run(&llvm, "", b"unit swapped\n");
    }
}

#[test]
fn unit_element_swap_mixed_copyable_writes_back_in_either_argument_order() {
    for constants in [false, true] {
        run_native(
            r#"
            fun entry(): Unit {
                var local = 10
                val original = local
                val elements = arrayOf<Int>(20)
                swap(&elements[0], &local)
                if (elements[0] == 10 && local == 20 && original == 10) { println("element first") }
                swap(&local, &elements[-(0)])
                if (elements[0] == 20 && local == 10 && original == 10) { println("root first") }
                swap(&elements[if (replace(&local, 30) == 10) { 0 } else { 1 }], &local)
                if (elements[0] == 30 && local == 20 && original == 10) { println("index first") }
            }
            "#,
            constants,
            1,
            b"element first\nroot first\nindex first\n",
        );
    }
}

#[test]
fn unit_element_swap_mixed_moveonly_preserves_drop_order_and_early_return() {
    for constants in [false, true] {
        run_native(
            r#"
            class Token(val name: String) { deinit() { println(this.name) } }
            fun complete(): Unit {
                var local = Token("local")
                val elements = arrayOf<Token>(Token("element"))
                swap(&local, &elements[0])
                println("exchanged")
            }
            fun early(stop: Boolean): Int {
                var local = Token("uncrossed local")
                val elements = arrayOf<Token>(Token("uncrossed element"))
                swap(&local, &elements[if (stop) { return 7 } else { 0 }])
                return 8
            }
            fun entry(): Unit {
                complete()
                if (early(true) == 7) { println("returned") }
            }
            "#,
            constants,
            6,
            b"exchanged\nlocal\nelement\nuncrossed element\nuncrossed local\nreturned\n",
        );
    }
}

#[test]
fn unit_element_swap_mixed_rejects_forged_root_identity() {
    use crate::ssa::{
        model::EntityId,
        verify::{VerifyErrorKind, verify_program},
    };
    let (mut program, entry) = lower(
        r#"
        fun entry(): Unit {
            var local = 10
            val elements = arrayOf<Int>(20)
            swap(&elements[0], &local)
        }
        "#,
        false,
    )
    .unwrap();
    verify_program(&program).unwrap();
    let function = &mut program.modules[0].functions[entry.index()];
    let unrelated = function
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::Constant(crate::ssa::model::ScalarConstant::Integer(20)) => {
                let [EntityId::Value(value)] = instruction.results.as_slice() else {
                    panic!("integer constant must produce a value");
                };
                Some(*value)
            }
            _ => None,
        })
        .unwrap();
    let swap = function
        .instructions
        .iter_mut()
        .find(|instruction| {
            matches!(
                instruction.operation,
                Operation::ProjectedSwap { root: Some(_), .. }
            )
        })
        .unwrap();
    let Operation::ProjectedSwap { root, .. } = &mut swap.operation else {
        unreachable!()
    };
    *root = Some(unrelated);
    assert!(
        verify_program(&program)
            .unwrap_err()
            .errors
            .iter()
            .any(|error| matches!(
                error.kind,
                VerifyErrorKind::OperationContract {
                    reason: "root exchange requires an exclusive loan of the exact owned root"
                }
            ))
    );
}

#[test]
fn unit_element_swap_inout_array_writes_back_and_cancels_the_prefix_on_return() {
    for constants in [false, true] {
        run_native(
            r#"
            fun leave(inout values: Array<Int>): Int {
                swap(&values[0], &values[return values[1]])
            }
            fun exchange(inout values: Array<Int>): Unit {
                swap(&values[1], &values[-(0)])
                if (values[0] == 20) { println("read in callee") }
            }
            fun entry(): Unit {
                var values = arrayOf<Int>(10, 20)
                if (leave(&values) == 20 && values[0] == 10 && values[1] == 20) {
                    println("unchanged")
                }
                exchange(&values)
                if (values[0] == 20 && values[1] == 10) { println("written back") }
            }
            "#,
            constants,
            1,
            b"unchanged\nread in callee\nwritten back\n",
        );
    }
}

#[test]
fn unit_element_swap_inout_moveonly_mutable_list_preserves_element_owners() {
    for constants in [false, true] {
        run_native(
            r#"
            class Token(val name: String) { deinit() { println(this.name) } }
            fun exchange(inout values: MutableList<Token>): Unit {
                var local = Token("local")
                swap(&local, &values[0])
                swap(&values[0], &values[1])
                println("in callee")
            }
            fun entry(): Unit {
                var values = mutableListOf<Token>(Token("first"), Token("second"))
                exchange(&values)
                println("in caller")
            }
            "#,
            constants,
            4,
            b"in callee\nfirst\nin caller\nlocal\nsecond\n",
        );
    }
}

#[test]
fn unit_element_swap_inout_rejects_ending_the_live_parent() {
    use crate::ssa::{
        model::EntityId,
        verify::{VerifyErrorKind, verify_program},
    };
    let (mut program, _) = lower(
        r#"
        fun exchange(inout values: Array<Int>): Unit { swap(&values[0], &values[1]) }
        fun entry(): Unit {
            var values = arrayOf<Int>(1, 2)
            exchange(&values)
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
    let [EntityId::Loan(parent)] = function.blocks[0].parameters.as_slice() else {
        panic!("exchange takes exactly one Inout loan");
    };
    let commit = function
        .instructions
        .iter_mut()
        .find(|instruction| matches!(instruction.operation, Operation::ProjectedSwap { .. }))
        .unwrap();
    commit.operation = Operation::BorrowEnd { loan: *parent };
    assert!(
        verify_program(&program)
            .unwrap_err()
            .errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::LoanDependencyActive { .. }))
    );
}

#[test]
fn unit_element_swap_inout_invalidates_a_place_after_header_writeback() {
    use crate::ssa::{
        model::PlaceAccess,
        verify::{VerifyErrorKind, verify_program},
    };
    let source = r#"
        fun resize(inout values: MutableList<Int>): Unit { values.add(30) }
        fun readThenAppend(inout values: MutableList<Int>): Int {
            val old = values[0]
            WRITEBACK
            return old
        }
        fun entry(): Unit {
            var values = mutableListOf<Int>(1, 2)
            if (readThenAppend(&values) == 1) { println("original read") }
        }
        "#;
    for writeback in [
        "values.add(30)",
        "resize(&values)",
        "values = mutableListOf<Int>(30)",
    ] {
        let (mut program, _) = lower(&source.replace("WRITEBACK", writeback), false).unwrap();
        verify_program(&program).unwrap();
        let function = program.modules[0]
            .functions
            .iter_mut()
            .find(|function| function.name.contains("test.readThenAppend."))
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
        let append = block
            .instructions
            .iter()
            .position(|instruction| {
                matches!(
                    function.instructions[instruction.index()].operation,
                    Operation::ContainerAppendInout { .. }
                        | Operation::DirectCall { .. }
                        | Operation::LoanReplace { .. }
                )
            })
            .unwrap();
        assert!(read < append);
        let read_instruction = block.instructions.remove(read);
        block.instructions.insert(append, read_instruction);
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
}
