//! 原始 lex/parse/names/raw/complete/ownership/公开完整 validator → 普通真实 lower。
use super::{
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
pub(super) fn unit_input_observed(
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

// 两种输入布局均消费真实 Unit 产品；旧差分负控独立在 legacy_boundaries。
pub(super) fn memory_document(fixture: &Fixture, report: &mut Report) -> Result<Lowered, Failure> {
    memory_document_observed(
        &fixture.sources,
        &fixture.parsed,
        report,
        |names, typed, owned, report| {
            facts::unit(fixture, names, typed, owned, report);
            entry_unit(fixture, names, typed, owned, report);
            if report.identity_requested {
                report.fe_identity = Some(super::query_inputs::unit(fixture, names, typed, owned));
            }
        },
    )
}

pub(super) fn memory_document_observed(
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
pub(super) fn memory_document_observed_with_entry(
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

pub(super) fn unit(fixture: &Fixture, report: &mut Report) -> Result<Lowered, Failure> {
    unit_observed(
        &fixture.sources,
        &fixture.parsed,
        fixture.path,
        report,
        |names, typed, owned, report| {
            facts::unit(fixture, names, typed, owned, report);
            entry_unit(fixture, names, typed, owned, report);
            if report.identity_requested {
                report.fe_identity = Some(super::query_inputs::unit(fixture, names, typed, owned));
            }
        },
    )
}

pub(super) fn unit_observed(
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

pub(super) fn unit_observed_with_entry(
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
                                .map(|node| super::fixture::location(sources, node.span())),
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
    let Ok(site) = super::fixture::loop_site(f) else {
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
