use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::source::SourceMap;

use super::{
    model::{ModelError, Origin, Ownership, Program, SsaTypeId, SsaTypeKind},
    render::render_program,
    verify::{VerifyErrorKind, VerifyLocation, verify_program},
};

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("aggregate-types.ko", "value class Pair")
        .expect("test source must be unique");
    Origin::Source(sources.span(source, 0, 5).expect("test span must be valid"))
}

#[test]
fn named_aggregate_types_derive_ownership_and_render_deterministically() {
    let mut program = Program::default();
    let module_id = program.add_module("aggregate");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let resource = module.intern_type(SsaTypeKind::Opaque {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let point = module
        .add_aggregate_type("Point", vec![integer, integer])
        .expect("copyable aggregate must be valid");
    let endpoint = module
        .add_aggregate_type("Endpoint", vec![resource])
        .expect("move-only aggregate must be valid");

    assert_eq!(module.type_ownership(point), Some(Ownership::Copyable));
    assert_eq!(module.type_ownership(endpoint), Some(Ownership::MoveOnly));
    assert_eq!(
        module.add_aggregate_type("Point", vec![integer]),
        Err(ModelError::DuplicateTypeName {
            name: "Point".to_owned()
        })
    );
    verify_program(&program).expect("derived aggregate definitions must verify");

    let first = render_program(&program);
    let second = render_program(&program);
    assert_eq!(first, second);
    assert!(first.contains("aggregate \"Point\" Copyable (!t0, !t0)"));
    assert!(first.contains("aggregate \"Endpoint\" MoveOnly (!t1)"));
}

#[test]
fn heap_owner_declaration_supports_recursive_handles_and_rejects_bad_definitions() {
    let mut program = Program::default();
    let module_id = program.add_module("heap");
    let module = program.module_mut(module_id).expect("module must exist");
    let node = module
        .declare_heap_owner("Node")
        .expect("heap owner declaration must be valid");
    let payload = module
        .add_aggregate_type("Node.payload", vec![node])
        .expect("heap handles break inline payload recursion");
    module
        .define_heap_owner(node, payload)
        .expect("self-reference is indirect through the heap-owner handle");
    assert_eq!(module.type_ownership(node), Some(Ownership::MoveOnly));
    assert_eq!(
        module.define_heap_owner(node, payload),
        Err(ModelError::TypeAlreadyDefined { ty: node })
    );

    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: false,
    });
    let undeclared = module
        .declare_heap_owner("UndeclaredPayload")
        .expect("second heap declaration must be valid");
    assert_eq!(
        module.define_heap_owner(undeclared, integer),
        Err(ModelError::ExpectedAggregate { ty: integer })
    );
    let empty_payload = module
        .add_aggregate_type("UndeclaredPayload.payload", Vec::new())
        .expect("empty payload aggregate must be valid");
    module
        .define_heap_owner(undeclared, empty_payload)
        .expect("failed definition must not poison the declaration");
    assert_eq!(
        module.define_heap_owner(integer, payload),
        Err(ModelError::ExpectedHeapOwner { ty: integer })
    );
    verify_program(&program).expect("recursive heap-owner handle must verify");
    assert!(render_program(&program).contains("heap_owner \"Node\" payload !t1"));
}

#[test]
fn boxed_enum_payload_definitions_verify_recursive_indirection() {
    let mut program = Program::default();
    let module_id = program.add_module("boxed-enum");
    let module = program.module_mut(module_id).unwrap();
    let boxed = module.declare_heap_owner("Box<Expr>").unwrap();
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let leaf = module
        .add_aggregate_type("Expr.Num", vec![integer])
        .unwrap();
    let branch = module
        .add_aggregate_type("Expr.Add", vec![boxed, boxed])
        .unwrap();
    let enumeration = module
        .add_tagged_union_type("Expr", vec![leaf, branch])
        .unwrap();
    module
        .define_heap_owner(boxed, enumeration)
        .expect("a tagged enum is a valid Box payload");
    assert_eq!(module.heap_payload(boxed), Some(enumeration));
    assert_eq!(module.type_ownership(boxed), Some(Ownership::MoveOnly));
    assert_eq!(
        module.type_ownership(enumeration),
        Some(Ownership::MoveOnly)
    );
    verify_program(&program).expect("Box handles break the enum's inline cycle");
    assert_text_eq!(render_program(&program), render_program(&program));

    let module = program.module_mut(module_id).unwrap();
    let SsaTypeKind::HeapOwner { payload, .. } = &mut module.types[boxed.index()] else {
        panic!("heap owner");
    };
    *payload = Some(integer);
    let errors =
        verify_program(&program).expect_err("forged scalar Box payload must remain invalid");
    assert!(
        errors
            .errors
            .iter()
            .any(|error| error.location == VerifyLocation::Type(boxed))
    );
}

#[test]
fn shared_owner_is_move_only_accepts_any_defined_payload_and_renders_stably() {
    let mut program = Program::default();
    let module_id = program.add_module("shared");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let owner = module
        .declare_shared_owner("Rc<Int>")
        .expect("shared owner declaration must be valid");
    module
        .define_shared_owner(owner, integer)
        .expect("a defined scalar payload must be valid");

    assert_eq!(module.type_ownership(owner), Some(Ownership::MoveOnly));
    assert_eq!(module.shared_payload(owner), Some(integer));
    assert_eq!(
        module.define_shared_owner(owner, integer),
        Err(ModelError::TypeAlreadyDefined { ty: owner })
    );
    assert_eq!(
        module.define_shared_owner(integer, integer),
        Err(ModelError::ExpectedSharedOwner { ty: integer })
    );
    verify_program(&program).expect("defined shared owner must verify");
    assert!(render_program(&program).contains("shared_owner \"Rc<Int>\" payload !t0"));
}

#[test]
fn named_type_builder_rejects_cross_module_fields_and_accepts_declared_handles() {
    let mut program = Program::default();
    let first = program.add_module("first");
    let second = program.add_module("second");
    let foreign = program
        .module_mut(second)
        .expect("second module must exist")
        .intern_type(SsaTypeKind::Boolean);
    let first_module = program.module_mut(first).expect("first module must exist");
    assert!(matches!(
        first_module.add_aggregate_type("Wrong", vec![foreign]),
        Err(ModelError::WrongTypeOwner { .. })
    ));

    let declared = first_module
        .declare_heap_owner("Declared")
        .expect("declaration must be valid");
    let inline = first_module
        .add_aggregate_type("Inline", vec![declared])
        .expect("a declared heap handle already has a fixed indirect shape");
    assert_eq!(
        first_module.type_ownership(inline),
        Some(Ownership::MoveOnly)
    );
    first_module
        .define_heap_owner(declared, inline)
        .expect("the payload must still be defined before verification");
    verify_program(&program).expect("fully defined handle and aggregate must verify");
}

#[test]
fn verifier_rejects_undefined_mismatched_and_inline_cyclic_types() {
    let mut program = Program::default();
    let module_id = program.add_module("invalid");
    let module = program.module_mut(module_id).expect("module must exist");
    let resource = module.intern_type(SsaTypeKind::Opaque {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let mismatched = SsaTypeId {
        module: module_id,
        index: module.types.len(),
    };
    module.types.push(SsaTypeKind::Aggregate {
        name: "Mismatched".to_owned(),
        fields: vec![resource],
        ownership: Ownership::Copyable,
    });
    let cyclic = SsaTypeId {
        module: module_id,
        index: module.types.len(),
    };
    module.types.push(SsaTypeKind::Aggregate {
        name: "Cyclic".to_owned(),
        fields: vec![cyclic],
        ownership: Ownership::Copyable,
    });
    let undefined = module
        .declare_heap_owner("Undefined")
        .expect("declaration itself is valid");

    let errors = verify_program(&program).expect_err("invalid type definitions must be rejected");
    assert!(errors.errors.iter().any(|error| {
        error.location == VerifyLocation::Type(mismatched)
            && matches!(
                error.kind,
                VerifyErrorKind::InvalidTypeDefinition {
                    reason: "aggregate ownership must be derived from all fields"
                }
            )
    }));
    assert!(errors.errors.iter().any(|error| {
        error.location == VerifyLocation::Type(mismatched)
            && matches!(
                error.kind,
                VerifyErrorKind::InvalidTypeDefinition {
                    reason: "named type identity must match the module name index"
                }
            )
    }));
    assert!(errors.errors.iter().any(|error| {
        error.location == VerifyLocation::Type(cyclic)
            && matches!(
                error.kind,
                VerifyErrorKind::InvalidTypeDefinition {
                    reason: "aggregate fields must not form an inline cycle"
                }
            )
    }));
    assert!(errors.errors.iter().any(|error| {
        error.location == VerifyLocation::Type(undefined)
            && matches!(
                error.kind,
                VerifyErrorKind::InvalidTypeDefinition {
                    reason: "heap owner declaration must be defined"
                }
            )
    }));
}

#[test]
fn named_types_can_be_used_in_function_signatures_after_definition() {
    let mut program = Program::default();
    let module_id = program.add_module("signature");
    let module = program.module_mut(module_id).expect("module must exist");
    let owner = module
        .declare_heap_owner("Owner")
        .expect("declaration must be valid");
    module
        .add_function("identity", vec![owner], origin())
        .expect("declarations can enter signatures before their payload is defined");
    module
        .add_aggregate_type("Owner.payload", Vec::new())
        .and_then(|payload| module.define_heap_owner(owner, payload))
        .expect("payload definition must succeed");
}

pub(in crate::ssa) mod shared_owner_operation_tests {
    use lang_frontend::source::SourceMap;

    use super::super::{
        model::{
            BlockId, EntityId, EntityType, Function, InstructionId, Operation, Origin, Ownership,
            Program, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
        },
        render::render_program,
        verify::{VerifyErrorKind, VerifyLocation, verify_program},
    };

    fn origin() -> Origin {
        let mut sources = SourceMap::default();
        let source = sources
            .add_source("shared-owner.ko", "Rc(value)")
            .expect("source");
        Origin::Source(sources.span(source, 0, 2).expect("span"))
    }

    fn value(entity: EntityId) -> ValueId {
        let EntityId::Value(value) = entity else {
            panic!("expected value");
        };
        value
    }

    fn append(
        function: &mut Function,
        block: BlockId,
        operation: Operation,
        results: Vec<EntityType>,
        origin: &Origin,
    ) -> (InstructionId, Vec<EntityId>) {
        function
            .append_instruction(block, operation, results, origin.clone())
            .expect("instruction")
    }

    fn add_function(
        program: &mut Program,
        payload_move_only: bool,
    ) -> (
        super::super::model::ModuleId,
        SsaTypeId,
        SsaTypeId,
        super::super::model::FunctionId,
        BlockId,
        ValueId,
    ) {
        let origin = origin();
        let module_id = program.add_module("shared");
        let module = program.module_mut(module_id).expect("module");
        let payload = if payload_move_only {
            module.intern_type(SsaTypeKind::Opaque {
                name: "Payload".to_owned(),
                ownership: Ownership::MoveOnly,
            })
        } else {
            module.intern_type(SsaTypeKind::Integer {
                bits: 64,
                signed: true,
            })
        };
        let owner = module.declare_shared_owner("Rc").expect("declare");
        module.define_shared_owner(owner, payload).expect("define");
        let function_id = module
            .add_function("shared", Vec::new(), origin.clone())
            .expect("function");
        let function = module.function_mut(function_id).expect("function");
        let entry = function
            .add_block(vec![EntityType::Value(payload)], origin)
            .expect("entry");
        let parameter = value(function.block(entry).expect("block").parameters[0]);
        (module_id, payload, owner, function_id, entry, parameter)
    }

    #[test]
    fn shared_allocate_retain_payload_place_and_drop_verify_and_render() {
        let origin = origin();
        let mut program = Program::default();
        let (module_id, payload, owner_ty, function_id, entry, payload_value) =
            add_function(&mut program, false);
        let function = program
            .module_mut(module_id)
            .unwrap()
            .function_mut(function_id)
            .unwrap();
        let owner = value(
            append(
                function,
                entry,
                Operation::SharedAllocate {
                    owner: owner_ty,
                    payload: payload_value,
                },
                vec![EntityType::Value(owner_ty)],
                &origin,
            )
            .1[0],
        );
        let retained = value(
            append(
                function,
                entry,
                Operation::SharedRetain {
                    owner: EntityId::Value(owner),
                },
                vec![EntityType::Value(owner_ty)],
                &origin,
            )
            .1[0],
        );
        append(
            function,
            entry,
            Operation::SharedPayloadPlace {
                owner: EntityId::Value(owner),
            },
            vec![EntityType::Place(payload)],
            &origin,
        );
        append(
            function,
            entry,
            Operation::Drop { owner: retained },
            Vec::new(),
            &origin,
        );
        append(
            function,
            entry,
            Operation::Drop { owner },
            Vec::new(),
            &origin,
        );
        function
            .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
            .unwrap();

        verify_program(&program).expect("shared owner operations must verify");
        let rendered = render_program(&program);
        assert!(rendered.contains("shared.allocate"));
        assert!(rendered.contains("shared.retain"));
        assert!(rendered.contains("shared.payload_place"));
    }

    #[test]
    fn shared_operation_type_contracts_reject_non_shared_owners() {
        let origin = origin();
        let mut program = Program::default();
        let (module_id, payload, owner_ty, function_id, entry, payload_value) =
            add_function(&mut program, false);
        let function = program
            .module_mut(module_id)
            .unwrap()
            .function_mut(function_id)
            .unwrap();
        let invalid = [
            append(
                function,
                entry,
                Operation::SharedAllocate {
                    owner: payload,
                    payload: payload_value,
                },
                vec![EntityType::Value(owner_ty)],
                &origin,
            )
            .0,
            append(
                function,
                entry,
                Operation::SharedRetain {
                    owner: EntityId::Value(payload_value),
                },
                vec![EntityType::Value(owner_ty)],
                &origin,
            )
            .0,
            append(
                function,
                entry,
                Operation::SharedPayloadPlace {
                    owner: EntityId::Value(payload_value),
                },
                vec![EntityType::Place(payload)],
                &origin,
            )
            .0,
        ];
        function
            .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
            .unwrap();
        let errors = verify_program(&program)
            .expect_err("invalid operations")
            .errors;
        for instruction in invalid {
            assert!(errors.iter().any(|error| {
                error.location == VerifyLocation::Instruction(instruction)
                    && matches!(error.kind, VerifyErrorKind::OperationContract { .. })
            }));
        }
    }

    #[test]
    fn shared_allocate_consumes_move_only_payload_but_retain_preserves_source_owner() {
        let origin = origin();
        let mut program = Program::default();
        let (module_id, _payload, owner_ty, function_id, entry, payload_value) =
            add_function(&mut program, true);
        let function = program
            .module_mut(module_id)
            .unwrap()
            .function_mut(function_id)
            .unwrap();
        let owner = value(
            append(
                function,
                entry,
                Operation::SharedAllocate {
                    owner: owner_ty,
                    payload: payload_value,
                },
                vec![EntityType::Value(owner_ty)],
                &origin,
            )
            .1[0],
        );
        let retained = value(
            append(
                function,
                entry,
                Operation::SharedRetain {
                    owner: EntityId::Value(owner),
                },
                vec![EntityType::Value(owner_ty)],
                &origin,
            )
            .1[0],
        );
        let invalid = append(
            function,
            entry,
            Operation::Drop {
                owner: payload_value,
            },
            Vec::new(),
            &origin,
        )
        .0;
        append(
            function,
            entry,
            Operation::Drop { owner: retained },
            Vec::new(),
            &origin,
        );
        append(
            function,
            entry,
            Operation::Drop { owner },
            Vec::new(),
            &origin,
        );
        function
            .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
            .unwrap();

        let errors = verify_program(&program)
            .expect_err("payload must be unavailable after allocation")
            .errors;
        assert!(errors.iter().any(|error| {
            error.location == VerifyLocation::Instruction(invalid)
                && matches!(
                    error.kind,
                    VerifyErrorKind::ValueUnavailable { value } if value == payload_value
                )
        }));
    }
}

pub(in crate::ssa) mod unit_lower_container_size_tests {
    //! SPEC-0274: source length reads preserve container ownership and borrow identity.
    use super::super::{
        model::{Definition, EntityId, EntityType, Function, LoanKind, Operation, Program},
        render::render_program,
        unit_lower::lower_owned_unit_with_entry,
        unit_lower_test_support::{analyze, declaration, parsed},
    };
    use crate::test_support::diagnostics::assert_text_eq;
    use lang_frontend::{
        name_resolution::SourceUnitInput, ownership_checking::owned_compilation_unit_view,
        source::SourceMap, type_checking::standard_environments,
    };

    #[test]
    fn unit_container_size_reads_all_three_kinds_without_consuming_parameters() {
        let program = fixture(
            r#"package p
        fun arraySize(items: Array<Int>): Int = ((items)).size
        fun listSize(items: List<Int>): Int = ((items)).size
        fun mutableSize(items: MutableList<Int>): Int = ((items)).size
        fun takeArray(own items: Array<Int>): Int {
            val first = items.size
            val second = ((items)).size
            return first + second
        }
        fun takeList(own items: List<Int>): Int {
            val first = items.size
            val second = ((items)).size
            return first + second
        }
        fun takeMutable(own items: MutableList<Int>): Int {
            val first = items.size
            val second = ((items)).size
            return first + second
        }"#,
            r#"package q
        fun entry(): Int {
            val a: Array<Int> = arrayOf()
            val a2 = arrayOf(1, 2)
            val b = listOf(1, 2)
            val b0: List<Int> = listOf()
            val c = mutableListOf(3)
            val c0: MutableList<Int> = mutableListOf()
            val length = ((a)).size + p.arraySize(a) + p.arraySize(a2) + p.listSize(b) + p.listSize(b0) + p.mutableSize(c) + p.mutableSize(c0)
            val arrayLength = p.takeArray(a)
            val arrayLength2 = p.takeArray(a2)
            val listLength = p.takeList(b)
            val listLength0 = p.takeList(b0)
            val mutableLength = p.takeMutable(c)
            val mutableLength0 = p.takeMutable(c0)
            return length + arrayLength + arrayLength2 + listLength + listLength0 + mutableLength + mutableLength0
        }"#,
        );
        for name in ["p.arraySize", "p.listSize", "p.mutableSize"] {
            let function = function(&program, name);
            let parameter = function.blocks[0].parameters[0];
            assert!(matches!(parameter, EntityId::Loan(_)));
            let lengths: Vec<_> = function
                .instructions
                .iter()
                .filter_map(|instruction| {
                    if let Operation::ContainerLength { owner } = instruction.operation {
                        Some(owner)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(lengths, [parameter], "length reads the active source loan");
            assert!(
                !function.instructions.iter().any(|instruction| matches!(
                    instruction.operation,
                    Operation::Drop { .. } | Operation::BorrowEnd { .. }
                )),
                "the callee must preserve its caller's container"
            );
        }
        for name in ["p.takeArray", "p.takeList", "p.takeMutable"] {
            let take = function(&program, name);
            assert_eq!(
                take.instructions
                    .iter()
                    .filter(|instruction| matches!(
                        instruction.operation,
                        Operation::ContainerLength { .. }
                    ))
                    .count(),
                2,
                "both source reads precede the unique owner cleanup"
            );
            assert_eq!(
                take.instructions
                    .iter()
                    .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
                    .count(),
                1,
                "Value parameter still has exactly one cleanup obligation"
            );
        }
    }

    #[test]
    fn unit_container_size_temporary_receiver_is_evaluated_once_and_cleanup_is_after_read() {
        let program = fixture(
            r#"package p
        fun sourceArray(): Array<Int> = arrayOf(1, 2)
        fun sourceList(): List<Int> = listOf(1, 2)
        fun sourceMutable(): MutableList<Int> = mutableListOf(1, 2)
        fun emptyArray(): Array<Int> = arrayOf()
        fun emptyList(): List<Int> = listOf()
        fun emptyMutable(): MutableList<Int> = mutableListOf()"#,
            r#"package q
        fun entry(): Int {
            val a = ((p.sourceArray())).size
            val b = ((p.sourceList())).size
            val c = ((p.sourceMutable())).size
            val d = ((p.emptyArray())).size
            val e = ((p.emptyList())).size
            val f = ((p.emptyMutable())).size
            return a + b + c + d + e + f
        }"#,
        );
        let entry = function(&program, "q.entry");
        let calls: Vec<_> = entry
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, instruction)| {
                matches!(instruction.operation, Operation::DirectCall { .. })
            })
            .collect();
        assert_eq!(
            calls.len(),
            6,
            "grouped receivers cannot duplicate source evaluation"
        );
        let lengths: Vec<_> = entry
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, instruction)| {
                matches!(instruction.operation, Operation::ContainerLength { .. })
            })
            .collect();
        let drops: Vec<_> = entry
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, instruction)| matches!(instruction.operation, Operation::Drop { .. }))
            .collect();
        assert_eq!(lengths.len(), 6);
        assert_eq!(
            drops.len(),
            6,
            "each temporary backing owner must be cleaned up"
        );
        for ((call, length), drop) in calls.iter().zip(&lengths).zip(&drops) {
            assert!(
                call.0 < length.0 && length.0 < drop.0,
                "each read completes before its receiver cleanup"
            );
            let EntityId::Value(owner) = call.1.results[0] else {
                panic!("source returns an owner")
            };
            assert!(
                matches!(drop.1.operation, Operation::Drop { owner: dropped } if dropped == owner),
                "cleanup consumes exactly the owner produced by this source call"
            );
        }
    }

    #[test]
    fn unit_container_size_borrow_uses_current_loan_after_cfg() {
        let program = fixture(
            r#"package p
        fun arraySize(items: Array<Int>, own flag: Boolean): Int {
            if (flag) { println("true") } else { println("false") }
            return ((items)).size
        }
        fun listSize(items: List<Int>, own flag: Boolean): Int {
            if (flag) { println("true") } else { println("false") }
            return ((items)).size
        }
        fun mutableSize(items: MutableList<Int>, own flag: Boolean): Int {
            if (flag) { println("true") } else { println("false") }
            return ((items)).size
        }"#,
            r#"package q
        fun entry(): Int {
            val a = arrayOf(1, 2)
            val b = listOf(1, 2)
            val c = mutableListOf(1, 2)
            val first = p.arraySize(a, true) + p.arraySize(a, false)
            val second = p.listSize(b, true) + p.listSize(b, false)
            val third = p.mutableSize(c, true) + p.mutableSize(c, false)
            return first + second + third + a[0] + b[0] + c[0]
        }"#,
        );
        for name in ["p.arraySize", "p.listSize", "p.mutableSize"] {
            let function = function(&program, name);
            let owner = function
                .instructions
                .iter()
                .find_map(|instruction| match instruction.operation {
                    Operation::ContainerLength { owner } => Some(owner),
                    _ => None,
                })
                .expect("size emits a header read");
            assert_ne!(
                owner, function.blocks[0].parameters[0],
                "size must use the loan transported through the conditional"
            );
            let EntityId::Loan(loan) = owner else {
                panic!("size uses Borrow loan")
            };
            let entity = function.entity(owner).unwrap();
            assert_eq!(
                entity.ty,
                function
                    .entity(function.blocks[0].parameters[0])
                    .unwrap()
                    .ty,
                "CFG preserves the concrete container target"
            );
            assert!(matches!(
                entity.ty,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    ..
                }
            ));
            assert!(
                matches!(entity.definition, Definition::BlockParameter { block, .. }
            if block != function.blocks[0].id)
            );
            assert!(
                !function.instructions.iter().any(|instruction| matches!(
                    instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan
                )),
                "header read does not end the incoming container loan"
            );
        }
    }

    fn function<'a>(program: &'a Program, name: &str) -> &'a Function {
        program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.contains(name))
            .unwrap_or_else(|| panic!("reachable function {name}"))
    }

    fn fixture(provider: &str, consumer: &str) -> Program {
        let mut sources = SourceMap::new();
        let (p_source, p) = parsed(&mut sources, "p/provider.ko", provider);
        let (q_source, q) = parsed(&mut sources, "q/consumer.ko", consumer);
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", p_source, &p),
            SourceUnitInput::new("root", "q/consumer.ko", q_source, &q),
        ];
        let reversed = [inputs[1], inputs[0]];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let entry = declaration(&names, "q", "entry");
        let unit =
            owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
                .expect("sealed Unit view");
        let (program, _) = lower_owned_unit_with_entry(&unit, entry)
            .expect("source size is a checked read of an existing container");
        let unit =
            owned_compilation_unit_view(&sources, &reversed, &names, &environment, &typed, &owned)
                .expect("sealed Unit view");
        let (permuted, _) = lower_owned_unit_with_entry(&unit, entry)
            .expect("source identity must survive input permutation");
        assert_text_eq!(render_program(&program), render_program(&permuted));
        assert_text_eq!(
            crate::llvm::render_verified_program(&program).unwrap(),
            crate::llvm::render_verified_program(&permuted).unwrap()
        );
        program
    }
}
