//! Ordinary source red probes for SPEC-0327 backend delivery; never execute generated code.
use super::{
    LoweringError,
    lower_frontend::orchestrate::lower_scalar_file,
    model::{EntityId, EntityType, Operation, Program, SsaTypeKind, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{declaration, parsed},
    verify::verify_program,
};
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
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
        let source = sources.add_source("convert.ko", text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let file = parse_file(&sources, &lexed).unwrap();
        assert!(file.diagnostics().is_empty());
        let names = resolve_names(&sources, &file, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let typed = check_types(&sources, &file, &names, &types).unwrap();
        complete_file_types(&sources, &file, &names, &types, &typed).unwrap();
        let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.deferred().is_empty());
        lower_scalar_file(&sources, &file, &names, &typed, &owned)
    };
    match &result {
        Ok(program) => println!(
            "MODE unit={unit} SOURCE {text:?} SSA {}",
            render_program(program)
        ),
        Err(error) => println!("MODE unit={unit} SOURCE {text:?} LOWER {error:?}"),
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
        println!("LLVM unit={unit} {source}->{target}\n{llvm}");
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
                assert!(
                    success.instructions.iter().any(|id| function
                        .instruction(*id)
                        .unwrap()
                        .operation
                        .entities()
                        .contains(&EntityId::Value(*result)))
                        || success
                            .terminator
                            .as_ref()
                            .unwrap()
                            .kind
                            .entities()
                            .contains(&EntityId::Value(*result))
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
