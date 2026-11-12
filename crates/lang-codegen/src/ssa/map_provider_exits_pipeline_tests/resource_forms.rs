//! Existing Resource storage must not be filtered by nominal shape or key syntax.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod representation {
    //! Query the actual represented storage, original Resource deinit and shared component ABI.
    use super::*;
    use crate::ssa::{
        map_provider_exits_pipeline_tests::{fixture::need, query_inputs::one},
        model::*,
    };
    use lang_frontend::parser::{Item, NameMarker};

    pub(super) fn check(
        case: &Case,
        sources: &SourceMap,
        parsed: &lang_frontend::parser::ParsedFile,
        program: &Program,
    ) -> Result<(), Failure> {
        let module = one(program.modules.iter(), "one Resource form module")?;
        let find = |wanted: &str| {
            one(parsed.ast().items().iter().filter_map(|(_,n)|matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok(wanted)).then_some(n.span())),"original function span")
        };
        let scan_span = find("scan")?;
        let read_span = find("read")?;
        let scan = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(scan_span)),
            "actual original scan",
        )?;
        let entry = scan.block(scan.entry_block().unwrap()).unwrap();
        let anchor = one(
            entry.parameters.iter().copied(),
            "actual owned source parameter",
        )?;
        let (kind, key, value) = module
            .map_container(scan.entity(anchor).unwrap().ty.semantic_type())
            .ok_or_else(|| failure("real represented Map", anchor))?;
        need(
            kind == if case.mutable {
                MapContainerKind::MutableMap
            } else {
                MapContainerKind::Map
            },
            "exact original container kind",
            kind,
        )?;
        need(
            if case.representation == 2 {
                module.type_kind(key)
                    == Some(&SsaTypeKind::Integer {
                        bits: 32,
                        signed: true,
                    })
            } else {
                module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
            },
            "exact original key storage",
            key,
        )?;
        let resource = match (case.representation, module.type_kind(value)) {
            (
                1,
                Some(SsaTypeKind::Aggregate {
                    fields,
                    ownership: Ownership::MoveOnly,
                    ..
                }),
            ) if fields.len() == 1 => fields[0],
            (3, Some(SsaTypeKind::NullableHandle { inner })) => *inner,
            (0 | 2, Some(SsaTypeKind::HeapOwner { .. })) => value,
            _ => {
                return Err(failure(
                    "existing Resource representation",
                    module.type_kind(value),
                ));
            }
        };
        let Some(SsaTypeKind::HeapOwner { payload, .. }) = module.type_kind(resource) else {
            return Err(failure("underlying Resource heap owner", resource));
        };
        if case.representation == 0 || case.representation == 3 {
            let payload = payload.ok_or_else(|| failure("actual Resource fields", resource))?;
            need(
                strings(module, payload) == 1,
                "owned String field remains in original Resource payload",
                module.type_kind(payload),
            )?;
        }
        let class = one(
            parsed.ast().items().iter().filter_map(|(_, n)| {
                let Item::Classifier(class) = n.payload() else {
                    return None;
                };
                matches!(class.name,NameMarker::Present(name) if sources.slice(name)==Ok("Resource"))
                    .then_some(class)
            }),
            "original Resource declaration",
        )?;
        let deinit = one(
            class
                .body
                .as_ref()
                .unwrap()
                .members
                .iter()
                .filter_map(|id| {
                    let n = parsed.ast().items().get(*id).ok()?;
                    matches!(n.payload(), Item::Deinit { .. }).then_some(n.span())
                }),
            "original Resource deinit span",
        )?;
        let actual = module
            .deinits
            .get(&resource)
            .and_then(|id| module.function(*id))
            .ok_or_else(|| failure("actual Resource deinit", resource))?;
        need(
            actual.origin == Origin::Source(deinit),
            "Resource storage uses its original deinit",
            actual.id,
        )?;
        let reader = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(read_span)),
            "actual reader",
        )?;
        let call = one(
            scan.instructions.iter().filter(
                |i| matches!(&i.operation,Operation::DirectCall{callee,..} if *callee==reader.id),
            ),
            "actual component reader call",
        )?;
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        let key_loan = one(
            scan.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. })),
            "actual provider key loan",
        )?;
        let value_loan = one(
            scan.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
            "actual provider value loan",
        )?;
        need(
            key_loan.results.len() == 1
                && value_loan.results.len() == 1
                && arguments.as_slice() == [key_loan.results[0], value_loan.results[0]],
            "reader consumes the actual current K/V loan operands",
            (call, key_loan, value_loan),
        )?;
        for component in [key_loan, value_loan] {
            let EntityId::Loan(loan) = component.results[0] else {
                return Err(failure("actual Shared component", component));
            };
            let end = one(
                scan.instructions
                    .iter()
                    .filter(|i| matches!(i.operation,Operation::BorrowEnd{loan:l} if l==loan)),
                "actual component End",
            )?;
            let block = scan.block(call.block).unwrap();
            need(
                end.block == call.block
                    && block.instructions.iter().position(|id| *id == call.id)
                        < block.instructions.iter().position(|id| *id == end.id),
                "actual component ends after its read",
                (call, end),
            )?;
        }
        let parameters = &reader
            .block(reader.entry_block().unwrap())
            .unwrap()
            .parameters;
        need(
            arguments.len() == 2 && parameters.len() == 2,
            "actual two-component reader ABI",
            arguments,
        )?;
        for (slot, target) in [key, value].into_iter().enumerate() {
            let ty = EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            };
            need(
                scan.entity(arguments[slot]).unwrap().ty == ty
                    && reader.entity(parameters[slot]).unwrap().ty == ty,
                "shared component preserves exact original represented type",
                (slot, target),
            )?;
        }
        need(!scan.instructions.iter().any(|i|matches!(i.operation,Operation::Drop{owner} if [value,resource].contains(&scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()))),"provider never extracts or drops Resource component owner",scan.id)
    }
    fn strings(module: &Module, ty: SsaTypeId) -> usize {
        match module.type_kind(ty) {
            Some(SsaTypeKind::StringOwner) => 1,
            Some(SsaTypeKind::Aggregate { fields, .. }) => {
                fields.iter().map(|&f| strings(module, f)).sum()
            }
            _ => 0,
        }
    }
}
use super::{
    Report,
    fixture::{Failure, clean, failure},
    pipeline, source_forms,
};
use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub mutable: bool,
    pub representation: usize,
}
pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for representation in 0..4 {
        for mutable in [false, true] {
            for exit in 0..4 {
                let tag = ["field", "structural", "int-key", "nullable-field"][representation];
                let kind = if mutable { "MutableMap" } else { "Map" };
                let key = if representation == 2 { "Int" } else { "String" };
                let payload = match representation {
                    1 => "Payload",
                    3 => "Resource?",
                    _ => "Resource",
                };
                let class = if representation == 0 || representation == 3 {
                    "class Resource(val tag:String){deinit(){println(this.tag)}}"
                } else {
                    "class Resource{deinit(){println(\"resource-drop\")}}"
                };
                let structural = if representation == 1 {
                    "value class Payload(val resource:Resource)"
                } else {
                    ""
                };
                let construct = match representation {
                    0 | 3 => "Resource(\"field-drop\".clone())",
                    1 => "Payload(Resource())",
                    _ => "Resource()",
                };
                let first = if key == "Int" { "1" } else { "\"z\".clone()" };
                let second = if key == "Int" { "2" } else { "\"a\".clone()" };
                let middle = if representation == 3 {
                    "source.put(\"n\".clone(),null);"
                } else {
                    ""
                };
                let read_key = if key == "Int" {
                    "val actualKey:Int=key; println(\"int-key\")"
                } else {
                    "println(key)"
                };
                let read_value = if representation == 3 {
                    "val absent=value==null;if(absent){println(\"none\")}else{println(\"some\")}"
                } else {
                    "println(\"value\")"
                };
                let conversion = if mutable {
                    "return source"
                } else {
                    "val result=source.consume();return result"
                };
                let transfer = ["", "continue", "break", "return 7"][exit];
                let returns = if exit == 3 { ":Int" } else { ":Unit" };
                let fallback = if exit == 3 { "return 0" } else { "" };
                let caller = if exit == 3 {
                    "val result=scan(actual);if(result==7){println(\"returned\")}else{println(\"fallback\")}"
                } else {
                    "scan(actual);println(\"done\")"
                };
                let empty_caller = if exit == 3 {
                    "val emptyResult=scan(empty);if(emptyResult==0){println(\"fallback\")}else{println(\"unexpected\")}"
                } else {
                    "scan(empty);println(\"done\")"
                };
                let source = format!(
                    r#"{class}
{structural}
class Guard{{deinit(){{println("guard-drop")}}}}
fun read(key:{key},value:{payload}):Unit{{{read_key};{read_value}}}
fun readMap(source:{kind}<{key},{payload}>):Unit{{if(source.size==0){{println("zero")}}else{{println("nonzero")}}}}
fun make():{kind}<{key},{payload}>{{var source=mutableMapOf<{key},{payload}>();source.put({first},{construct});{middle}source.put({second},{construct});{conversion}}}
fun scan(own source:{kind}<{key},{payload}>){returns}{{for((key,value) in source){{val local=Guard();read(key,value);{transfer}}};println("after");readMap(source);{fallback}}}
fun entry():Unit{{println("nonempty");val actual=make();{caller};println("empty");val empty={factory}<{key},{payload}>();{empty_caller};println("entry-end")}}
"#,
                    factory = if mutable { "mutableMapOf" } else { "mapOf" }
                );
                let mut golden = String::from("nonempty\n");
                let keys = if representation == 2 {
                    vec!["int-key", "int-key"]
                } else if representation == 3 {
                    vec!["z", "n", "a"]
                } else {
                    vec!["z", "a"]
                };
                for (i, key) in keys
                    .iter()
                    .enumerate()
                    .take(if exit >= 2 { 1 } else { keys.len() })
                {
                    golden.push_str(key);
                    golden.push('\n');
                    golden.push_str(if representation == 3 {
                        if i == 1 { "none\n" } else { "some\n" }
                    } else {
                        "value\n"
                    });
                    golden.push_str("guard-drop\n");
                }
                if exit != 3 {
                    golden.push_str("after\nnonzero\n");
                }
                for _ in 0..2 {
                    golden.push_str(if representation == 0 || representation == 3 {
                        "field-drop\n"
                    } else {
                        "resource-drop\n"
                    });
                }
                golden.push_str(if exit == 3 { "returned\n" } else { "done\n" });
                golden.push_str("empty\nafter\nzero\n");
                golden.push_str(if exit == 3 { "fallback\n" } else { "done\n" });
                golden.push_str("entry-end\n");
                cases.push(Case {
                    label: format!(
                        "{tag}-{kind}-{}",
                        ["normal", "continue", "break", "return"][exit]
                    ),
                    source,
                    golden,
                    mutable,
                    representation,
                });
            }
        }
    }
    assert_eq!(cases.len(), 32);
    assert_eq!(
        cases
            .iter()
            .map(|c| c.label.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        32
    );
    for representation in 0..4 {
        for mutable in [false, true] {
            assert_eq!(
                cases
                    .iter()
                    .filter(|c| c.representation == representation && c.mutable == mutable)
                    .count(),
                4
            );
        }
    }

    cases
}
fn observe(case: &Case, unit: bool) -> Result<(), Failure> {
    diagnostic!(
        "RESOURCE FORMS INPUT case={} rooted={unit} source={:?} golden={:?}",
        case.label,
        case.source,
        case.golden
    );
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("provider.ko", &case.source)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let mut report = Report::default();
    let result = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        unit,
        |names, typed, owned, report| {
            source_forms::facts::unit(&sources, &parsed, names, typed, owned, report)
        },
    );
    diagnostic!(
        "RESOURCE FORMS {} rooted={unit} report={report:#?} outcome={:?}",
        case.label,
        result.as_ref().map(|(_, id)| id)
    );
    let (program, _) = result?;
    report.query(
        "Unit temporary Entry keeps outer source and actual producer/owner transport",
        &source_forms::graph::temporary_entries(&parsed, &program),
    );
    if !report.passed() {
        return Err(failure("original frontend observations", report));
    }
    source_forms::graph::owned_map(&sources, &parsed, &program, case.mutable)?;
    representation::check(case, &sources, &parsed, &program)?;
    crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
    crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    Ok(())
}
#[test]
fn existing_resource_representations_reach_verified_llvm() {
    let mut outcomes = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            let result = observe(&case, unit);
            diagnostic!(
                "RESOURCE FORMS CASE {} rooted={unit} result={result:?}",
                case.label
            );
            outcomes.push((case.label.clone(), unit, result));
        }
    }
    assert_diagnostic!(
        outcomes.len() == 64 && outcomes.iter().all(|(_, _, r)| r.is_ok()),
        "{outcomes:#?}"
    );
}

pub(in crate::ssa) mod pipeline {
//! 原始 lex/parse/names/raw/complete/ownership/公开完整 validator → 普通真实 lower。
use super::super::{
    Report, facts,
    fixture::{Failure, Fixture, clean, complete_failure, failure, lower_failure},
};
use crate::ssa::{
    model::{FunctionId, Program},
    unit_lower::lower_owned_unit_with_entry,
};
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SymbolKind, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, check_compilation_unit_ownership, owned_compilation_unit_view,
    },
    parser::{Item, NameMarker, ParsedFile},
    source::SourceMap,
    type_checking::{
        CompilationUnitTypes, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

type Lowered = (Program, FunctionId);

// The boolean selects only the input layout, never another checker or a fallback.
pub(in crate::ssa) fn unit_input_observed(
    sources: &SourceMap,
    parsed: &ParsedFile,
    path: &str,
    report: &mut Report,
    rooted: bool,
    observe: impl FnOnce(
        &ValidatedCompilationUnitNames,
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
        &mut Report,
    ),
) -> Result<Lowered, Failure> {
    let input = if rooted {
        SourceUnitInput::new("root", path, parsed.source_id(), parsed)
    } else {
        SourceUnitInput::memory_document(parsed.source_id(), parsed)
    };
    unit_input_observed_with_entry(sources, parsed, input, "entry", report, observe)
}

// 两种输入布局均消费真实 Unit 产品；不保留 Legacy fallback。
pub(in crate::ssa) fn memory_document(fixture: &Fixture, report: &mut Report) -> Result<Lowered, Failure> {
    memory_document_observed(
        &fixture.sources,
        &fixture.parsed,
        report,
        |names, typed, owned, report| {
            facts::unit(fixture, names, typed, owned, report);
            entry_unit(fixture, names, typed, owned, report);
            if report.identity_requested {
                report.fe_identity = Some(super::super::query_inputs::unit(fixture, names, typed, owned));
            }
        },
    )
}

pub(in crate::ssa) fn memory_document_observed(
    sources: &SourceMap,
    parsed: &ParsedFile,
    report: &mut Report,
    observe: impl FnOnce(
        &ValidatedCompilationUnitNames,
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
        &mut Report,
    ),
) -> Result<Lowered, Failure> {
    memory_document_observed_with_entry(sources, parsed, "entry", report, observe)
}

// Preserve the original callable name when the fixture's entry is not `entry`.
pub(in crate::ssa) fn memory_document_observed_with_entry(
    sources: &SourceMap,
    parsed: &ParsedFile,
    entry_name: &str,
    report: &mut Report,
    observe: impl FnOnce(
        &ValidatedCompilationUnitNames,
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
        &mut Report,
    ),
) -> Result<Lowered, Failure> {
    unit_input_observed_with_entry(
        sources,
        parsed,
        SourceUnitInput::memory_document(parsed.source_id(), parsed),
        entry_name,
        report,
        observe,
    )
}

pub(in crate::ssa) fn unit(fixture: &Fixture, report: &mut Report) -> Result<Lowered, Failure> {
    unit_observed(
        &fixture.sources,
        &fixture.parsed,
        fixture.path,
        report,
        |names, typed, owned, report| {
            facts::unit(fixture, names, typed, owned, report);
            entry_unit(fixture, names, typed, owned, report);
            if report.identity_requested {
                report.fe_identity = Some(super::super::query_inputs::unit(fixture, names, typed, owned));
            }
        },
    )
}

pub(in crate::ssa) fn unit_observed(
    sources: &SourceMap,
    parsed: &ParsedFile,
    path: &str,
    report: &mut Report,
    observe: impl FnOnce(
        &ValidatedCompilationUnitNames,
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
        &mut Report,
    ),
) -> Result<Lowered, Failure> {
    unit_observed_with_entry(sources, parsed, path, "entry", report, observe)
}

pub(in crate::ssa) fn unit_observed_with_entry(
    sources: &SourceMap,
    parsed: &ParsedFile,
    path: &str,
    entry_name: &str,
    report: &mut Report,
    observe: impl FnOnce(
        &ValidatedCompilationUnitNames,
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
        &mut Report,
    ),
) -> Result<Lowered, Failure> {
    unit_input_observed_with_entry(
        sources,
        parsed,
        SourceUnitInput::new("root", path, parsed.source_id(), parsed),
        entry_name,
        report,
        observe,
    )
}

// MemoryDocument 与原 rooted 入口每次独立跑完整分析，不按源码或失败结果选路。
fn unit_input_observed_with_entry(
    sources: &SourceMap,
    parsed: &ParsedFile,
    input: SourceUnitInput<'_>,
    entry_name: &str,
    report: &mut Report,
    observe: impl FnOnce(
        &ValidatedCompilationUnitNames,
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
        &mut Report,
    ),
) -> Result<Lowered, Failure> {
    // 不加 package、wrapper、标准库文件或第二 source；保留 SourceMap 与原始源码字节。
    let inputs = [input];
    report.check(
        "unit lex/parse and single original input",
        true,
        (input.key(), parsed.source_id()),
    );
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(sources, &inputs)
        .map_err(|error| failure("unit index infrastructure", error))?;
    clean(sources, "unit index diagnostics", index.diagnostics())?;
    let names = resolve_compilation_unit_names(sources, &inputs, &index, &name_environment)
        .map_err(|error| failure("unit names infrastructure", error))?;
    clean(sources, "unit names diagnostics", names.diagnostics())?;
    let names = names
        .validate()
        .map_err(|error| failure("unit names qualification", error))?;
    let [source_unit] = names.names().index().source_units() else {
        return Err(failure(
            "unit source qualification",
            "expected exactly one source",
        ));
    };
    if source_unit.source_id() != parsed.source_id() || parsed.package().is_some() {
        return Err(failure(
            "unit source qualification",
            "expected original default-package source",
        ));
    }
    let entries = names.names().index().declarations().iter().filter(|declaration| {
        declaration.name() == entry_name && declaration.kind() == SymbolKind::Function
            && declaration.source_unit() == source_unit.id()
            && declaration.name_span().source_id() == parsed.source_id()
            && names.names().index().packages().get(declaration.package().index())
                .is_some_and(|package| package.name().expect("named package").segments().is_empty())
            && parsed.roots().contains(&declaration.root())
            && parsed.ast().items().get(declaration.root()).is_ok_and(|node| {
                matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. } if *name == declaration.name_span())
            })
    }).collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        return Err(failure(
            "unit real default-package entry selection",
            entries,
        ));
    };
    report.check(
        "unit actual source-qualified default-package entry",
        true,
        entry,
    );
    let raw = check_compilation_unit_types(sources, &inputs, &names, &environment)
        .map_err(|error| failure("unit raw type infrastructure", error))?;
    report.check(
        "unit raw diagnostics clean",
        raw.diagnostics().is_empty(),
        raw.diagnostics(),
    );
    {
        let complete =
            complete_compilation_unit_types(sources, &inputs, &names, &environment, &raw)
                .map_err(|error| complete_failure(sources, error))?;
        let same = complete.is_same_analysis(&raw)
            && std::ptr::eq(complete.types(), &raw)
            && raw.is_compatible_with(sources, &inputs, &names, &environment);
        report.check(
            "unit complete keeps original raw analysis",
            same,
            "same raw product and input chain",
        );
        if !same {
            return Err(failure(
                "unit complete analysis identity",
                "original raw identity mismatch",
            ));
        }
    }
    clean(sources, "unit raw diagnostics", raw.diagnostics())?;
    let typed = raw
        .validate()
        .map_err(|error| failure("unit basic type qualification", error))?;
    report.check(
        "unit original raw consumed by basic qualification",
        true,
        "validate: Ok",
    );
    let owned = check_compilation_unit_ownership(sources, &inputs, &names, &environment, &typed)
        .map_err(|error| failure("unit ownership infrastructure", error))?;
    clean(sources, "unit ownership diagnostics", owned.diagnostics())?;
    if !owned.deferred().is_empty() {
        return Err(failure(
            "unit ownership deferred",
            owned
                .deferred()
                .iter()
                .map(|deferred| {
                    let expression = deferred.expression();
                    let location = if expression.source_unit() == source_unit.id() {
                        Some(
                            parsed
                                .ast()
                                .expressions()
                                .get(expression.expression())
                                .map(|node| super::super::fixture::location(sources, node.span())),
                        )
                    } else {
                        None
                    };
                    (deferred, location)
                })
                .collect::<Vec<_>>(),
        ));
    }
    let same = owned.is_compatible_with(&typed);
    report.check(
        "unit ownership original analysis and no deferred",
        same,
        owned.deferred(),
    );
    if !same {
        return Err(failure(
            "unit ownership analysis identity",
            "typed identity mismatch",
        ));
    }
    observe(&names, typed.types(), &owned, report);
    // 公开 validate 包括完整 iterations_are_valid；不是部分 query 代替生产资格。
    let owned = owned
        .validate()
        .map_err(|error| failure("unit full ownership validator", error))?;
    report.check("unit full ownership validator", true, "validate: Ok");
    let view = owned_compilation_unit_view(sources, &inputs, &names, &environment, &typed, &owned)
        .map_err(|error| failure("unit owned view qualification", error))?;
    let same = std::ptr::eq(view.sources(), sources)
        && std::ptr::eq(view.inputs(), inputs.as_slice())
        && std::ptr::eq(view.names(), &names)
        && std::ptr::eq(view.types(), typed.types())
        && std::ptr::eq(view.ownership(), owned.ownership());
    report.check(
        "unit view preserves all original references",
        same,
        "source/input/name/type/ownership",
    );
    if !same {
        return Err(failure(
            "unit view identity",
            "original references mismatch",
        ));
    }
    report.check("unit lower actually attempted", true, entry.id());
    let (program, lowered_entry) = lower_owned_unit_with_entry(&view, entry.id())
        .map_err(|error| lower_failure(sources, error))?;
    let entry_span = parsed
        .ast()
        .items()
        .get(entry.root())
        .map_err(|error| failure("unit selected entry AST identity", error))?
        .span();
    let entries = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin == crate::ssa::model::Origin::Source(entry_span))
        .collect::<Vec<_>>();
    let same = matches!(entries.as_slice(), [function] if function.id == lowered_entry);
    report.check(
        "unit returned entry is the selected original source function",
        same,
        (entry.id(), entry_span, lowered_entry),
    );
    if !same {
        return Err(failure(
            "unit returned entry source identity",
            (lowered_entry, entries),
        ));
    }
    Ok((program, lowered_entry))
}

// Actual Unit entry argument-to-parameter delivery, independent of the exit-shape query.
fn entry_unit(
    f: &Fixture,
    names: &lang_frontend::name_resolution::ValidatedCompilationUnitNames,
    typed: &lang_frontend::type_checking::CompilationUnitTypes,
    owned: &lang_frontend::ownership_checking::CompilationUnitOwnership,
    report: &mut Report,
) {
    use lang_frontend::{
        name_resolution::UnitReferenceTarget,
        ownership_checking::{
            LoanKind, UnitCallArgumentOwnershipKind as K, UnitLoanTarget, UnitValueDeliveryKind,
        },
        parser::Expression,
        type_checking::{ParameterMode, UnitCallTarget, UnitExpressionId},
    };
    let Ok(site) = super::super::fixture::loop_site(f) else {
        return;
    };
    let [source_names] = names.names().source_units() else {
        return;
    };
    let unit = source_names.source_unit();
    let mode = if f.map {
        ParameterMode::Value
    } else {
        ParameterMode::Borrow
    };
    for call_id in site.entry_calls {
        let call_id_unit = UnitExpressionId::new(unit, call_id);
        let Some(call) = typed.call(call_id_unit) else {
            report.check("unit actual entry scan descriptor", false, call_id_unit);
            continue;
        };
        let node = f.parsed.ast().expressions().get(call_id);
        let argument = node.ok().and_then(|n| {
            if let Expression::Call { arguments, .. } = n.payload() {
                arguments.first()
            } else {
                None
            }
        });
        let Some(argument) = argument else {
            report.check("unit actual entry scan argument", false, call_id_unit);
            continue;
        };
        let argument_id = UnitExpressionId::new(unit, argument.value);
        let argument_node = f.parsed.ast().expressions().get(argument.value);
        let target = match call.target() {
            UnitCallTarget::Declaration(d) => names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|v| v.id() == d),
            _ => None,
        };
        let refs = names
            .names()
            .references()
            .iter()
            .filter(|r| argument_node.as_ref().is_ok_and(|n| r.span() == n.span()))
            .collect::<Vec<_>>();
        let source = match refs.as_slice() {
            [r] => {
                if let UnitReferenceTarget::Symbol(s) = r.target() {
                    Some(*s)
                } else {
                    None
                }
            }
            _ => None,
        };
        report.check("unit real entry scan target/source parameter slot/mode", target.is_some_and(|d| d.name() == "scan" && d.source_unit() == unit)
            && call.receiver().is_none() && matches!(call.arguments(), [a] if a.argument_index() == 0 && a.parameter_index() == 0 && a.mode() == mode && Some(a.parameter_type()) == typed.expression_type(argument_id))
            && source.is_some(), (call_id_unit, call, argument_node, refs));
        let contracts = owned
            .call_argument_contracts_for(call_id_unit)
            .collect::<Vec<_>>();
        report.check("unit actual entry scan source ownership contract", matches!(contracts.as_slice(), [c] if c.argument() == argument_id && c.parameter_index() == 0 && c.kind() == if f.map { K::Value } else { K::SharedLoan }), &contracts);
        let loans = owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == call_id_unit)
            .collect::<Vec<_>>();
        report.check("unit actual entry source borrow/value delivery facts", if f.map { loans.is_empty() } else {
            matches!(loans.as_slice(), [loan] if loan.argument() == argument_id && loan.kind() == LoanKind::Shared
                && matches!(loan.target(), UnitLoanTarget::Place(p) if Some(p.root()) == source && p.is_root()))
        }, loans);
        let deliveries = owned
            .value_deliveries()
            .iter()
            .filter(|v| v.call() == call_id_unit)
            .collect::<Vec<_>>();
        report.check("unit actual entry source Move/value delivery", if f.map {
            matches!(deliveries.as_slice(), [d] if d.argument() == argument_id && d.kind() == UnitValueDeliveryKind::Move && d.place().is_some_and(|p| Some(p.root()) == source && p.is_root()))
        } else { deliveries.is_empty() }, deliveries);
    }
}
}
