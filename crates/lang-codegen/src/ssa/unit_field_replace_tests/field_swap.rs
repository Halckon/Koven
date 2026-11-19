//! Native writes and verifier contracts for direct heap-field swaps.
use super::*;
use crate::ssa::verify::{VerifyErrorKind, verify_program};

#[test]
fn unit_field_swap_copies_back_all_supported_places_and_preserves_snapshots() {
    for constants in [false, true] {
        run_native(
            r#"
            class Holder(var left: Int, var right: Int)
            fun entry(): Unit {
                val holder = Holder(10, 20)
                var local = 30
                val original = local
                val values = arrayOf<Int>(40)
                swap(&holder.left, &holder.right)
                swap(&local, &holder.left)
                swap(&holder.right, &values[0])
                if (holder.left == 30 && holder.right == 40 && values[0] == 10
                    && local == 20 && original == 30) { println("exchanged") }
            }
            "#,
            constants,
            2,
            b"exchanged\n",
        );
    }
}

#[test]
fn unit_field_swap_moves_owners_and_cancels_commit_on_early_return() {
    for constants in [false, true] {
        run_native(
            r#"
            class Token(val name: String) { deinit() { println(this.name) } }
            class Holder(var left: Token, var right: Token) { deinit() { println("holder") } }
            fun exchange(): Unit {
                val holder = Holder(Token("left"), Token("right"))
                var local = Token("local")
                val values = arrayOf<Token>(Token("element"))
                swap(&holder.left, &holder.right)
                swap(&holder.left, &local)
                swap(&values[0], &holder.right)
                println("exchanged")
            }
            fun leave(): Int {
                val holder = Holder(Token("kept left"), Token("kept right"))
                val values = arrayOf<Token>(Token("kept element"))
                swap(&holder.left, &values[return 7])
            }
            fun entry(): Unit {
                exchange()
                if (leave() == 7) { println("returned") }
            }
            "#,
            constants,
            11,
            b"exchanged\nleft\nright\nholder\nelement\nlocal\nkept element\nholder\nkept right\nkept left\nreturned\n",
        );
    }
}

#[test]
fn unit_field_swap_verifier_rejects_same_field() {
    let (mut program, id) = lower(
        r#"
        class Holder(var left: Int, var right: Int)
        fun entry(): Unit {
            val holder = Holder(10, 20)
            swap(&holder.left, &holder.right)
        }
        "#,
        false,
    )
    .unwrap();
    verify_program(&program).unwrap();
    let function = &mut program.modules[0].functions[id.index()];
    let [left, right]: [&mut usize; 2] = function
        .instructions
        .iter_mut()
        .filter_map(|instruction| match &mut instruction.operation {
            Operation::FieldPlace { field, .. } => Some(field),
            _ => None,
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("two direct field projections");
    assert_ne!(*left, *right);
    *right = *left;
    assert!(
        verify_program(&program)
            .unwrap_err()
            .errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::BorrowConflict { .. }))
    );
}

#[test]
fn unit_field_swap_uses_the_current_owner_after_a_control_flow_join() {
    for constants in [false, true] {
        run_native(
            r#"
            class Holder(var left: Int, var right: Int)
            fun exchange(flag: Boolean): Unit {
                var holder = Holder(10, 20)
                if (flag) { holder = Holder(30, 40) }
                swap(&holder.left, &holder.right)
                if (holder.left == 40 && holder.right == 30) { println("replacement") }
                if (holder.left == 20 && holder.right == 10) { println("original") }
            }
            fun entry(): Unit { exchange(true); exchange(false) }
            "#,
            constants,
            3,
            b"replacement\noriginal\n",
        );
    }
}

#[test]
fn unit_field_swap_reads_a_disjoint_sibling_before_a_later_index_exits() {
    for constants in [false, true] {
        run_native(
            r#"
            class Holder(var left: Int, var right: Int)
            fun leave(): Int {
                val holder = Holder(10, 20)
                val values = arrayOf<Int>(99)
                swap(&holder.left, &values[return holder.right])
            }
            fun entry(): Unit { if (leave() == 20) { println("read sibling") } }
            "#,
            constants,
            2,
            b"read sibling\n",
        );
    }
}

#[test]
fn unit_field_swap_preserves_sibling_assignment_and_its_destructor_before_return() {
    for constants in [false, true] {
        run_native(
            r#"
            class Token(val name: String) { deinit() { println(this.name) } }
            class Holder(var left: Token, var right: Token)
            fun leave(flag: Boolean): Int {
                val holder = Holder(Token("left"), Token("old"))
                val values = arrayOf<Token>(Token("element"))
                swap(&holder.left, &values[if (flag) {
                    holder.right = Token("new")
                    println("assigned")
                    return 7
                } else { 0 }])
                return 8
            }
            fun entry(): Unit { if (leave(true) == 7) { println("returned") } }
            "#,
            constants,
            6,
            b"old\nassigned\nelement\nnew\nleft\nreturned\n",
        );
    }
}

#[test]
fn unit_field_swap_preserves_the_joined_owner_across_the_later_index_branch() {
    for constants in [false, true] {
        run_native(
            r#"
            class Holder(var left: Int, var right: Int)
            fun probe(first: Boolean, exit: Boolean): Int {
                val holder = if (first) { Holder(10, 20) } else { Holder(30, 40) }
                val values = arrayOf<Int>(99)
                swap(&holder.left, &values[if (exit) { return holder.right } else { 0 }])
                return holder.left
            }
            fun entry(): Unit {
                if (probe(true, true) == 20 && probe(false, true) == 40
                    && probe(false, false) == 99) { println("joined sibling") }
            }
            "#,
            constants,
            6,
            b"joined sibling\n",
        );
    }
}

#[test]
fn unit_field_swap_inout_parameter_and_receiver_preserve_resource_owners() {
    for constants in [false, true] {
        run_native(
            r#"
            class Token(val name: String) { deinit() { println(this.name) } }
            class Holder(var left: Token, var right: Token) {
                inout fun reverse(): Unit {
                    swap(&this.left, &right)
                    println("receiver")
                }
            }
            fun reverse(inout holder: Holder): Unit {
                swap(&holder.left, &holder.right)
                println("parameter")
            }
            fun entry(): Unit {
                var holder = Holder(Token("left"), Token("right"))
                reverse(&holder)
                holder.reverse()
                println("done")
            }
            "#,
            constants,
            3,
            b"parameter\nreceiver\ndone\nright\nleft\n",
        );
    }
}

#[test]
fn unit_field_swap_inout_preserves_parent_across_a_conditional_index_exit() {
    for constants in [false, true] {
        run_native(
            r#"
            class Holder(var left: Int, var right: Int) {
                inout fun apply(stop: Boolean): Int {
                    val values = arrayOf<Int>(99)
                    swap(&left, &values[if (stop) { return this.right } else { 0 }])
                    return left
                }
            }
            fun leave(inout holder: Holder): Int {
                val values = arrayOf<Int>(50)
                swap(&holder.left, &values[return holder.right])
            }
            fun entry(): Unit {
                var holder = Holder(10, 20)
                if (leave(&holder) == 20 && holder.left == 10) { println("parameter unchanged") }
                if (holder.apply(true) == 20 && holder.left == 10) { println("receiver unchanged") }
                if (holder.apply(false) == 99 && holder.left == 99 && holder.right == 20) {
                    println("receiver wrote back")
                }
            }
            "#,
            constants,
            4,
            b"parameter unchanged\nreceiver unchanged\nreceiver wrote back\n",
        );
    }
}

#[test]
fn unit_field_swap_inout_verifier_rejects_same_field_and_ending_live_parent() {
    use crate::ssa::model::EntityId;
    let source = r#"
        class Holder(var left: Int, var right: Int)
        fun reverse(inout holder: Holder): Unit { swap(&holder.left, &holder.right) }
        fun entry(): Unit {
            var holder = Holder(10, 20)
            reverse(&holder)
        }
    "#;
    for ends_parent in [false, true] {
        let (mut program, _) = lower(source, false).unwrap();
        verify_program(&program).unwrap();
        let function = program.modules[0]
            .functions
            .iter_mut()
            .find(|function| function.name.contains("test.reverse."))
            .unwrap();
        let &[EntityId::Loan(parent)] = function.blocks[0].parameters.as_slice() else {
            panic!("one Inout parameter");
        };
        if ends_parent {
            let swap = function
                .instructions
                .iter_mut()
                .find(|instruction| {
                    matches!(instruction.operation, Operation::ProjectedSwap { .. })
                })
                .unwrap();
            swap.operation = Operation::BorrowEnd { loan: parent };
        } else {
            let fields: Vec<_> = function
                .instructions
                .iter_mut()
                .filter_map(|instruction| match &mut instruction.operation {
                    Operation::ExclusiveHeapFieldLoan { field, .. } => Some(field),
                    _ => None,
                })
                .collect();
            let [left, right]: [&mut usize; 2] = fields.try_into().expect("two direct child loans");
            assert_ne!(*left, *right);
            *right = *left;
        }
        assert!(verify_program(&program).unwrap_err().errors.iter().any(|error| matches!(
            error.kind, VerifyErrorKind::LoanDependencyActive { parent: failed, .. } if failed == parent
        )));
    }
}

#[test]
fn unit_field_swap_inout_verifier_rejects_projection_after_parent_alias_ended() {
    use crate::ssa::model::EntityId;
    let (mut program, _) = lower(
        r#"
        class Holder(var left: Int, var right: Int)
        fun reverse(inout holder: Holder, flag: Boolean): Unit {
            if (flag) { swap(&holder.left, &holder.right) }
        }
        fun entry(): Unit {
            var holder = Holder(10, 20)
            reverse(&holder, true)
        }
        "#,
        false,
    )
    .unwrap();
    verify_program(&program).unwrap();
    let function = program.modules[0]
        .functions
        .iter_mut()
        .find(|function| function.name.contains("test.reverse."))
        .unwrap();
    let EntityId::Loan(original) = function.blocks[0].parameters[0] else {
        panic!("first parameter is the original loan");
    };
    let projection = function
        .instructions
        .iter_mut()
        .find(|instruction| {
            matches!(
                instruction.operation,
                Operation::ExclusiveHeapFieldLoan { .. }
            )
        })
        .unwrap();
    let (id, block, origin) = (projection.id, projection.block, projection.origin.clone());
    let Operation::ExclusiveHeapFieldLoan { base, .. } = &mut projection.operation else {
        unreachable!()
    };
    let forwarded = *base;
    assert_ne!(forwarded, original);
    *base = original;

    let terminator = function.blocks[block.index()].terminator.take();
    let (end, _) = function
        .append_instruction(
            block,
            Operation::BorrowEnd { loan: forwarded },
            Vec::new(),
            origin,
        )
        .unwrap();
    let block = &mut function.blocks[block.index()];
    block.terminator = terminator;
    assert_eq!(block.instructions.pop(), Some(end));
    let before = block
        .instructions
        .iter()
        .position(|&instruction| instruction == id)
        .unwrap();
    block.instructions.insert(before, end);

    assert!(
        verify_program(&program)
            .unwrap_err()
            .errors
            .iter()
            .any(|error| matches!(
                error.kind, VerifyErrorKind::LoanInactive { loan } if loan == original
            ))
    );
}
