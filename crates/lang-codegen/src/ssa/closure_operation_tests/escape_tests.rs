use super::*;

#[path = "delivery_tests.rs"]
mod delivery_tests;
mod projected_content_tests;
mod root_content_tests;
mod shared_retain_tests;

use crate::ssa::{
    model::SequentialContainerKind,
    verify::{VerifyError, VerifyLocation},
};

const ESCAPE_REASON: &str = "borrowed closure cannot escape through owned value delivery";

fn delivery_origin(fragment: &str) -> Origin {
    let text = "val closure = { value }\nreturn closure\nconsume(closure)\nreturn wrapper\n";
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("closure-escape-ssa.ko", text)
        .expect("test source must be unique");
    let start = text.find(fragment).expect("delivery must occur in source");
    Origin::Source(
        sources
            .span(source, start, start + fragment.len())
            .expect("delivery span must be valid"),
    )
}

fn shared_closure_type(module: &mut Module) -> (SsaTypeId, SsaTypeId, SsaTypeId) {
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let reference = module
        .add_shared_reference_type(integer)
        .expect("shared Int reference must be valid");
    let environment = module
        .add_aggregate_type("BorrowedEnv", vec![reference])
        .expect("shared capture environment must be valid");
    let closure = module
        .add_concrete_closure_type(
            "BorrowedClosure",
            vec![],
            vec![],
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Shared,
                ty: integer,
            }],
        )
        .expect("shared capture descriptor must match environment storage");
    (integer, environment, closure)
}

fn borrowed_factory_return() -> (Program, VerifyLocation, Origin) {
    let formation = origin();
    let delivery = delivery_origin("return closure");
    let mut program = Program::default();
    let module_id = program.add_module("borrowed-factory-escape");
    let module = program.module_mut(module_id).expect("module must exist");
    let (integer, environment, closure) = shared_closure_type(module);
    let (thunk, thunk_entry, _) = add_thunk(module, "thunk", environment, &[], vec![], &formation);
    module
        .function_mut(thunk)
        .expect("thunk must exist")
        .set_terminator(
            thunk_entry,
            TerminatorKind::Return { values: vec![] },
            formation.clone(),
        )
        .expect("thunk must terminate without consuming its borrowed environment");

    let factory = module
        .add_function("factory", vec![closure], formation.clone())
        .expect("factory signature must return the concrete closure");
    let function = module.function_mut(factory).expect("factory must exist");
    let entry = function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            formation.clone(),
        )
        .expect("Borrow Int entry must be valid");
    let EntityId::Loan(loan) = function.block(entry).expect("entry must exist").parameters[0]
    else {
        panic!("factory parameter must remain a shared loan");
    };
    let captured = append_values(
        function,
        entry,
        Operation::ClosureConstruct {
            closure,
            thunk,
            captures: vec![ClosureCaptureOperand::Shared(loan)],
        },
        &[closure],
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
        .expect("Return must deliver the well-typed closure");
    (program, VerifyLocation::Terminator(entry), delivery)
}

fn entry_closure_value_call() -> (Program, VerifyLocation, Origin) {
    let formation = origin();
    let delivery = delivery_origin("consume(closure)");
    let mut program = Program::default();
    let module_id = program.add_module("entry-closure-value-call");
    let module = program.module_mut(module_id).expect("module must exist");
    let (_, _, closure) = shared_closure_type(module);
    let (callee, callee_entry, parameters) =
        add_function(module, "consume", &[closure], vec![], &formation);
    let function = module.function_mut(callee).expect("callee must exist");
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
        .expect("callee must discharge its entry owner before returning");
    let (caller, entry, parameters) =
        add_function(module, "forward", &[closure], vec![], &formation);
    let function = module.function_mut(caller).expect("caller must exist");
    let (call, results) = function
        .append_instruction(
            entry,
            Operation::DirectCall {
                callee,
                receiver: None,
                arguments: vec![EntityId::Value(parameters[0])],
            },
            vec![],
            delivery.clone(),
        )
        .expect("Value call signature and result arity must be valid");
    assert!(results.is_empty());
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, formation)
        .expect("caller must return after delivering its entry owner once");
    (program, VerifyLocation::Instruction(call), delivery)
}

fn entry_container_return() -> (Program, VerifyLocation, Origin) {
    let formation = origin();
    let delivery = delivery_origin("return wrapper");
    let mut program = Program::default();
    let module_id = program.add_module("entry-container-closure-escape");
    let module = program.module_mut(module_id).expect("module must exist");
    let (_, _, closure) = shared_closure_type(module);
    let wrapper = module
        .add_sequential_container_type(SequentialContainerKind::List, closure)
        .expect("List storage must retain its concrete closure element type");
    let (function_id, entry, parameters) =
        add_function(module, "forward", &[wrapper], vec![wrapper], &formation);
    module
        .function_mut(function_id)
        .expect("function must exist")
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![parameters[0]],
            },
            delivery.clone(),
        )
        .expect("Return must match the List signature and consume its entry owner once");
    (program, VerifyLocation::Terminator(entry), delivery)
}

fn assert_owned_escape_rejected((program, location, origin): (Program, VerifyLocation, Origin)) {
    let errors = verify_program(&program)
        .expect_err("well-typed owned delivery must reject the borrowed closure escape");
    // A different type/loan/ownership error cannot stand in for the escape defense.
    assert_eq!(
        errors.errors,
        vec![VerifyError {
            kind: VerifyErrorKind::OperationContract {
                reason: ESCAPE_REASON,
            },
            location,
            origin: Some(origin),
        }]
    );
}

#[test]
fn borrowed_closure_escape_rejects_factory_return_at_delivery_origin() {
    assert_owned_escape_rejected(borrowed_factory_return());
}

#[test]
fn borrowed_closure_escape_rejects_entry_value_call_at_delivery_origin() {
    assert_owned_escape_rejected(entry_closure_value_call());
}

#[test]
fn borrowed_closure_escape_rejects_entry_container_return_at_delivery_origin() {
    assert_owned_escape_rejected(entry_container_return());
}

#[derive(Clone, Copy, Debug)]
enum CleanStorage {
    EmptyList,
    NullOwner,
    InactiveTaggedCapture,
}

#[derive(Clone, Copy, Debug)]
enum CleanDelivery {
    Direct,
    Aggregate,
    OwnedCapture,
    Cfg,
}

fn known_clean_delivery(storage: CleanStorage, delivery: CleanDelivery) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("known-clean-closure-storage");
    let module = program.module_mut(module_id).expect("module must exist");
    let (_, _, borrowed) = shared_closure_type(module);
    let mut empty_variant = None;
    let clean_type = match storage {
        CleanStorage::EmptyList => module
            .add_sequential_container_type(SequentialContainerKind::List, borrowed)
            .expect("empty List must still declare its concrete borrowed closure element"),
        CleanStorage::NullOwner => {
            let payload = module
                .add_aggregate_type("Node.payload", vec![borrowed])
                .expect("potential non-null payload must contain the borrowed closure");
            let owner = module.declare_heap_owner("Node").expect("owner type");
            module.define_heap_owner(owner, payload).expect("payload");
            module.add_nullable_handle_type(owner).expect("nullable")
        }
        CleanStorage::InactiveTaggedCapture => {
            let empty = module
                .add_aggregate_type("Choice.Empty", vec![])
                .expect("empty variant payload must be valid");
            let captured = module
                .add_aggregate_type("Choice.Captured", vec![borrowed])
                .expect("inactive variant must contain the borrowed closure");
            empty_variant = Some(empty);
            module
                .add_tagged_union_type("Choice", vec![empty, captured])
                .expect("both variant types must be valid")
        }
    };

    let mut outer_thunk = None;
    let result_type = match delivery {
        CleanDelivery::Aggregate => module
            .add_aggregate_type("Wrapper", vec![clean_type])
            .expect("wrapper must preserve the may-contain field type"),
        CleanDelivery::OwnedCapture => {
            let environment = module
                .add_aggregate_type("OwnedEnv", vec![clean_type])
                .expect("owned capture environment must be valid");
            let closure = module
                .add_concrete_closure_type(
                    "OwnedClosure",
                    vec![],
                    vec![],
                    environment,
                    vec![ClosureCaptureType {
                        mode: ClosureCaptureMode::Owned,
                        ty: clean_type,
                    }],
                )
                .expect("outer closure owns the actual known-clean value");
            let (thunk, entry, _) =
                add_thunk(module, "owned_thunk", environment, &[], vec![], &origin);
            module
                .function_mut(thunk)
                .expect("thunk")
                .set_terminator(
                    entry,
                    TerminatorKind::Return { values: vec![] },
                    origin.clone(),
                )
                .expect("thunk only borrows the owned capture environment");
            outer_thunk = Some(thunk);
            closure
        }
        CleanDelivery::Direct | CleanDelivery::Cfg => clean_type,
    };
    let (main, entry, _) = add_function(module, "main", &[], vec![result_type], &origin);
    let function = module.function_mut(main).expect("main");
    let construction = match storage {
        CleanStorage::EmptyList => Operation::ContainerConstruct {
            container: clean_type,
            elements: vec![],
        },
        CleanStorage::NullOwner => Operation::NullableNull {
            nullable: clean_type,
        },
        CleanStorage::InactiveTaggedCapture => {
            let empty = empty_variant.expect("tagged empty payload type");
            let payload = append_values(
                function,
                entry,
                Operation::AggregateConstruct {
                    aggregate: empty,
                    fields: vec![],
                },
                &[empty],
                &origin,
            )[0];
            Operation::TaggedConstruct {
                tagged: clean_type,
                variant: 0,
                payload,
            }
        }
    };
    let clean = append_values(function, entry, construction, &[clean_type], &origin)[0];
    let (return_block, returned) = match delivery {
        CleanDelivery::Direct => (entry, clean),
        CleanDelivery::Aggregate => {
            let wrapper = append_values(
                function,
                entry,
                Operation::AggregateConstruct {
                    aggregate: result_type,
                    fields: vec![clean],
                },
                &[result_type],
                &origin,
            )[0];
            (entry, wrapper)
        }
        CleanDelivery::OwnedCapture => {
            let wrapper = append_values(
                function,
                entry,
                Operation::ClosureConstruct {
                    closure: result_type,
                    thunk: outer_thunk.expect("owned closure thunk"),
                    captures: vec![ClosureCaptureOperand::Owned(clean)],
                },
                &[result_type],
                &origin,
            )[0];
            (entry, wrapper)
        }
        CleanDelivery::Cfg => {
            let target = function
                .add_block(vec![EntityType::Value(clean_type)], origin.clone())
                .expect("CFG target must preserve the concrete storage type");
            function
                .set_terminator(
                    entry,
                    TerminatorKind::Branch(Edge {
                        target,
                        arguments: vec![EntityId::Value(clean)],
                    }),
                    origin.clone(),
                )
                .expect("known-clean owner must move explicitly across the edge");
            let parameter = value(function.block(target).expect("target").parameters[0]);
            (target, parameter)
        }
    };
    function
        .set_terminator(
            return_block,
            TerminatorKind::Return {
                values: vec![returned],
            },
            origin,
        )
        .expect("Return must consume the known-clean value once");
    program
}

fn assert_known_clean_deliveries(storage: CleanStorage) {
    // A storage type that may contain a shared closure does not prove that this
    // particular empty/null/inactive value contains one, even after wrapping or moving.
    for delivery in [
        CleanDelivery::Direct,
        CleanDelivery::Aggregate,
        CleanDelivery::OwnedCapture,
        CleanDelivery::Cfg,
    ] {
        verify_program(&known_clean_delivery(storage, delivery)).unwrap_or_else(|errors| {
            panic!("known-clean {storage:?} with {delivery:?} must verify: {errors:?}")
        });
    }
}

#[test]
fn borrowed_closure_escape_allows_empty_list_and_its_owned_wrappers_and_cfg() {
    assert_known_clean_deliveries(CleanStorage::EmptyList);
}

#[test]
fn borrowed_closure_escape_allows_nullable_null_and_its_owned_wrappers_and_cfg() {
    assert_known_clean_deliveries(CleanStorage::NullOwner);
}

#[test]
fn borrowed_closure_escape_allows_inactive_tagged_and_its_owned_wrappers_and_cfg() {
    assert_known_clean_deliveries(CleanStorage::InactiveTaggedCapture);
}
