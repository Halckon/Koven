use super::*;
use crate::ssa::model::{PlaceId, ScalarConstant};

type EscapeFixture = (Program, VerifyLocation, Origin);

#[derive(Clone, Copy, Debug)]
enum Construction {
    Aggregate,
    Tagged,
    Heap,
    Shared,
    Container,
}

fn call_delivery(indirect: bool) -> EscapeFixture {
    let formation = origin();
    let delivery = delivery_origin("consume(closure)");
    let mut program = Program::default();
    let module_id = program.add_module("call-owned-delivery");
    let module = program.module_mut(module_id).expect("module");
    let (_, _, closure) = shared_closure_type(module);
    let (callee, callee_entry, parameters) =
        add_function(module, "consume", &[closure], vec![], &formation);
    let function = module.function_mut(callee).expect("callee");
    if !indirect {
        function.receiver = Some(EntityType::Value(closure));
    }
    append_values(
        function,
        callee_entry,
        Operation::Drop {
            owner: parameters[0],
        },
        &[],
        &formation,
    );
    function
        .set_terminator(
            callee_entry,
            TerminatorKind::Return { values: vec![] },
            formation.clone(),
        )
        .expect("callee must discharge its entry closure");
    let pointer = module
        .add_function_pointer_type(vec![closure], vec![])
        .expect("indirect Value argument signature");
    let (caller, entry, parameters) =
        add_function(module, "forward", &[closure], vec![], &formation);
    let function = module.function_mut(caller).expect("caller");
    let address = indirect.then(|| {
        append_values(
            function,
            entry,
            Operation::FunctionAddress { target: callee },
            &[pointer],
            &formation,
        )[0]
    });
    let operation = match address {
        Some(callable) => Operation::CallableInvoke {
            callable,
            arguments: vec![EntityId::Value(parameters[0])],
        },
        None => Operation::DirectCall {
            callee,
            receiver: Some(EntityId::Value(parameters[0])),
            arguments: vec![],
        },
    };
    let (instruction, _) = function
        .append_instruction(entry, operation, vec![], delivery.clone())
        .expect("well-typed Value delivery");
    if let Some(owner) = address {
        append_values(function, entry, Operation::Drop { owner }, &[], &formation);
    }
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, formation)
        .expect("caller discharges both the closure and any function pointer");
    (program, VerifyLocation::Instruction(instruction), delivery)
}

fn storage_delivery(kind: Construction) -> EscapeFixture {
    let formation = origin();
    let delivery = delivery_origin("consume(closure)");
    let mut program = Program::default();
    let module_id = program.add_module("storage-owned-delivery");
    let module = program.module_mut(module_id).expect("module");
    let (_, _, closure) = shared_closure_type(module);
    let payload = module
        .add_aggregate_type("Payload", vec![closure])
        .expect("aggregate contains the borrowed closure");
    let (input_type, output_type) = match kind {
        Construction::Aggregate => (closure, payload),
        Construction::Tagged => (
            payload,
            module
                .add_tagged_union_type("Choice", vec![payload])
                .expect("tagged"),
        ),
        Construction::Heap => {
            let owner = module.declare_heap_owner("Node").expect("heap owner");
            module
                .define_heap_owner(owner, payload)
                .expect("heap payload");
            (payload, owner)
        }
        Construction::Shared => {
            let owner = module.declare_shared_owner("Shared").expect("shared owner");
            module
                .define_shared_owner(owner, closure)
                .expect("shared payload");
            (closure, owner)
        }
        Construction::Container => (
            closure,
            module
                .add_sequential_container_type(SequentialContainerKind::List, closure)
                .expect("List of concrete closures"),
        ),
    };
    // Entry payloads avoid an earlier AggregateConstruct rejection substituting
    // for the specific TaggedConstruct or HeapAllocate delivery under test.
    let (main, entry, parameters) = add_function(module, "main", &[input_type], vec![], &formation);
    let function = module.function_mut(main).expect("main");
    let input = parameters[0];
    let operation = match kind {
        Construction::Aggregate => Operation::AggregateConstruct {
            aggregate: output_type,
            fields: vec![input],
        },
        Construction::Tagged => Operation::TaggedConstruct {
            tagged: output_type,
            variant: 0,
            payload: input,
        },
        Construction::Heap => Operation::HeapAllocate {
            owner: output_type,
            payload: input,
        },
        Construction::Shared => Operation::SharedAllocate {
            owner: output_type,
            payload: input,
        },
        Construction::Container => Operation::ContainerConstruct {
            container: output_type,
            elements: vec![input],
        },
    };
    let (instruction, results) = function
        .append_instruction(
            entry,
            operation,
            vec![EntityType::Value(output_type)],
            delivery.clone(),
        )
        .expect("storage construction must have the exact payload type");
    append_values(
        function,
        entry,
        Operation::Drop {
            owner: value(results[0]),
        },
        &[],
        &formation,
    );
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, formation)
        .expect("the constructed storage owner is dropped exactly once");
    (program, VerifyLocation::Instruction(instruction), delivery)
}

fn owned_capture_return() -> EscapeFixture {
    let formation = origin();
    let delivery = delivery_origin("return closure");
    let mut program = Program::default();
    let module_id = program.add_module("owned-capture-delivery");
    let module = program.module_mut(module_id).expect("module");
    let (_, _, borrowed) = shared_closure_type(module);
    let environment = module
        .add_aggregate_type("OuterEnv", vec![borrowed])
        .expect("environment");
    let outer = module
        .add_concrete_closure_type(
            "OuterClosure",
            vec![],
            vec![],
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Owned,
                ty: borrowed,
            }],
        )
        .expect("outer closure owns the inner closure");
    let (thunk, thunk_entry, _) = add_thunk(module, "thunk", environment, &[], vec![], &formation);
    module
        .function_mut(thunk)
        .expect("thunk")
        .set_terminator(
            thunk_entry,
            TerminatorKind::Return { values: vec![] },
            formation.clone(),
        )
        .expect("thunk borrows its environment");
    let (main, entry, parameters) =
        add_function(module, "main", &[borrowed], vec![outer], &formation);
    let function = module.function_mut(main).expect("main");
    let captured = append_values(
        function,
        entry,
        Operation::ClosureConstruct {
            closure: outer,
            thunk,
            captures: vec![ClosureCaptureOperand::Owned(parameters[0])],
        },
        &[outer],
        &formation,
    )[0];
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![captured],
            },
            delivery.clone(),
        )
        .expect("only the outer Return escapes, local owned capture formation stays legal");
    (program, VerifyLocation::Terminator(entry), delivery)
}

fn append_place(
    function: &mut Function,
    entry: BlockId,
    operation: Operation,
    ty: SsaTypeId,
    origin: &Origin,
) -> PlaceId {
    let EntityId::Place(place) = function
        .append_instruction(
            entry,
            operation,
            vec![EntityType::Place(ty)],
            origin.clone(),
        )
        .expect("typed place projection")
        .1[0]
    else {
        panic!("place result");
    };
    place
}

fn field_replace(heap: bool) -> EscapeFixture {
    let formation = origin();
    let delivery = delivery_origin("consume(closure)");
    let mut program = Program::default();
    let module_id = program.add_module("field-owned-delivery");
    let module = program.module_mut(module_id).expect("module");
    let (_, _, closure) = shared_closure_type(module);
    let payload = module
        .add_aggregate_type("Payload", vec![closure])
        .expect("payload");
    let receiver_type = if heap {
        let owner = module.declare_heap_owner("Node").expect("owner");
        module.define_heap_owner(owner, payload).expect("payload");
        owner
    } else {
        payload
    };
    let main = module
        .add_function("replace", vec![], formation.clone())
        .expect("function");
    let function = module.function_mut(main).expect("main");
    let entry = function
        .add_block(
            vec![
                EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: receiver_type,
                },
                EntityType::Value(closure),
            ],
            formation.clone(),
        )
        .expect("exclusive receiver plus owned replacement");
    let parameters = &function.block(entry).expect("entry").parameters;
    let EntityId::Loan(receiver) = parameters[0] else {
        panic!("receiver loan");
    };
    let replacement = value(parameters[1]);
    let operation = if heap {
        Operation::HeapFieldReplace {
            receiver,
            field: 0,
            value: replacement,
        }
    } else {
        Operation::InlineFieldReplace {
            receiver,
            field: 0,
            value: replacement,
        }
    };
    let (instruction, _) = function
        .append_instruction(entry, operation, vec![], delivery.clone())
        .expect("field replacement");
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, formation)
        .expect("Return ends the borrowed entry receiver without consuming its owner");
    (program, VerifyLocation::Instruction(instruction), delivery)
}

fn heap_field_exchange() -> EscapeFixture {
    let formation = origin();
    let delivery = delivery_origin("consume(closure)");
    let mut program = Program::default();
    let module_id = program.add_module("exchange-owned-delivery");
    let module = program.module_mut(module_id).expect("module");
    let (_, _, borrowed) = shared_closure_type(module);
    // HeapFieldExchange already excludes a direct ConcreteClosure field. Its
    // supported List field can still own an unknown borrowed-closure payload.
    let field_type = module
        .add_sequential_container_type(SequentialContainerKind::List, borrowed)
        .expect("supported exchange field retaining the concrete closure element");
    let payload = module
        .add_aggregate_type("Payload", vec![field_type])
        .expect("payload");
    let owner_type = module.declare_heap_owner("Node").expect("owner");
    module
        .define_heap_owner(owner_type, payload)
        .expect("payload");
    let (main, entry, parameters) = add_function(
        module,
        "main",
        &[owner_type, field_type],
        vec![],
        &formation,
    );
    let function = module.function_mut(main).expect("main");
    let owner = parameters[0];
    let base = append_place(
        function,
        entry,
        Operation::HeapPayloadPlace { owner },
        payload,
        &formation,
    );
    let field = append_place(
        function,
        entry,
        Operation::FieldPlace { base, field: 0 },
        field_type,
        &formation,
    );
    let EntityId::Loan(loan) = function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place: field,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: field_type,
            }],
            formation.clone(),
        )
        .expect("exact exclusive field loan")
        .1[0]
    else {
        panic!("field loan");
    };
    let (instruction, results) = function
        .append_instruction(
            entry,
            Operation::HeapFieldExchange {
                owner,
                field: 0,
                loan,
                replacement: parameters[1],
            },
            vec![EntityType::Value(field_type)],
            delivery.clone(),
        )
        .expect("exchange returns the old List and consumes its exact field loan");
    for owner in [value(results[0]), owner] {
        append_values(function, entry, Operation::Drop { owner }, &[], &formation);
    }
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, formation)
        .expect("owners discharged");
    (program, VerifyLocation::Instruction(instruction), delivery)
}

fn container_replace() -> EscapeFixture {
    let formation = origin();
    let delivery = delivery_origin("consume(closure)");
    let mut program = Program::default();
    let module_id = program.add_module("container-owned-delivery");
    let module = program.module_mut(module_id).expect("module");
    let (_, _, closure) = shared_closure_type(module);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let container = module
        .add_sequential_container_type(SequentialContainerKind::Array, closure)
        .expect("mutable container");
    let (main, entry, parameters) =
        add_function(module, "main", &[container, closure], vec![], &formation);
    let function = module.function_mut(main).expect("main");
    let index = append_values(
        function,
        entry,
        Operation::Constant(ScalarConstant::Integer(0)),
        &[integer],
        &formation,
    )[0];
    let (instruction, _) = function
        .append_instruction(
            entry,
            Operation::ContainerReplace {
                owner: parameters[0],
                index,
                value: parameters[1],
            },
            vec![],
            delivery.clone(),
        )
        .expect("replace without an active conflicting loan");
    append_values(
        function,
        entry,
        Operation::Drop {
            owner: parameters[0],
        },
        &[],
        &formation,
    );
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, formation)
        .expect("container discharged");
    (program, VerifyLocation::Instruction(instruction), delivery)
}

#[derive(Clone, Copy, Debug)]
enum Projection {
    Root,
    InlineField,
    HeapField,
    ContainerElement,
}

fn mutate_delivery(projection: Projection, cfg: bool) -> EscapeFixture {
    let formation = origin();
    let delivery = delivery_origin("consume(closure)");
    let mut program = Program::default();
    let module_id = program.add_module("projected-owned-delivery");
    let module = program.module_mut(module_id).expect("module");
    let (_, _, closure) = shared_closure_type(module);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let payload = module
        .add_aggregate_type("Payload", vec![closure])
        .expect("payload");
    let owner_type = match projection {
        Projection::Root => closure,
        Projection::InlineField => payload,
        Projection::HeapField => {
            let owner = module.declare_heap_owner("Node").expect("owner");
            module.define_heap_owner(owner, payload).expect("payload");
            owner
        }
        Projection::ContainerElement => module
            .add_sequential_container_type(SequentialContainerKind::Array, closure)
            .expect("array"),
    };
    let (main, entry, parameters) =
        add_function(module, "main", &[owner_type, closure], vec![], &formation);
    let function = module.function_mut(main).expect("main");
    let place = match projection {
        Projection::Root => append_place(
            function,
            entry,
            Operation::RootPlace {
                owner: parameters[0],
            },
            closure,
            &formation,
        ),
        Projection::InlineField | Projection::HeapField => {
            let operation = if matches!(projection, Projection::HeapField) {
                Operation::HeapPayloadPlace {
                    owner: parameters[0],
                }
            } else {
                Operation::RootPlace {
                    owner: parameters[0],
                }
            };
            let base = append_place(function, entry, operation, payload, &formation);
            append_place(
                function,
                entry,
                Operation::FieldPlace { base, field: 0 },
                closure,
                &formation,
            )
        }
        Projection::ContainerElement => {
            let index = append_values(
                function,
                entry,
                Operation::Constant(ScalarConstant::Integer(0)),
                &[integer],
                &formation,
            )[0];
            append_place(
                function,
                entry,
                Operation::ContainerElementPlace {
                    owner: EntityId::Value(parameters[0]),
                    index,
                },
                closure,
                &formation,
            )
        }
    };
    let (block, owner, replacement, place) = if cfg {
        let target = function
            .add_block(
                vec![
                    EntityType::Value(owner_type),
                    EntityType::Value(closure),
                    EntityType::Place(closure),
                ],
                formation.clone(),
            )
            .expect("explicit owner/replacement/projected-place transport");
        function
            .set_terminator(
                entry,
                TerminatorKind::Branch(Edge {
                    target,
                    arguments: vec![
                        EntityId::Value(parameters[0]),
                        EntityId::Value(parameters[1]),
                        EntityId::Place(place),
                    ],
                }),
                formation.clone(),
            )
            .expect("linear capabilities move together");
        let parameters = &function.block(target).expect("target").parameters;
        let EntityId::Place(place) = parameters[2] else {
            panic!("projected place parameter");
        };
        (target, value(parameters[0]), value(parameters[1]), place)
    } else {
        (entry, parameters[0], parameters[1], place)
    };
    let (instruction, _) = function
        .append_instruction(
            block,
            Operation::Mutate {
                place,
                value: replacement,
            },
            vec![],
            delivery.clone(),
        )
        .expect("well-typed mutation");
    append_values(function, block, Operation::Drop { owner }, &[], &formation);
    function
        .set_terminator(block, TerminatorKind::Return { values: vec![] }, formation)
        .expect("replacement consumed and owner discharged");
    (program, VerifyLocation::Instruction(instruction), delivery)
}

#[test]
fn borrowed_closure_escape_rejects_direct_value_receiver() {
    assert_owned_escape_rejected(call_delivery(false));
}

#[test]
fn borrowed_closure_escape_rejects_indirect_value_argument() {
    assert_owned_escape_rejected(call_delivery(true));
}

#[test]
fn borrowed_closure_escape_rejects_aggregate_construct() {
    assert_owned_escape_rejected(storage_delivery(Construction::Aggregate));
}

#[test]
fn borrowed_closure_escape_rejects_tagged_construct() {
    assert_owned_escape_rejected(storage_delivery(Construction::Tagged));
}

#[test]
fn borrowed_closure_escape_rejects_heap_allocate() {
    assert_owned_escape_rejected(storage_delivery(Construction::Heap));
}

#[test]
fn borrowed_closure_escape_rejects_shared_allocate() {
    assert_owned_escape_rejected(storage_delivery(Construction::Shared));
}

#[test]
fn borrowed_closure_escape_rejects_container_construct() {
    assert_owned_escape_rejected(storage_delivery(Construction::Container));
}

#[test]
fn borrowed_closure_escape_rejects_heap_field_replace() {
    assert_owned_escape_rejected(field_replace(true));
}

#[test]
fn borrowed_closure_escape_rejects_inline_field_replace() {
    assert_owned_escape_rejected(field_replace(false));
}

#[test]
fn borrowed_closure_escape_rejects_heap_field_exchange() {
    assert_owned_escape_rejected(heap_field_exchange());
}

#[test]
fn borrowed_closure_escape_rejects_container_replace() {
    assert_owned_escape_rejected(container_replace());
}

#[test]
fn borrowed_closure_escape_rejects_inline_field_mutate() {
    assert_owned_escape_rejected(mutate_delivery(Projection::InlineField, false));
}

#[test]
fn borrowed_closure_escape_rejects_inline_field_mutate_cfg_alias() {
    assert_owned_escape_rejected(mutate_delivery(Projection::InlineField, true));
}

#[test]
fn borrowed_closure_escape_rejects_heap_field_mutate() {
    assert_owned_escape_rejected(mutate_delivery(Projection::HeapField, false));
}

#[test]
fn borrowed_closure_escape_rejects_heap_field_mutate_cfg_alias() {
    assert_owned_escape_rejected(mutate_delivery(Projection::HeapField, true));
}

#[test]
fn borrowed_closure_escape_rejects_container_element_mutate() {
    assert_owned_escape_rejected(mutate_delivery(Projection::ContainerElement, false));
}

#[test]
fn borrowed_closure_escape_rejects_container_element_mutate_cfg_alias() {
    assert_owned_escape_rejected(mutate_delivery(Projection::ContainerElement, true));
}

#[test]
fn borrowed_closure_escape_rejects_owned_capture_outer_return() {
    assert_owned_escape_rejected(owned_capture_return());
}

#[test]
fn borrowed_closure_escape_allows_root_place_local_mutate_and_cfg() {
    for cfg in [false, true] {
        let (program, _, _) = mutate_delivery(Projection::Root, cfg);
        verify_program(&program).expect("local root mutation and Drop do not escape the closure");
    }
}

#[test]
fn borrowed_closure_escape_allows_function_pointer_signature_mentions() {
    for (parameter, returned) in [(true, false), (false, true), (true, true)] {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module("pointer-signature-is-not-storage");
        let module = program.module_mut(module_id).expect("module");
        let (_, _, closure) = shared_closure_type(module);
        let pointer = module
            .add_function_pointer_type(
                if parameter { vec![closure] } else { vec![] },
                if returned { vec![closure] } else { vec![] },
            )
            .expect("signature can mention a borrowed closure without storing its environment");
        let (main, entry, parameters) =
            add_function(module, "forward", &[pointer], vec![pointer], &origin);
        module
            .function_mut(main)
            .expect("main")
            .set_terminator(
                entry,
                TerminatorKind::Return {
                    values: vec![parameters[0]],
                },
                origin,
            )
            .expect("function pointer owner is delivered exactly once");
        verify_program(&program).expect("a bare function pointer does not own its signature types");
    }
}
