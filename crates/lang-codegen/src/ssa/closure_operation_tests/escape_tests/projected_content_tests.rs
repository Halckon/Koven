use super::*;
use crate::ssa::model::PlaceId;

#[derive(Clone, Copy, Debug)]
enum RootWrite {
    ClearInline,
    TaintInline,
    ClearInlineCfg,
    TaintInlineCfg,
    ReplaceSharedHandle,
}

fn append_place(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    ty: SsaTypeId,
    at: &Origin,
) -> PlaceId {
    let EntityId::Place(place) = function
        .append_instruction(block, operation, vec![EntityType::Place(ty)], at.clone())
        .expect("place operation must have its declared projection type")
        .1[0]
    else {
        panic!("projection must produce a place");
    };
    place
}

fn shared_field_types(module: &mut Module) -> (SsaTypeId, SsaTypeId, SsaTypeId) {
    let (_, _, closure) = shared_closure_type(module);
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, closure)
        .expect("List retains its possible borrowed closure element type");
    let inner = module.declare_shared_owner("Inner").expect("inner owner");
    module
        .define_shared_owner(inner, list)
        .expect("inner payload");
    let aggregate = module
        .add_aggregate_type("Fields", vec![inner])
        .expect("inline aggregate contains the shared owner field");
    (list, inner, aggregate)
}

fn precreated_field_after_root_write(write: RootWrite) -> (Program, VerifyLocation, Origin) {
    let formation = origin();
    let delivery = delivery_origin("return wrapper");
    let mut program = Program::default();
    let module_id = program.add_module("precreated-field-current-content");
    let module = program.module_mut(module_id).expect("module");
    let (list, inner, aggregate) = shared_field_types(module);
    let outer = if matches!(write, RootWrite::ReplaceSharedHandle) {
        let owner = module.declare_shared_owner("Outer").expect("outer owner");
        module
            .define_shared_owner(owner, aggregate)
            .expect("outer allocation holds the aggregate payload");
        Some(owner)
    } else {
        None
    };
    let root_type = outer.unwrap_or(aggregate);
    let (id, entry, parameters) = add_function(
        module,
        "overwrite_then_retain_field",
        &[root_type],
        vec![inner],
        &formation,
    );
    let function = module.function_mut(id).expect("function");
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
    let clean_inner = append_values(
        function,
        entry,
        Operation::SharedAllocate {
            owner: inner,
            payload: empty,
        },
        &[inner],
        &formation,
    )[0];
    // Dirty aggregate contents arrive directly as an entry owner, never through a
    // forbidden AggregateConstruct of unknown borrowed-closure storage.
    let clean_aggregate = append_values(
        function,
        entry,
        Operation::AggregateConstruct {
            aggregate,
            fields: vec![clean_inner],
        },
        &[aggregate],
        &formation,
    )[0];
    let mut owner = if matches!(write, RootWrite::TaintInline | RootWrite::TaintInlineCfg) {
        clean_aggregate
    } else {
        parameters[0]
    };
    let mut root = append_place(
        function,
        entry,
        Operation::RootPlace { owner },
        root_type,
        &formation,
    );
    let (field_base, retained_outer) = if let Some(outer) = outer {
        // Keep the old allocation alive independently of replacing the original
        // handle. Its existing payload/field projections still address old contents.
        let retained = append_values(
            function,
            entry,
            Operation::SharedRetain {
                owner: EntityId::Value(owner),
            },
            &[outer],
            &formation,
        )[0];
        let payload = append_place(
            function,
            entry,
            Operation::SharedPayloadPlace {
                owner: EntityId::Value(owner),
            },
            aggregate,
            &formation,
        );
        (payload, Some(retained))
    } else {
        (root, None)
    };
    let mut field = append_place(
        function,
        entry,
        Operation::FieldPlace {
            base: field_base,
            field: 0,
        },
        inner,
        &formation,
    );
    let mut replacement = match write {
        RootWrite::ClearInline | RootWrite::ClearInlineCfg => clean_aggregate,
        RootWrite::TaintInline | RootWrite::TaintInlineCfg => parameters[0],
        RootWrite::ReplaceSharedHandle => append_values(
            function,
            entry,
            Operation::SharedAllocate {
                owner: outer.expect("shared handle case has an outer owner type"),
                payload: clean_aggregate,
            },
            &[root_type],
            &formation,
        )[0],
    };
    let write_block = if matches!(write, RootWrite::ClearInlineCfg | RootWrite::TaintInlineCfg) {
        let target = function
            .add_block(
                vec![
                    EntityType::Value(aggregate),
                    EntityType::Place(aggregate),
                    EntityType::Place(inner),
                    EntityType::Value(aggregate),
                ],
                formation.clone(),
            )
            .expect("CFG target must preserve owner, root, child field and replacement types");
        function
            .set_terminator(
                entry,
                TerminatorKind::Branch(Edge {
                    target,
                    arguments: vec![
                        EntityId::Value(owner),
                        EntityId::Place(root),
                        EntityId::Place(field),
                        EntityId::Value(replacement),
                    ],
                }),
                formation.clone(),
            )
            .expect("root owner and both existing places must cross the same CFG edge");
        let parameters = &function.block(target).expect("target").parameters;
        owner = value(parameters[0]);
        let EntityId::Place(root_parameter) = parameters[1] else {
            panic!("root parameter must remain a place");
        };
        let EntityId::Place(field_parameter) = parameters[2] else {
            panic!("child parameter must remain a place");
        };
        root = root_parameter;
        field = field_parameter;
        replacement = value(parameters[3]);
        target
    } else {
        entry
    };
    append_values(
        function,
        write_block,
        Operation::Mutate {
            place: root,
            value: replacement,
        },
        &[],
        &formation,
    );
    // No loan existed during the whole-root write. The pre-created field place
    // is borrowed only after the replacement has committed.
    let EntityId::Loan(loan) = function
        .append_instruction(
            write_block,
            Operation::BorrowBegin {
                place: field,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: inner,
            }],
            formation.clone(),
        )
        .expect("field loan must borrow the complete inner shared owner")
        .1[0]
    else {
        panic!("field borrow must produce a loan");
    };
    let retained_inner = append_values(
        function,
        write_block,
        Operation::SharedRetain {
            owner: EntityId::Loan(loan),
        },
        &[inner],
        &formation,
    )[0];
    append_values(
        function,
        write_block,
        Operation::BorrowEnd { loan },
        &[],
        &formation,
    );
    append_values(
        function,
        write_block,
        Operation::Drop { owner },
        &[],
        &formation,
    );
    if let Some(owner) = retained_outer {
        append_values(
            function,
            write_block,
            Operation::Drop { owner },
            &[],
            &formation,
        );
    }
    function
        .set_terminator(
            write_block,
            TerminatorKind::Return {
                values: vec![retained_inner],
            },
            delivery.clone(),
        )
        .expect("Return delivers only the independently retained inner owner");
    (program, VerifyLocation::Terminator(write_block), delivery)
}

#[test]
fn borrowed_closure_projected_content_allows_inline_field_after_whole_root_clear() {
    verify_program(&precreated_field_after_root_write(RootWrite::ClearInline).0)
        .expect("existing inline field projection must see the new clean root contents");
}

#[test]
fn borrowed_closure_projected_content_rejects_inline_field_after_whole_root_taint() {
    assert_owned_escape_rejected(precreated_field_after_root_write(RootWrite::TaintInline));
}

#[test]
fn borrowed_closure_projected_content_rejects_old_shared_payload_after_handle_clear() {
    assert_owned_escape_rejected(precreated_field_after_root_write(
        RootWrite::ReplaceSharedHandle,
    ));
}

#[test]
fn borrowed_closure_projected_content_allows_inline_field_after_cfg_root_clear() {
    // The existing field address, its complete root and their owner move together;
    // replacing that root in the target block changes the same inline field contents.
    verify_program(&precreated_field_after_root_write(RootWrite::ClearInlineCfg).0)
        .expect("CFG owner/place transport must preserve the inline root-to-field path proof");
}

#[test]
fn borrowed_closure_projected_content_rejects_inline_field_after_cfg_root_taint() {
    assert_owned_escape_rejected(precreated_field_after_root_write(RootWrite::TaintInlineCfg));
}

fn mismatched_cfg_root_and_field() -> (Program, VerifyLocation, Origin) {
    let formation = origin();
    let delivery = delivery_origin("return wrapper");
    let mut program = Program::default();
    let module_id = program.add_module("mismatched-root-field-cfg");
    let module = program.module_mut(module_id).expect("module");
    let (list, inner, aggregate) = shared_field_types(module);
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let (id, entry, inputs) = add_function(
        module,
        "clear_other_root_then_retain_first_field",
        &[aggregate, aggregate, boolean],
        vec![inner],
        &formation,
    );
    let function = module.function_mut(id).expect("function");
    let mut roots = Vec::new();
    let mut fields = Vec::new();
    for owner in &inputs[..2] {
        let root = append_place(
            function,
            entry,
            Operation::RootPlace { owner: *owner },
            aggregate,
            &formation,
        );
        roots.push(root);
        fields.push(append_place(
            function,
            entry,
            Operation::FieldPlace {
                base: root,
                field: 0,
            },
            inner,
            &formation,
        ));
    }
    let target = function
        .add_block(
            vec![
                EntityType::Value(aggregate),
                EntityType::Value(aggregate),
                EntityType::Place(aggregate),
                EntityType::Place(inner),
            ],
            formation.clone(),
        )
        .expect("join transports two distinct owners, the other root and the first field");
    function
        .set_terminator(
            entry,
            TerminatorKind::Conditional {
                condition: inputs[2],
                when_true: Edge {
                    target,
                    arguments: vec![
                        EntityId::Value(inputs[0]),
                        EntityId::Value(inputs[1]),
                        EntityId::Place(roots[1]),
                        EntityId::Place(fields[0]),
                    ],
                },
                when_false: Edge {
                    target,
                    arguments: vec![
                        EntityId::Value(inputs[1]),
                        EntityId::Value(inputs[0]),
                        EntityId::Place(roots[0]),
                        EntityId::Place(fields[1]),
                    ],
                },
            },
            formation.clone(),
        )
        .expect("each branch must pair the selected field with the owner that is not overwritten");
    let parameters = &function.block(target).expect("target").parameters;
    let first_owner = value(parameters[0]);
    let other_owner = value(parameters[1]);
    let EntityId::Place(other_root) = parameters[2] else {
        panic!("other root must remain a place");
    };
    let EntityId::Place(first_field) = parameters[3] else {
        panic!("first field must remain a place");
    };
    let empty = append_values(
        function,
        target,
        Operation::ContainerConstruct {
            container: list,
            elements: vec![],
        },
        &[list],
        &formation,
    )[0];
    let clean_inner = append_values(
        function,
        target,
        Operation::SharedAllocate {
            owner: inner,
            payload: empty,
        },
        &[inner],
        &formation,
    )[0];
    let clean_aggregate = append_values(
        function,
        target,
        Operation::AggregateConstruct {
            aggregate,
            fields: vec![clean_inner],
        },
        &[aggregate],
        &formation,
    )[0];
    append_values(
        function,
        target,
        Operation::Mutate {
            place: other_root,
            value: clean_aggregate,
        },
        &[],
        &formation,
    );
    let EntityId::Loan(loan) = function
        .append_instruction(
            target,
            Operation::BorrowBegin {
                place: first_field,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: inner,
            }],
            formation.clone(),
        )
        .expect("loan borrows the still-unknown field of the first owner")
        .1[0]
    else {
        panic!("field borrow must produce a loan");
    };
    let retained = append_values(
        function,
        target,
        Operation::SharedRetain {
            owner: EntityId::Loan(loan),
        },
        &[inner],
        &formation,
    )[0];
    append_values(
        function,
        target,
        Operation::BorrowEnd { loan },
        &[],
        &formation,
    );
    for owner in [first_owner, other_owner] {
        append_values(function, target, Operation::Drop { owner }, &[], &formation);
    }
    function
        .set_terminator(
            target,
            TerminatorKind::Return {
                values: vec![retained],
            },
            delivery.clone(),
        )
        .expect("only the retained inner owner escapes");
    (program, VerifyLocation::Terminator(target), delivery)
}

#[test]
fn borrowed_closure_projected_content_rejects_mismatched_cfg_root_field_pairing() {
    // The four join aliases have equal may-origin sets, but on either actual
    // branch the overwritten root belongs to the other owner, never this field.
    assert_owned_escape_rejected(mismatched_cfg_root_and_field());
}
