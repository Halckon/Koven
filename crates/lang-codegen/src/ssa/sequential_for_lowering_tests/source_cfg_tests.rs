//! SPEC-0182: local source loan identity follows body CFG edges and merges.
use super::cleanup_tests::{cleanup, loan, transport, value};
use super::{
    super::model::{EntityType, Program},
    EntityId, LoanKind, Operation, TerminatorKind, analyze, defining_instruction,
};

fn verified(source: &str) -> Program {
    let program = analyze(source);
    crate::llvm::render_verified_program(&program).expect("source CFG must lower to LLVM");
    program
}

#[test]
fn conditional_break_ends_rebound_source_before_temporary_drop() {
    for constructor in ["listOf", "arrayOf", "mutableListOf"] {
        let text = format!(
            "fun scan(): Unit {{ for (value in {constructor}(1)) {{ if (value == 1) {{ break }} }} }}"
        );
        let program = verified(&text);
        let function = &program.modules[0].functions[0];
        let element = function
            .instructions
            .iter()
            .find(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::ContainerElementPlace { .. }
                )
            })
            .unwrap();
        let Operation::ContainerElementPlace {
            owner: body_source, ..
        } = element.operation
        else {
            unreachable!();
        };
        let body = function.block(element.block).unwrap();
        let element_loan = function
            .instructions
            .iter()
            .find_map(|instruction| {
                matches!(instruction.operation,
                    Operation::BorrowBegin { place, kind: LoanKind::Shared }
                    if EntityId::Place(place) == element.results[0]
                )
                .then_some(instruction.results.first().copied())
                .flatten()
            })
            .unwrap();
        let EntityType::Loan { target, .. } = function.entity(body_source).unwrap().ty else {
            panic!("source loan type");
        };
        let body_owner = body
            .parameters
            .iter()
            .copied()
            .find(|entity| function.entity(*entity).unwrap().ty == EntityType::Value(target))
            .unwrap();
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &body.terminator.as_ref().unwrap().kind
        else {
            panic!("conditional break body");
        };
        let break_block = function.block(when_true.target).unwrap();
        assert_eq!(
            cleanup(function, break_block),
            [
                Operation::BorrowEnd {
                    loan: loan(transport(function, when_true, element_loan))
                },
                Operation::BorrowEnd {
                    loan: loan(transport(function, when_true, body_source))
                },
                Operation::Drop {
                    owner: value(transport(function, when_true, body_owner))
                },
            ]
        );
        let normal_block = function.block(when_false.target).unwrap();
        assert_eq!(
            cleanup(function, normal_block),
            [Operation::BorrowEnd {
                loan: loan(transport(function, when_false, element_loan))
            },]
        );
        let TerminatorKind::Branch(backedge) = &normal_block.terminator.as_ref().unwrap().kind
        else {
            panic!("normal sibling returns to header");
        };
        assert!(
            backedge
                .arguments
                .contains(&transport(function, when_false, body_source))
        );
        assert!(
            backedge
                .arguments
                .contains(&transport(function, when_false, body_owner))
        );
        assert!(matches!(
            defining_instruction(function, element_loan).operation,
            Operation::BorrowBegin { .. }
        ));
    }
}

#[test]
fn source_cleanup_rebinds_sibling_single_and_multiple_exit_cfg() {
    for body in [
        "if (flag) { break } else { break }",
        "if (flag) { break } else { continue }",
        "if (flag) { if (value == 1) { break } } else { continue }",
        "if (flag) { println(\"left\") } else { println(\"right\") }; break",
        "if (flag) { continue }; if (value == 1) { break }",
        "if (flag) { println(\"normal\") } else { break }; break",
        "if (flag) { return value } else { return 0 }",
        "for (inner in arrayOf(3, 4)) { if (flag) { break } }; if (flag) { break }",
        "for (inner in arrayOf(3, 4)) { if (flag) { return inner } }; break",
    ] {
        eprintln!("source cleanup CFG: {body}");
        verified(&format!(
            "fun scan(flag: Boolean): Int {{ for (value in arrayOf(1, 2)) {{ {body} }}; return 0 }}"
        ));
    }
}

#[test]
fn borrowed_source_cfg_does_not_end_the_callers_source_loan() {
    let program = verified(
        "fun scan(xs: List<Int>, flag: Boolean): Unit { for (value in xs) { if (flag) { break } else { continue } } }",
    );
    let function = &program.modules[0].functions[0];
    let source = function
        .instructions
        .iter()
        .find_map(|instruction| {
            if let Operation::ContainerLength { owner } = instruction.operation {
                Some(owner)
            } else {
                None
            }
        })
        .unwrap();
    let source_type = function.entity(source).unwrap().ty;
    for instruction in &function.instructions {
        if let Operation::BorrowEnd { loan } = instruction.operation {
            assert_ne!(
                function.entity(EntityId::Loan(loan)).unwrap().ty,
                source_type
            );
        }
    }
}
