//! Basic/const 各自取得真实 capability；只读 query 不改写同轮阶段产物。
use super::{
    Pathway, Report, facts,
    fixture::{Fixture, LowerOutcome, Stopped, clean, stopped},
    roots,
};
use lang_frontend::{
    name_resolution::{SymbolKind, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        const_owned_compilation_unit_view, owned_compilation_unit_view,
    },
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

pub(super) fn collect(
    fixture: &Fixture,
    pathway: Pathway,
    reverse_inputs: bool,
    entry_name: &str,
    whole_set: bool,
    report: &mut Report,
) -> Result<LowerOutcome, Stopped> {
    let sources = &fixture.sources;
    let inputs = fixture.inputs(pathway, reverse_inputs)?;
    report.check(
        "unit lex/parse clean and actual input order",
        true,
        inputs
            .iter()
            .map(|input| input.source_id())
            .collect::<Vec<_>>(),
    );
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(sources, &inputs)
        .map_err(|error| stopped("unit index infrastructure", error))?;
    clean(sources, "unit index diagnostics", index.diagnostics())?;
    let names = resolve_compilation_unit_names(sources, &inputs, &index, &name_environment)
        .map_err(|error| stopped("unit names infrastructure", error))?;
    clean(sources, "unit names diagnostics", names.diagnostics())?;
    let names = names
        .validate()
        .map_err(|error| stopped("unit names qualification", error))?;
    fixture.check_input_identity(pathway, &names, report);
    let entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == entry_name && declaration.kind() == SymbolKind::Function
        })
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        return Err(stopped("unit real entry identity", entries));
    };
    report.check("unit names and real entry", true, entry);
    let expected_entry = fixture.function_location(entry.name_span());
    report.check(
        "unit real entry full source origin",
        expected_entry.is_some(),
        &expected_entry,
    );
    if whole_set {
        let entry_source = names
            .names()
            .index()
            .source_units()
            .iter()
            .find(|source| source.id() == entry.source_unit())
            .map(|source| source.source_id());
        let file = fixture
            .files
            .iter()
            .find(|file| Some(file.parsed.source_id()) == entry_source);
        report.check(
            "unit entry empty and calls no Map function",
            file.is_some_and(|file| facts::empty_entry(file, entry.name_span())),
            (entry, entry_source),
        );
    }
    let raw = check_compilation_unit_types(sources, &inputs, &names, &environment)
        .map_err(|error| stopped("unit raw infrastructure", error))?;
    report.check(
        "unit raw diagnostics",
        raw.diagnostics().is_empty(),
        raw.diagnostics(),
    );
    {
        let complete =
            complete_compilation_unit_types(sources, &inputs, &names, &environment, &raw)
                .map_err(|error| stopped("unit complete admission", error))?;
        let same = complete.is_same_analysis(&raw) && std::ptr::eq(complete.types(), &raw);
        report.check(
            "unit complete original analysis",
            same,
            raw.is_compatible_with(sources, &inputs, &names, &environment),
        );
        if !same {
            return Err(stopped(
                "unit complete identity",
                "not the original raw product",
            ));
        }
    }
    clean(sources, "unit raw diagnostics", raw.diagnostics())?;
    match pathway {
        Pathway::Single | Pathway::Basic => {
            let typed = raw
                .validate()
                .map_err(|error| stopped("unit basic type qualification", error))?;
            report.check(
                "unit basic type qualification",
                true,
                "original raw consumed",
            );
            let owned =
                check_compilation_unit_ownership(sources, &inputs, &names, &environment, &typed)
                    .map_err(|error| stopped("unit basic ownership infrastructure", error))?;
            clean(
                sources,
                "unit basic ownership diagnostics",
                owned.diagnostics(),
            )?;
            if !owned.deferred().is_empty() {
                return Err(stopped("unit basic ownership deferred", owned.deferred()));
            }
            report.check(
                "unit basic ownership original identity",
                owned.is_compatible_with(&typed),
                owned.deferred(),
            );
            facts::unit(fixture, &names, typed.types(), &owned, whole_set, report);
            // validate() 公开入口包括完整 iterations_are_valid；view factory 另验来源身份。
            let owned = owned
                .validate()
                .map_err(|error| stopped("unit basic full ownership validator", error))?;
            report.check("unit basic full ownership validator", true, "validate: Ok");
            let view =
                owned_compilation_unit_view(sources, &inputs, &names, &environment, &typed, &owned)
                    .map_err(|error| stopped("unit basic owned view identity", error))?;
            report.check(
                "unit basic view preserves original inputs",
                std::ptr::eq(view.sources(), sources)
                    && std::ptr::eq(view.inputs(), inputs.as_slice())
                    && std::ptr::eq(view.names(), &names)
                    && std::ptr::eq(view.types(), typed.types())
                    && std::ptr::eq(view.ownership(), owned.ownership()),
                "source/input/name/type/ownership references",
            );
            roots::collect(
                fixture,
                &names,
                typed.types(),
                entry.id(),
                report,
                |root, _| crate::ssa::unit_lower::lower_owned_unit_with_entry(&view, root),
            )
        }
        Pathway::Const => {
            // 没有常量声明也必须取得专用 capability；绝不借 basic view 或转换伪造。
            let typed = raw
                .validate_constants()
                .map_err(|error| stopped("unit const type qualification", error))?;
            report.check(
                "unit const type qualification",
                true,
                "original raw consumed by validate_constants",
            );
            let owned = check_compilation_unit_constant_ownership(
                sources,
                &inputs,
                &names,
                &environment,
                &typed,
            )
            .map_err(|error| stopped("unit const ownership infrastructure", error))?;
            clean(
                sources,
                "unit const ownership diagnostics",
                owned.ownership().diagnostics(),
            )?;
            if !owned.ownership().deferred().is_empty() {
                return Err(stopped(
                    "unit const ownership deferred",
                    owned.ownership().deferred(),
                ));
            }
            report.check(
                "unit const ownership original identity",
                owned.is_compatible_with(&typed),
                owned.ownership().deferred(),
            );
            facts::unit(
                fixture,
                &names,
                typed.types(),
                owned.ownership(),
                whole_set,
                report,
            );
            let owned = owned
                .validate()
                .map_err(|error| stopped("unit const full ownership validator", error))?;
            report.check(
                "unit const full ownership validator",
                true,
                "constant ownership validate: Ok",
            );
            let view = const_owned_compilation_unit_view(
                sources,
                &inputs,
                &names,
                &environment,
                &typed,
                &owned,
            )
            .map_err(|error| stopped("unit const owned view identity", error))?;
            report.check(
                "unit const view preserves original capability",
                std::ptr::eq(view.sources(), sources)
                    && std::ptr::eq(view.inputs(), inputs.as_slice())
                    && std::ptr::eq(view.names(), &names)
                    && std::ptr::eq(view.types(), typed.types())
                    && std::ptr::eq(view.ownership(), owned.ownership())
                    && std::ptr::eq(view.constant_ownership(), &owned),
                "const references, no cast",
            );
            roots::collect(
                fixture,
                &names,
                typed.types(),
                entry.id(),
                report,
                |root, _| {
                    crate::ssa::unit_lower::constant::lower_const_owned_unit_with_entry(&view, root)
                },
            )
        }
    }
}
