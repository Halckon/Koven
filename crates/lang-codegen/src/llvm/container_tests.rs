use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    BlockId, EntityId, EntityType, Function, Module, Operation, Origin, Program,
    SequentialContainerKind, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
};

use super::render_verified_program;

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

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value, got {entity:?}");
    };
    value
}

fn ret(function: &mut Function, block: BlockId, values: Vec<ValueId>, origin: &Origin) {
    function
        .set_terminator(block, TerminatorKind::Return { values }, origin.clone())
        .expect("return must be settable");
}

#[test]
fn fixed_headers_and_list_construction_use_one_checked_continuous_allocation() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("container-layout");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
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
            owner: parameters[0],
        },
        integer,
        &origin,
    );
    ret(function, entry, vec![parameters[0]], &origin);

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
    assert!(first.contains("call { i64, i1 } @llvm.umul.with.overflow.i64(i64 2, i64 16)"));
    assert!(first.contains("getelementptr inbounds %koven.t1, ptr"));
    assert!(first.contains("extractvalue %koven.container.t2 %v0, 1"));
    assert!(first.contains("call void @abort()"));
    assert!(first.contains("unreachable"));
    assert!(!first.contains(" = alloca "));
    assert!(!first.contains("invoke"));
}

#[test]
fn runtime_length_generation_calls_initializer_in_an_explicit_loop() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("container-generate");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
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
    assert!(llvm.contains("call i64 @f0.identity"));
    assert!(llvm.contains("icmp slt i64 %v0, 0"));
    assert!(llvm.contains("icmp ult i64 %v1.index, %v0"));
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
    let (make, entry, _) = add_function(module, "make", &[], vec![list], &origin);
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
    ret(function, entry, vec![owner], &origin);

    let llvm = render_verified_program(&program).expect("ZST container LLVM must verify");
    assert!(llvm.contains("@koven.zst.sentinel = private constant i8 0"));
    assert!(!llvm.contains("call ptr @malloc"));
    assert!(!llvm.contains("getelementptr"));
    assert!(!llvm.contains("store %koven.t0"));
}
