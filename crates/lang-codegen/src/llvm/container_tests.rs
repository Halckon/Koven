use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    BlockId, Edge, EntityId, EntityType, Function, LoanKind, Module, Operation, Origin, Ownership,
    Program, SequentialContainerKind, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
};
use crate::ssa::provider;
use crate::ssa::verify::verify_program;

use super::{
    container::{checked_list_length, max_logical_length},
    render_verified_program,
};

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("container-codegen.ko", "fun containers() = 0")
        .expect("test source must be unique");
    Origin::Source(sources.span(source, 0, 3).expect("test span must be valid"))
}

fn add_function(
    module: &mut Module,
    name: &str,
    parameters: &[SsaTypeId],
    returns: Vec<SsaTypeId>,
    origin: &Origin,
) -> (crate::ssa::model::FunctionId, BlockId, Vec<ValueId>) {
    let id = module
        .add_function(name, returns, origin.clone())
        .expect("function signature must be valid");
    let function = module.function_mut(id).expect("function must exist");
    let entry = function
        .add_block(
            parameters.iter().copied().map(EntityType::Value).collect(),
            origin.clone(),
        )
        .expect("entry block must be valid");
    let parameters = function
        .block(entry)
        .expect("entry block must exist")
        .parameters
        .iter()
        .copied()
        .map(value)
        .collect();
    (id, entry, parameters)
}

fn append_value(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    ty: SsaTypeId,
    origin: &Origin,
) -> ValueId {
    let results = function
        .append_instruction(
            block,
            operation,
            vec![EntityType::Value(ty)],
            origin.clone(),
        )
        .expect("instruction must append")
        .1;
    let [EntityId::Value(value)] = results.as_slice() else {
        panic!("expected one value result, got {results:?}");
    };
    *value
}

fn append_place(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    ty: SsaTypeId,
    origin: &Origin,
) -> crate::ssa::model::PlaceId {
    let results = function
        .append_instruction(
            block,
            operation,
            vec![EntityType::Place(ty)],
            origin.clone(),
        )
        .expect("place instruction must append")
        .1;
    let [EntityId::Place(place)] = results.as_slice() else {
        panic!("expected one place result, got {results:?}");
    };
    *place
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value, got {entity:?}");
    };
    value
}

fn loan(entity: EntityId) -> crate::ssa::model::LoanId {
    let EntityId::Loan(loan) = entity else {
        panic!("expected loan, got {entity:?}");
    };
    loan
}

fn ret(function: &mut Function, block: BlockId, values: Vec<ValueId>, origin: &Origin) {
    function
        .set_terminator(block, TerminatorKind::Return { values }, origin.clone())
        .expect("return must be settable");
}

#[test]
fn borrowed_provider_cfg_lowers_length_once_and_cleanup_exits() {
    #[derive(Clone, Copy)]
    enum ElementCase {
        Int,
        Empty,
        MoveOnlyToken,
    }
    for (label, case) in [
        ("provider-int", ElementCase::Int),
        ("provider-empty", ElementCase::Empty),
        ("provider-token", ElementCase::MoveOnlyToken),
    ] {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module(label);
        let module = program.module_mut(module_id).expect("module exists");
        let integer = module.intern_type(SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        });
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let element = match case {
            ElementCase::Int => integer,
            ElementCase::Empty => module
                .add_aggregate_type("Empty", Vec::new())
                .expect("empty aggregate"),
            ElementCase::MoveOnlyToken => module.intern_type(SsaTypeKind::ZeroSized {
                name: "Token".to_owned(),
                ownership: Ownership::MoveOnly,
            }),
        };
        let array = module
            .add_sequential_container_type(SequentialContainerKind::Array, element)
            .expect("Array<Element>");
        let (iterate, entry, parameters) =
            add_function(module, "iterate", &[array, boolean], vec![array], &origin);
        let function = module.function_mut(iterate).expect("iterate exists");
        let source_type = EntityType::Loan {
            kind: LoanKind::Shared,
            target: array,
        };
        let state_types = vec![
            EntityType::Value(array),
            source_type,
            EntityType::Value(integer),
            EntityType::Value(integer),
            EntityType::Value(boolean),
        ];
        let header = function
            .add_block(state_types.clone(), origin.clone())
            .expect("header");
        let body = function
            .add_block(state_types, origin.clone())
            .expect("body");
        let exit = function
            .add_block(vec![EntityType::Value(array), source_type], origin.clone())
            .expect("exit");
        let early_return = if matches!(case, ElementCase::MoveOnlyToken) {
            Some(
                function
                    .add_block(vec![EntityType::Value(array), source_type], origin.clone())
                    .expect("early return"),
            )
        } else {
            None
        };
        let source_place = append_place(
            function,
            entry,
            Operation::RootPlace {
                owner: parameters[0],
            },
            array,
            &origin,
        );
        let source_loan = loan(
            function
                .append_instruction(
                    entry,
                    Operation::BorrowBegin {
                        place: source_place,
                        kind: LoanKind::Shared,
                    },
                    vec![source_type],
                    origin.clone(),
                )
                .expect("source loan")
                .1[0],
        );
        let snapshot = provider::snapshot(function, entry, source_loan, integer, &origin)
            .expect("provider snapshot");
        let (snapshot_length, snapshot_cursor) = (snapshot.length(), snapshot.cursor());
        let provider_header = provider::enter_header(
            function,
            entry,
            header,
            snapshot,
            vec![
                EntityId::Value(parameters[0]),
                EntityId::Loan(source_loan),
                EntityId::Value(snapshot_cursor),
                EntityId::Value(snapshot_length),
                EntityId::Value(parameters[1]),
            ],
            1,
            2,
            3,
            &origin,
        )
        .expect("provider entry fixes length/cursor slots");

        let header_params = &function.block(header).expect("header").parameters;
        let (header_owner, header_loan, header_length, header_cursor, header_stop) = (
            value(header_params[0]),
            loan(header_params[1]),
            value(header_params[2]),
            value(header_params[3]),
            value(header_params[4]),
        );
        let element = provider::guard_and_begin(
            function,
            &provider_header,
            boolean,
            element,
            Edge {
                target: body,
                arguments: vec![
                    EntityId::Value(header_owner),
                    EntityId::Loan(header_loan),
                    EntityId::Value(header_length),
                    EntityId::Value(header_cursor),
                    EntityId::Value(header_stop),
                ],
            },
            Edge {
                target: exit,
                arguments: vec![EntityId::Value(header_owner), EntityId::Loan(header_loan)],
            },
            &origin,
        )
        .expect("guarded provider element");

        let body_params = &function.block(body).expect("body").parameters;
        let (body_owner, body_loan, body_length, body_cursor, body_stop) = (
            value(body_params[0]),
            loan(body_params[1]),
            value(body_params[2]),
            value(body_params[3]),
            value(body_params[4]),
        );
        let next = provider::finish_element_and_advance(function, element, integer, &origin)
            .expect("next cursor");
        let next_value = next.value();
        function
            .set_terminator(
                body,
                TerminatorKind::Conditional {
                    condition: body_stop,
                    when_true: Edge {
                        target: early_return.unwrap_or(exit),
                        arguments: vec![EntityId::Value(body_owner), EntityId::Loan(body_loan)],
                    },
                    when_false: provider_header
                        .backedge(
                            vec![
                                EntityId::Value(body_owner),
                                EntityId::Loan(body_loan),
                                EntityId::Value(next_value),
                                EntityId::Value(body_length),
                                EntityId::Value(body_stop),
                            ],
                            next,
                        )
                        .expect("provider backedge fixes length/cursor slots"),
                },
                origin.clone(),
            )
            .expect("body branch");

        let exit_params = &function.block(exit).expect("exit").parameters;
        let exit_owner = value(exit_params[0]);
        let exit_loan = loan(exit_params[1]);
        function
            .append_instruction(
                exit,
                Operation::BorrowEnd { loan: exit_loan },
                Vec::new(),
                origin.clone(),
            )
            .expect("source loan end");
        ret(function, exit, vec![exit_owner], &origin);
        if let Some(early_return) = early_return {
            let params = &function
                .block(early_return)
                .expect("early return")
                .parameters;
            let (owner, source_loan) = (value(params[0]), loan(params[1]));
            function
                .append_instruction(
                    early_return,
                    Operation::BorrowEnd { loan: source_loan },
                    Vec::new(),
                    origin.clone(),
                )
                .expect("early return source loan end");
            ret(function, early_return, vec![owner], &origin);
        }

        let entry_ops = function
            .block(entry)
            .expect("entry")
            .instructions
            .iter()
            .map(|id| &function.instruction(*id).expect("instruction").operation)
            .collect::<Vec<_>>();
        assert_eq!(
            entry_ops
                .iter()
                .filter(|op| matches!(op, Operation::ContainerLength { .. }))
                .count(),
            1
        );
        assert!(entry_ops.iter().any(|op| matches!(
            op,
            Operation::Constant(crate::ssa::model::ScalarConstant::Integer(0))
        )));
        assert!(entry_ops.iter().any(|op| matches!(
            op,
            Operation::Constant(crate::ssa::model::ScalarConstant::Integer(1))
        )));
        let TerminatorKind::Branch(entry_edge) = &function
            .block(entry)
            .expect("entry")
            .terminator
            .as_ref()
            .expect("entry branch")
            .kind
        else {
            panic!("provider must enter header once");
        };
        assert_eq!(entry_edge.target, header);
        assert_eq!(entry_edge.arguments[2], EntityId::Value(snapshot_length));
        assert_eq!(entry_edge.arguments[3], EntityId::Value(snapshot_cursor));
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &function
            .block(header)
            .expect("header")
            .terminator
            .as_ref()
            .expect("guard")
            .kind
        else {
            panic!("provider header must guard its body");
        };
        assert_eq!(when_true.target, body);
        assert_eq!(when_false.target, exit);
        let header_ops = function
            .block(header)
            .expect("header")
            .instructions
            .iter()
            .map(|id| &function.instruction(*id).expect("instruction").operation)
            .collect::<Vec<_>>();
        assert!(matches!(
            header_ops.as_slice(),
            [Operation::Compare {
                operator: crate::ssa::model::ComparisonOperator::LessThan,
                left,
                right,
            }] if *left == header_cursor && *right == header_length
        ));
        let body_ops = function
            .block(body)
            .expect("body")
            .instructions
            .iter()
            .map(|id| &function.instruction(*id).expect("instruction").operation)
            .collect::<Vec<_>>();
        assert!(matches!(
            body_ops[0],
            Operation::ContainerElementPlace { owner, index }
                if *owner == EntityId::Loan(body_loan) && *index == body_cursor
        ));
        assert!(matches!(body_ops[1], Operation::BorrowBegin { .. }));
        assert!(matches!(body_ops[2], Operation::BorrowEnd { .. }));
        assert!(matches!(
            body_ops[3],
            Operation::Binary {
                operator: crate::ssa::model::BinaryOperator::Add,
                left,
                right,
            } if *left == body_cursor && *right == provider_header.step()
        ));
        let TerminatorKind::Conditional { when_false, .. } = &function
            .block(body)
            .expect("body")
            .terminator
            .as_ref()
            .expect("body branch")
            .kind
        else {
            panic!("provider body must branch");
        };
        assert_eq!(when_false.target, header);
        assert_eq!(when_false.arguments[2], EntityId::Value(header_length));
        assert_eq!(when_false.arguments[3], EntityId::Value(next_value));

        verify_program(&program).expect("provider CFG must satisfy SSA ownership");
        let first = render_verified_program(&program).expect("provider LLVM must verify");
        let second =
            render_verified_program(&program).expect("provider LLVM must be deterministic");
        assert_eq!(first, second);
        let entry_ir = first
            .split("bb0:")
            .nth(1)
            .expect("entry block")
            .split("bb1:")
            .next()
            .expect("entry end");
        assert_eq!(entry_ir.matches("extractvalue %koven.container").count(), 1);
        let exit_header = first
            .lines()
            .find(|line| line.starts_with("bb3:"))
            .expect("exit block");
        assert!(exit_header.contains("%bb1"));
        match case {
            ElementCase::MoveOnlyToken => {
                assert!(!exit_header.contains("%p1.valid"));
                assert_eq!(first.matches("ret %koven.container").count(), 2);
            }
            ElementCase::Int | ElementCase::Empty => {
                assert!(exit_header.contains("%p1.valid"));
                assert_eq!(first.matches("ret %koven.container").count(), 1);
            }
        }
        assert!(first.contains("icmp slt i32"), "{first}");
        assert!(first.contains("icmp sge i32"), "{first}");
        match case {
            ElementCase::Int => assert!(first.contains("getelementptr i32, ptr"), "{first}"),
            ElementCase::Empty | ElementCase::MoveOnlyToken => {
                assert!(!first.contains("getelementptr"), "{first}");
            }
        }
        assert!(!first.contains("call ptr @malloc"), "{first}");
        assert!(!first.contains("@koven.iterator"), "{first}");
    }
}

#[test]
fn fixed_headers_and_list_construction_use_one_checked_continuous_allocation() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("container-layout");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let point = module
        .add_aggregate_type("Point", vec![integer, integer])
        .expect("Point must be valid");
    let array = module
        .add_sequential_container_type(SequentialContainerKind::Array, point)
        .expect("Array<Point> must be valid");
    let mutable = module
        .add_sequential_container_type(SequentialContainerKind::MutableList, integer)
        .expect("MutableList<Int> must be valid");

    let (make, entry, parameters) = add_function(
        module,
        "make",
        &[integer, integer, integer, integer],
        vec![array],
        &origin,
    );
    let function = module.function_mut(make).expect("make must exist");
    let first = append_value(
        function,
        entry,
        Operation::AggregateConstruct {
            aggregate: point,
            fields: parameters[..2].to_vec(),
        },
        point,
        &origin,
    );
    let second = append_value(
        function,
        entry,
        Operation::AggregateConstruct {
            aggregate: point,
            fields: parameters[2..].to_vec(),
        },
        point,
        &origin,
    );
    let owner = append_value(
        function,
        entry,
        Operation::ContainerConstruct {
            container: array,
            elements: vec![first, second],
        },
        array,
        &origin,
    );
    ret(function, entry, vec![owner], &origin);

    let (length, entry, parameters) =
        add_function(module, "length", &[array], vec![array], &origin);
    let function = module.function_mut(length).expect("length must exist");
    append_value(
        function,
        entry,
        Operation::ContainerLength {
            owner: EntityId::Value(parameters[0]),
        },
        integer,
        &origin,
    );
    ret(function, entry, vec![parameters[0]], &origin);

    let (borrowed_length, entry, parameters) =
        add_function(module, "borrowed_length", &[array], vec![array], &origin);
    let function = module
        .function_mut(borrowed_length)
        .expect("borrowed length must exist");
    let owner = parameters[0];
    let place = append_place(
        function,
        entry,
        Operation::RootPlace { owner },
        array,
        &origin,
    );
    let loan = function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: array,
            }],
            origin.clone(),
        )
        .expect("borrow must append")
        .1;
    let [EntityId::Loan(loan)] = loan.as_slice() else {
        panic!("expected a loan result");
    };
    append_value(
        function,
        entry,
        Operation::ContainerLength {
            owner: EntityId::Loan(*loan),
        },
        integer,
        &origin,
    );
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: *loan },
            vec![],
            origin.clone(),
        )
        .expect("loan end must append");
    ret(function, entry, vec![owner], &origin);

    let (keep_mutable, entry, parameters) =
        add_function(module, "keep_mutable", &[mutable], vec![mutable], &origin);
    ret(
        module
            .function_mut(keep_mutable)
            .expect("keep_mutable must exist"),
        entry,
        vec![parameters[0]],
        &origin,
    );

    let first = render_verified_program(&program).expect("container LLVM must verify");
    let second = render_verified_program(&program).expect("container LLVM must be deterministic");
    assert_eq!(first, second);
    assert!(first.contains("%koven.container.t2 = type { ptr, i64 }"));
    assert!(first.contains("%koven.container.t3 = type { ptr, i64, i64 }"));
    assert_eq!(first.matches("call ptr @malloc").count(), 1);
    assert!(first.contains("call { i64, i1 } @llvm.umul.with.overflow.i64(i64 2, i64 8)"));
    assert!(first.contains("getelementptr %koven.t1, ptr"));
    assert!(!first.contains("getelementptr inbounds %koven.t1, ptr"));
    assert!(first.contains("extractvalue %koven.container.t2 %v0, 1"));
    assert!(first.contains("trunc i64"));
    assert!(first.contains("= load %koven.container.t2, ptr"));
    assert!(first.contains("call void @abort()"));
    assert!(first.contains("unreachable"));
    let make_ir = first
        .split("@f0.make")
        .nth(1)
        .expect("make function must be rendered")
        .split("\n}")
        .next()
        .expect("make body must be rendered");
    assert!(!make_ir.contains(" = alloca "));
    assert!(!first.contains("invoke"));
}

#[test]
fn list_form_length_guard_prevents_invalid_header_lengths() {
    assert_eq!(max_logical_length(16), u16::MAX as u64);
    assert_eq!(max_logical_length(31), i32::MAX as u64);
    assert_eq!(max_logical_length(64), i32::MAX as u64);
    assert_eq!(
        checked_list_length(i32::MAX as usize, 64).expect("Int maximum fits"),
        i32::MAX as u64
    );
    assert!(checked_list_length(i32::MAX as usize + 1, 64).is_err());
    assert_eq!(
        checked_list_length(u16::MAX as usize, 16).expect("target maximum fits"),
        u16::MAX as u64
    );
    assert!(checked_list_length(u16::MAX as usize + 1, 16).is_err());
    assert_eq!(
        checked_list_length(0, 128).expect("wide target cannot overflow"),
        0
    );
}

#[test]
fn runtime_length_generation_calls_initializer_in_an_explicit_loop() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("container-generate");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, integer)
        .expect("List<Int> must be valid");
    let (initializer, entry, parameters) =
        add_function(module, "identity", &[integer], vec![integer], &origin);
    ret(
        module
            .function_mut(initializer)
            .expect("initializer must exist"),
        entry,
        vec![parameters[0]],
        &origin,
    );
    let (make, entry, parameters) = add_function(module, "make", &[integer], vec![list], &origin);
    let function = module.function_mut(make).expect("make must exist");
    let owner = append_value(
        function,
        entry,
        Operation::ContainerGenerate {
            container: list,
            length: parameters[0],
            initializer,
        },
        list,
        &origin,
    );
    ret(function, entry, vec![owner], &origin);

    let llvm = render_verified_program(&program).expect("generated container LLVM must verify");
    assert!(llvm.contains("v1.loop:"));
    assert!(llvm.contains("v1.body:"));
    assert!(llvm.contains("v1.done:"));
    assert!(llvm.contains("call i32 @f0.identity"));
    assert!(llvm.contains("icmp slt i32 %v0, 0"));
    assert!(llvm.contains("zext i32 %v0 to i64"));
    assert!(!llvm.contains("icmp slt i64"), "size_t is unsigned: {llvm}");
    assert!(llvm.contains("icmp ult i64 %v1.index, %v1.size"));
    assert_eq!(llvm.matches("call ptr @malloc").count(), 1);
    assert!(!llvm.contains("landingpad"));
    assert!(!llvm.contains("resume"));
}

#[test]
fn zst_container_uses_aligned_sentinel_without_allocation_or_addressing() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("container-zst");
    let module = program.module_mut(module_id).expect("module must exist");
    let empty = module
        .add_aggregate_type("Empty", Vec::new())
        .expect("empty aggregate must be valid");
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, empty)
        .expect("List<Empty> must be valid");
    let index_type = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let (make, entry, parameters) =
        add_function(module, "make", &[index_type], vec![list], &origin);
    let function = module.function_mut(make).expect("make must exist");
    let first = append_value(
        function,
        entry,
        Operation::AggregateConstruct {
            aggregate: empty,
            fields: Vec::new(),
        },
        empty,
        &origin,
    );
    let second = append_value(
        function,
        entry,
        Operation::AggregateConstruct {
            aggregate: empty,
            fields: Vec::new(),
        },
        empty,
        &origin,
    );
    let owner = append_value(
        function,
        entry,
        Operation::ContainerConstruct {
            container: list,
            elements: vec![first, second],
        },
        list,
        &origin,
    );
    let place = append_place(
        function,
        entry,
        Operation::ContainerElementPlace {
            owner: EntityId::Value(owner),
            index: parameters[0],
        },
        empty,
        &origin,
    );
    append_value(
        function,
        entry,
        Operation::Read {
            source: crate::ssa::model::PlaceAccess::Place(place),
        },
        empty,
        &origin,
    );
    ret(function, entry, vec![owner], &origin);

    let llvm = render_verified_program(&program).expect("ZST container LLVM must verify");
    assert!(llvm.contains("@koven.zst.sentinel = private constant i8 0"));
    assert!(!llvm.contains("call ptr @malloc"));
    assert!(!llvm.contains("getelementptr"));
    assert!(!llvm.contains("store %koven.t0"));
    assert!(llvm.contains("icmp sge i32"), "{llvm}");
    assert!(llvm.contains("call void @abort()"));
}

#[test]
fn checked_element_place_aborts_before_address_formation() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("container-index");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let array = module
        .add_sequential_container_type(SequentialContainerKind::Array, integer)
        .expect("Array<Int> must be valid");
    let index = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let (read, entry, parameters) =
        add_function(module, "read", &[array, index], vec![array], &origin);
    let function = module.function_mut(read).expect("read must exist");
    let place = append_place(
        function,
        entry,
        Operation::ContainerElementPlace {
            owner: EntityId::Value(parameters[0]),
            index: parameters[1],
        },
        integer,
        &origin,
    );
    append_value(
        function,
        entry,
        Operation::Read {
            source: crate::ssa::model::PlaceAccess::Place(place),
        },
        integer,
        &origin,
    );
    ret(function, entry, vec![parameters[0]], &origin);

    let llvm = render_verified_program(&program).expect("checked index LLVM must verify");
    let negative = llvm.find("icmp slt i32 %v1, 0").expect("negative check");
    let upper = llvm.find("icmp sge i32 %v1").expect("upper-bound check");
    let branch = llvm.find("br i1 %p0.invalid").expect("abort branch");
    let convert = llvm
        .find("zext i32 %v1 to i64")
        .expect("checked target index");
    let gep = llvm.find("getelementptr i64").expect("element address");
    assert!(negative < upper && upper < branch && branch < convert && convert < gep);
    assert!(llvm.contains("call void @abort()"));
    assert!(!llvm.contains("call ptr @malloc"));
}

#[test]
fn replacement_commits_new_owner_before_dropping_the_old_element() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("container-replace");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let payload = module
        .add_aggregate_type("Resource.payload", vec![integer])
        .expect("payload must be valid");
    let resource = module
        .declare_heap_owner("Resource")
        .expect("owner declaration must be valid");
    module
        .define_heap_owner(resource, payload)
        .expect("owner definition must be valid");
    let array = module
        .add_sequential_container_type(SequentialContainerKind::Array, resource)
        .expect("Array<Resource> must be valid");
    let index = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let (replace, entry, parameters) = add_function(
        module,
        "replace",
        &[array, resource, index],
        vec![array],
        &origin,
    );
    let function = module.function_mut(replace).expect("replace must exist");
    function
        .append_instruction(
            entry,
            Operation::ContainerReplace {
                owner: parameters[0],
                index: parameters[2],
                value: parameters[1],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("replace must append");
    ret(function, entry, vec![parameters[0]], &origin);

    let llvm = render_verified_program(&program).expect("replacement LLVM must verify");
    let body = llvm
        .split("define internal %koven.container.t3 @f0.replace")
        .nth(1)
        .expect("replace body")
        .split("}\n")
        .next()
        .expect("replace body end");
    let load = body.find("load ptr").expect("old owner load");
    let store = body.find("store ptr %v1").expect("new owner store");
    let drop = body
        .find("call void @koven.drop.t2")
        .expect("old owner drop");
    assert!(load < store && store < drop);
}

#[test]
fn container_drop_walks_move_only_elements_in_reverse_and_frees_once() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("container-drop");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let payload = module
        .add_aggregate_type("Resource.payload", vec![integer])
        .expect("payload must be valid");
    let resource = module
        .declare_heap_owner("Resource")
        .expect("owner declaration must be valid");
    module
        .define_heap_owner(resource, payload)
        .expect("owner definition must be valid");
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, resource)
        .expect("List<Resource> must be valid");
    let (drop_list, entry, parameters) =
        add_function(module, "drop_list", &[list], Vec::new(), &origin);
    let function = module
        .function_mut(drop_list)
        .expect("drop_list must exist");
    function
        .append_instruction(
            entry,
            Operation::Drop {
                owner: parameters[0],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("drop must append");
    ret(function, entry, Vec::new(), &origin);

    let llvm = render_verified_program(&program).expect("container drop LLVM must verify");
    let helper = llvm
        .split("define internal void @koven.drop.t3")
        .nth(1)
        .expect("container drop helper")
        .split("}\n")
        .next()
        .expect("container drop helper end");
    assert!(helper.contains("drop.loop:"));
    assert!(helper.contains("%drop.index = sub i64 %remaining, 1"));
    assert!(helper.contains("getelementptr ptr"));
    assert!(helper.contains("call void @koven.drop.t2"));
    assert!(helper.contains("icmp ne i64 %length, 0"));
    assert_eq!(helper.matches("call void @free").count(), 1);
}

#[test]
fn copyable_element_drop_skips_element_loop_but_releases_non_empty_buffer() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("copyable-container-drop");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, integer)
        .expect("List<Int> must be valid");
    let (drop_list, entry, parameters) =
        add_function(module, "drop_list", &[list], Vec::new(), &origin);
    let function = module
        .function_mut(drop_list)
        .expect("drop_list must exist");
    function
        .append_instruction(
            entry,
            Operation::Drop {
                owner: parameters[0],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("drop must append");
    ret(function, entry, Vec::new(), &origin);

    let llvm = render_verified_program(&program).expect("copyable drop LLVM must verify");
    let helper = llvm
        .split("define internal void @koven.drop.t1")
        .nth(1)
        .expect("container drop helper")
        .split("}\n")
        .next()
        .expect("container drop helper end");
    assert!(!helper.contains("drop.loop:"));
    assert_eq!(helper.matches("call void @free").count(), 1);
}

#[test]
fn move_only_zst_drop_preserves_logical_count_without_address_or_free() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("zst-container-drop");
    let module = program.module_mut(module_id).expect("module must exist");
    let token = module.intern_type(SsaTypeKind::ZeroSized {
        name: "Token".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, token)
        .expect("List<Token> must be valid");
    let (drop_list, entry, parameters) =
        add_function(module, "drop_list", &[list], Vec::new(), &origin);
    let function = module
        .function_mut(drop_list)
        .expect("drop_list must exist");
    function
        .append_instruction(
            entry,
            Operation::Drop {
                owner: parameters[0],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("drop must append");
    ret(function, entry, Vec::new(), &origin);

    let llvm = render_verified_program(&program).expect("MoveOnly ZST drop LLVM must verify");
    let helper = llvm
        .split("define internal void @koven.drop.t1")
        .nth(1)
        .expect("container drop helper")
        .split("}\n")
        .next()
        .expect("container drop helper end");
    assert!(llvm.contains("%koven.zst.t0 = type {}"));
    assert!(helper.contains("drop.loop:"));
    assert!(helper.contains("call void @koven.drop.t0(%koven.zst.t0 zeroinitializer)"));
    assert!(!helper.contains("getelementptr"));
    assert!(!helper.contains("load %koven.zst.t0"));
    assert!(!helper.contains("call void @free"));
}
