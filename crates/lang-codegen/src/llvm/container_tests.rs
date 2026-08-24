use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    BlockId, EntityId, EntityType, Function, Module, Operation, Origin, Ownership, Program,
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
    let (read, entry, parameters) =
        add_function(module, "read", &[array, integer], vec![array], &origin);
    let function = module.function_mut(read).expect("read must exist");
    let place = append_place(
        function,
        entry,
        Operation::ContainerElementPlace {
            owner: parameters[0],
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
    let negative = llvm.find("icmp slt i64 %v1, 0").expect("negative check");
    let upper = llvm.find("icmp uge i64 %v1").expect("upper-bound check");
    let branch = llvm.find("br i1 %p0.invalid").expect("abort branch");
    let gep = llvm
        .find("getelementptr inbounds i64")
        .expect("element address");
    assert!(negative < upper && upper < branch && branch < gep);
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
    let (replace, entry, parameters) = add_function(
        module,
        "replace",
        &[array, resource, integer],
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
        .split("define %koven.container.t3 @f0.replace")
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
    assert!(helper.contains("getelementptr inbounds ptr"));
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
