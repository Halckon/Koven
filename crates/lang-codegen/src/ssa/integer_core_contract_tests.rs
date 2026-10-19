//! Ordinary-source pairs for the shared integer intrinsic lowering contract.
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
        resolve_names,
    },
    ownership_checking::{
        check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        check_ownership,
    },
    source::{SourceMap, Span},
    type_checking::{
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

use super::{
    lower_scalar_file_with_entry,
    model::{
        Definition, EntityId, EntityType, Function, FunctionId, Operation, Program, ScalarConstant,
        SsaTypeKind, TerminatorKind,
    },
    unit_lower::{constant::lower_constant_unit_with_entry, lower_scalar_unit_with_entry},
    unit_lower_test_support::{declaration, parsed},
    verify::verify_program,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceKey(String, usize, usize);

fn source_key(sources: &SourceMap, span: Span) -> SourceKey {
    SourceKey(
        sources.source_name(span.source_id()).unwrap().to_owned(),
        span.start(),
        span.end(),
    )
}

fn text_key(text: &str, fragment: &str) -> SourceKey {
    let start = text.rfind(fragment).expect("fixture fragment exists");
    SourceKey("pair/entry.ko".to_owned(), start, start + fragment.len())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DefinitionKey {
    Instruction(SourceKey, usize),
    Parameter(SourceKey, usize),
}

fn definition_key(sources: &SourceMap, function: &Function, entity: EntityId) -> DefinitionKey {
    match function.entity(entity).unwrap().definition {
        Definition::InstructionResult { instruction, index } => DefinitionKey::Instruction(
            source_key(
                sources,
                function.instruction(instruction).unwrap().origin.span(),
            ),
            index,
        ),
        Definition::BlockParameter { block, index } => DefinitionKey::Parameter(
            source_key(sources, function.block(block).unwrap().origin.span()),
            index,
        ),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Intrinsic {
    origin: SourceKey,
    inverted: bool,
    receiver: DefinitionKey,
    receiver_type: SsaTypeKind,
    result_type: SsaTypeKind,
}

#[derive(Debug, PartialEq, Eq)]
struct Call {
    origin: SourceKey,
    callee: SourceKey,
    arguments: Vec<DefinitionKey>,
    result_type: SsaTypeKind,
}

#[derive(Debug, PartialEq, Eq)]
struct Contract {
    intrinsics: Vec<Intrinsic>,
    calls: Vec<Call>,
    constants: Vec<(SourceKey, i128, SsaTypeKind)>,
    returned: Vec<DefinitionKey>,
}

fn contract(sources: &SourceMap, program: &Program, entry: FunctionId) -> Contract {
    verify_program(program).expect("ordinary lowered SSA verifies");
    let module = &program.modules[0];
    let function = module.function(entry).unwrap();
    let kind = |entity| {
        let EntityType::Value(ty) = function.entity(entity).unwrap().ty else {
            panic!("integer fixture has scalar Value entities");
        };
        module.types[ty.index()].clone()
    };
    let definition = |entity| definition_key(sources, function, entity);
    let mut result = Contract {
        intrinsics: Vec::new(),
        calls: Vec::new(),
        constants: Vec::new(),
        returned: Vec::new(),
    };
    for instruction in &function.instructions {
        let origin = source_key(sources, instruction.origin.span());
        assert_eq!(origin.0, "pair/entry.ko");
        match &instruction.operation {
            Operation::IntegerNot { operand } | Operation::CheckedIntegerConvert { operand } => {
                let inverted = matches!(instruction.operation, Operation::IntegerNot { .. });
                assert_eq!(instruction.results.len(), if inverted { 1 } else { 2 });
                result.intrinsics.push(Intrinsic {
                    origin: origin.clone(),
                    inverted,
                    receiver: definition(EntityId::Value(*operand)),
                    receiver_type: kind(EntityId::Value(*operand)),
                    result_type: kind(instruction.results[0]),
                });
                if !inverted {
                    let value = definition(instruction.results[0]);
                    let failed = definition(instruction.results[1]);
                    assert_eq!(kind(instruction.results[1]), SsaTypeKind::Boolean);
                    let block = function.block(instruction.block).unwrap();
                    let last = function
                        .instruction(*block.instructions.last().unwrap())
                        .unwrap();
                    assert_eq!(source_key(sources, last.origin.span()), origin);
                    assert!(matches!(
                        last.operation,
                        Operation::CheckedIntegerConvert { .. }
                    ));
                    let terminator = block.terminator.as_ref().unwrap();
                    assert_eq!(source_key(sources, terminator.origin.span()), origin);
                    let TerminatorKind::Conditional {
                        condition,
                        when_true,
                        when_false,
                    } = &terminator.kind
                    else {
                        panic!("checked conversion branches on failure");
                    };
                    assert_eq!(definition(EntityId::Value(*condition)), failed);
                    let failure = function.block(when_true.target).unwrap();
                    assert!(failure.instructions.is_empty());
                    let abort = failure.terminator.as_ref().unwrap();
                    assert!(matches!(abort.kind, TerminatorKind::Abort));
                    assert_eq!(source_key(sources, abort.origin.span()), origin);
                    assert!(
                        when_true
                            .arguments
                            .iter()
                            .all(|entity| definition(*entity) != value)
                    );
                    let success = function.block(when_false.target).unwrap();
                    let TerminatorKind::Return { values } =
                        &success.terminator.as_ref().unwrap().kind
                    else {
                        panic!("fixture returns the checked result only on success");
                    };
                    assert_eq!(values.len(), 1);
                    assert_eq!(definition(EntityId::Value(values[0])), value);
                }
            }
            Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } => {
                assert!(receiver.is_none());
                assert_eq!(instruction.results.len(), 1);
                result.calls.push(Call {
                    origin,
                    callee: source_key(sources, module.function(*callee).unwrap().origin.span()),
                    arguments: arguments.iter().map(|entity| definition(*entity)).collect(),
                    result_type: kind(instruction.results[0]),
                });
            }
            Operation::Constant(ScalarConstant::Integer(value)) => {
                assert_eq!(instruction.results.len(), 1);
                result
                    .constants
                    .push((origin, *value, kind(instruction.results[0])));
            }
            _ => {}
        }
    }
    for block in &function.blocks {
        if let TerminatorKind::Return { values } = &block.terminator.as_ref().unwrap().kind {
            result.returned.extend(
                values
                    .iter()
                    .map(|value| definition(EntityId::Value(*value))),
            );
        }
    }
    result
}

fn fixture(package: &str, integer: &str, target: &str, constants: bool) -> String {
    let constant = if constants {
        format!("const val SEED: {integer} = 17\n")
    } else {
        String::new()
    };
    let parameter = if constants {
        String::new()
    } else {
        format!("value: {integer}")
    };
    let receiver = if constants { "SEED" } else { "value" };
    format!(
        "package {package}\n// 整数接收者\n{constant}fun tick(value: {integer}): {integer} {{ println(\"{package}\"); return value }}\nfun entry({parameter}): {target} {{ val receiver = {receiver}; return tick(receiver).inv().to{target}() }}"
    )
}

fn paired(text: &str, other_text: &str, constants: bool) -> Contract {
    let mut previous = None;
    for entry_first in [false, true] {
        let mut sources = SourceMap::new();
        let ((source, file), (other_source, other)) = if entry_first {
            let entry = parsed(&mut sources, "pair/entry.ko", text);
            let other = parsed(&mut sources, "other/entry.ko", other_text);
            (entry, other)
        } else {
            let other = parsed(&mut sources, "other/entry.ko", other_text);
            let entry = parsed(&mut sources, "pair/entry.ko", text);
            (entry, other)
        };
        let (name_environment, environment) = standard_environments();
        let names = resolve_names(&sources, &file, &name_environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let typed = check_types(&sources, &file, &names, &environment).unwrap();
        complete_file_types(&sources, &file, &names, &environment, &typed).unwrap();
        let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        let entry = names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "entry" && symbol.kind() == SymbolKind::Function)
            .unwrap()
            .id();
        let (single, entry) =
            lower_scalar_file_with_entry(&sources, &file, &names, &typed, &owned, entry).unwrap();
        let expected = contract(&sources, &single, entry);
        let inputs = [
            SourceUnitInput::new("root", "other/entry.ko", other_source, &other),
            SourceUnitInput::new("root", "pair/entry.ko", source, &file),
        ];
        for inputs in [inputs, [inputs[1], inputs[0]]] {
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names =
                resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
                    .unwrap()
                    .validate()
                    .unwrap();
            let recovery =
                check_compilation_unit_types(&sources, &inputs, &names, &environment).unwrap();
            complete_compilation_unit_types(&sources, &inputs, &names, &environment, &recovery)
                .unwrap();
            if constants {
                assert!(
                    recovery.clone().validate().is_err(),
                    "basic capability rejects real const input"
                );
            } else {
                let typed = recovery.clone().validate().unwrap();
                let owned = check_compilation_unit_ownership(
                    &sources,
                    &inputs,
                    &names,
                    &environment,
                    &typed,
                )
                .unwrap()
                .validate()
                .unwrap();
                let (program, entry) = lower_scalar_unit_with_entry(
                    &sources,
                    &inputs,
                    &names,
                    &environment,
                    &typed,
                    &owned,
                    declaration(&names, "pair", "entry"),
                )
                .unwrap();
                assert_eq!(contract(&sources, &program, entry), expected);
            }
            // Always exercise the real constant wrapper and its own validated capability.
            let typed = recovery.validate_constants().unwrap();
            let owned = check_compilation_unit_constant_ownership(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
            )
            .unwrap()
            .validate()
            .unwrap();
            let (program, entry) = lower_constant_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
                &owned,
                declaration(&names, "pair", "entry"),
            )
            .unwrap();
            assert_eq!(contract(&sources, &program, entry), expected);
        }
        if let Some(previous) = &previous {
            assert_eq!(
                &expected, previous,
                "source insertion order preserves logical identity"
            );
        }
        previous = Some(expected);
    }
    previous.unwrap()
}

fn assert_contract(text: &str, contract: Contract, target_bits: u16, constants: bool) {
    let int = SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    };
    let call = text_key(text, "tick(receiver)");
    let inverted = text_key(text, "tick(receiver).inv()");
    let conversion = text_key(
        text,
        if target_bits == 32 {
            "tick(receiver).inv().toInt()"
        } else {
            "tick(receiver).inv().toLong()"
        },
    );
    assert_eq!(
        contract.intrinsics,
        vec![
            Intrinsic {
                origin: inverted.clone(),
                inverted: true,
                receiver: DefinitionKey::Instruction(call.clone(), 0),
                receiver_type: int.clone(),
                result_type: int.clone(),
            },
            Intrinsic {
                origin: conversion.clone(),
                inverted: false,
                receiver: DefinitionKey::Instruction(inverted, 0),
                receiver_type: int.clone(),
                result_type: SsaTypeKind::Integer {
                    bits: target_bits,
                    signed: true
                },
            },
        ]
    );
    let entry_start = text.find("fun entry").unwrap();
    let tick_start = text.find("fun tick").unwrap();
    let argument = if constants {
        DefinitionKey::Instruction(text_key(text, "SEED"), 0)
    } else {
        DefinitionKey::Parameter(text_key(text, &text[entry_start..]), 0)
    };
    // One real side-effecting call supplies the inv receiver; convert uses that inv result.
    assert_eq!(
        contract.calls,
        vec![Call {
            origin: call,
            callee: text_key(text, &text[tick_start..entry_start - 1]),
            arguments: vec![argument],
            result_type: int.clone(),
        }]
    );
    assert_eq!(
        contract.returned,
        vec![DefinitionKey::Instruction(conversion, 0)]
    );
    assert_eq!(
        contract.constants,
        if constants {
            vec![(text_key(text, "SEED"), 17, int)]
        } else {
            Vec::new()
        }
    );
}

#[test]
fn paired_integer_intrinsics_preserve_source_identity_and_receiver_use() {
    for (target, bits) in [("Int", 32), ("Long", 64)] {
        let text = fixture("pair", "Int", target, false);
        let other = fixture("other", "UByte", "ULong", false);
        assert_contract(&text, paired(&text, &other, false), bits, false);
    }
}

#[test]
fn paired_const_integer_intrinsics_preserve_source_identity_and_receiver_use() {
    let text = fixture("pair", "Int", "Long", true);
    let other = fixture("other", "UByte", "ULong", true).replace("= 17\n", "= 19\n");
    assert_contract(&text, paired(&text, &other, true), 64, true);
}
