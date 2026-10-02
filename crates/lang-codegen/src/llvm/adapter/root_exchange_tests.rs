use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    BlockId, EntityId, EntityType, Function, LoanKind, Module, Origin, Program,
    SequentialContainerKind, SsaTypeId, SsaTypeKind, TerminatorKind,
};

use super::*;

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("root-exchange.ko", "replace(&root, new)")
        .expect("source");
    Origin::Source(sources.span(source, 0, 7).expect("span"))
}

#[derive(Clone, Copy, Debug)]
enum StorageCase {
    Boolean,
    Char,
    Signed,
    Unsigned,
    Aggregate,
    EmptyAggregate,
    OwnedAggregate,
    Tagged,
    OwnedTagged,
    Heap,
    Shared,
    String,
    NullableHeap,
    NullableShared,
    Array,
    List,
    MutableList,
    ZeroSizedCopy,
    ZeroSizedOwner,
}

const STORAGE_CASES: &[StorageCase] = &[
    StorageCase::Boolean,
    StorageCase::Char,
    StorageCase::Signed,
    StorageCase::Unsigned,
    StorageCase::Aggregate,
    StorageCase::EmptyAggregate,
    StorageCase::OwnedAggregate,
    StorageCase::Tagged,
    StorageCase::OwnedTagged,
    StorageCase::Heap,
    StorageCase::Shared,
    StorageCase::String,
    StorageCase::NullableHeap,
    StorageCase::NullableShared,
    StorageCase::Array,
    StorageCase::List,
    StorageCase::MutableList,
    StorageCase::ZeroSizedCopy,
    StorageCase::ZeroSizedOwner,
];

fn storage_type(module: &mut Module, case: StorageCase) -> SsaTypeId {
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    match case {
        StorageCase::Boolean => module.intern_type(SsaTypeKind::Boolean),
        StorageCase::Char => module.intern_type(SsaTypeKind::Char),
        StorageCase::Signed => integer,
        StorageCase::Unsigned => module.intern_type(SsaTypeKind::Integer {
            bits: 64,
            signed: false,
        }),
        StorageCase::Aggregate => module
            .add_aggregate_type("Pair", vec![integer, integer])
            .expect("aggregate"),
        StorageCase::EmptyAggregate => module
            .add_aggregate_type("Empty", Vec::new())
            .expect("empty aggregate"),
        StorageCase::OwnedAggregate => {
            let string = module.add_string_owner_type();
            module
                .add_aggregate_type("OwnedPair", vec![string, integer])
                .expect("owned aggregate")
        }
        StorageCase::Tagged | StorageCase::OwnedTagged => {
            let payload = if matches!(case, StorageCase::OwnedTagged) {
                module.add_string_owner_type()
            } else {
                integer
            };
            let present = module
                .add_aggregate_type("Present", vec![payload])
                .expect("present payload");
            let absent = module
                .add_aggregate_type("Absent", Vec::new())
                .expect("absent payload");
            module
                .add_tagged_union_type("State", vec![absent, present])
                .expect("tagged union")
        }
        StorageCase::Heap | StorageCase::NullableHeap => {
            let payload = module
                .add_aggregate_type("Heap.payload", vec![integer])
                .expect("heap payload");
            let owner = module.declare_heap_owner("Heap").expect("heap owner");
            module
                .define_heap_owner(owner, payload)
                .expect("heap definition");
            if matches!(case, StorageCase::NullableHeap) {
                module
                    .add_nullable_handle_type(owner)
                    .expect("nullable heap")
            } else {
                owner
            }
        }
        StorageCase::Shared | StorageCase::NullableShared => {
            let owner = module.declare_shared_owner("Shared").expect("shared owner");
            module
                .define_shared_owner(owner, integer)
                .expect("shared definition");
            if matches!(case, StorageCase::NullableShared) {
                module
                    .add_nullable_handle_type(owner)
                    .expect("nullable shared")
            } else {
                owner
            }
        }
        StorageCase::String => module.add_string_owner_type(),
        StorageCase::Array | StorageCase::List | StorageCase::MutableList => {
            let kind = match case {
                StorageCase::Array => SequentialContainerKind::Array,
                StorageCase::List => SequentialContainerKind::List,
                _ => SequentialContainerKind::MutableList,
            };
            module
                .add_sequential_container_type(kind, integer)
                .expect("container")
        }
        StorageCase::ZeroSizedCopy | StorageCase::ZeroSizedOwner => {
            module.intern_type(SsaTypeKind::ZeroSized {
                name: "Token".to_owned(),
                ownership: if matches!(case, StorageCase::ZeroSizedOwner) {
                    Ownership::MoveOnly
                } else {
                    Ownership::Copyable
                },
            })
        }
    }
}

fn append(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    results: Vec<EntityType>,
    origin: &Origin,
) -> Vec<EntityId> {
    function
        .append_instruction(block, operation, results, origin.clone())
        .expect("instruction")
        .1
}

fn root_loan(
    function: &mut Function,
    block: BlockId,
    owner: ValueId,
    ty: SsaTypeId,
    origin: &Origin,
) -> (PlaceId, LoanId) {
    let EntityId::Place(place) = append(
        function,
        block,
        Operation::RootPlace { owner },
        vec![EntityType::Place(ty)],
        origin,
    )[0] else {
        panic!("expected root place");
    };
    let EntityId::Loan(loan) = append(
        function,
        block,
        Operation::BorrowBegin {
            place,
            kind: LoanKind::Exclusive,
        },
        vec![EntityType::Loan {
            kind: LoanKind::Exclusive,
            target: ty,
        }],
        origin,
    )[0] else {
        panic!("expected exclusive loan");
    };
    (place, loan)
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value");
    };
    value
}

struct Fixture {
    program: Program,
    inputs: [ValueId; 2],
    places: Vec<PlaceId>,
    loans: Vec<LoanId>,
    results: [ValueId; 2],
}

fn fixture(case: StorageCase, swap: bool) -> Fixture {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("root_exchange");
    let module = program.module_mut(module_id).expect("module");
    let ty = storage_type(module, case);
    let pair = module
        .add_aggregate_type("Results", vec![ty, ty])
        .expect("results");
    let id = module
        .add_function("exchange", vec![pair], origin.clone())
        .expect("function");
    let function = module.function_mut(id).expect("function");
    let entry = function
        .add_block(vec![EntityType::Value(ty); 2], origin.clone())
        .expect("entry");
    let parameters = &function.block(entry).expect("entry").parameters;
    let inputs = [value(parameters[0]), value(parameters[1])];
    let (place, loan) = root_loan(function, entry, inputs[0], ty, &origin);
    let mut places = vec![place];
    let mut loans = vec![loan];
    let operation = if swap {
        let (place, loan) = root_loan(function, entry, inputs[1], ty, &origin);
        places.push(place);
        loans.push(loan);
        Operation::RootSwap {
            owners: inputs,
            loans: [loans[0], loans[1]],
        }
    } else {
        Operation::RootReplace {
            owner: inputs[0],
            loan,
            replacement: inputs[1],
        }
    };
    let outputs = append(
        function,
        entry,
        operation,
        vec![EntityType::Value(ty); 2],
        &origin,
    );
    let results = [value(outputs[0]), value(outputs[1])];
    let returned = value(
        append(
            function,
            entry,
            Operation::AggregateConstruct {
                aggregate: pair,
                fields: results.to_vec(),
            },
            vec![EntityType::Value(pair)],
            &origin,
        )[0],
    );
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![returned],
            },
            origin,
        )
        .expect("return");
    Fixture {
        program,
        inputs,
        places,
        loans,
        results,
    }
}

fn function_body(ir: &str) -> &str {
    ir.split("@f0.exchange(")
        .nth(1)
        .expect("exchange function")
        .split_once('{')
        .expect("function body")
        .1
        .split_once("\n}")
        .expect("end of body")
        .0
}

#[test]
fn root_replace_moves_all_native_storage_without_runtime_calls() {
    for &case in STORAGE_CASES {
        let fixture = fixture(case, false);
        let ir = render_verified_program(&fixture.program, None)
            .unwrap_or_else(|error| panic!("{case:?}: {error:?}"));
        let body = function_body(&ir);
        let memory: Vec<_> = body
            .lines()
            .filter(|line| line.contains("load ") || line.contains("store "))
            .collect();
        assert_eq!(memory.len(), 3, "{case:?}: {body}");
        assert!(memory[0].contains("store "), "{case:?}: {body}");
        assert!(
            memory[1].contains(&format!("%v{} = load ", fixture.results[1].index())),
            "{case:?}: {body}"
        );
        assert!(
            memory[1].contains(&format!("ptr %p{}", fixture.places[0].index())),
            "{case:?}: {body}"
        );
        assert!(
            memory[2].contains(&format!(
                "%v{}, ptr %p{}",
                fixture.inputs[1].index(),
                fixture.places[0].index()
            )),
            "{case:?}: {body}"
        );
        assert!(
            body.contains(&format!("%v{}, 0", fixture.inputs[1].index())),
            "new root must be replacement: {case:?}: {body}"
        );
        assert!(
            body.contains(&format!("%v{}, 1", fixture.results[1].index())),
            "detached old must come from load: {case:?}: {body}"
        );
        assert!(
            !body.contains("call "),
            "exchange must not drop, clone, retain or allocate: {case:?}: {body}"
        );
    }
}

#[test]
fn root_swap_loads_both_roots_before_either_store() {
    for &case in STORAGE_CASES {
        let fixture = fixture(case, true);
        let ir = render_verified_program(&fixture.program, None)
            .unwrap_or_else(|error| panic!("{case:?}: {error:?}"));
        let body = function_body(&ir);
        let memory: Vec<_> = body
            .lines()
            .filter(|line| line.contains("load ") || line.contains("store "))
            .collect();
        assert_eq!(memory.len(), 6, "{case:?}: {body}");
        for line in &memory[..2] {
            assert!(line.contains("store "), "{case:?}: {body}");
        }
        for (position, result) in [fixture.results[1], fixture.results[0]].iter().enumerate() {
            assert!(
                memory[position + 2].contains(&format!("%v{} = load ", result.index())),
                "{case:?}: {body}"
            );
            assert!(
                memory[position + 2]
                    .contains(&format!("ptr %p{}", fixture.places[position].index())),
                "{case:?}: {body}"
            );
        }
        for (position, result) in fixture.results.iter().enumerate() {
            assert!(
                memory[position + 4].contains(&format!(
                    "%v{}, ptr %p{}",
                    result.index(),
                    fixture.places[position].index()
                )),
                "{case:?}: {body}"
            );
            assert!(
                body.contains(&format!("%v{}, {position}", result.index())),
                "result must match replacement: {case:?}: {body}"
            );
        }
        assert!(
            !body.contains("call "),
            "exchange must not drop, clone, retain or allocate: {case:?}: {body}"
        );
    }
}

#[test]
fn root_exchange_consumes_pointer_and_zero_sized_loan_maps() {
    for swap in [false, true] {
        let fixture = fixture(StorageCase::ZeroSizedOwner, swap);
        let module = &fixture.program.modules[0];
        let context = Context::create();
        let llvm = context.create_module("loan_maps");
        let (_, machine) = super::super::native_target_machine().expect("native target");
        let mut module_lowerer = ModuleLowerer::new(
            &context,
            &llvm,
            module,
            &machine.get_target_data(),
            None,
            None,
        )
        .expect("module lowerer");
        module_lowerer
            .declare_functions()
            .expect("function declarations");
        let function = &module.functions[0];
        let llvm_function = module_lowerer.functions[&function.id];
        let dependencies = FunctionDependencies {
            type_map: &module_lowerer.type_map,
            runtime: &module_lowerer.runtime,
            functions: &module_lowerer.functions,
            debug: None,
        };
        let mut lowerer = FunctionLowerer::new(
            &context,
            &llvm,
            module,
            function,
            llvm_function,
            dependencies,
        );
        lowerer.create_blocks_and_parameters().expect("blocks");
        lowerer
            .builder
            .position_at_end(lowerer.block(function.blocks[0].id).expect("entry"));
        for id in &function.blocks[0].instructions {
            let instruction = function.instruction(*id).expect("instruction");
            if matches!(
                instruction.operation,
                Operation::RootReplace { .. } | Operation::RootSwap { .. }
            ) {
                for (&loan, &owner) in fixture.loans.iter().zip(&fixture.inputs) {
                    lowerer
                        .zero_sized_loans
                        .insert(loan, lowerer.value(owner).expect("owner"));
                }
            }
            lowerer
                .lower_instruction(instruction)
                .expect("instruction lowering");
        }
        assert!(
            lowerer.loans.is_empty(),
            "exchange consumed every pointer loan"
        );
        assert!(
            lowerer.zero_sized_loans.is_empty(),
            "exchange consumed every zero-sized loan"
        );
        lowerer
            .lower_terminator(
                &function.blocks[0]
                    .terminator
                    .as_ref()
                    .expect("terminator")
                    .kind,
            )
            .expect("return lowering");
        llvm.verify().expect("valid LLVM");
    }
}

#[test]
fn root_exchange_unit_locals_use_zero_sized_storage_and_keep_void_function_abi() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("unit_roots");
    let module = program.module_mut(module_id).expect("module");
    let unit = module.intern_type(SsaTypeKind::Unit);
    let id = module
        .add_function("exchange", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(id).expect("function");
    let entry = function
        .add_block(Vec::new(), origin.clone())
        .expect("entry");
    let mut constants = Vec::new();
    for _ in 0..2 {
        constants.push(value(
            append(
                function,
                entry,
                Operation::Constant(ScalarConstant::Unit),
                vec![EntityType::Value(unit)],
                &origin,
            )[0],
        ));
    }
    let (_, loan) = root_loan(function, entry, constants[0], unit, &origin);
    let outputs = append(
        function,
        entry,
        Operation::RootReplace {
            owner: constants[0],
            loan,
            replacement: constants[1],
        },
        vec![EntityType::Value(unit); 2],
        &origin,
    );
    let copied = value(
        append(
            function,
            entry,
            Operation::Copy {
                source: value(outputs[0]),
            },
            vec![EntityType::Value(unit)],
            &origin,
        )[0],
    );
    let (_, loan) = root_loan(function, entry, copied, unit, &origin);
    let read = append(
        function,
        entry,
        Operation::Read {
            source: PlaceAccess::Loan(loan),
        },
        vec![EntityType::Value(unit)],
        &origin,
    )[0];
    append(
        function,
        entry,
        Operation::BorrowEnd { loan },
        Vec::new(),
        &origin,
    );
    let join = function
        .add_block(vec![EntityType::Value(unit); 2], origin.clone())
        .expect("join");
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: vec![read, outputs[1]],
            }),
            origin.clone(),
        )
        .expect("branch");
    let parameters = &function.block(join).expect("join").parameters;
    let owners = [value(parameters[0]), value(parameters[1])];
    let (_, first) = root_loan(function, join, owners[0], unit, &origin);
    let (_, second) = root_loan(function, join, owners[1], unit, &origin);
    append(
        function,
        join,
        Operation::RootSwap {
            owners,
            loans: [first, second],
        },
        vec![EntityType::Value(unit); 2],
        &origin,
    );
    function
        .set_terminator(join, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");

    let ir = render_verified_program(&program, None).expect("Unit root exchange");
    assert!(ir.contains("define internal void @f0.exchange()"), "{ir}");
    let body = function_body(&ir);
    assert_eq!(body.matches("alloca {}").count(), 4, "{body}");
    assert_eq!(body.matches("load {}, ptr").count(), 4, "{body}");
    assert_eq!(body.matches("phi {}").count(), 2, "{body}");
    assert!(body.contains("ret void"), "{body}");
    assert!(!body.contains("call "), "{body}");
}

#[test]
fn field_exchange_moves_all_native_storage_without_drop_or_runtime_calls() {
    for &case in STORAGE_CASES {
        for across_cfg in [false, true] {
            let origin = origin();
            let mut program = Program::default();
            let module_id = program.add_module("field_exchange");
            let module = program.module_mut(module_id).unwrap();
            let ty = storage_type(module, case);
            let payload = module
                .add_aggregate_type("Parent.payload", vec![ty, ty])
                .unwrap();
            let parent = module.declare_heap_owner("Parent").unwrap();
            module.define_heap_owner(parent, payload).unwrap();
            let returned = module
                .add_aggregate_type("Result", vec![parent, ty])
                .unwrap();
            let id = module
                .add_function("exchange", vec![returned], origin.clone())
                .unwrap();
            let function = module.function_mut(id).unwrap();
            let entry = function
                .add_block(
                    vec![EntityType::Value(parent), EntityType::Value(ty)],
                    origin.clone(),
                )
                .unwrap();
            let args = function.block(entry).unwrap().parameters.clone();
            let owner = value(args[0]);
            let replacement = value(args[1]);
            let EntityId::Place(payload_place) = append(
                function,
                entry,
                Operation::HeapPayloadPlace { owner },
                vec![EntityType::Place(payload)],
                &origin,
            )[0] else {
                panic!("payload")
            };
            let EntityId::Place(field_place) = append(
                function,
                entry,
                Operation::FieldPlace {
                    base: payload_place,
                    field: 1,
                },
                vec![EntityType::Place(ty)],
                &origin,
            )[0] else {
                panic!("field")
            };
            let EntityId::Loan(loan) = append(
                function,
                entry,
                Operation::BorrowBegin {
                    place: field_place,
                    kind: LoanKind::Exclusive,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: ty,
                }],
                &origin,
            )[0] else {
                panic!("loan")
            };
            let (entry, owner, loan, replacement) = if across_cfg {
                let next = function
                    .add_block(
                        vec![
                            EntityType::Value(parent),
                            EntityType::Loan {
                                kind: LoanKind::Exclusive,
                                target: ty,
                            },
                            EntityType::Value(ty),
                        ],
                        origin.clone(),
                    )
                    .unwrap();
                function
                    .set_terminator(
                        entry,
                        TerminatorKind::Branch(Edge {
                            target: next,
                            arguments: vec![
                                EntityId::Value(owner),
                                EntityId::Loan(loan),
                                EntityId::Value(replacement),
                            ],
                        }),
                        origin.clone(),
                    )
                    .unwrap();
                let args = &function.block(next).unwrap().parameters;
                let EntityId::Loan(loan) = args[1] else {
                    panic!("transported loan")
                };
                (next, value(args[0]), loan, value(args[2]))
            } else {
                (entry, owner, loan, replacement)
            };
            let old = value(
                append(
                    function,
                    entry,
                    Operation::HeapFieldExchange {
                        owner,
                        field: 1,
                        loan,
                        replacement,
                    },
                    vec![EntityType::Value(ty)],
                    &origin,
                )[0],
            );
            let result = value(
                append(
                    function,
                    entry,
                    Operation::AggregateConstruct {
                        aggregate: returned,
                        fields: vec![owner, old],
                    },
                    vec![EntityType::Value(returned)],
                    &origin,
                )[0],
            );
            function
                .set_terminator(
                    entry,
                    TerminatorKind::Return {
                        values: vec![result],
                    },
                    origin,
                )
                .unwrap();
            let ir = render_verified_program(&program, None)
                .unwrap_or_else(|error| panic!("{case:?}: {error:?}"));
            let body = function_body(&ir);
            let memory = body
                .lines()
                .filter(|line| line.contains("load ") || line.contains("store "))
                .collect::<Vec<_>>();
            assert_eq!(memory.len(), 2, "{case:?}: {body}");
            assert!(
                memory[0].contains(&format!("%v{} = load ", old.index())),
                "{case:?}: {body}"
            );
            assert!(
                memory[1].contains(&format!("%v{}, ptr ", replacement.index(),)),
                "{case:?}: {body}"
            );
            assert!(!body.contains("call "), "{case:?}: {body}");
            assert!(!body.contains("atomic"), "{case:?}: {body}");
            assert!(!body.contains("alloca"), "{case:?}: {body}");
            assert_eq!(body.contains("phi ptr"), across_cfg, "{case:?}: {body}");
        }
    }
}
