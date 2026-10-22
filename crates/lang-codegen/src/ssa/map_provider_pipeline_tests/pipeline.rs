//! 原始 lex/parse/names/raw/complete/ownership/公开完整 validator → 普通真实 lower。
use super::{
    Report, facts,
    fixture::{Failure, Fixture, at, clean, complete_failure, failure, lower_failure},
};
use crate::ssa::{
    lower_scalar_file_with_entry,
    model::{FunctionId, Program},
    unit_lower::lower_owned_unit_with_entry,
};
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
        resolve_names,
    },
    ownership_checking::{
        check_compilation_unit_ownership, check_ownership, owned_compilation_unit_view,
        validate_iteration_facts,
    },
    parser::{Item, NameMarker},
    type_checking::{
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

type Lowered = (Program, FunctionId);

pub(super) fn single(fixture: &Fixture, report: &mut Report) -> Result<Lowered, Failure> {
    let sources = &fixture.sources;
    let parsed = &fixture.parsed;
    report.check("single lex/parse clean", true, parsed.source_id());
    let (name_environment, environment) = standard_environments();
    let names = resolve_names(sources, parsed, &name_environment)
        .map_err(|error| failure("single names infrastructure", error))?;
    clean(sources, "single names diagnostics", names.diagnostics())?;
    let entries = names.symbols().iter().filter(|symbol| {
        symbol.name() == "entry" && !symbol.is_synthetic() && symbol.kind() == SymbolKind::Function
            && symbol.span().source_id() == parsed.source_id()
            && parsed.roots().iter().any(|root| {
                parsed.ast().items().get(*root).is_ok_and(|node| {
                    matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. } if *name == symbol.span())
                })
            })
    }).collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        return Err(failure("single real entry selection", entries));
    };
    report.check("single actual source entry symbol", true, entry);
    let raw = check_types(sources, parsed, &names, &environment)
        .map_err(|error| failure("single raw type infrastructure", error))?;
    report.check(
        "single raw diagnostics clean",
        raw.diagnostics().is_empty(),
        raw.diagnostics(),
    );
    // raw 即使有诊断仍执行完整准入，保存 complete 原始根因，不冒充 lower 拒绝。
    let complete = complete_file_types(sources, parsed, &names, &environment, &raw)
        .map_err(|error| complete_failure(sources, error))?;
    let same = complete.is_same_analysis(&raw)
        && std::ptr::eq(complete.types(), &raw)
        && raw.is_compatible_with_names(&names);
    report.check(
        "single complete keeps original raw/names analysis",
        same,
        raw.source_id(),
    );
    if !same {
        return Err(failure(
            "single complete analysis identity",
            "original raw identity mismatch",
        ));
    }
    clean(sources, "single raw diagnostics", raw.diagnostics())?;
    let owned = check_ownership(sources, parsed, &names, complete.types())
        .map_err(|error| failure("single ownership infrastructure", error))?;
    clean(sources, "single ownership diagnostics", owned.diagnostics())?;
    if !owned.deferred().is_empty() {
        return Err(failure(
            "single ownership deferred",
            owned
                .deferred()
                .iter()
                .map(|deferred| {
                    let location = parsed
                        .ast()
                        .expressions()
                        .get(deferred.expression())
                        .map(|node| super::fixture::location(sources, node.span()));
                    (deferred, location)
                })
                .collect::<Vec<_>>(),
        ));
    }
    let same = owned.is_compatible_with(&names, &raw);
    report.check(
        "single ownership original analysis and no deferred",
        same,
        owned.source_id(),
    );
    if !same {
        return Err(failure(
            "single ownership analysis identity",
            "raw/names mismatch",
        ));
    }
    facts::single(fixture, &names, &raw, &owned, report);
    // 完整公共 validator 独立于只读 input query；失败保留原 reason/span。
    validate_iteration_facts(parsed, &names, complete.types(), &owned).map_err(|error| {
        at(
            sources,
            "single full iteration validator",
            &error,
            error.span(),
        )
    })?;
    report.check(
        "single full iteration validator",
        true,
        "validate_iteration_facts: Ok",
    );
    report.check("single lower actually attempted", true, entry.id());
    lower_scalar_file_with_entry(
        sources,
        parsed,
        &names,
        complete.types(),
        &owned,
        entry.id(),
    )
    .map_err(|error| lower_failure(sources, error))
}

pub(super) fn unit(fixture: &Fixture, report: &mut Report) -> Result<Lowered, Failure> {
    let sources = &fixture.sources;
    let parsed = &fixture.parsed;
    // 不加 package、wrapper、标准库文件或第二 source；与 single 完全相同字节和逻辑路径。
    let inputs = [SourceUnitInput::new(
        "root",
        fixture.path,
        parsed.source_id(),
        parsed,
    )];
    report.check(
        "unit lex/parse and single original input",
        true,
        (fixture.path, parsed.source_id()),
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
        declaration.name() == "entry" && declaration.kind() == SymbolKind::Function
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
    facts::unit(fixture, &names, typed.types(), &owned, report);
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
    lower_owned_unit_with_entry(&view, entry.id()).map_err(|error| lower_failure(sources, error))
}
