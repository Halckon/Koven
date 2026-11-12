//! Ordinary-source Range value ABI probes; no fabricated facts or native execution.
use crate::test_support::diagnostics::{
    assert_diagnostic, assert_eq_diagnostic, diagnostic, expect,
};
mod llvm_contract {
    //! Read-only checks of LLVM generated from the ordinary dynamic Range fixtures.
    use std::collections::BTreeMap;

    pub(super) fn assert_half_open_cfg(llvm: &str, expected: usize) {
        let mut count = 0;
        for function in llvm.split("define ").skip(1) {
            let body = function.split_once("\n}").expect("defined function body").0;
            let mut blocks: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
            let mut current = None;
            for line in body.lines() {
                if !line.starts_with(' ')
                    && let Some((label, _)) = line.split_once(':')
                {
                    current = Some(label);
                } else if let Some(label) = current
                    && !line.trim().is_empty()
                    && !line.trim().starts_with(';')
                {
                    blocks.entry(label).or_default().push(line.trim());
                }
            }
            let mut previous_ready = None;
            // Walk textual instruction order, not the map's label ordering.
            for line in body.lines().map(str::trim) {
                let Some((predicate, operands)) = line.split_once(" = icmp slt ") else {
                    continue;
                };
                if !predicate.ends_with(".nonempty") {
                    continue;
                }
                count += 1;
                let (width, operands) = operands.split_once(' ').unwrap();
                assert!(matches!(width, "i32" | "i64"));
                let (_, bound) = operands.split_once(", ").unwrap();
                let (origin, instructions) = blocks
                    .iter()
                    .find(|(_, lines)| lines.contains(&line))
                    .unwrap();
                if let Some(ready) = previous_ready {
                    assert_eq!(
                        *origin, ready,
                        "next constructor continues in prior ready block"
                    );
                }
                let prefix = format!("br i1 {predicate}, label %");
                let branch = instructions
                    .iter()
                    .find_map(|line| line.strip_prefix(&prefix))
                    .unwrap();
                let (present, absent) = branch.split_once(", label %").unwrap();
                let present_lines = &blocks[present];
                let absent_lines = &blocks[absent];
                let suffix = format!(" = sub {width} {bound}, 1");
                let last = present_lines
                    .iter()
                    .find_map(|line| line.strip_suffix(&suffix))
                    .unwrap();
                assert_eq!(
                    present_lines
                        .iter()
                        .filter(|line| line.contains(" = sub "))
                        .count(),
                    1
                );
                assert!(!absent_lines.iter().any(|line| line.contains(" = sub ")));
                assert!(!instructions.iter().any(|line| line.contains(&suffix)));
                let ready = present_lines
                    .last()
                    .unwrap()
                    .strip_prefix("br label %")
                    .unwrap();
                assert_eq!(absent_lines.last().unwrap(), &format!("br label %{ready}"));
                let phi = format!(" = phi {width} [ {last}, %{present} ], [ 0, %{absent} ]");
                assert_eq!(
                    blocks[ready]
                        .iter()
                        .filter(|line| line.contains(&phi))
                        .count(),
                    1
                );
                previous_ready = Some(ready);
            }
        }
        assert_eq!(count, expected);
    }
}

use super::{
    lower_frontend::orchestrate::lower_scalar_file,
    model::{
        EntityId, FunctionId, Operation, Origin, Ownership, Program, SsaTypeKind, TerminatorKind,
    },
    render::render_program,
    unit_lower::lower_owned_unit_with_entry,
    verify::verify_program,
};
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    lexer::lex,
    name_resolution::{
        DeclarationId, SourceUnitInput, SymbolKind, index_compilation_unit,
        resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_ownership, owned_compilation_unit_view},
    parser::parse_file,
    source::{SourceMap, Span},
    type_checking::{
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

pub(super) struct SourceProgram {
    pub(super) program: Program,
    pub(super) functions: Vec<(String, Span)>,
    pub(super) entry: FunctionId,
    entry_span: Span,
    declaration: DeclarationId,
}

fn lower_unit_source(text: &str, package_layout: bool) -> Result<SourceProgram, String> {
    let mut sources = SourceMap::new();
    let text = if package_layout {
        format!("package p\n{text}")
    } else {
        text.to_owned()
    };
    let path = if package_layout {
        "p/range.ko"
    } else {
        "range.ko"
    };
    let source = sources.add_source(path, &text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty());
    let inputs = [if package_layout {
        SourceUnitInput::new("root", path, source, &file)
    } else {
        SourceUnitInput::memory_document(source, &file)
    }];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .map_err(|error| format!("names API: {error:?}"))?
        .validate()
        .map_err(|error| format!("names diagnostics: {error:?}"))?;
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .map_err(|error| format!("types API: {error:?}"))?;
    complete_compilation_unit_types(&sources, &inputs, &names, &types, &raw)
        .map_err(|error| format!("complete: {error:?}"))?;
    let (typed, owned) = analyze_basic_unit_ownership(&sources, &inputs, &names, &types, raw)
        .map_err(|error| format!("ownership API: {error:?}"))?
        .into_result()
        .map_err(|error| format!("types validation: {error:?}"))?;
    let owned = owned
        .validate()
        .map_err(|error| format!("ownership diagnostics: {error:?}"))?;
    let mut entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "entry"
                && declaration.kind() == SymbolKind::Function
                && declaration.name_span().source_id() == source
        });
    let declaration = entries.next().expect("one source entry exists");
    let entry = declaration.id();
    let entry_span = file.ast().items().get(declaration.root()).unwrap().span();
    assert!(entries.next().is_none(), "source entry is unique");
    let unit = owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned)
        .map_err(|error| format!("owned unit view: {error:?}"))?;
    let (program, entry) =
        lower_owned_unit_with_entry(&unit, entry).map_err(|error| format!("lower: {error:?}"))?;
    let functions = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.kind() == SymbolKind::Function
                && declaration.name_span().source_id() == source
        })
        .map(|declaration| {
            let span = file.ast().items().get(declaration.root()).unwrap().span();
            (declaration.name().to_owned(), span)
        })
        .collect();
    Ok(SourceProgram {
        program,
        functions,
        entry,
        entry_span,
        declaration: declaration.id(),
    })
}

/// Original MemoryDocument layout; collect all lower/verify/render results before oracles.
/// Only the declared entry and its actual reachable closure are lowering roots.
pub(super) fn collect_memory_entry_observations(
    sources: &[String],
) -> Vec<Result<SourceProgram, String>> {
    sources
        .iter()
        .enumerate()
        .map(|(case, text)| {
            diagnostic!("SHAPE4 MEMORY case={case} path=range.ko SOURCE {text:?} admission=pending");
            let result = std::panic::catch_unwind(|| observe_memory_entry(text, case))
                .unwrap_or_else(|panic| {
                    let message = panic
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| panic.downcast_ref::<&str>().copied())
                        .unwrap_or("non-string panic");
                    Err(format!(
                        "unexpected panic (unfinished stage unknown; later stages not passed): {message}"
                    ))
                });
            diagnostic!(
                "SHAPE4 MEMORY case={case} outcome={:?} oracle=pending",
                result.as_ref().map(|_| ())
            );
            result
        })
        .collect()
}

fn observe_memory_entry(text: &str, case: usize) -> Result<SourceProgram, String> {
    let source = lower_unit_source(text, false)?;
    diagnostic!(
        "SHAPE4 MEMORY case={case} admission=ok names=ok types=ok owned=ok lower=ok declaration={:?} entry={:?} span={:?}",
        source.declaration,
        source.entry,
        source.entry_span
    );
    let module = source
        .program
        .module(source.entry.module())
        .ok_or_else(|| "identity: returned entry module is absent".to_owned())?;
    let function = module
        .function(source.entry)
        .ok_or_else(|| "identity: returned entry function is absent".to_owned())?;
    if function.origin != Origin::Source(source.entry_span)
        || source
            .functions
            .iter()
            .filter(|(name, span)| name == "entry" && *span == source.entry_span)
            .count()
            != 1
    {
        return Err("identity: same-analysis declaration/entry origin mismatch".to_owned());
    }
    // Bind coordinate-based numeric/borrowed queries to this returned entry.
    let guards = source
        .program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .flat_map(|function| {
            function.blocks.iter().filter_map(move |block| {
                block
                    .terminator
                    .as_ref()
                    .filter(|term| matches!(term.kind, TerminatorKind::NullableBranch { .. }))
                    .map(|term| (function.id, &term.origin))
            })
        })
        .collect::<Vec<_>>();
    let [(guard_function, Origin::Source(guard_span))] = guards.as_slice() else {
        return Err("identity: exactly one source nullable guard required".to_owned());
    };
    if *guard_function != source.entry
        || guard_span.source_id() != source.entry_span.source_id()
        || guard_span.start() < source.entry_span.start()
        || guard_span.end() > source.entry_span.end()
    {
        return Err(format!("identity: guard outside returned entry {guards:?}"));
    }
    let origins = function
        .instructions
        .iter()
        .map(|instruction| &instruction.origin)
        .chain(
            function
                .blocks
                .iter()
                .filter_map(|block| block.terminator.as_ref().map(|term| &term.origin)),
        );
    for origin in origins {
        if let Origin::Source(span) = origin
            && span.source_id() != source.entry_span.source_id()
        {
            return Err(format!("identity: foreign entry origin {span:?}"));
        }
    }
    verify_program(&source.program).map_err(|error| format!("SSA verify: {error:?}"))?;
    diagnostic!("SHAPE4 MEMORY case={case} identity=ok SSA=ok");
    let llvm = crate::llvm::render_verified_program(&source.program)
        .map_err(|error| format!("LLVM: {error:?}"))?;
    diagnostic!("SHAPE4 MEMORY case={case} LLVM=ok {llvm}");
    Ok(source)
}

// Temporary test-only exit boundary, not a fallback from a failed Unit attempt.
// Only the four explicit shape-boundary wholes below retain this original pipeline.
fn lower_legacy_shape_boundary(text: &str) -> Result<Program, String> {
    let mut sources = SourceMap::new();
    let (environment, types) = standard_environments();
    let source = sources.add_source("range.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    assert!(lexed.diagnostics().is_empty());
    let file = parse_file(&sources, &lexed).unwrap();
    assert!(file.diagnostics().is_empty());
    let names = resolve_names(&sources, &file, &environment).unwrap();
    if !names.diagnostics().is_empty() {
        return Err(format!("names diagnostics: {:?}", names.diagnostics()));
    }
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    complete_file_types(&sources, &file, &names, &types, &typed)
        .map_err(|error| format!("complete: {error:?}"))?;
    let owned = check_ownership(&sources, &file, &names, &typed).unwrap();
    if !owned.diagnostics().is_empty() || !owned.deferred().is_empty() {
        return Err(format!(
            "ownership: {:?}, deferred: {:?}",
            owned.diagnostics(),
            owned.deferred()
        ));
    }
    lower_scalar_file(&sources, &file, &names, &typed, &owned)
        .map_err(|error| format!("lower: {error:?}"))
}

/// Both original source layouts now use Unit; false/true means no header/package p.
pub(super) fn collect_sources(sources: &[String]) -> Vec<Result<Program, String>> {
    collect_source_functions(sources)
        .into_iter()
        .map(|result| result.map(|source| source.program))
        .collect()
}

/// Query-only declaration spans from the same parsed input that produced each Program.
pub(super) fn collect_source_functions(sources: &[String]) -> Vec<Result<SourceProgram, String>> {
    let mut results = Vec::new();
    for source in sources {
        for package_layout in [false, true] {
            let result = lower_unit_source(source, package_layout);
            match &result {
                Ok(_) => diagnostic!("UNIT layout_package={package_layout} lower=ok"),
                Err(error) => {
                    diagnostic!("UNIT layout_package={package_layout} SOURCE {source:?} {error}")
                }
            }
            results.push(result);
        }
    }
    results
}

/// Exactly four existing wholes still pin distinct Legacy/Unit SSA shapes:
/// single_direct_nullable_while_birth_and_local_retirement_reach_llvm,
/// unit_direct_nullable_while_birth_and_local_retirement_reach_llvm,
/// numeric_for_preserves_the_outer_nullable_while_proof, and
/// borrowed_for_preserves_the_outer_nullable_while_proof.
/// Remove this boundary only after those original loan/terminal/origin contracts
/// receive an explicit retirement decision; do not silently rewrite their expectations.
pub(super) fn collect_shape_boundaries(sources: &[String]) -> Vec<Result<Program, String>> {
    let mut results = Vec::new();
    for source in sources {
        for unit in [false, true] {
            let result = if unit {
                lower_unit_source(source, true).map(|source| source.program)
            } else {
                lower_legacy_shape_boundary(source)
            };
            match &result {
                Ok(_) => diagnostic!("SHAPE BOUNDARY unit={unit} lower=ok"),
                Err(error) => diagnostic!("SHAPE BOUNDARY unit={unit} SOURCE {source:?} {error}"),
            }
            results.push(result);
        }
    }
    results
}

fn verified(result: Result<Program, String>) -> Program {
    let program = expect(result, "real Range value source must reach SSA");
    expect(verify_program(&program), "ordinary source SSA verifies");
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
        diagnostic!("CORE LLVM outcome={:?}", llvm.as_ref().map(|_| "verified"));
        if let Ok(ir) = &llvm {
            diagnostic!("{ir}");
        }
        llvm_results.push(llvm);
    }
    assert_eq!(llvm_results.len(), 40);
    for result in llvm_results {
        expect(result, "ordinary Range operation reaches verified LLVM");
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
        diagnostic!("CALL ORDER SSA {rendered}");
        diagnostic!("STAGE LLVM");
        let llvm = expect(
            crate::llvm::render_verified_program(&program),
            "real Range LLVM verifies",
        );
        diagnostic!("RANGE LLVM {llvm}");
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
            diagnostic!(
                "STORAGE LLVM index={index} outcome={:?}",
                llvm.as_ref().map(|_| "verified")
            );
            if let Ok(ir) = &llvm {
                diagnostic!("{ir}");
            }
            llvm_results.push(llvm);
        }
    }
    assert_eq!(llvm_results.len(), 28);
    for result in llvm_results {
        expect(result, "ordinary Range storage reaches verified LLVM");
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
            assert_diagnostic!(error.starts_with("ownership"), "{error}");
            assert_eq_diagnostic!(error.matches("code: L0163").count(), 1, "{error}");
            assert_eq_diagnostic!(
                error.matches("Diagnostic { severity:").count(),
                1,
                "{error}"
            );
            let source = &sources[index / 2];
            let start = source.find("values.requireValue(1)").unwrap()
                + if index % 2 == 1 { 10 } else { 0 };
            assert_diagnostic!(
                error.contains(&format!(
                    "primary_span: Span {{ source_id: SourceId(0), start: {start}, end: {} }}",
                    start + "values.requireValue(1)".len()
                )),
                "{error}"
            );
        } else {
            let program = verified(result);
            let llvm = expect(
                crate::llvm::render_verified_program(&program),
                "explicit borrow and Copyable read reach LLVM",
            );
            diagnostic!("REQUIRE VALUE LLVM {llvm}");
        }
    }
}
