use super::*;

#[derive(Clone, Copy, Debug)]
enum RetainSource {
    KnownEmpty,
    KnownEmptyCfg,
    ClearedRoot,
    TaintedRoot,
    UnknownEntryLoan,
}

fn shared_retain_loan_return(source: RetainSource) -> (Program, VerifyLocation, Origin) {
    let formation = origin();
    let delivery = delivery_origin("return wrapper");
    let mut program = Program::default();
    let module_id = program.add_module("shared-retain-loan-closure-content");
    let module = program.module_mut(module_id).expect("module must exist");
    let (_, _, closure) = shared_closure_type(module);
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, closure)
        .expect("List must preserve its possible borrowed closure element type");
    let shared = module
        .declare_shared_owner("SharedList")
        .expect("shared owner declaration must be valid");
    module
        .define_shared_owner(shared, list)
        .expect("shared owner must hold the List payload");
    let id = module
        .add_function("retain_and_return", vec![shared], formation.clone())
        .expect("function must return the retained shared owner");
    let function = module.function_mut(id).expect("function must exist");
    let parameters = match source {
        RetainSource::KnownEmpty | RetainSource::KnownEmptyCfg => vec![],
        RetainSource::ClearedRoot | RetainSource::TaintedRoot => vec![EntityType::Value(shared)],
        RetainSource::UnknownEntryLoan => vec![EntityType::Loan {
            kind: LoanKind::Shared,
            target: shared,
        }],
    };
    let entry = function
        .add_block(parameters, formation.clone())
        .expect("entry signature must match the source capability");
    let (mut loan, mut local_owner) = match source {
        RetainSource::KnownEmpty
        | RetainSource::KnownEmptyCfg
        | RetainSource::ClearedRoot
        | RetainSource::TaintedRoot => {
            // The element type may contain captures, but this particular List has none.
            let empty = append_values(
                function,
                entry,
                Operation::ContainerConstruct {
                    container: list,
                    elements: vec![],
                },
                &[list],
                &formation,
            )[0];
            let clean_owner = append_values(
                function,
                entry,
                Operation::SharedAllocate {
                    owner: shared,
                    payload: empty,
                },
                &[shared],
                &formation,
            )[0];
            let owner = if matches!(source, RetainSource::ClearedRoot) {
                value(function.block(entry).expect("entry").parameters[0])
            } else {
                clean_owner
            };
            let EntityId::Place(place) = function
                .append_instruction(
                    entry,
                    Operation::RootPlace { owner },
                    vec![EntityType::Place(shared)],
                    formation.clone(),
                )
                .expect("local owner must provide its complete root place")
                .1[0]
            else {
                panic!("RootPlace must produce a place");
            };
            let replacement = match source {
                RetainSource::ClearedRoot => Some(clean_owner),
                RetainSource::TaintedRoot => {
                    Some(value(function.block(entry).expect("entry").parameters[0]))
                }
                _ => None,
            };
            if let Some(value) = replacement {
                // Loan retain must inspect the contents after this complete root write.
                append_values(
                    function,
                    entry,
                    Operation::Mutate { place, value },
                    &[],
                    &formation,
                );
            }
            let EntityId::Loan(loan) = function
                .append_instruction(
                    entry,
                    Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: shared,
                    }],
                    formation.clone(),
                )
                .expect("shared loan must borrow the complete shared owner")
                .1[0]
            else {
                panic!("BorrowBegin must produce a loan");
            };
            (loan, Some(owner))
        }
        RetainSource::UnknownEntryLoan => {
            let EntityId::Loan(loan) = function.block(entry).expect("entry").parameters[0] else {
                panic!("entry parameter must remain a shared loan");
            };
            (loan, None)
        }
    };
    let retain_block = if matches!(source, RetainSource::KnownEmptyCfg) {
        let target = function
            .add_block(
                vec![
                    EntityType::Value(shared),
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: shared,
                    },
                ],
                formation.clone(),
            )
            .expect("CFG target must transport the borrowed owner and its shared loan");
        function
            .set_terminator(
                entry,
                TerminatorKind::Branch(Edge {
                    target,
                    arguments: vec![
                        EntityId::Value(local_owner.expect("CFG source owns the empty payload")),
                        EntityId::Loan(loan),
                    ],
                }),
                formation.clone(),
            )
            .expect("owner and loan must cross the same CFG edge");
        let parameters = &function.block(target).expect("target").parameters;
        local_owner = Some(value(parameters[0]));
        let EntityId::Loan(parameter) = parameters[1] else {
            panic!("CFG loan parameter must remain a shared loan");
        };
        loan = parameter;
        target
    } else {
        entry
    };
    let retained = append_values(
        function,
        retain_block,
        Operation::SharedRetain {
            owner: EntityId::Loan(loan),
        },
        &[shared],
        &formation,
    )[0];
    append_values(
        function,
        retain_block,
        Operation::BorrowEnd { loan },
        &[],
        &formation,
    );
    if let Some(owner) = local_owner {
        append_values(
            function,
            retain_block,
            Operation::Drop { owner },
            &[],
            &formation,
        );
    }
    function
        .set_terminator(
            retain_block,
            TerminatorKind::Return {
                values: vec![retained],
            },
            delivery.clone(),
        )
        .expect("Return must deliver exactly the independently retained owner");
    (program, VerifyLocation::Terminator(retain_block), delivery)
}

#[test]
fn borrowed_closure_shared_retain_loan_allows_known_empty_payload_return() {
    // Retain adds ownership of the same known-empty payload; it cannot introduce a capture.
    verify_program(&shared_retain_loan_return(RetainSource::KnownEmpty).0)
        .expect("known-empty shared payload must keep its content proof through Loan retain");
}

#[test]
fn borrowed_closure_shared_retain_loan_allows_known_empty_payload_after_cfg_transport() {
    // The edge moves the owner and loan together without changing the payload contents.
    verify_program(&shared_retain_loan_return(RetainSource::KnownEmptyCfg).0)
        .expect("CFG transport must preserve the known-empty payload proof through Loan retain");
}

#[test]
fn borrowed_closure_shared_retain_loan_rejects_unknown_entry_payload_return() {
    // An arbitrary caller loan supplies no evidence that its List payload is capture-free.
    assert_owned_escape_rejected(shared_retain_loan_return(RetainSource::UnknownEntryLoan));
}

#[test]
fn borrowed_closure_shared_retain_loan_allows_root_cleared_before_borrow() {
    // The unknown entry owner is replaced with an empty payload before the loan exists.
    verify_program(&shared_retain_loan_return(RetainSource::ClearedRoot).0)
        .expect("Loan retain must use the proven clean current root contents after replacement");
}

#[test]
fn borrowed_closure_shared_retain_loan_rejects_root_tainted_before_borrow() {
    // Initial allocation was empty; retaining after an unknown write must not reuse that proof.
    assert_owned_escape_rejected(shared_retain_loan_return(RetainSource::TaintedRoot));
}
