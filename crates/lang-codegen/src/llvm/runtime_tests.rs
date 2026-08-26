use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    Edge, EntityId, EntityType, LoanKind, Operation, Origin, PlaceAccess, Program, SsaTypeKind,
    TerminatorKind,
};

use super::render_verified_program;

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("heap-runtime.ko", "class Owner")
        .expect("test source must be unique");
    Origin::Source(sources.span(source, 0, 5).expect("test span must be valid"))
}

#[test]
fn heap_owner_uses_checked_system_allocation_and_recursive_unique_drop_glue() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("heap_runtime");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let leaf_owner = module
        .declare_heap_owner("Leaf")
        .expect("leaf owner declaration must be valid");
    let leaf_payload = module
        .add_aggregate_type("Leaf.payload", Vec::new())
        .expect("empty payload must be valid");
    module
        .define_heap_owner(leaf_owner, leaf_payload)
        .expect("leaf owner definition must be valid");
    let pair = module
        .add_aggregate_type("LeafPair", vec![leaf_owner, leaf_owner])
        .expect("move-only pair must be valid");
    let outer_payload = module
        .add_aggregate_type("Outer.payload", vec![integer, pair])
        .expect("outer payload must be valid");
    let outer_owner = module
        .declare_heap_owner("Outer")
        .expect("outer owner declaration must be valid");
    module
        .define_heap_owner(outer_owner, outer_payload)
        .expect("outer owner definition must be valid");

    let function_id = module
        .add_function("exercise", vec![integer], origin.clone())
        .expect("function signature must be valid");
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let entry = function
        .add_block(Vec::new(), origin.clone())
        .expect("entry block must be valid");

    let initial = value(
        function
            .append_instruction(
                entry,
                Operation::Constant(crate::ssa::model::ScalarConstant::Integer(7)),
                vec![EntityType::Value(integer)],
                origin.clone(),
            )
            .expect("integer constant must be appendable")
            .1[0],
    );
    let mut leaves = Vec::new();
    for _ in 0..2 {
        let payload = value(
            function
                .append_instruction(
                    entry,
                    Operation::AggregateConstruct {
                        aggregate: leaf_payload,
                        fields: Vec::new(),
                    },
                    vec![EntityType::Value(leaf_payload)],
                    origin.clone(),
                )
                .expect("empty payload construction must be appendable")
                .1[0],
        );
        leaves.push(value(
            function
                .append_instruction(
                    entry,
                    Operation::HeapAllocate {
                        owner: leaf_owner,
                        payload,
                    },
                    vec![EntityType::Value(leaf_owner)],
                    origin.clone(),
                )
                .expect("leaf allocation must be appendable")
                .1[0],
        ));
    }
    let pair_value = value(
        function
            .append_instruction(
                entry,
                Operation::AggregateConstruct {
                    aggregate: pair,
                    fields: leaves,
                },
                vec![EntityType::Value(pair)],
                origin.clone(),
            )
            .expect("pair construction must be appendable")
            .1[0],
    );
    let payload = value(
        function
            .append_instruction(
                entry,
                Operation::AggregateConstruct {
                    aggregate: outer_payload,
                    fields: vec![initial, pair_value],
                },
                vec![EntityType::Value(outer_payload)],
                origin.clone(),
            )
            .expect("outer payload construction must be appendable")
            .1[0],
    );
    let owner = value(
        function
            .append_instruction(
                entry,
                Operation::HeapAllocate {
                    owner: outer_owner,
                    payload,
                },
                vec![EntityType::Value(outer_owner)],
                origin.clone(),
            )
            .expect("outer allocation must be appendable")
            .1[0],
    );
    let payload_place = place(
        function
            .append_instruction(
                entry,
                Operation::HeapPayloadPlace { owner },
                vec![EntityType::Place(outer_payload)],
                origin.clone(),
            )
            .expect("payload place must be appendable")
            .1[0],
    );
    let integer_place = place(
        function
            .append_instruction(
                entry,
                Operation::FieldPlace {
                    base: payload_place,
                    field: 0,
                },
                vec![EntityType::Place(integer)],
                origin.clone(),
            )
            .expect("field place must be appendable")
            .1[0],
    );
    let loan = loan(
        function
            .append_instruction(
                entry,
                Operation::BorrowBegin {
                    place: integer_place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: integer,
                }],
                origin.clone(),
            )
            .expect("shared loan must be appendable")
            .1[0],
    );
    let read = value(
        function
            .append_instruction(
                entry,
                Operation::Read {
                    source: PlaceAccess::Loan(loan),
                },
                vec![EntityType::Value(integer)],
                origin.clone(),
            )
            .expect("loan read must be appendable")
            .1[0],
    );
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("borrow end must be appendable");
    let replacement = value(
        function
            .append_instruction(
                entry,
                Operation::Constant(crate::ssa::model::ScalarConstant::Integer(9)),
                vec![EntityType::Value(integer)],
                origin.clone(),
            )
            .expect("replacement must be appendable")
            .1[0],
    );
    function
        .append_instruction(
            entry,
            Operation::Mutate {
                place: integer_place,
                value: replacement,
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("field mutation must be appendable");
    function
        .append_instruction(entry, Operation::Drop { owner }, Vec::new(), origin.clone())
        .expect("owner drop must be appendable");
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![read] }, origin)
        .expect("return must be valid");

    let first = render_verified_program(&program).expect("heap runtime lowering must succeed");
    let second = render_verified_program(&program).expect("repeat lowering must succeed");
    assert_eq!(first, second);

    assert_eq!(first.matches("call ptr @malloc(i64 1)").count(), 2);
    assert!(first.contains("declare ptr @malloc(i64)"));
    assert!(first.contains("declare void @abort() #0"));
    assert!(first.contains("attributes #0 = { noreturn }"));
    assert_eq!(first.matches("call void @abort()").count(), 3);
    assert_eq!(first.matches("unreachable").count(), 3);
    assert!(!first.contains("invoke "));
    assert!(!first.contains("landingpad"));
    assert!(!first.contains("resume "));

    assert_eq!(first.matches("call void @koven.drop.t1").count(), 2);
    assert_eq!(first.matches("call void @koven.drop.t3").count(), 1);
    assert_eq!(first.matches("call void @koven.drop.t4").count(), 1);
    assert_eq!(first.matches("call void @koven.drop.t5").count(), 1);
    assert_eq!(first.matches("call void @free").count(), 2);
    assert!(first.contains("getelementptr inbounds nuw %koven.t4"));
    assert!(first.contains("load i32"));
    assert!(first.contains("store i32 9"));
}

#[test]
fn shared_owner_uses_non_atomic_checked_retain_and_release_to_zero_drop() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("shared_runtime");
    let module = program.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let owner_type = module.declare_shared_owner("Rc<Int>").expect("declare");
    module
        .define_shared_owner(owner_type, integer)
        .expect("define");
    let function_id = module
        .add_function("shared", vec![integer], origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(Vec::new(), origin.clone())
        .expect("entry");
    let payload = value(
        function
            .append_instruction(
                entry,
                Operation::Constant(crate::ssa::model::ScalarConstant::Integer(7)),
                vec![EntityType::Value(integer)],
                origin.clone(),
            )
            .unwrap()
            .1[0],
    );
    let owner = value(
        function
            .append_instruction(
                entry,
                Operation::SharedAllocate {
                    owner: owner_type,
                    payload,
                },
                vec![EntityType::Value(owner_type)],
                origin.clone(),
            )
            .unwrap()
            .1[0],
    );
    let retained = value(
        function
            .append_instruction(
                entry,
                Operation::SharedRetain {
                    owner: EntityId::Value(owner),
                },
                vec![EntityType::Value(owner_type)],
                origin.clone(),
            )
            .unwrap()
            .1[0],
    );
    let payload_place = place(
        function
            .append_instruction(
                entry,
                Operation::SharedPayloadPlace {
                    owner: EntityId::Value(retained),
                },
                vec![EntityType::Place(integer)],
                origin.clone(),
            )
            .unwrap()
            .1[0],
    );
    let read = value(
        function
            .append_instruction(
                entry,
                Operation::Read {
                    source: PlaceAccess::Place(payload_place),
                },
                vec![EntityType::Value(integer)],
                origin.clone(),
            )
            .unwrap()
            .1[0],
    );
    function
        .append_instruction(entry, Operation::Drop { owner }, Vec::new(), origin.clone())
        .unwrap();
    function
        .append_instruction(
            entry,
            Operation::Drop { owner: retained },
            Vec::new(),
            origin.clone(),
        )
        .unwrap();
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![read] }, origin)
        .unwrap();

    let llvm = render_verified_program(&program).expect("shared runtime lowering");
    assert!(llvm.contains("%koven.shared.t1 = type { i64, i64 }"));
    assert!(llvm.contains("store i64 1"));
    assert!(llvm.contains("icmp eq i64"));
    assert!(llvm.contains("add i64"));
    assert!(llvm.contains("sub i64"));
    assert!(llvm.contains("strong.last"));
    assert!(llvm.contains("call void @free"));
    assert!(!llvm.contains("atomicrmw"));
    assert!(!llvm.contains("cmpxchg"));
    assert!(!llvm.contains("invoke "));
}

#[test]
fn nested_shared_owner_with_zst_payload_recursively_releases_each_control_block() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("nested_shared_runtime");
    let module = program.module_mut(module_id).expect("module");
    let zst = module
        .add_aggregate_type("Empty", Vec::new())
        .expect("empty aggregate");
    let inner_type = module.declare_shared_owner("Rc<Empty>").expect("inner");
    module
        .define_shared_owner(inner_type, zst)
        .expect("inner definition");
    let outer_type = module.declare_shared_owner("Rc<Rc<Empty>>").expect("outer");
    module
        .define_shared_owner(outer_type, inner_type)
        .expect("outer definition");
    let function_id = module
        .add_function("nested", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).unwrap();
    let entry = function.add_block(Vec::new(), origin.clone()).unwrap();
    let empty = value(
        function
            .append_instruction(
                entry,
                Operation::AggregateConstruct {
                    aggregate: zst,
                    fields: Vec::new(),
                },
                vec![EntityType::Value(zst)],
                origin.clone(),
            )
            .unwrap()
            .1[0],
    );
    let inner = value(
        function
            .append_instruction(
                entry,
                Operation::SharedAllocate {
                    owner: inner_type,
                    payload: empty,
                },
                vec![EntityType::Value(inner_type)],
                origin.clone(),
            )
            .unwrap()
            .1[0],
    );
    let outer = value(
        function
            .append_instruction(
                entry,
                Operation::SharedAllocate {
                    owner: outer_type,
                    payload: inner,
                },
                vec![EntityType::Value(outer_type)],
                origin.clone(),
            )
            .unwrap()
            .1[0],
    );
    function
        .append_instruction(
            entry,
            Operation::Drop { owner: outer },
            Vec::new(),
            origin.clone(),
        )
        .unwrap();
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .unwrap();

    let llvm = render_verified_program(&program).expect("nested shared lowering");
    assert_eq!(llvm.matches("call void @free").count(), 2);
    assert!(llvm.contains("call void @koven.drop.t1"));
    assert!(llvm.contains("call void @koven.drop.t2"));
}

#[test]
fn recursive_heap_type_predeclares_finite_self_recursive_drop_glue() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("recursive_drop");
    let module = program.module_mut(module_id).expect("module must exist");
    let node = module
        .declare_heap_owner("Node")
        .expect("node declaration must be valid");
    let payload = module
        .add_aggregate_type("Node.payload", vec![node])
        .expect("recursive payload must be valid");
    module
        .define_heap_owner(node, payload)
        .expect("node definition must be valid");
    let function_id = module
        .add_function("drop_node", Vec::new(), origin.clone())
        .expect("function must be valid");
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let entry = function
        .add_block(vec![EntityType::Value(node)], origin.clone())
        .expect("entry block must be valid");
    let owner = value(function.block(entry).expect("entry must exist").parameters[0]);
    function
        .append_instruction(entry, Operation::Drop { owner }, Vec::new(), origin.clone())
        .expect("node drop must be appendable");
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return must be valid");

    let llvm = render_verified_program(&program).expect("recursive drop glue must stay finite");
    assert!(llvm.contains("define internal void @koven.drop.t0(ptr %0)"));
    assert!(llvm.contains("define internal void @koven.drop.t1(%koven.t1 %0)"));
    assert_eq!(llvm.matches("call void @koven.drop.t0").count(), 2);
    assert_eq!(llvm.matches("call void @koven.drop.t1").count(), 1);
    assert_eq!(llvm.matches("call void @free").count(), 1);
}

#[test]
fn allocation_success_block_is_the_real_phi_predecessor() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("allocation_phi");
    let module = program.module_mut(module_id).expect("module must exist");
    let payload = module
        .add_aggregate_type("Owner.payload", Vec::new())
        .expect("empty payload must be valid");
    let owner = module
        .declare_heap_owner("Owner")
        .expect("owner declaration must be valid");
    module
        .define_heap_owner(owner, payload)
        .expect("owner definition must be valid");
    let function_id = module
        .add_function("allocate_then_join", Vec::new(), origin.clone())
        .expect("function must be valid");
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let entry = function
        .add_block(Vec::new(), origin.clone())
        .expect("entry block must be valid");
    let join = function
        .add_block(vec![EntityType::Value(owner)], origin.clone())
        .expect("join block must be valid");
    let payload_value = value(
        function
            .append_instruction(
                entry,
                Operation::AggregateConstruct {
                    aggregate: payload,
                    fields: Vec::new(),
                },
                vec![EntityType::Value(payload)],
                origin.clone(),
            )
            .expect("payload construction must be appendable")
            .1[0],
    );
    let allocated = value(
        function
            .append_instruction(
                entry,
                Operation::HeapAllocate {
                    owner,
                    payload: payload_value,
                },
                vec![EntityType::Value(owner)],
                origin.clone(),
            )
            .expect("allocation must be appendable")
            .1[0],
    );
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: vec![EntityId::Value(allocated)],
            }),
            origin.clone(),
        )
        .expect("entry branch must be valid");
    let joined = value(function.block(join).expect("join must exist").parameters[0]);
    function
        .append_instruction(
            join,
            Operation::Drop { owner: joined },
            Vec::new(),
            origin.clone(),
        )
        .expect("joined owner drop must be appendable");
    function
        .set_terminator(join, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("join return must be valid");

    let llvm = render_verified_program(&program).expect("allocation CFG must verify");
    assert!(llvm.contains("%v0 = phi ptr [ %v2, %v2.initialized ]"));
    assert!(!llvm.contains("%v0 = phi ptr [ %v2, %bb0 ]"));
}

fn value(entity: EntityId) -> crate::ssa::model::ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value, got {entity:?}");
    };
    value
}

fn place(entity: EntityId) -> crate::ssa::model::PlaceId {
    let EntityId::Place(place) = entity else {
        panic!("expected place, got {entity:?}");
    };
    place
}

fn loan(entity: EntityId) -> crate::ssa::model::LoanId {
    let EntityId::Loan(loan) = entity else {
        panic!("expected loan, got {entity:?}");
    };
    loan
}
