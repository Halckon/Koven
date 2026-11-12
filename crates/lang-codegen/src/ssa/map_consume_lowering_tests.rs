//! SPEC-0291: ordinary source through complete FE, ownership, SSA and in-memory LLVM.
//! All twelve original paths are collected before success or identity assertions.
//! The single-source column now uses MemoryDocument Unit facts, not Legacy analysis.

use crate::test_support::diagnostics::diagnostic;
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SourceUnitKey, SymbolKind, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::check_compilation_unit_ownership,
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use super::model::Program;

mod identity;

const CASES: [(&str, &str); 6] = [
    (
        "int_int_val_empty_group",
        r#"fun entry(): Unit {
 val source = mutableMapOf<Int, Int>()
 val result = (source).consume()
 if (result.size == 0) { println("empty int map") }
}"#,
    ),
    (
        "int_int_var_nonempty_overwrite",
        r#"fun entry(): Unit {
 var source = mutableMapOf<Int, Int>()
 source.put(7, 70)
 source.put(7, 71)
 val result = source.consume()
 if (result.get(7)!! == 71) { println("overwritten int value") }
 if (result.size == 1) { println("one int entry") }
}"#,
    ),
    (
        "string_string_val_empty",
        r#"fun entry(): Unit {
 val source = mutableMapOf<String, String>()
 val result = source.consume()
 if (result.size == 0) { println("empty string map") }
}"#,
    ),
    (
        "string_string_var_nonempty_overwrite_group",
        r#"fun entry(): Unit {
 var source = mutableMapOf<String, String>()
 source.put("key".clone(), "old".clone())
 source.put("key".clone(), "new".clone())
 val result = (source).consume()
 if (result.size == 1) { println("one string entry") }
}"#,
    ),
    (
        "int_int_discard_nonempty",
        r#"fun entry(): Unit {
 var source = mutableMapOf<Int, Int>()
 source.put(3, 30)
 source.consume()
 println("discarded int map")
}"#,
    ),
    (
        "string_string_discard_nonempty",
        r#"fun entry(): Unit {
 var source = mutableMapOf<String, String>()
 source.put("key".clone(), "value".clone())
 source.consume()
 println("discarded string map")
}"#,
    ),
];

struct Observation {
    label: String,
    stages: Vec<String>,
    outcome: Result<(), String>,
    produced: Option<Produced>,
}

struct Produced {
    program: Program,
    llvm: String,
    sources: SourceMap,
}

fn step<T, E: std::fmt::Debug>(
    stage: &str,
    result: Result<T, E>,
    stages: &mut Vec<String>,
) -> Result<T, String> {
    match result {
        Ok(value) => {
            stages.push(format!("{stage}: reached"));
            Ok(value)
        }
        Err(error) => Err(format!("{stage}: {error:?}")),
    }
}

fn gate(
    stage: &str,
    valid: bool,
    detail: impl std::fmt::Debug,
    stages: &mut Vec<String>,
) -> Result<(), String> {
    if valid {
        stages.push(format!("{stage}: passed"));
        Ok(())
    } else {
        Err(format!("{stage}: {detail:?}"))
    }
}

fn lower_source(
    sources: &SourceMap,
    parsed: &ParsedFile,
    rooted: bool,
    stages: &mut Vec<String>,
) -> Result<Program, String> {
    let (environment, types) = standard_environments();
    let inputs = [if rooted {
        SourceUnitInput::new("root", "consume.ko", parsed.source_id(), parsed)
    } else {
        SourceUnitInput::memory_document(parsed.source_id(), parsed)
    }];
    let index = step("index", index_compilation_unit(sources, &inputs), stages)?;
    gate(
        "one original source with the selected Unit input identity",
        matches!(index.source_units(), [source]
        if source.source_id() == parsed.source_id()
            && if rooted {
                source.key().root().is_some_and(|root| root.as_str() == "root")
                    && source.key().logical_path().is_some_and(|path| path.as_str() == "consume.ko")
            } else {
                matches!(source.key(), SourceUnitKey::MemoryDocument)
            }),
        index.source_units(),
        stages,
    )?;
    let source_unit = index.source_units()[0].id();
    let names = step(
        "names",
        resolve_compilation_unit_names(sources, &inputs, &index, &environment),
        stages,
    )?;
    gate(
        "names diagnostics",
        names.diagnostics().is_empty(),
        names.diagnostics(),
        stages,
    )?;
    let names = step(
        "qualified names",
        names
            .validate()
            .map_err(|value| format!("{:?}", value.diagnostics())),
        stages,
    )?;
    let typed = step(
        "raw types",
        check_compilation_unit_types(sources, &inputs, &names, &types),
        stages,
    )?;
    gate(
        "raw type diagnostics",
        typed.diagnostics().is_empty(),
        typed.diagnostics(),
        stages,
    )?;
    gate(
        "one real consume descriptor",
        typed.map_consumes().len() == 1,
        typed.map_consumes(),
        stages,
    )?;
    let complete = step(
        "complete types",
        complete_compilation_unit_types(sources, &inputs, &names, &types, &typed),
        stages,
    )?;
    gate(
        "complete same analysis",
        complete.is_same_analysis(&typed),
        "original typed analysis",
        stages,
    )?;
    let typed = step(
        "qualified types",
        typed
            .validate()
            .map_err(|value| format!("{:?}", value.diagnostics())),
        stages,
    )?;
    let owned = step(
        "ownership",
        check_compilation_unit_ownership(sources, &inputs, &names, &types, &typed),
        stages,
    )?;
    gate(
        "ownership diagnostics",
        owned.diagnostics().is_empty(),
        owned.diagnostics(),
        stages,
    )?;
    gate(
        "ownership completeness",
        owned.deferred().is_empty(),
        owned.deferred(),
        stages,
    )?;
    let owned = step(
        "qualified ownership",
        owned
            .validate()
            .map_err(|value| format!("{:?}", value.diagnostics())),
        stages,
    )?;
    let entries: Vec<_> = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "entry"
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == source_unit
                && declaration.name_span().source_id() == parsed.source_id()
        })
        .map(|declaration| declaration.id())
        .collect();
    gate(
        "one actual entry declaration",
        entries.len() == 1,
        &entries,
        stages,
    )?;
    let result = super::unit_lower::lower_scalar_unit_with_entry(
        sources, &inputs, &names, &types, &typed, &owned, entries[0],
    );
    step(
        "lower SSA",
        result.map_err(|error| lowering_error(sources, error)),
        stages,
    )
    .map(|(program, _)| program)
}

fn lowering_error(sources: &SourceMap, error: super::LoweringError) -> String {
    let origin = error
        .span
        .map(|span| (sources.source_name(span.source_id()), sources.slice(span)));
    format!("{error:?}; real source origin={origin:?}")
}

fn observe(text: &str, unit: bool, stages: &mut Vec<String>) -> Result<Produced, String> {
    let mut sources = SourceMap::new();
    let source = step("source", sources.add_source("consume.ko", text), stages)?;
    let lexed = step("lex", lex(&sources, source), stages)?;
    let parsed = step("parse", parse_file(&sources, &lexed), stages)?;
    gate(
        "parse diagnostics",
        parsed.diagnostics().is_empty(),
        parsed.diagnostics(),
        stages,
    )?;
    let program = lower_source(&sources, &parsed, unit, stages)?;
    step(
        "verify SSA",
        super::verify::verify_program(&program),
        stages,
    )?;
    // Read only the real program. Do not infer or manufacture owner identities.
    stages.push(format!("actual SSA:\n{}", super::render_program(&program)));
    let llvm = step(
        "in-memory LLVM",
        crate::llvm::render_verified_program(&program),
        stages,
    )?;
    let headers: Vec<_> = llvm
        .lines()
        .filter(|line| line.starts_with("%koven.map.t"))
        .collect();
    stages.push(format!(
        "actual LLVM Map headers: {headers:?}; bytes={}",
        llvm.len()
    ));
    Ok(Produced {
        program,
        llvm,
        sources,
    })
}

#[test]
fn map_consume_owned_locals_reach_verified_llvm() {
    let mut observations = Vec::with_capacity(12);
    for (name, text) in CASES {
        for unit in [false, true] {
            let mut stages = Vec::new();
            let (outcome, produced) = match observe(text, unit, &mut stages) {
                Ok(produced) => (Ok(()), Some(produced)),
                Err(error) => (Err(error), None),
            };
            observations.push(Observation {
                label: format!("{name}/{}", if unit { "unit" } else { "single" }),
                stages,
                outcome,
                produced,
            });
        }
    }
    // Preserve the complete original stage observations before any assertions.
    for observation in &observations {
        diagnostic!(
            "MAP CONSUME {}\n{}\noutcome={:?}",
            observation.label,
            observation.stages.join("\n"),
            observation.outcome
        );
    }
    assert_eq!(observations.len(), 12);
    assert!(
        observations
            .iter()
            .all(|observation| observation.outcome.is_ok()),
        "all twelve ordinary-source observations must reach verified LLVM; see complete observations above"
    );
    // Identity queries run only after every source and both entries were observed.
    for (index, observation) in observations.iter().enumerate() {
        let produced = observation
            .produced
            .as_ref()
            .expect("successful real output");
        let (_, text) = CASES[index / 2];
        identity::assert_identity(
            &observation.label,
            text,
            &produced.sources,
            &produced.program,
            &produced.llvm,
        );
    }
}

pub(in crate::ssa) mod unit_lower_borrow_string_tests {
//! SPEC-0274: String binary views preserve the caller's shared owner.

use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::super::{
    model::{
        Definition, EntityId, EntityType, Function, LoanKind, Operation, Program, SsaTypeKind,
    },
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn unit_borrow_string_binary_preserves_parameter_loans_and_owned_operands() {
    let program = lower_fixture(
        r#"package p
        fun same(a: String, b: String): Boolean = ((a)) == (b)
        fun different(a: String, b: String): Boolean = (a) != ((b))
        fun decorated(text: String): String = (text) + "!"
        fun prefix(text: String): String = ">" + (text)
        fun mixed(text: String, own suffix: String): String = text + suffix
        "#,
        r#"package q
        fun entry(): Unit {
            val source = "界\0" + "é"
            val same = p.same(source, "界\0é")
            val different = p.different("other", source)
            println(p.decorated(source))
            println(p.prefix(source))
            println(p.mixed(source, "?"))
            println(source)
        }"#,
    );
    for name in ["p.same", "p.different"] {
        let function = function(&program, name);
        let parameters = &function.blocks[0].parameters;
        assert!(matches!(
            parameters.as_slice(),
            [EntityId::Loan(_), EntityId::Loan(_)]
        ));
        assert!(function.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::StringEqual { left, right } if [left, right] == parameters.as_slice()
        )));
        assert!(
            function.instructions.iter().all(|instruction| !matches!(
                instruction.operation,
                Operation::BorrowBegin { .. }
                    | Operation::BorrowEnd { .. }
                    | Operation::Drop { .. }
            )),
            "shared parameters belong to the caller, not the callee"
        );
    }
    let different = function(&program, "p.different");
    assert!(
        different
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::BooleanNot { .. }))
    );

    for name in ["p.decorated", "p.mixed"] {
        let function = function(&program, name);
        let borrowed = function.blocks[0].parameters[0];
        assert!(matches!(borrowed, EntityId::Loan(_)));
        assert!(function.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::StringConcat { left, right: EntityId::Value(_) } if left == borrowed
        )));
    }
    let mixed = function(&program, "p.mixed");
    let prefix = function(&program, "p.prefix");
    assert!(
        prefix.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::StringConcat { left: EntityId::Value(_), right }
                if right == prefix.blocks[0].parameters[0]
        )),
        "Borrow String also remains a loan when it is the right operand"
    );
    let EntityId::Value(suffix) = mixed.blocks[0].parameters[1] else {
        panic!("explicit own suffix remains an owned operand");
    };
    assert_eq!(
        mixed
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation, Operation::Drop { owner } if owner == suffix
            ))
            .count(),
        1,
        "only the consumed suffix owner is cleaned up by mixed"
    );
    assert!(
        !program.modules[0]
            .functions
            .iter()
            .flat_map(|function| &function.instructions)
            .any(|instruction| matches!(
                instruction.operation,
                Operation::StringClone { .. } | Operation::SharedRetain { .. }
            )),
        "binary shared reads must not allocate replacement owners"
    );
}

#[test]
fn unit_borrow_string_binary_uses_rebound_loan_after_rhs_control_flow() {
    let program = lower_fixture(
        r#"package p
        fun choose(text: String, own flag: Boolean): String {
            val joined = (text) + (if (flag) "界" else "")
            println(text)
            return joined
        }"#,
        r#"package q
        fun entry(): Unit {
            val source = "dynamic" + "é"
            println(p.choose(source, true))
            println(p.choose(source, false))
            println(source)
        }"#,
    );
    let function = function(&program, "p.choose");
    let initial = function.blocks[0].parameters[0];
    let view = function
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::StringConcat { left, .. } => Some(left),
            _ => None,
        })
        .expect("concat has a String view");
    assert_ne!(
        view, initial,
        "RHS CFG replaces the entry loan with an edge parameter"
    );
    let EntityId::Loan(loan) = view else {
        panic!("concat uses a shared loan")
    };
    let entity = function.entity(view).unwrap();
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = entity.ty
    else {
        panic!("shared String loan remains typed after CFG");
    };
    assert_eq!(
        program.modules[0].types[target.index()],
        SsaTypeKind::StringOwner
    );
    assert!(
        matches!(entity.definition, Definition::BlockParameter { block, .. } if block != function.blocks[0].id)
    );
    assert!(
        function.instructions.iter().any(|instruction| matches!(
            instruction.operation, Operation::PrintString { value } if value == loan
        )),
        "pending operand and later source use share the same rebound loan"
    );
    assert!(
        !function.instructions.iter().any(|instruction| matches!(
            instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan
        )),
        "the callee does not end its incoming shared loan"
    );
}

#[test]
fn unit_borrow_string_comparisons_use_rebound_loans_after_rhs_control_flow() {
    let program = lower_fixture(
        r#"package p
        fun sameAfter(text: String, own flag: Boolean): Boolean {
            val result = (text) == (if (flag) "界\0é" else "界\0ê")
            println(text)
            return result
        }
        fun differentAfter(text: String, own flag: Boolean): Boolean {
            val result = (text) != (if (flag) "界\0é" else "界\0ê")
            println(text)
            return result
        }"#,
        r#"package q
        fun entry(): Unit {
            val source = "界\0" + "é"
            val equal = p.sameAfter(source, true)
            val unequal = p.sameAfter(source, false)
            val notDifferent = p.differentAfter(source, true)
            val different = p.differentAfter(source, false)
            println(source)
        }"#,
    );
    for name in ["p.sameAfter", "p.differentAfter"] {
        let function = function(&program, name);
        let (view, result) = function
            .instructions
            .iter()
            .find_map(|instruction| match instruction.operation {
                Operation::StringEqual { left, .. } => Some((left, instruction.results[0])),
                _ => None,
            })
            .expect("comparison reads both String views");
        assert_ne!(
            view, function.blocks[0].parameters[0],
            "comparison must use the loan transported through the RHS branch"
        );
        let EntityId::Loan(loan) = view else {
            panic!("comparison preserves the Borrow loan")
        };
        let entity = function.entity(view).unwrap();
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = entity.ty
        else {
            panic!("rebound comparison view is a shared loan");
        };
        assert_eq!(
            program.modules[0].types[target.index()],
            SsaTypeKind::StringOwner
        );
        assert!(
            matches!(entity.definition, Definition::BlockParameter { block, .. }
            if block != function.blocks[0].id)
        );
        assert!(
            function.instructions.iter().any(|instruction| matches!(
                instruction.operation, Operation::PrintString { value } if value == loan
            )),
            "the source remains readable with the same loan after comparison"
        );
        assert!(
            !function.instructions.iter().any(|instruction| matches!(
                instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan
            )),
            "comparison must not end its incoming loan"
        );
        let negations = function
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::BooleanNot { operand } => Some(EntityId::Value(operand)),
                _ => None,
            })
            .collect::<Vec<_>>();
        if name == "p.differentAfter" {
            assert_eq!(
                negations,
                [result],
                "!= negates the exact StringEqual result"
            );
        } else {
            assert!(negations.is_empty(), "== preserves the StringEqual result");
        }
    }
}

fn function<'a>(program: &'a Program, name: &str) -> &'a Function {
    program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .expect("reachable helper exists")
}

fn lower_fixture(provider: &str, consumer: &str) -> Program {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", provider);
    let (consumer_source, consumer) = parsed(&mut sources, "q/consumer.ko", consumer);
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    let lower = |inputs: &[_]| {
        lower_scalar_unit_with_entry(
            &sources,
            inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "q", "entry"),
        )
        .expect("Borrow String operations must lower through the unit entry")
    };
    let (program, entry) = lower(&inputs);
    let (reversed, reverse_entry) = lower(&[inputs[1], inputs[0]]);
    assert_text_eq!(render_program(&program), render_program(&reversed));
    assert_eq!(
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap(),
        crate::llvm::render_verified_program_with_entry(&reversed, reverse_entry).unwrap(),
    );
    program
}
}

pub(in crate::ssa) mod unit_lower_straight_line_tests {
//! E2 Stage B: original straight-line test delegated to the verified Unit contract.
use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::name_resolution::SymbolKind;

use super::super::{
    model::{
        CheckedArithmeticOperator, EntityId, EntityType, Function, LoanKind, Operation, Origin,
        PlaceAccess, ScalarConstant, TerminatorKind, ValueId,
    },
    render::render_program,
    verify::verify_program,
};
use crate::{
    llvm::render_verified_program, native_tests::boxed_enum_tests::unit_source::UnitSource,
};

// Byte-for-byte source from lowers_straight_line_blocks_locals_assignments_and_returns.
// Do not add own, a package, a wrapper, or a synthetic main to this comparison.
const SOURCE: &str = "fun compute(input: Int): Int {\n\
             val doubled: Int = input + input\n\
             var total: Int = doubled\n\
             { total += 1 }\n\
             return total\n\
         }\n\
         fun observe(input: Int): Unit {\n\
             val local: Int = input\n\
         }";

pub(in crate::ssa) fn assert_original_straight_line_roots_through_strict_memory_document_unit() {
    // UnitSource uses MemoryDocument, strict final admission, and real ownership products.
    let unit = UnitSource::analyze("lowering.ko", SOURCE).expect("strict original Unit source");
    unit.assert_frontend_clean();
    assert_eq!(
        unit.sources().source_text(unit.parsed().source_id()),
        Ok(SOURCE)
    );
    let mut counts = Vec::new();
    for (name, expected) in [("compute", 2), ("observe", 0)] {
        let declaration = unit.declaration(name, SymbolKind::Function);
        let index = unit.names().names().index();
        let declared = &index.declarations()[declaration.index()];
        assert_eq!(declared.id(), declaration);
        assert_eq!(declared.name(), name);
        let package = index.packages()[declared.package().index()]
            .name()
            .expect("original memory document has a valid default package");
        assert!(package.segments().is_empty());
        // Unit declaration roots use koven.<name>.d<declaration index>; these roots
        // have no package, generic arguments, static Self, or callable specialization.
        let expected_name = format!("koven.{}.d{}", declared.name(), declaration.index());
        let span = unit.declaration_span(declaration);
        let origin = Origin::Source(span);
        let (program, entry) = unit
            .lower_entry(declaration)
            .expect("real declaration root");
        verify_program(&program).expect("Unit SSA including ownership verifies");
        let module = program.module(entry.module()).expect("entry module");
        let function = module.function(entry).expect("selected source function");
        assert_eq!(function.origin, origin);
        assert_eq!(function.name, expected_name);
        assert_eq!(
            module
                .functions
                .iter()
                .filter(|f| f.origin == origin)
                .count(),
            1
        );
        assert_eq!(checked_count(function), expected);
        assert_eq!(abort_count(function), expected);
        counts.push(abort_count(function));

        let [entry_loan @ EntityId::Loan(_)] = function.blocks[0].parameters.as_slice() else {
            panic!("unchanged default Int parameter must remain Borrow");
        };
        assert!(matches!(
            function.entity(*entry_loan).expect("entry loan").ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        ));
        for instruction in &function.instructions {
            if matches!(instruction.operation, Operation::CheckedArithmetic { .. }) {
                let Origin::Source(operation_span) = instruction.origin else {
                    panic!("checked operation has original source provenance");
                };
                assert_eq!(operation_span.source_id(), span.source_id());
                assert!(
                    span.start() <= operation_span.start() && operation_span.end() <= span.end()
                );
            }
        }
        if name == "compute" {
            assert_compute(function, *entry_loan);
        } else {
            assert!(function.return_types.is_empty());
            assert!(function.instructions.iter().any(|instruction| matches!(
                instruction.operation,
                Operation::Read { source: PlaceAccess::Loan(loan) } if EntityId::Loan(loan) == *entry_loan
            )), "observe must read its input for the original local binding");
            assert_eq!(returns(function), [Vec::<ValueId>::new()]);
        }
        let ssa = render_program(&program);
        // Restore the original named-function renderer oracle per selected root,
        // not a claim of whole-file Program equivalence across lowering routes.
        assert_eq!(
            ssa.matches(&format!("  func {expected_name:?}(")).count(),
            1
        );
        assert_eq!(ssa.matches("checked.add").count(), expected);
        assert_eq!(ssa.matches("abort @source").count(), expected);
        let llvm = render_verified_program(&program).expect("original Unit root LLVM verifies");
        let (repeated, repeated_entry) = unit.lower_entry(declaration).expect("repeat same root");
        verify_program(&repeated).expect("repeated SSA and ownership verify");
        // FunctionId carries a program-local owner; compare stable indices across runs.
        assert_eq!(entry.module().index(), repeated_entry.module().index());
        assert_eq!(entry.index(), repeated_entry.index());
        assert_eq!(function.id, entry);
        let repeated_module = repeated
            .module(repeated_entry.module())
            .expect("repeated entry module belongs to repeated program");
        let repeated_function = repeated_module
            .function(repeated_entry)
            .expect("repeated selected source function");
        assert_eq!(repeated_function.id, repeated_entry);
        assert_eq!(repeated_function.origin, origin);
        assert_eq!(
            repeated_module
                .functions
                .iter()
                .filter(|f| f.origin == origin)
                .count(),
            1,
            "repeated root retains unique original declaration identity"
        );
        assert_text_eq!(ssa, render_program(&repeated));
        assert_text_eq!(
            llvm,
            render_verified_program(&repeated).expect("repeated LLVM verifies")
        );
    }
    assert_eq!(counts, [2, 0]);
}

fn checked_count(function: &Function) -> usize {
    function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::CheckedArithmetic { .. }))
        .count()
}

fn abort_count(function: &Function) -> usize {
    function
        .blocks
        .iter()
        .filter(|block| {
            block
                .terminator
                .as_ref()
                .is_some_and(|terminator| matches!(terminator.kind, TerminatorKind::Abort))
        })
        .count()
}

fn returns(function: &Function) -> Vec<Vec<ValueId>> {
    function
        .blocks
        .iter()
        .filter_map(|block| match &block.terminator.as_ref()?.kind {
            TerminatorKind::Return { values } => Some(values.clone()),
            _ => None,
        })
        .collect()
}

fn producer(function: &Function, mut value: ValueId) -> &Operation {
    // Follow finite Copy chains so assertions express source dataflow, not copy counts.
    for _ in 0..=function.instructions.len() {
        let instruction = function
            .instructions
            .iter()
            .find(|instruction| instruction.results.contains(&EntityId::Value(value)))
            .expect("value has an instruction producer");
        if let Operation::Copy { source } = instruction.operation {
            value = source;
        } else {
            return &instruction.operation;
        }
    }
    panic!("cyclic Copy chain");
}

fn assert_compute(function: &Function, entry_loan: EntityId) {
    let checked = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::CheckedArithmetic { .. }))
        .collect::<Vec<_>>();
    assert_eq!(checked.len(), 2);
    let mut current_loan = entry_loan;
    let mut current_block = function.blocks[0].id;
    for instruction in &checked {
        assert!(matches!(
            instruction.operation,
            Operation::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Add,
                ..
            }
        ));
        assert_eq!(
            instruction.block, current_block,
            "ordered straight-line success chain"
        );
        let [EntityId::Value(_), EntityId::Value(failure)] = instruction.results.as_slice() else {
            panic!("checked add produces its result and failure flag");
        };
        let block = function.block(current_block).expect("checked block");
        assert_eq!(block.instructions.last(), Some(&instruction.id));
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &block.terminator.as_ref().expect("checked terminator").kind
        else {
            panic!("checked add ends with failure conditional");
        };
        assert_eq!(condition, failure);
        assert_ne!(when_true.target, when_false.target);
        // The second add carries the first success block's rebound loan, not entry IDs.
        assert_eq!(when_true.arguments, [current_loan]);
        assert_eq!(when_false.arguments, [current_loan]);
        for edge in [when_true, when_false] {
            let target = function.block(edge.target).expect("target");
            assert_eq!(target.parameters.len(), 1);
            assert!(matches!(target.parameters[0], EntityId::Loan(_)));
            assert_eq!(
                function
                    .entity(target.parameters[0])
                    .expect("target loan")
                    .ty,
                function.entity(current_loan).expect("current loan").ty
            );
        }
        let abort = function
            .block(when_true.target)
            .expect("abort block")
            .terminator
            .as_ref()
            .expect("abort terminator");
        assert!(matches!(abort.kind, TerminatorKind::Abort));
        assert_eq!(
            abort.origin, instruction.origin,
            "exact checked source Abort"
        );
        current_block = when_false.target;
        current_loan = function
            .block(current_block)
            .expect("success block")
            .parameters[0];
    }
    let Operation::CheckedArithmetic { left, right, .. } = checked[0].operation else {
        unreachable!()
    };
    for operand in [left, right] {
        assert!(
            matches!(producer(function, operand),
                Operation::Read { source: PlaceAccess::Loan(loan) } if EntityId::Loan(*loan) == entry_loan
            ),
            "doubled reads the unchanged Borrow input twice"
        );
    }
    let Operation::CheckedArithmetic { left, right, .. } = checked[1].operation else {
        unreachable!()
    };
    assert!(
        std::ptr::eq(producer(function, left), &checked[0].operation),
        "nested total += 1 uses doubled through the mutable local"
    );
    assert!(matches!(
        producer(function, right),
        Operation::Constant(ScalarConstant::Integer(1))
    ));
    let result = returns(function);
    let [values] = result.as_slice() else {
        panic!("one successful return");
    };
    let [value] = values.as_slice() else {
        panic!("compute returns one Int");
    };
    assert!(
        std::ptr::eq(producer(function, *value), &checked[1].operation),
        "return observes the nested assignment, not stale doubled"
    );
    assert!(matches!(
        function
            .block(current_block)
            .expect("final success")
            .terminator
            .as_ref()
            .expect("return")
            .kind,
        TerminatorKind::Return { .. }
    ));
}
}
