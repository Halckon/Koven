//! Ordinary-source Range value ABI probes; no fabricated facts or native execution.
mod llvm_contract;

use super::{
    lower_frontend::orchestrate::lower_scalar_file,
    model::{EntityId, Operation, Ownership, Program, SsaTypeKind},
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
fn lower_source(text: &str, unit: bool) -> Result<Program, String> {
    let mut sources = SourceMap::new();
    let (environment, types) = standard_environments();
    let result = if unit {
        println!("STAGE unit=true parse");
        let text = format!("package p\n{text}");
        let (source, file) = parsed(&mut sources, "p/range.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/range.ko", source, &file)];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        println!("STAGE unit=true names");
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .map_err(|error| format!("names API: {error:?}"))?
            .validate()
            .map_err(|error| format!("names diagnostics: {error:?}"))?;
        println!("STAGE unit=true types");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .map_err(|error| format!("types API: {error:?}"))?;
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
            .map_err(|error| format!("complete: {error:?}"))?;
        let typed = typed
            .validate()
            .map_err(|error| format!("types validation: {error:?}"))?;
        println!("STAGE unit=true ownership");
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .map_err(|error| format!("ownership API: {error:?}"))?
            .validate()
            .map_err(|error| format!("ownership diagnostics: {error:?}"))?;
        println!("STAGE unit=true lower");
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
        println!("STAGE unit=false parse");
        let source = sources.add_source("range.ko", text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let file = parse_file(&sources, &lexed).unwrap();
        assert!(file.diagnostics().is_empty());
        println!("STAGE unit=false names");
        let names = resolve_names(&sources, &file, &environment).unwrap();
        if !names.diagnostics().is_empty() {
            return Err(format!("names diagnostics: {:?}", names.diagnostics()));
        }
        println!("STAGE unit=false types");
        let typed = check_types(&sources, &file, &names, &types).unwrap();
        complete_file_types(&sources, &file, &names, &types, &typed)
            .map_err(|error| format!("complete: {error:?}"))?;
        println!("STAGE unit=false ownership");
        let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
        if !owned.diagnostics().is_empty() || !owned.deferred().is_empty() {
            return Err(format!(
                "ownership: {:?}, deferred: {:?}",
                owned.diagnostics(),
                owned.deferred()
            ));
        }
        println!("STAGE unit=false lower");
        lower_scalar_file(&sources, &file, &names, &typed, &owned)
    }
    .map_err(|error| format!("lower: {error:?}"));
    match &result {
        Ok(program) => println!(
            "MODE unit={unit} SOURCE {text:?} SSA {}",
            render_program(program)
        ),
        Err(error) => println!("MODE unit={unit} SOURCE {text:?} LOWER {error:?}"),
    }
    result
}

pub(super) fn collect_sources(sources: &[String]) -> Vec<Result<Program, String>> {
    let mut results = Vec::new();
    for source in sources {
        for unit in [false, true] {
            println!("BEGIN unit={unit} SOURCE {source:?}");
            let result = lower_source(source, unit);
            if let Err(error) = &result {
                println!("STOP unit={unit} SOURCE {source:?} {error}");
            }
            results.push(result);
        }
    }
    results
}

fn verified(result: Result<Program, String>) -> Program {
    let program = result.expect("real Range value source must reach SSA");
    verify_program(&program).expect("ordinary source SSA verifies");
    let mut operations = 0;
    for module in &program.modules {
        for function in &module.functions {
            for instruction in &function.instructions {
                let range_value = match instruction.operation {
                    Operation::IntegerRangeConstruct { first, bound, .. } => {
                        let [EntityId::Value(range)] = instruction.results.as_slice() else {
                            panic!("one Range value result");
                        };
                        let range_type = function
                            .entity(EntityId::Value(*range))
                            .unwrap()
                            .ty
                            .semantic_type();
                        let endpoint = module
                            .integer_range_endpoint(range_type)
                            .expect("registered Range identity");
                        assert_eq!(
                            function
                                .entity(EntityId::Value(first))
                                .unwrap()
                                .ty
                                .semantic_type(),
                            endpoint
                        );
                        assert_eq!(
                            function
                                .entity(EntityId::Value(bound))
                                .unwrap()
                                .ty
                                .semantic_type(),
                            endpoint
                        );
                        *range
                    }
                    Operation::IntegerRangeContains { range, .. } => range,
                    Operation::IntegerRangeEqual { left, .. } => left,
                    _ => continue,
                };
                operations += 1;
                let ty = function
                    .entity(EntityId::Value(range_value))
                    .unwrap()
                    .ty
                    .semantic_type();
                let endpoint = module
                    .integer_range_endpoint(ty)
                    .expect("registered numeric Range");
                assert!(matches!(
                    module.type_kind(endpoint),
                    Some(SsaTypeKind::Integer {
                        bits: 32 | 64,
                        signed: true
                    })
                ));
                assert_eq!(module.type_ownership(ty), Some(Ownership::Copyable));
                assert!(module.valid_integer_range_type(ty, endpoint));
            }
        }
    }
    assert!(operations > 0);
    program
}

fn assert_operand_call_identities(program: &Program) {
    let mut matched = 0;
    for module in &program.modules {
        for function in &module.functions {
            let contains =
                function
                    .instructions
                    .iter()
                    .find_map(|instruction| match instruction.operation {
                        Operation::IntegerRangeContains { value, range } => Some((value, range)),
                        _ => None,
                    });
            let Some((value, range)) = contains else {
                continue;
            };
            let calls = function
                .instructions
                .iter()
                .filter_map(|instruction| match instruction.operation {
                    Operation::DirectCall { callee, .. } => {
                        Some((callee, instruction.results.as_slice()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            matched += 1;
            assert_eq!(calls.len(), 3);
            assert_eq!(calls[0].0, calls[1].0);
            assert_ne!(calls[0].0, calls[2].0);
            assert_eq!(calls[0].1, &[EntityId::Value(value)]);
            let construct = function
                .instructions
                .iter()
                .find(|instruction| instruction.results == [EntityId::Value(range)])
                .unwrap();
            let Operation::IntegerRangeConstruct {
                first,
                bound,
                inclusive: false,
            } = construct.operation
            else {
                panic!("membership RHS is half-open constructor");
            };
            assert_eq!(calls[1].1, &[EntityId::Value(first)]);
            assert_eq!(calls[2].1, &[EntityId::Value(bound)]);
        }
    }
    assert_eq!(matched, 1);
}

#[test]
fn range_construction_membership_and_equality_reach_verified_ssa() {
    let mut sources = Vec::new();
    for (integer, range, min, max) in [
        ("Int", "IntRange", "-2147483648", "2147483647"),
        (
            "Long",
            "LongRange",
            "-9223372036854775808L",
            "9223372036854775807L",
        ),
    ] {
        for root in ["left..right", "left..<right"] {
            sources.push(format!(
                "fun entry(left: {integer}, right: {integer}): {range} = {root}"
            ));
        }
        for root in [
            "value in (left..right)",
            "value !in (left..<right)",
            "(left..right) == (left..<right)",
            "(left..right) != (right..left)",
        ] {
            sources.push(format!(
                "fun entry(left: {integer}, right: {integer}, value: {integer}): Boolean = {root}"
            ));
        }
        for root in [
            format!("{min}..{max}"),
            format!("{max}..{max}"),
            format!("{min}..<{min}"),
            format!("{max}..<{min}"),
        ] {
            sources.push(format!("fun entry(): {range} = {root}"));
        }
    }
    let mut llvm_results = Vec::new();
    for result in collect_sources(&sources) {
        let program = verified(result);
        let llvm = crate::llvm::render_verified_program(&program);
        println!("CORE LLVM {llvm:?}");
        llvm_results.push(llvm);
    }
    assert_eq!(llvm_results.len(), 40);
    for result in llvm_results {
        result.expect("ordinary Range operation reaches verified LLVM");
    }
}

#[test]
fn range_operand_calls_and_outer_join_reach_verified_llvm() {
    let mut sources = Vec::new();
    for (integer, suffix) in [("Int", ""), ("Long", "L")] {
        sources.push(format!(
            "fun first(): {integer} {{ println(\"first\"); return 1{suffix} }}\nfun last(): {integer} {{ println(\"last\"); return 3{suffix} }}\nfun entry(): Boolean = first() in (first()..<last())"
        ));
    }
    for (integer, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        sources.push(format!("fun entry(a: {integer}, b: {integer}, flag: Boolean): {range} {{ val live = \"live\".clone(); val first = a..<b; val second = b..<a; val chosen = if (flag) first else second; println(live); return chosen }}"));
    }
    for (index, result) in collect_sources(&sources).into_iter().enumerate() {
        let program = verified(result);
        if index < 4 {
            assert_operand_call_identities(&program);
        }
        let rendered = render_program(&program);
        println!("CALL ORDER SSA {rendered}");
        println!("STAGE LLVM");
        let llvm =
            crate::llvm::render_verified_program(&program).expect("real Range LLVM verifies");
        println!("RANGE LLVM {llvm}");
        llvm_contract::assert_half_open_cfg(&llvm, if index < 4 { 1 } else { 2 });
    }
}

#[test]
fn range_values_cross_fields_containers_and_consumed_map_results() {
    let mut sources = Vec::new();
    for (integer, range) in [("Int", "IntRange"), ("Long", "LongRange")] {
        sources.push(format!("fun entry(a: {integer}, b: {integer}): Boolean {{ val original = a..b; val copy = original; return original == copy }}"));
        sources.push(format!("fun pass(own value: {range}): {range} = value\nfun entry(a: {integer}, b: {integer}): Boolean = pass(a..b) == (a..b)"));
        for class in ["class", "value class"] {
            sources.push(format!("{class} Holder(val value: {range})\nfun entry(a: {integer}, b: {integer}): Boolean {{ val holder = Holder(a..b); return a in holder.value }}"));
        }
        sources.push(format!("fun entry(a: {integer}, b: {integer}): Boolean {{ val values = listOf(a..b); val first = values[0]; return (a in first) && (a in values[0]) }}"));
        sources.push(format!("fun entry(a: {integer}, b: {integer}): Boolean {{ var values = mutableMapOf<Int, {range}>(); values.put(1, a..b); borrow val item = values.requireValue(1); val copy = item; return (a in copy) && values.contains(1) }}"));
        for operation in ["get", "remove"] {
            sources.push(format!("fun entry(a: {integer}, b: {integer}): Boolean {{ var values = mutableMapOf<Int, {range}>(); values.put(1, a..b); val found = values.{operation}(1); return if (found != null) (a in found) else false }}"));
        }
    }
    let mut llvm_results = Vec::new();
    for (index, result) in collect_sources(&sources).into_iter().enumerate() {
        let source = &sources[index / 2];
        if source.contains("borrow val item") {
            let error = result
                .err()
                .expect("existing whole-function borrow CFG frontier remains rejected");
            let start = source.find("a..b").unwrap() + if index % 2 == 1 { 10 } else { 0 };
            assert_eq!(
                error,
                format!(
                    "lower: LoweringError {{ kind: UnsupportedNode, span: Some(Span {{ source_id: SourceId(0), start: {start}, end: {} }}) }}",
                    start + 4
                )
            );
        } else {
            let program = verified(result);
            let llvm = crate::llvm::render_verified_program(&program);
            println!("STORAGE LLVM index={index} {llvm:?}");
            llvm_results.push(llvm);
        }
    }
    assert_eq!(llvm_results.len(), 28);
    for result in llvm_results {
        result.expect("ordinary Range storage reaches verified LLVM");
    }
}

#[test]
fn range_require_value_preserves_explicit_borrow_contract() {
    let mut sources = [("Int", "IntRange"), ("Long", "LongRange")].map(|(integer, range)| {
        format!("fun entry(a: {integer}, b: {integer}): Boolean {{ var values = mutableMapOf<Int, {range}>(); values.put(1, a..b); val copy = values.requireValue(1); return (a in copy) && values.contains(1) }}")
    }).to_vec();
    for (integer, range, zero) in [("Int", "IntRange", "0"), ("Long", "LongRange", "0L")] {
        sources.push(format!("fun make(a: {integer}, b: {integer}): {range} = a..b\nfun check(value: {range}): Boolean = {zero} in value\nfun entry(a: {integer}, b: {integer}): Unit {{ var values = mutableMapOf<Int, {range}>(); values.put(1, make(a, b)); borrow val item = values.requireValue(1); val copy = item; check(copy); values.contains(1) }}"));
    }
    for (index, result) in collect_sources(&sources).into_iter().enumerate() {
        if index < 4 {
            let error = result
                .err()
                .expect("ordinary val cannot directly bind a borrowed result");
            assert!(error.starts_with("ownership"), "{error}");
            assert_eq!(error.matches("code: L0163").count(), 1, "{error}");
            assert_eq!(
                error.matches("Diagnostic { severity:").count(),
                1,
                "{error}"
            );
            let source = &sources[index / 2];
            let start = source.find("values.requireValue(1)").unwrap()
                + if index % 2 == 1 { 10 } else { 0 };
            assert!(
                error.contains(&format!(
                    "primary_span: Span {{ source_id: SourceId(0), start: {start}, end: {} }}",
                    start + "values.requireValue(1)".len()
                )),
                "{error}"
            );
        } else {
            let program = verified(result);
            let llvm = crate::llvm::render_verified_program(&program)
                .expect("explicit borrow and Copyable read reach LLVM");
            println!("REQUIRE VALUE LLVM {llvm}");
        }
    }
}
