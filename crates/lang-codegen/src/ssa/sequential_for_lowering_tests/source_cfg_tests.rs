//! SPEC-0182: local source loan identity follows body CFG edges and merges.
use std::collections::BTreeSet;

use super::cleanup_tests::{cleanup, loan, transport, value};
use super::{
    Definition, EntityId, Function, Instruction, LoanKind, Operation, SourceProgram,
    TerminatorKind, analyze, defining_instruction, incoming_edges,
};

fn verified(source: &str) -> SourceProgram {
    let analysis = analyze(source, "scan");
    crate::llvm::render_verified_program(&analysis.program).expect("source CFG must lower to LLVM");
    analysis
}

fn only_instruction(function: &Function, predicate: impl Fn(&Operation) -> bool) -> &Instruction {
    let instructions = function
        .instructions
        .iter()
        .filter(|instruction| predicate(&instruction.operation))
        .collect::<Vec<_>>();
    assert_eq!(
        instructions.len(),
        1,
        "expected one instruction: {instructions:?}"
    );
    instructions[0]
}

fn rooted_transport(function: &Function, root: EntityId) -> BTreeSet<EntityId> {
    function.entity(root).expect("known transport root");
    let mut rooted = BTreeSet::from([root]);
    loop {
        let mut changed = false;
        for block in &function.blocks {
            for (_, edge) in incoming_edges(function, block.id) {
                assert_eq!(edge.arguments.len(), block.parameters.len());
                for (argument, parameter) in edge.arguments.iter().zip(&block.parameters) {
                    if rooted.contains(argument) {
                        changed |= rooted.insert(*parameter);
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    // Only exact edge slots preserve identity. Instructions, including SharedReborrow,
    // create distinct roots; a mixed-root merge or an unrooted cycle is not an alias.
    for &entity in &rooted {
        if entity == root {
            continue;
        }
        let Definition::BlockParameter { block, index } = function
            .entity(entity)
            .expect("known transported entity")
            .definition
        else {
            panic!("transport must produce a block parameter: {entity:?}");
        };
        let block = function.block(block).expect("known transport target");
        assert_eq!(block.parameters.get(index), Some(&entity));
        let incoming = incoming_edges(function, block.id);
        assert!(!incoming.is_empty(), "transport must have an incoming edge");
        for (_, edge) in incoming {
            assert_eq!(edge.arguments.len(), block.parameters.len());
            assert!(
                rooted.contains(&edge.arguments[index]),
                "mixed transport roots at {entity:?}: {edge:?}"
            );
        }
    }
    rooted
}

#[test]
fn conditional_break_ends_rebound_source_before_temporary_drop() {
    for constructor in ["listOf", "arrayOf", "mutableListOf"] {
        let text = format!(
            "fun scan(): Unit {{ for (value in {constructor}(1)) {{ if (value == 1) {{ break }} }} }}"
        );
        let analysis = verified(&text);
        let function = analysis.entry();
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
        let length = only_instruction(function, |operation| {
            matches!(operation, Operation::ContainerLength { .. })
        });
        let Operation::ContainerLength { owner: source } = length.operation else {
            unreachable!();
        };
        let source_borrow = defining_instruction(function, source);
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = source_borrow.operation
        else {
            panic!("temporary source shared loan");
        };
        assert_eq!(source_borrow.results, [source]);
        let source_place = defining_instruction(function, EntityId::Place(place));
        let Operation::RootPlace { owner } = source_place.operation else {
            panic!("temporary source root owner");
        };
        assert_eq!(source_place.results, [EntityId::Place(place)]);
        let source_aliases = rooted_transport(function, source);
        let owner_aliases = rooted_transport(function, EntityId::Value(owner));
        assert!(source_aliases.contains(&body_source));
        let body_owners = body
            .parameters
            .iter()
            .copied()
            .filter(|entity| owner_aliases.contains(entity))
            .collect::<Vec<_>>();
        assert_eq!(body_owners.len(), 1, "one transported temporary owner");
        let body_owner = body_owners[0];
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
    // Preserve the full original determinism failure as well as its reduced CFG cases.
    eprintln!("original determinism red input: factory List + continue + break + total");
    verified(
        r#"
        fun source(): List<Int> = listOf(7, 2, 9)
        fun scan(): Int {
            var total = 0
            for (value in source()) {
                if (value == 2) { continue }
                if (value == 9) { break }
                total = total + value
            }
            return total
        }
        "#,
    );
}

#[test]
fn borrowed_source_cfg_does_not_end_the_callers_source_loan() {
    let analysis = verified(
        "fun scan(xs: List<Int>, flag: Boolean): Unit { for (value in xs) { if (flag) { break } else { continue } } }",
    );
    let function = analysis.entry();
    let preheader = function.block(function.entry_block().unwrap()).unwrap();
    let caller = preheader.parameters[0];
    assert_eq!(
        function.entity(caller).unwrap().definition,
        Definition::BlockParameter {
            block: preheader.id,
            index: 0,
        }
    );
    let length = only_instruction(function, |operation| {
        matches!(operation, Operation::ContainerLength { .. })
    });
    let Operation::ContainerLength { owner: source } = length.operation else {
        unreachable!();
    };
    let reborrow = only_instruction(function, |operation| {
        matches!(operation, Operation::SharedReborrow { .. })
    });
    assert_eq!(reborrow, defining_instruction(function, source));
    assert_eq!(reborrow.results, [source]);
    assert_eq!(
        reborrow.operation,
        Operation::SharedReborrow {
            source: loan(caller)
        }
    );
    assert_eq!(reborrow.block, preheader.id);
    assert_eq!(length.block, preheader.id);
    assert_ne!(source, caller);
    let caller_aliases = rooted_transport(function, caller);
    let source_aliases = rooted_transport(function, source);
    assert!(caller_aliases.is_disjoint(&source_aliases));
    for instruction in &function.instructions {
        if let Operation::BorrowEnd { loan: ended } = instruction.operation {
            assert!(
                !caller_aliases.contains(&EntityId::Loan(ended)),
                "caller loan must remain live: {instruction:?}"
            );
        }
    }

    let TerminatorKind::Branch(enter) = &preheader.terminator.as_ref().unwrap().kind else {
        panic!("enter provider header");
    };
    let header = function.block(enter.target).unwrap();
    let header_source = transport(function, enter, source);
    let TerminatorKind::Conditional {
        when_true: body_edge,
        when_false: exhausted_edge,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("provider guard");
    };
    let body = function.block(body_edge.target).unwrap();
    let exhausted = function.block(exhausted_edge.target).unwrap();
    let body_source = transport(function, body_edge, header_source);
    let element = only_instruction(function, |operation| {
        matches!(operation, Operation::ContainerElementPlace { .. })
    });
    assert_eq!(element.block, body.id);
    assert!(matches!(
        element.operation,
        Operation::ContainerElementPlace { owner, .. } if owner == body_source
    ));
    let element_borrow = only_instruction(function, |operation| {
        matches!(operation,
            Operation::BorrowBegin { place, kind: LoanKind::Shared }
                if EntityId::Place(*place) == element.results[0]
        )
    });
    assert_eq!(element_borrow.block, body.id);
    assert_eq!(element_borrow.results.len(), 1);
    let element_loan = element_borrow.results[0];
    assert_eq!(
        cleanup(function, exhausted),
        [Operation::BorrowEnd {
            loan: loan(transport(function, exhausted_edge, header_source))
        }]
    );
    let TerminatorKind::Conditional {
        when_true: break_edge,
        when_false: continue_edge,
        ..
    } = &body.terminator.as_ref().unwrap().kind
    else {
        panic!("break/continue body");
    };
    let break_block = function.block(break_edge.target).unwrap();
    let continue_block = function.block(continue_edge.target).unwrap();
    assert_eq!(
        cleanup(function, break_block),
        [
            Operation::BorrowEnd {
                loan: loan(transport(function, break_edge, element_loan))
            },
            Operation::BorrowEnd {
                loan: loan(transport(function, break_edge, body_source))
            },
        ]
    );
    assert_eq!(
        cleanup(function, continue_block),
        [Operation::BorrowEnd {
            loan: loan(transport(function, continue_edge, element_loan))
        }]
    );
    let continued_source = transport(function, continue_edge, body_source);
    let TerminatorKind::Branch(backedge) = &continue_block.terminator.as_ref().unwrap().kind else {
        panic!("continue returns to provider header");
    };
    assert_eq!(backedge.target, header.id);
    assert_eq!(
        transport(function, backedge, continued_source),
        header_source
    );
    for block in &function.blocks {
        if ![exhausted.id, break_block.id, continue_block.id].contains(&block.id) {
            assert!(
                cleanup(function, block).is_empty(),
                "unexpected cleanup in {block:?}"
            );
        }
    }
}
