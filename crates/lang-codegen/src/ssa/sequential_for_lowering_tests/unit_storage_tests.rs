//! SPEC-0248: source Unit list-form storage and logical iteration contracts.

use super::super::model::{
    BinaryOperator, Block, BlockId, ComparisonOperator, EntityId, EntityType, Function,
    Instruction, LoanKind, Module, Operation, Ownership, PlaceAccess, Program, ScalarConstant,
    SequentialContainerKind, SsaTypeKind, TerminatorKind,
};
use super::cleanup_tests::{loan, transport, value};
use super::{analyze, defining_instruction, incoming_edges, parameter_slot};

#[test]
fn nonempty_unit_array_source_lowers_to_verified_ssa() {
    analyze(
        r#"
        fun first(): Unit { println("first") }
        fun source(): Array<Unit> {
            println("source")
            return arrayOf<Unit>(first())
        }
        fun main(): Unit {
            var count = 0
            for (item in source()) {
                val copy: Unit = item
                count += 1
                println("body")
            }
            if (count == 1) { println("count-ok") }
            println("done")
        }
        "#,
    );
}

fn analyze_case(filename: &str, source_text: &str) -> Program {
    use lang_frontend::{
        lexer::lex,
        name_resolution::resolve_names,
        ownership_checking::check_ownership,
        parser::parse_file,
        source::SourceMap,
        type_checking::{check_types, standard_environments},
    };
    let mut sources = SourceMap::new();
    let source = sources.add_source(filename, source_text).expect("source");
    let lexed = lex(&sources, source).expect("lexer");
    let parsed = parse_file(&sources, &lexed).expect("parser");
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    for diagnostics in [
        lexed.diagnostics(),
        parsed.diagnostics(),
        names.diagnostics(),
        typed.diagnostics(),
        owned.diagnostics(),
    ] {
        assert!(diagnostics.is_empty(), "{filename}: {diagnostics:?}");
    }
    let program = super::super::lower_frontend::orchestrate::lower_scalar_file(
        &sources, &parsed, &names, &typed, &owned,
    )
    .unwrap_or_else(|error| panic!("{filename}: {error:?}\n{source_text}"));
    super::super::verify::verify_program(&program).expect("Unit container SSA must verify");
    program
}

fn fixture(provider: &str, constructor: &str, arguments: &str, count: usize) -> String {
    format!(
        r#"
        fun first(): Unit {{ println("first") }}
        fun second(): Unit {{ println("second") }}
        fun third(): Unit {{ println("third") }}
        fun source(): {provider}<Unit> {{
            println("source")
            return {constructor}<Unit>({arguments})
        }}
        fun main(): Unit {{
            var count = 0
            for (item in source()) {{
                count += 1
                val copy: Unit = item
                println("body")
            }}
            if (count == {count}) {{ println("count-ok") }}
            println("done")
        }}
        "#
    )
}

fn named<'a>(module: &'a Module, name: &str) -> &'a Function {
    module
        .functions
        .iter()
        .find(|f| f.name == name)
        .expect(name)
}

fn only(function: &Function, predicate: impl Fn(&Operation) -> bool) -> &Instruction {
    let instructions = function
        .instructions
        .iter()
        .filter(|instruction| predicate(&instruction.operation))
        .collect::<Vec<_>>();
    assert_eq!(instructions.len(), 1, "{instructions:?}");
    instructions[0]
}

fn position(block: &Block, instruction: &Instruction) -> usize {
    assert_eq!(instruction.block, block.id);
    block
        .instructions
        .iter()
        .position(|id| *id == instruction.id)
        .unwrap()
}

fn assert_factory(
    module: &Module,
    expected_kind: SequentialContainerKind,
    expected_calls: &[&str],
) {
    let source = named(module, "source");
    let construct = only(source, |op| {
        matches!(op, Operation::ContainerConstruct { .. })
    });
    let Operation::ContainerConstruct {
        container,
        elements,
    } = &construct.operation
    else {
        unreachable!()
    };
    let Some(SsaTypeKind::SequentialContainer { kind, element }) = module.type_kind(*container)
    else {
        panic!("concrete sequential container type")
    };
    assert_eq!(*kind, expected_kind);
    assert_eq!(module.type_kind(*element), Some(&SsaTypeKind::Unit));
    assert_eq!(module.type_ownership(*element), Some(Ownership::Copyable));
    assert_eq!(elements.len(), expected_calls.len());
    assert_eq!(source.return_types, [*container]);
    let calls = source
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
        .collect::<Vec<_>>();
    let constants = source
        .instructions
        .iter()
        .filter(|i| i.operation == Operation::Constant(ScalarConstant::Unit))
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), expected_calls.len());
    assert_eq!(constants.len(), expected_calls.len());
    let block = source.block(construct.block).unwrap();
    let mut previous = None;
    for ((name, call), operand) in expected_calls.iter().zip(calls).zip(elements) {
        let producer = named(module, name);
        assert!(
            producer.return_types.is_empty(),
            "Unit producer retains void ABI"
        );
        assert_eq!(
            call.operation,
            Operation::DirectCall {
                callee: producer.id,
                receiver: None,
                arguments: Vec::new()
            }
        );
        assert!(
            call.results.is_empty(),
            "Unit call has no general SSA result"
        );
        let materialized = defining_instruction(source, EntityId::Value(*operand));
        assert_eq!(
            materialized.operation,
            Operation::Constant(ScalarConstant::Unit)
        );
        assert_eq!(materialized.results, [EntityId::Value(*operand)]);
        assert_eq!(
            source.entity(EntityId::Value(*operand)).unwrap().ty,
            EntityType::Value(*element)
        );
        let call_position = position(block, call);
        assert_eq!(
            position(block, materialized),
            call_position + 1,
            "materialize the already-evaluated producer exactly once"
        );
        if let Some(previous) = previous {
            assert!(previous < call_position);
        }
        previous = Some(position(block, materialized));
        assert!(position(block, materialized) < position(block, construct));
    }
    let marker = only(
        source,
        |op| matches!(op, Operation::StringLiteral { bytes, .. } if bytes == b"source"),
    );
    let marker_root = only(
        source,
        |op| matches!(op, Operation::RootPlace { owner } if EntityId::Value(*owner) == marker.results[0]),
    );
    let marker_borrow = only(
        source,
        |op| matches!(op, Operation::BorrowBegin { place, kind: LoanKind::Shared } if EntityId::Place(*place) == marker_root.results[0]),
    );
    let marker_print = only(
        source,
        |op| matches!(op, Operation::PrintString { value } if EntityId::Loan(*value) == marker_borrow.results[0]),
    );
    assert!(position(block, marker) < position(block, marker_print));
    assert!(position(block, marker_print) < position(block, construct));
    if let Some(first_name) = expected_calls.first() {
        let first = only(
            source,
            |op| matches!(op, Operation::DirectCall { callee, .. } if *callee == named(module, first_name).id),
        );
        assert!(position(block, marker_print) < position(block, first));
    }
}

fn cleanup(module: &Module, function: &Function, block: &Block) -> Vec<Operation> {
    block
        .instructions
        .iter()
        .filter_map(|id| {
            let operation = &function.instruction(*id).unwrap().operation;
            let entity = match operation {
                Operation::BorrowEnd { loan } => EntityId::Loan(*loan),
                Operation::Drop { owner } => EntityId::Value(*owner),
                _ => return None,
            };
            let ty = function.entity(entity).unwrap().ty.semantic_type();
            // println creates and releases separate String temporaries. Only Unit and
            // container cleanup participates in the provider/element identity contract.
            matches!(
                module.type_kind(ty),
                Some(SsaTypeKind::Unit | SsaTypeKind::SequentialContainer { .. })
            )
            .then(|| operation.clone())
        })
        .collect()
}

// Track identity along the normal checked-arithmetic path. A dominating Copyable
// scalar may be reused; transported owner/loan identities must follow block parameters.
fn normal_identity(
    function: &Function,
    start: BlockId,
    target: BlockId,
    mut entity: EntityId,
) -> EntityId {
    let mut current = start;
    let mut visited = Vec::new();
    while current != target {
        assert!(
            !visited.contains(&current),
            "normal body must not contain another loop"
        );
        visited.push(current);
        let block = function.block(current).unwrap();
        let edge = match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Branch(edge) => edge,
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => {
                let aborts = |target| {
                    matches!(
                        function
                            .block(target)
                            .unwrap()
                            .terminator
                            .as_ref()
                            .unwrap()
                            .kind,
                        TerminatorKind::Abort
                    )
                };
                match (aborts(when_true.target), aborts(when_false.target)) {
                    (true, false) => when_false,
                    (false, true) => when_true,
                    _ => panic!("expected one checked-arithmetic failure edge"),
                }
            }
            other => panic!("normal path does not reach expected block: {other:?}"),
        };
        if edge.arguments.contains(&entity) {
            entity = transport(function, edge, entity);
        }
        current = edge.target;
    }
    entity
}

fn assert_iteration(module: &Module, expect_read: bool) {
    let run = named(module, "main");
    assert!(run.return_types.is_empty(), "Unit entry retains void ABI");
    let source_id = named(module, "source").id;
    let source = only(
        run,
        |op| matches!(op, Operation::DirectCall { callee, .. } if *callee == source_id),
    );
    let length = only(run, |op| matches!(op, Operation::ContainerLength { .. }));
    let preheader = run.block(run.entry_block().unwrap()).unwrap();
    assert!(incoming_edges(run, preheader.id).is_empty());
    assert_eq!(source.block, preheader.id);
    assert_eq!(length.block, preheader.id);
    let [source_owner] = source.results[..] else {
        panic!("one temporary container owner")
    };
    let Operation::ContainerLength { owner: source_loan } = length.operation else {
        unreachable!()
    };
    let borrow = defining_instruction(run, source_loan);
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = borrow.operation
    else {
        panic!("shared source loan")
    };
    let root = defining_instruction(run, EntityId::Place(place));
    assert_eq!(
        root.operation,
        Operation::RootPlace {
            owner: value(source_owner)
        }
    );
    let positions = [source, root, borrow, length].map(|i| position(preheader, i));
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    let TerminatorKind::Branch(enter) = &preheader.terminator.as_ref().unwrap().kind else {
        panic!("enter header")
    };
    let header = run.block(enter.target).unwrap();
    let header_owner = transport(run, enter, source_owner);
    let header_source = transport(run, enter, source_loan);
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("guarded header")
    };
    let guard = defining_instruction(run, EntityId::Value(*condition));
    assert_eq!(guard.block, header.id);
    let Operation::Compare {
        operator: ComparisonOperator::LessThan,
        left: cursor,
        right: snapshot,
    } = guard.operation
    else {
        panic!("cursor < length")
    };
    let cursor_slot = parameter_slot(header, EntityId::Value(cursor));
    let length_slot = parameter_slot(header, EntityId::Value(snapshot));
    let owner_slot = parameter_slot(header, header_owner);
    let source_slot = parameter_slot(header, header_source);
    assert_eq!(length.results, [enter.arguments[length_slot]]);
    let zero = defining_instruction(run, enter.arguments[cursor_slot]);
    assert_eq!(zero.block, preheader.id);
    assert_eq!(
        zero.operation,
        Operation::Constant(ScalarConstant::Integer(0))
    );
    let body = run.block(when_true.target).unwrap();
    let exhausted = run.block(when_false.target).unwrap();
    assert_ne!(body.id, exhausted.id);
    assert_eq!(incoming_edges(run, body.id), [(header.id, when_true)]);
    let body_owner = transport(run, when_true, header_owner);
    let body_source = transport(run, when_true, header_source);
    let body_cursor = transport(run, when_true, EntityId::Value(cursor));
    let element = only(run, |op| {
        matches!(op, Operation::ContainerElementPlace { .. })
    });
    assert_eq!(element.block, body.id);
    assert_eq!(
        element.operation,
        Operation::ContainerElementPlace {
            owner: body_source,
            index: value(body_cursor)
        }
    );
    let element_borrow = only(
        run,
        |op| matches!(op, Operation::BorrowBegin { place, kind: LoanKind::Shared } if EntityId::Place(*place) == element.results[0]),
    );
    assert_eq!(element_borrow.block, body.id);
    assert!(position(body, element) < position(body, element_borrow));
    let element_loan = element_borrow.results[0];
    let unit = run.entity(element_loan).unwrap().ty.semantic_type();
    assert_eq!(module.type_kind(unit), Some(&SsaTypeKind::Unit));
    assert_eq!(module.type_ownership(unit), Some(Ownership::Copyable));
    let reads = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::Read { .. }))
        .collect::<Vec<_>>();
    assert_eq!(reads.len(), usize::from(expect_read));
    if expect_read {
        let read = reads[0];
        assert_ne!(
            read.block, body.id,
            "Unit read occurs after checked count += 1 CFG"
        );
        let rebound_loan = normal_identity(run, body.id, read.block, element_loan);
        assert_eq!(
            read.operation,
            Operation::Read {
                source: PlaceAccess::Loan(loan(rebound_loan))
            }
        );
        assert_eq!(read.results.len(), 1);
        assert_eq!(
            run.entity(read.results[0]).unwrap().ty,
            EntityType::Value(unit)
        );
    }
    let header_incoming = incoming_edges(run, header.id);
    assert_eq!(header_incoming.len(), 2);
    assert!(header_incoming.contains(&(preheader.id, enter)));
    let (back_block_id, backedge) = header_incoming
        .into_iter()
        .find(|(block, _)| *block != preheader.id)
        .unwrap();
    let back_block = run.block(back_block_id).unwrap();
    let at_back = |entity| normal_identity(run, body.id, back_block.id, entity);
    assert_eq!(backedge.arguments[owner_slot], at_back(body_owner));
    assert_eq!(backedge.arguments[source_slot], at_back(body_source));
    assert_eq!(backedge.arguments[length_slot], EntityId::Value(snapshot));
    let advance = defining_instruction(run, backedge.arguments[cursor_slot]);
    assert_eq!(advance.block, back_block.id);
    let Operation::Binary {
        operator: BinaryOperator::Add,
        left,
        right,
    } = advance.operation
    else {
        panic!("advance logical cursor")
    };
    assert_eq!(EntityId::Value(left), at_back(body_cursor));
    let step = defining_instruction(run, EntityId::Value(right));
    assert_eq!(step.block, preheader.id);
    assert_eq!(
        step.operation,
        Operation::Constant(ScalarConstant::Integer(1))
    );
    assert_eq!(
        cleanup(module, run, back_block),
        [Operation::BorrowEnd {
            loan: loan(at_back(element_loan))
        }]
    );
    let end = only(run, |op| {
        *op == Operation::BorrowEnd {
            loan: loan(at_back(element_loan)),
        }
    });
    assert!(position(back_block, end) < position(back_block, advance));
    assert_eq!(
        cleanup(module, run, exhausted),
        [
            Operation::BorrowEnd {
                loan: loan(transport(run, when_false, header_source))
            },
            Operation::Drop {
                owner: value(transport(run, when_false, header_owner))
            },
        ]
    );
    for block in &run.blocks {
        if block.id != back_block.id && block.id != exhausted.id {
            assert!(
                cleanup(module, run, block).is_empty(),
                "unexpected cleanup in {block:?}"
            );
        }
    }
    assert!(
        !run.instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::Consume { .. }))
    );
}

#[test]
fn unit_container_nine_source_cases_preserve_factory_and_iteration_identities() {
    for (filename, provider, constructor, kind, arguments, calls, count) in [
        (
            "unit_array_empty.ko",
            "Array",
            "arrayOf",
            SequentialContainerKind::Array,
            "",
            &[][..],
            0,
        ),
        (
            "unit_array_single.ko",
            "Array",
            "arrayOf",
            SequentialContainerKind::Array,
            "first()",
            &["first"][..],
            1,
        ),
        (
            "unit_array_multi.ko",
            "Array",
            "arrayOf",
            SequentialContainerKind::Array,
            "first(), second(), third()",
            &["first", "second", "third"][..],
            3,
        ),
        (
            "unit_list_empty.ko",
            "List",
            "listOf",
            SequentialContainerKind::List,
            "",
            &[][..],
            0,
        ),
        (
            "unit_list_single.ko",
            "List",
            "listOf",
            SequentialContainerKind::List,
            "first()",
            &["first"][..],
            1,
        ),
        (
            "unit_list_multi.ko",
            "List",
            "listOf",
            SequentialContainerKind::List,
            "first(), second(), third()",
            &["first", "second", "third"][..],
            3,
        ),
        (
            "unit_mutable_list_empty.ko",
            "MutableList",
            "mutableListOf",
            SequentialContainerKind::MutableList,
            "",
            &[][..],
            0,
        ),
        (
            "unit_mutable_list_single.ko",
            "MutableList",
            "mutableListOf",
            SequentialContainerKind::MutableList,
            "first()",
            &["first"][..],
            1,
        ),
        (
            "unit_mutable_list_multi.ko",
            "MutableList",
            "mutableListOf",
            SequentialContainerKind::MutableList,
            "first(), second(), third()",
            &["first", "second", "third"][..],
            3,
        ),
    ] {
        let program = analyze_case(filename, &fixture(provider, constructor, arguments, count));
        assert_eq!(program.modules.len(), 1);
        let module = &program.modules[0];
        assert_factory(module, kind, calls);
        assert_iteration(module, true);
    }
}

#[test]
fn unit_discard_binding_keeps_logical_iteration_without_a_read() {
    let source = fixture("List", "listOf", "first(), second(), third()", 3)
        .replace("for (item in source())", "for (_ in source())")
        .replace("val copy: Unit = item", "");
    let program = analyze_case("unit_list_discard.ko", &source);
    assert_factory(
        &program.modules[0],
        SequentialContainerKind::List,
        &["first", "second", "third"],
    );
    assert_iteration(&program.modules[0], false);
}

#[test]
fn unit_diverged_operand_preserves_only_evaluated_prefix() {
    for (provider, constructor) in [
        ("Array", "arrayOf"),
        ("List", "listOf"),
        ("MutableList", "mutableListOf"),
    ] {
        let source = format!(
            r#"
            fun first(): Unit {{ println("first") }}
            fun second(): Unit {{ println("second") }}
            fun source(): {provider}<Unit> {{
                return {constructor}<Unit>(first(), error("stop"), second())
            }}
        "#
        );
        let program = analyze_case("unit_diverged_operand.ko", &source);
        let module = &program.modules[0];
        let factory = named(module, "source");
        let first = only(
            factory,
            |op| matches!(op, Operation::DirectCall { callee, .. } if *callee == named(module, "first").id),
        );
        assert!(first.results.is_empty());
        assert!(!factory.instructions.iter().any(|i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == named(module, "second").id)));
        assert!(
            !factory
                .instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::ContainerConstruct { .. }))
        );
        let materialized = only(factory, |op| {
            *op == Operation::Constant(ScalarConstant::Unit)
        });
        let block = factory.block(first.block).unwrap();
        assert_eq!(position(block, materialized), position(block, first) + 1);
        assert!(matches!(
            block.terminator.as_ref().unwrap().kind,
            TerminatorKind::Abort
        ));
    }
}

#[test]
fn unit_container_fresh_frontend_chains_render_identically() {
    let source = fixture(
        "MutableList",
        "mutableListOf",
        "first(), second(), third()",
        3,
    );
    let first = analyze_case("unit_mutable_list_determinism.ko", &source);
    let second = analyze_case("unit_mutable_list_determinism.ko", &source);
    assert_eq!(
        super::super::render::render_program(&first),
        super::super::render::render_program(&second)
    );
    assert_eq!(
        crate::llvm::render_verified_program(&first).expect("first LLVM"),
        crate::llvm::render_verified_program(&second).expect("second LLVM")
    );
}
