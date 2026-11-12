//! SPEC-0329 ordinary source probes; no object emission or native execution.
use crate::test_support::diagnostics::diagnostic;
mod cfg_contract {
    //! Exact endpoint recurrence in ordinary-source SSA; no execution or fabricated facts.
    use lang_frontend::source::Span;

    use super::verify_program;
    use crate::ssa::{
        integer_range_value_tests::collect_source_functions,
        model::{
            BinaryOperator, Block, BlockId, ComparisonOperator, Definition, Edge, EntityId, EntityType,
            Function, Instruction, Module, Operation, Origin, Program, ScalarConstant, SsaTypeId,
            SsaTypeKind, TerminatorKind, ValueId,
        },
    };

    #[test]
    fn numeric_snapshot_and_terminal_edges_preserve_exact_endpoints() {
        for (integer, range, bits) in [("Int", "IntRange", 32), ("Long", "LongRange", 64)] {
            for finish in ["", "continue"] {
                let source = format!(
                    "fun make(a:{integer},b:{integer}):{range}{{println(\"source\");return a..b}}\n\
                     fun entry(a:{integer},b:{integer}):Unit{{for(item in make(a,b)){{val copy:{integer}=item;{finish}}}}}"
                );
                let programs = collect_source_functions(&[source]);
                assert_eq!(
                    programs.len(),
                    2,
                    "both original source layouts use the Unit entry"
                );
                for source in programs {
                    let source = source.expect("ordinary numeric iteration reaches SSA");
                    verify_program(&source.program).expect("ordinary numeric iteration SSA verifies");
                    assert_endpoint_cfg(&source.program, bits, &source.functions);
                }
            }
        }
    }

    fn assert_endpoint_cfg(program: &Program, bits: u16, functions: &[(String, Span)]) {
        let module = only(program.modules.iter(), "one ordinary source module");
        let span = |name: &str| {
            only(
                functions.iter().filter(|(actual, _)| actual == name),
                "one exact function declaration in the actual lowered source",
            )
            .1
        };
        let make = source_function(module, span("make"));
        let function = source_function(module, span("entry"));
        let call = only(
            function
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. })),
            "entry evaluates its source through exactly one direct call",
        );
        let Operation::DirectCall { callee, .. } = call.operation else {
            unreachable!("direct call was selected");
        };
        assert_eq!(
            callee, make.id,
            "the sole source call targets the real make FunctionId"
        );
        let snapshot = result(call);
        let range_type = value_type(function, snapshot);
        let endpoint_type = module
            .integer_range_endpoint(range_type)
            .expect("call result has the registered numeric Range identity");
        assert!(module.valid_integer_range_type(range_type, endpoint_type));
        assert_eq!(
            module.type_kind(endpoint_type),
            Some(&SsaTypeKind::Integer { bits, signed: true })
        );

        let projections = function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::AggregateProject { .. }))
            .collect::<Vec<_>>();
        assert_eq!(
            projections.len(),
            3,
            "the source has exactly three snapshot fields"
        );
        let [first, last, empty] = [0, 1, 2].map(|field| {
            let projection = only(
                projections.iter().copied().filter(|instruction| {
                    matches!(instruction.operation,
                        Operation::AggregateProject { aggregate, field: actual }
                            if aggregate == snapshot && actual == field)
                }),
                "each exact field is projected once from the same call result",
            );
            assert_eq!(projection.block, call.block);
            assert!(
                call.id.index() < projection.id.index(),
                "source precedes its projections"
            );
            result(projection)
        });
        assert_eq!(value_type(function, first), endpoint_type);
        assert_eq!(value_type(function, last), endpoint_type);
        assert_eq!(
            module.type_kind(value_type(function, empty)),
            Some(&SsaTypeKind::Boolean)
        );

        let preheader = call.block;
        assert_eq!(function.entry_block(), Some(preheader));
        assert!(
            incoming(function, preheader).is_empty(),
            "source cannot be reevaluated by a backedge"
        );
        let (empty_test, empty_edge, body_edge) = conditional(function, preheader);
        assert_eq!(
            empty_test, empty,
            "the stored empty bit selects the zero-round edge"
        );
        let body = body_edge.target;
        let exhausted = empty_edge.target;
        assert_ne!(body, exhausted);
        let current_slot = only(
            body_edge
                .arguments
                .iter()
                .enumerate()
                .filter_map(|(index, value)| (*value == EntityId::Value(first)).then_some(index)),
            "the first body edge transports exactly the snapshot first endpoint",
        );
        let body_current = value(function.block(body).unwrap().parameters[current_slot]);

        let comparison = only(
            function
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::Compare { .. })),
            "there is exactly one terminal comparison",
        );
        let Operation::Compare {
            operator,
            left: current,
            right: terminal_last,
        } = comparison.operation
        else {
            unreachable!("comparison was selected");
        };
        assert_eq!(
            operator,
            ComparisonOperator::Equal,
            "termination tests current == last"
        );
        assert_eq!(value_type(function, current), endpoint_type);
        assert_eq!(value_type(function, terminal_last), endpoint_type);
        let terminal = comparison.block;
        // Single-file uses the body directly; unit carries the cleaned state through
        // one unconditional continuation edge. No general graph walk is needed here.
        let continuation = if terminal == body {
            None
        } else {
            let edge = branch(function, body);
            assert_eq!(edge.target, terminal);
            assert_eq!(incoming(function, terminal), vec![(body, edge)]);
            Some(edge)
        };
        let body_source =
            |endpoint| continuation.map_or(endpoint, |edge| before_edge(function, endpoint, edge));
        assert_eq!(
            body_source(current),
            body_current,
            "terminal current is this body's current slot"
        );
        let body_last = body_source(terminal_last);
        assert_eq!(
            before_edge(function, body_last, body_edge),
            last,
            "terminal last originates from field 1 of the same source snapshot"
        );
        let (terminal_test, final_edge, increment_edge) = conditional(function, terminal);
        assert_eq!(terminal_test, result(comparison));
        assert_eq!(
            final_edge.target, exhausted,
            "reaching last exits without incrementing"
        );
        let increment = increment_edge.target;
        assert_ne!(increment, exhausted);
        assert_ne!(increment, body);
        assert_eq!(
            incoming(function, increment),
            vec![(terminal, increment_edge)],
            "the terminal false edge is the increment block's sole entrance"
        );

        let addition = only(
            function
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::Binary { .. })),
            "there is exactly one low-level cursor operation",
        );
        let Operation::Binary {
            operator,
            left,
            right,
        } = addition.operation
        else {
            unreachable!("binary operation was selected");
        };
        assert_eq!(operator, BinaryOperator::Add);
        assert_eq!(addition.block, increment);
        assert_eq!(
            before_edge(function, left, increment_edge),
            current,
            "increment reads the exact compared current, including unit rebinding"
        );
        let one = defining_instruction(function, right);
        assert!(matches!(
            one.operation,
            Operation::Constant(ScalarConstant::Integer(1))
        ));
        assert_eq!(one.block, increment);
        assert!(one.id.index() < addition.id.index());
        assert_eq!(value_type(function, right), endpoint_type);
        let next = result(addition);
        assert_eq!(value_type(function, next), endpoint_type);
        let backedge = branch(function, increment);
        assert_eq!(backedge.target, body);
        assert_eq!(backedge.arguments[current_slot], EntityId::Value(next));
        assert_eq!(
            before_edge(
                function,
                before_edge(function, body_last, backedge),
                increment_edge
            ),
            terminal_last,
            "the backedge preserves the exact last endpoint instead of substituting current"
        );
        assert_eq!(
            incoming(function, body),
            vec![(preheader, body_edge), (increment, backedge)],
            "only first and next feed the body current slot"
        );
        assert_eq!(
            incoming(function, exhausted),
            vec![(preheader, empty_edge), (terminal, final_edge)],
            "zero-round and reached-last share the exhaustion exit"
        );
        assert!(matches!(
            terminator(function, exhausted),
            TerminatorKind::Return { values } if values.is_empty()
        ));
        assert_eq!(
            function.blocks.len(),
            4 + usize::from(continuation.is_some())
        );
        assert!(
            function.instructions.iter().all(|instruction| !matches!(
                instruction.operation,
                Operation::BorrowBegin { .. } | Operation::ContainerLength { .. }
            )),
            "numeric snapshot and element create no provider loan or Int length"
        );
    }

    fn source_function(module: &Module, span: Span) -> &Function {
        only(
            module.functions.iter().filter(
                |function| matches!(&function.origin, Origin::Source(actual) if *actual == span),
            ),
            "the full original declaration identifies exactly one SSA function",
        )
    }

    fn only<T>(mut values: impl Iterator<Item = T>, message: &str) -> T {
        let result = values.next().expect(message);
        assert!(values.next().is_none(), "{message}");
        result
    }

    fn value(entity: EntityId) -> ValueId {
        let EntityId::Value(value) = entity else {
            panic!("numeric endpoints must be owned Values");
        };
        value
    }

    fn result(instruction: &Instruction) -> ValueId {
        value(only(
            instruction.results.iter().copied(),
            "exactly one instruction result",
        ))
    }

    fn value_type(function: &Function, value: ValueId) -> SsaTypeId {
        let EntityType::Value(ty) = function.entity(EntityId::Value(value)).unwrap().ty else {
            panic!("numeric value has a Value type");
        };
        ty
    }

    fn defining_instruction(function: &Function, value: ValueId) -> &Instruction {
        let Definition::InstructionResult {
            instruction,
            index: 0,
        } = function.entity(EntityId::Value(value)).unwrap().definition
        else {
            panic!("constant is a direct instruction result");
        };
        function.instruction(instruction).unwrap()
    }

    fn terminator(function: &Function, block: BlockId) -> &TerminatorKind {
        &function
            .block(block)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
    }

    fn conditional(function: &Function, block: BlockId) -> (ValueId, &Edge, &Edge) {
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = terminator(function, block)
        else {
            panic!("the endpoint guard must be a conditional terminator");
        };
        (*condition, when_true, when_false)
    }

    fn branch(function: &Function, block: BlockId) -> &Edge {
        let TerminatorKind::Branch(edge) = terminator(function, block) else {
            panic!("the cursor continuation must be an unconditional branch");
        };
        edge
    }

    fn edges(block: &Block) -> Vec<&Edge> {
        match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            TerminatorKind::Return { .. } => Vec::new(),
            _ => panic!("this successful scalar fixture has no other exit kinds"),
        }
    }

    fn incoming(function: &Function, target: BlockId) -> Vec<(BlockId, &Edge)> {
        function
            .blocks
            .iter()
            .flat_map(|block| {
                edges(block)
                    .into_iter()
                    .filter(move |edge| edge.target == target)
                    .map(move |edge| (block.id, edge))
            })
            .collect()
    }

    /// Translate one known CFG edge only; a dominating Value retains its identity.
    fn before_edge(function: &Function, endpoint: ValueId, edge: &Edge) -> ValueId {
        match function
            .entity(EntityId::Value(endpoint))
            .unwrap()
            .definition
        {
            Definition::BlockParameter { block, index } if block == edge.target => {
                value(edge.arguments[index])
            }
            _ => endpoint,
        }
    }
}

use super::{integer_range_value_tests::collect_sources, verify::verify_program};

fn require_verified_sources(sources: &[String], llvm: bool) {
    // Observe both entries for every source before asserting the first missing stage.
    let observations = collect_sources(sources);
    assert_eq!(observations.len(), sources.len() * 2);
    for result in observations {
        let program = result.expect("numeric value iteration must reach SSA");
        verify_program(&program).expect("real source SSA verifies");
        if llvm {
            let rendered = crate::llvm::render_verified_program(&program)
                .expect("numeric iteration LLVM verifies in memory");
            diagnostic!("NUMERIC ITERATION LLVM {rendered}");
        }
    }
}

#[test]
fn numeric_sources_and_extreme_exits_reach_verified_ssa() {
    let mut sources = Vec::new();
    for (integer, range, minimum, maximum, previous) in [
        ("Int", "IntRange", "-2147483648", "2147483647", "2147483646"),
        (
            "Long",
            "LongRange",
            "-9223372036854775808L",
            "9223372036854775807L",
            "9223372036854775806L",
        ),
    ] {
        sources.push(format!(
            "fun entry(value:{range}):Unit{{for(item in value){{val copy:{integer}=item}}}}"
        ));
        sources.push(format!("class Holder(val value:{range})\nfun entry(holder:Holder):Unit{{for(item in holder.value){{val copy:{integer}=item}}}}"));
        for (source, exit) in [
            (format!("{minimum}..{maximum}"), "break"),
            (format!("{maximum}..{maximum}"), "continue"),
            (format!("{previous}..{maximum}"), "continue"),
            (format!("{minimum}..<{minimum}"), ""),
            (format!("{maximum}..<{maximum}"), ""),
            (format!("{maximum}..{minimum}"), ""),
        ] {
            sources.push(format!("fun entry():Unit{{for(_ in {source}){{{exit}}}}}"));
        }
    }
    require_verified_sources(&sources, false);
}

#[test]
fn numeric_source_snapshot_and_call_order_reach_verified_ssa() {
    let mut sources = Vec::new();
    for (integer, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        sources.push(format!(
            "fun make(a:{integer},b:{integer}):{range}{{println(\"source\");return a..b}}\nfun entry(a:{integer},b:{integer}):Unit{{for(item in make(a,b)){{val copy:{integer}=item;break}}}}"
        ));
        sources.push(format!(
            "fun entry(input:{range},replacement:{range},fallback:{integer}):{integer}{{var current=input;for(item in current){{current=replacement;val copy:{integer}=item;continue}};val after=current;return fallback}}"
        ));
        sources.push(format!(
            "fun entry(input:{range},fallback:{integer}):{integer}{{for(item in input){{return item}};return fallback}}"
        ));
    }
    require_verified_sources(&sources, false);
}

#[test]
fn numeric_resource_exits_and_nested_borrow_reach_verified_llvm() {
    let mut sources = Vec::new();
    for range in ["IntRange", "LongRange"] {
        for exit in ["", "continue", "break", "return"] {
            sources.push(format!(
                "class Guard(val name:String){{deinit(){{println(this.name)}}}}\nfun entry(input:{range}):Unit{{for(_ in input){{val first=Guard(\"first\");val second=Guard(\"second\");{exit}}}}}"
            ));
        }
        sources.push(format!(
            "fun entry(input:{range},values:List<Int>):Unit{{for(value in values){{for(item in input){{val copy=value;continue}};val after=value}}}}"
        ));
        sources.push(format!(
            "fun entry(input:{range},values:List<Int>):Unit{{for(item in input){{for(value in values){{val copy=value;break}};continue}}}}"
        ));
    }
    require_verified_sources(&sources, true);
}

#[test]
fn complete_bounded_numeric_fixture_preserves_provider_provenance() {
    let source = format!(
        "{}\nfun entry():Unit{{main()}}",
        include_str!("../../../lang-cli/tests/fixtures/integer-range-iteration-0329/success.ko")
    );
    require_verified_sources(&[source], true);
}

pub(in crate::ssa) mod checked_integer_conversion_backend_tests {
//! Ordinary source red probes for SPEC-0327 backend delivery; never execute generated code.
use super::super::{
    LoweringError,
    model::{EntityId, EntityType, Operation, Program, SsaTypeKind, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{declaration, parsed},
    verify::verify_program,
};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use crate::test_support::diagnostics::diagnostic;
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::check_compilation_unit_ownership,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

const INTEGERS: [&str; 8] = [
    "Byte", "Short", "Int", "Long", "UByte", "UShort", "UInt", "ULong",
];

fn lower_source(text: &str, unit: bool) -> Result<Program, LoweringError> {
    let mut sources = SourceMap::new();
    let (environment, types) = standard_environments();
    let result = if unit {
        let text = format!("package p\n{text}");
        let (source, file) = parsed(&mut sources, "p/convert.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/convert.ko", source, &file)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
        let typed = typed.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            declaration(&names, "p", "entry"),
        )
        .map(|(program, _)| program)
    } else {
        let analysis = UnitSource::analyze("convert.ko", text)
            .expect("integer builtin source must pass strict Unit analysis");
        analysis.assert_frontend_clean();
        let entry = analysis.declaration("entry", SymbolKind::Function);
        analysis.lower_entry(entry).map(|(program, _)| program)
    };
    match &result {
        Ok(program) => diagnostic!(
            "MODE unit={unit} SOURCE {text:?} SSA {}",
            render_program(program)
        ),
        Err(error) => diagnostic!("MODE unit={unit} SOURCE {text:?} LOWER {error:?}"),
    }
    result
}

fn checked_program(result: Result<Program, LoweringError>) -> Program {
    let program = result.expect("ordinary checked conversion must lower");
    verify_program(&program).expect("real lowered program verifies");
    program
}

fn pair_sources() -> Vec<(bool, String, String, Result<Program, LoweringError>)> {
    let mut cases = Vec::new();
    for unit in [false, true] {
        for source in INTEGERS {
            for target in INTEGERS {
                let text = format!("fun entry(value: {source}): {target} = value.to{target}()");
                cases.push((
                    unit,
                    source.to_owned(),
                    target.to_owned(),
                    lower_source(&text, unit),
                ));
            }
        }
    }
    cases
}

#[test]
fn all_integer_pairs_lower_from_real_typed_sources() {
    let cases = pair_sources();
    assert_eq!(cases.len(), 128);
    for (_, source, target, result) in cases {
        let program = checked_program(result);
        assert_conversion_cfg(&program, &source, &target, false);
    }
}

#[test]
fn conversion_receiver_call_has_one_source_evaluation() {
    let source =
        "fun tick(): Int { println(\"tick\"); return 1 }\nfun entry(): Long = tick().toLong()";
    let cases = [false, true].map(|unit| lower_source(source, unit));
    for result in cases {
        let program = checked_program(result);
        assert_conversion_cfg(&program, "Int", "Long", true);
    }
}

#[test]
fn checked_conversion_preserves_live_owner_and_pending_argument() {
    let sources = [
        "fun entry(number: Int): Long { val retained = \"retained\"; val converted = number.toLong(); println(retained); return converted }",
        "fun accept(own label: String, number: Long): Long { println(label); return number }\nfun entry(number: Int): Long = accept(\"pending\", number.toLong())",
    ];
    let cases = sources
        .into_iter()
        .flat_map(|source| [false, true].map(|unit| lower_source(source, unit)))
        .collect::<Vec<_>>();
    for result in cases {
        let program = checked_program(result);
        assert_conversion_cfg(&program, "Int", "Long", false);
    }
}

#[test]
fn all_integer_pairs_generate_verified_dynamic_llvm() {
    let cases = pair_sources();
    for (unit, source, target, result) in cases {
        let program = checked_program(result);
        let llvm =
            crate::llvm::render_verified_program(&program).expect("real dynamic LLVM verifies");
        diagnostic!("LLVM unit={unit} {source}->{target}\n{llvm}");
        assert_dynamic_conversion_ir(&program, &llvm, &source, &target);
    }
}

fn integer_kind(name: &str) -> SsaTypeKind {
    let (bits, signed) = match name {
        "Byte" => (8, true),
        "Short" => (16, true),
        "Int" => (32, true),
        "Long" => (64, true),
        "UByte" => (8, false),
        "UShort" => (16, false),
        "UInt" => (32, false),
        "ULong" => (64, false),
        _ => panic!("only the fixed eight integer sources are admitted"),
    };
    SsaTypeKind::Integer { bits, signed }
}

fn assert_conversion_cfg(program: &Program, source: &str, target: &str, receiver_call: bool) {
    let mut count = 0;
    for module in &program.modules {
        for function in &module.functions {
            for instruction in &function.instructions {
                let Operation::CheckedIntegerConvert { operand } = instruction.operation else {
                    continue;
                };
                count += 1;
                let [EntityId::Value(result), EntityId::Value(failed)] =
                    instruction.results.as_slice()
                else {
                    panic!("conversion exposes value and failure");
                };
                let kind = |entity| {
                    let EntityType::Value(ty) = function.entity(entity).unwrap().ty else {
                        panic!("value entity");
                    };
                    &module.types[ty.index()]
                };
                assert_eq!(kind(EntityId::Value(operand)), &integer_kind(source));
                assert_eq!(kind(EntityId::Value(*result)), &integer_kind(target));
                assert_eq!(kind(EntityId::Value(*failed)), &SsaTypeKind::Boolean);
                let block = function.block(instruction.block).unwrap();
                assert_eq!(block.instructions.last(), Some(&instruction.id));
                let TerminatorKind::Conditional {
                    condition,
                    when_true,
                    when_false,
                } = &block.terminator.as_ref().unwrap().kind
                else {
                    panic!("checked failure must select an explicit edge");
                };
                assert_eq!(condition, failed);
                let failure = function.block(when_true.target).unwrap();
                assert!(failure.instructions.is_empty());
                assert!(matches!(
                    failure.terminator.as_ref().unwrap().kind,
                    TerminatorKind::Abort
                ));
                assert!(!when_true.arguments.contains(&EntityId::Value(*result)));
                assert_ne!(when_true.target, when_false.target);
                let success = function.block(when_false.target).unwrap();
                let used_result = when_false
                    .arguments
                    .iter()
                    .position(|arg| arg == &EntityId::Value(*result))
                    .map_or(EntityId::Value(*result), |pos| success.parameters[pos]);
                assert!(
                    success.instructions.iter().any(|id| function
                        .instruction(*id)
                        .unwrap()
                        .operation
                        .entities()
                        .contains(&used_result))
                        || success
                            .terminator
                            .as_ref()
                            .unwrap()
                            .kind
                            .entities()
                            .contains(&used_result)
                );
                for owner in function
                    .instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
                {
                    let [original] = owner.results.as_slice() else {
                        panic!("String literal owner");
                    };
                    let position = when_false
                        .arguments
                        .iter()
                        .position(|argument| argument == original)
                        .expect("live/pending owner carried on success");
                    assert_eq!(when_true.arguments.get(position), Some(original));
                    let rebound = success.parameters[position];
                    assert_ne!(rebound, *original);
                    assert!(success.instructions.iter().any(|id| {
                        function
                            .instruction(*id)
                            .unwrap()
                            .operation
                            .entities()
                            .contains(&rebound)
                    }));
                    assert!(success.instructions.iter().all(|id| {
                        !function
                            .instruction(*id)
                            .unwrap()
                            .operation
                            .entities()
                            .contains(original)
                    }));
                }
                if receiver_call {
                    let calls = function
                        .instructions
                        .iter()
                        .filter(|call| matches!(call.operation, Operation::DirectCall { .. }))
                        .collect::<Vec<_>>();
                    assert_eq!(calls.len(), 1);
                    assert_eq!(calls[0].results, [EntityId::Value(operand)]);
                    let Operation::DirectCall { callee, .. } = calls[0].operation else {
                        unreachable!()
                    };
                    assert!(module.function(callee).unwrap().name.contains("tick"));
                }
            }
        }
    }
    assert_eq!(count, 1);
}

fn assert_dynamic_conversion_ir(program: &Program, llvm: &str, source: &str, target: &str) {
    let SsaTypeKind::Integer {
        bits: source_bits,
        signed: source_signed,
    } = integer_kind(source)
    else {
        unreachable!()
    };
    let SsaTypeKind::Integer {
        bits: target_bits,
        signed: target_signed,
    } = integer_kind(target)
    else {
        unreachable!()
    };
    let minimum = if target_signed {
        -(1_i128 << (target_bits - 1))
    } else {
        0
    };
    let maximum = (1_i128 << (target_bits - u16::from(target_signed))) - 1;
    let conversion = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .flat_map(|f| &f.instructions)
        .find(|i| matches!(i.operation, Operation::CheckedIntegerConvert { .. }))
        .unwrap();
    let [EntityId::Value(result), EntityId::Value(failed)] = conversion.results.as_slice() else {
        unreachable!()
    };
    let name = format!("%v{}", result.index());
    let extend = if source_signed { "sext" } else { "zext" };
    let extension = format!("{name}.mathematical = {extend} i{source_bits} ");
    let lower = format!("{name}.below = icmp slt i128 {name}.mathematical, {minimum}");
    let upper = format!("{name}.above = icmp sgt i128 {name}.mathematical, {maximum}");
    let failed_line = format!("%v{} = or i1 {name}.below, {name}.above", failed.index());
    let cast = format!("{name} = trunc i128 {name}.mathematical to i{target_bits}");
    let positions = [&extension, &lower, &upper, &failed_line, &cast].map(|part| {
        llvm.find(part)
            .unwrap_or_else(|| panic!("missing {part} in {llvm}"))
    });
    assert!(positions.windows(2).all(|p| p[0] < p[1]));
    assert!(llvm.contains(&format!("br i1 %v{}, label", failed.index())));
    assert!(llvm.contains("call void @koven.abort.report"));
    assert!(llvm.contains("unreachable"));
    assert!(!llvm.contains("poison"));
}
}
