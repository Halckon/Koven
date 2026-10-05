//! Escape proofs must follow current root contents after writes and exchanges.

use super::*;
use crate::ssa::model::{LoanId, PlaceId};

#[derive(Clone, Copy, Debug)]
enum Commit {
    Take,
    ReplaceNew,
    ReplaceOld,
    SwapFirst,
    SwapSecond,
}

fn root_place(
    function: &mut Function,
    block: BlockId,
    owner: ValueId,
    ty: SsaTypeId,
    at: &Origin,
) -> PlaceId {
    let EntityId::Place(place) = function
        .append_instruction(
            block,
            Operation::RootPlace { owner },
            vec![EntityType::Place(ty)],
            at.clone(),
        )
        .expect("root place")
        .1[0]
    else {
        panic!("place result")
    };
    place
}

fn exclusive(
    function: &mut Function,
    block: BlockId,
    place: PlaceId,
    ty: SsaTypeId,
    at: &Origin,
) -> LoanId {
    let EntityId::Loan(loan) = function
        .append_instruction(
            block,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: ty,
            }],
            at.clone(),
        )
        .expect("loan")
        .1[0]
    else {
        panic!("loan result")
    };
    loan
}

fn empty(function: &mut Function, block: BlockId, ty: SsaTypeId, at: &Origin) -> ValueId {
    append_values(
        function,
        block,
        Operation::ContainerConstruct {
            container: ty,
            elements: vec![],
        },
        &[ty],
        at,
    )[0]
}

fn root_contents(unknown: bool, commit: Commit) -> (Program, VerifyLocation, Origin) {
    let formation = origin();
    let delivery = delivery_origin("return wrapper");
    let mut program = Program::default();
    let module_id = program.add_module("root-current-closure-contents");
    let module = program.module_mut(module_id).expect("module");
    let (_, _, closure) = shared_closure_type(module);
    let ty = module
        .add_sequential_container_type(SequentialContainerKind::List, closure)
        .expect("List");
    let parameters = if unknown { vec![ty] } else { vec![] };
    let (id, entry, inputs) = add_function(module, "main", &parameters, vec![ty], &formation);
    let function = module.function_mut(id).expect("function");
    let initial = empty(function, entry, ty, &formation);
    let storage = root_place(function, entry, initial, ty, &formation);
    let incoming = if unknown {
        inputs[0]
    } else {
        empty(function, entry, ty, &formation)
    };
    append_values(
        function,
        entry,
        Operation::Mutate {
            place: storage,
            value: incoming,
        },
        &[],
        &formation,
    );
    let returned = match commit {
        Commit::Take => append_values(
            function,
            entry,
            Operation::RootPlaceTake {
                owner: initial,
                place: storage,
            },
            &[ty],
            &formation,
        )[0],
        Commit::ReplaceNew | Commit::ReplaceOld => {
            let replacement = empty(function, entry, ty, &formation);
            let loan = exclusive(function, entry, storage, ty, &formation);
            let results = append_values(
                function,
                entry,
                Operation::RootReplace {
                    owner: initial,
                    loan,
                    replacement,
                },
                &[ty, ty],
                &formation,
            );
            let (returned, dropped) = if matches!(commit, Commit::ReplaceNew) {
                (results[0], results[1])
            } else {
                (results[1], results[0])
            };
            append_values(
                function,
                entry,
                Operation::Drop { owner: dropped },
                &[],
                &formation,
            );
            returned
        }
        Commit::SwapFirst | Commit::SwapSecond => {
            let other = empty(function, entry, ty, &formation);
            let other_storage = root_place(function, entry, other, ty, &formation);
            let first_loan = exclusive(function, entry, storage, ty, &formation);
            let second_loan = exclusive(function, entry, other_storage, ty, &formation);
            let results = append_values(
                function,
                entry,
                Operation::RootSwap {
                    owners: [initial, other],
                    loans: [first_loan, second_loan],
                },
                &[ty, ty],
                &formation,
            );
            let (returned, dropped) = if matches!(commit, Commit::SwapFirst) {
                (results[0], results[1])
            } else {
                (results[1], results[0])
            };
            append_values(
                function,
                entry,
                Operation::Drop { owner: dropped },
                &[],
                &formation,
            );
            returned
        }
    };
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![returned],
            },
            delivery.clone(),
        )
        .expect("return");
    (program, VerifyLocation::Terminator(entry), delivery)
}

#[test]
fn borrowed_closure_root_rejects_unknown_written_then_taken() {
    assert_owned_escape_rejected(root_contents(true, Commit::Take));
}

#[test]
fn borrowed_closure_root_rejects_current_old_value_after_replace() {
    assert_owned_escape_rejected(root_contents(true, Commit::ReplaceOld));
}

#[test]
fn borrowed_closure_root_rejects_current_content_swapped_into_second_root() {
    assert_owned_escape_rejected(root_contents(true, Commit::SwapSecond));
}

#[test]
fn borrowed_closure_root_allows_clean_writes_and_current_clean_exchange_results() {
    for commit in [
        Commit::Take,
        Commit::ReplaceNew,
        Commit::ReplaceOld,
        Commit::SwapFirst,
        Commit::SwapSecond,
    ] {
        verify_program(&root_contents(false, commit).0)
            .unwrap_or_else(|errors| panic!("clean root {commit:?}: {errors:?}"));
    }
    // Unknown old contents are dropped; only the proven clean replacement escapes.
    for commit in [Commit::ReplaceNew, Commit::SwapFirst] {
        verify_program(&root_contents(true, commit).0).unwrap_or_else(|errors| {
            panic!("clean result after unknown old root {commit:?}: {errors:?}")
        });
    }
}
