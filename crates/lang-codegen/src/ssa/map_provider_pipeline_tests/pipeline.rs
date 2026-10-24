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
        SourceUnitInput, SourceUnitKey, SymbolKind, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{check_compilation_unit_ownership, owned_compilation_unit_view},
    parser::{Item, NameMarker},
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

type Lowered = (Program, FunctionId);

// 每次调用独立建立整条 Unit 产品链，不复用另一列的 names/types/ownership。
pub(super) fn single(fixture: &Fixture, report: &mut Report) -> Result<Lowered, Failure> {
    lower(fixture, report, false)
}

pub(super) fn unit(fixture: &Fixture, report: &mut Report) -> Result<Lowered, Failure> {
    lower(fixture, report, true)
}

fn lower(fixture: &Fixture, report: &mut Report, rooted: bool) -> Result<Lowered, Failure> {
    let sources = &fixture.sources;
    let parsed = &fixture.parsed;
    // 原单源列使用真实 MemoryDocument；原 Unit 列保留 root/provider.ko。
    // SourceMap 路径、源码字节和默认包不变；不加 wrapper 或第二 source。
    let inputs = [if rooted {
        SourceUnitInput::new("root", fixture.path, parsed.source_id(), parsed)
    } else {
        SourceUnitInput::memory_document(parsed.source_id(), parsed)
    }];
    report.check(
        "unit lex/parse and single original input",
        true,
        (rooted, fixture.path, parsed.source_id()),
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
    let same_input = source_unit.source_id() == parsed.source_id()
        && parsed.package().is_none()
        && if rooted {
            source_unit
                .key()
                .root()
                .is_some_and(|root| root.as_str() == "root")
                && source_unit
                    .key()
                    .logical_path()
                    .is_some_and(|path| path.as_str() == fixture.path)
        } else {
            source_unit.key() == &SourceUnitKey::MemoryDocument
        };
    report.check(
        "unit original input kind and source identity",
        same_input,
        source_unit,
    );
    if !same_input {
        return Err(failure(
            "unit source qualification",
            "expected original default-package source and selected input kind",
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
