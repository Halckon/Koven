//! Real source-qualified Unit products, shared by one-source and reordered inputs.
use super::*;
use crate::ssa::{
    unit_lower::{constant::lower_const_owned_unit_with_entry, lower_owned_unit_with_entry},
    unit_lower_test_support::parsed,
    verify::verify_program,
};
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{
        check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        const_owned_compilation_unit_view, owned_compilation_unit_view,
    },
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EntryKind {
    RootedUnit,
    Unit,
    // Only the retained legacy-only Copyable pending contract observes this kind.
    Single,
}

pub(super) type Observer<'a> =
    dyn Fn(&SourceMap, &ParsedFile, &Program, FunctionId, EntryKind, Option<&[ParameterMode]>) + 'a;

pub(super) fn paired(text: &str, constants: bool) -> Vec<Event> {
    let first = paired_in_order_observed(text, constants, false, |_, _, _| {});
    assert_eq!(
        first,
        paired_in_order_observed(text, constants, true, |_, _, _| {})
    );
    first
}

pub(super) fn paired_in_order_observed(
    text: &str,
    constants: bool,
    entry_first: bool,
    observe: impl Fn(&SourceMap, &Program, FunctionId),
) -> Vec<Event> {
    paired_in_order_observed_entry(
        text,
        constants,
        entry_first,
        &|sources, _, program, entry, _, _| {
            observe(sources, program, entry);
        },
    )
}

pub(super) fn paired_in_order_observed_entry(
    text: &str,
    constants: bool,
    entry_first: bool,
    observe: &Observer<'_>,
) -> Vec<Event> {
    compare_in_order(
        text,
        constants,
        entry_first,
        EntryKind::RootedUnit,
        |sources, input| unit_product(sources, &[input], constants),
        observe,
    )
}

pub(super) fn compare_in_order(
    text: &str,
    constants: bool,
    entry_first: bool,
    baseline_kind: EntryKind,
    baseline: impl FnOnce(
        &SourceMap,
        SourceUnitInput<'_>,
    ) -> (Program, FunctionId, Option<Vec<ParameterMode>>),
    observe: &Observer<'_>,
) -> Vec<Event> {
    let mut sources = SourceMap::new();
    // Identical names in an unrelated logical source must not affect entry facts.
    // SourceMap insertion order and the actual two-source inputs reverse separately.
    let other_text = "package other\nfun entry(): String = \"unreachable\"";
    let ((source, file), (other_source, other)) = if entry_first {
        let entry = parsed(&mut sources, "pair/entry.ko", text);
        let other = parsed(&mut sources, "other/entry.ko", other_text);
        (entry, other)
    } else {
        let other = parsed(&mut sources, "other/entry.ko", other_text);
        let entry = parsed(&mut sources, "pair/entry.ko", text);
        (entry, other)
    };
    assert_eq!(sources.source_text(source), Ok(text));
    assert_eq!(sources.source_text(other_source), Ok(other_text));
    let input = SourceUnitInput::new("root", "pair/entry.ko", source, &file);
    // The new baseline analyzes only this input in the very same SourceMap.
    // Its names/types/ownership are real Unit products, not old Single facts.
    let (program, entry, modes) = baseline(&sources, input);
    let expected = events(&sources, &program, entry);
    let expected_relations = string_relations(&sources, &program, entry);
    observe(
        &sources,
        &file,
        &program,
        entry,
        baseline_kind,
        modes.as_deref(),
    );
    let inputs = [
        SourceUnitInput::new("root", "other/entry.ko", other_source, &other),
        input,
    ];
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let (unit, entry, modes) = unit_product(&sources, &inputs, constants);
        assert_eq!(events(&sources, &unit, entry), expected, "{text}");
        assert_eq!(
            string_relations(&sources, &unit, entry),
            expected_relations,
            "{text}"
        );
        observe(
            &sources,
            &file,
            &unit,
            entry,
            EntryKind::Unit,
            modes.as_deref(),
        );
    }
    expected
}

fn unit_product(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    constants: bool,
) -> (Program, FunctionId, Option<Vec<ParameterMode>>) {
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(sources, inputs).expect("real Unit index");
    assert!(index.diagnostics().is_empty(), "{:?}", index.diagnostics());
    assert_eq!(index.source_units().len(), inputs.len());
    let names = resolve_compilation_unit_names(sources, inputs, &index, &name_environment)
        .expect("real Unit name resolution");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().expect("validated Unit names");
    let entry_inputs = inputs
        .iter()
        .filter(|input| input.logical_path() == Some("pair/entry.ko"))
        .collect::<Vec<_>>();
    let [input] = entry_inputs.as_slice() else {
        panic!("one exact entry source input");
    };
    assert_eq!(input.root_identity(), Some("root"));
    let entry_sources = names
        .names()
        .index()
        .source_units()
        .iter()
        .filter(|source| source.source_id() == input.source_id())
        .collect::<Vec<_>>();
    let [source] = entry_sources.as_slice() else {
        panic!("one indexed entry source");
    };
    assert_eq!(source.key().root().unwrap().as_str(), "root");
    assert_eq!(
        source.key().logical_path().unwrap().as_str(),
        "pair/entry.ko"
    );
    let entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "entry"
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == source.id()
                && input.parsed().roots().contains(&declaration.root())
        })
        .collect::<Vec<_>>();
    let [declaration] = entries.as_slice() else {
        panic!("one original top-level entry declaration");
    };
    assert_eq!(declaration.name_span().source_id(), input.source_id());
    assert_eq!(sources.slice(declaration.name_span()), Ok("entry"));
    assert!(
        names.names().index().packages()[declaration.package().index()]
            .name()
            .unwrap()
            .segments()
            .iter()
            .map(String::as_str)
            .eq(["pair"])
    );
    let entry_span = input
        .parsed()
        .ast()
        .items()
        .get(declaration.root())
        .unwrap()
        .span();
    let raw = check_compilation_unit_types(sources, inputs, &names, &environment)
        .expect("real Unit types");
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    {
        let complete = complete_compilation_unit_types(sources, inputs, &names, &environment, &raw)
            .expect("complete Unit type admission");
        assert!(complete.is_same_analysis(&raw) && std::ptr::eq(complete.types(), &raw));
    }
    let modes = raw
        .signatures()
        .declaration(declaration.id())
        .and_then(|signature| signature.callable())
        .map(|callable| {
            callable
                .parameters()
                .iter()
                .map(|parameter| parameter.mode())
                .collect::<Vec<_>>()
        });
    let (program, entry) = if constants {
        assert!(
            raw.clone().validate().is_err(),
            "basic capability must reject const input"
        );
        let typed = raw
            .validate_constants()
            .expect("specialized constant types");
        let owned = check_compilation_unit_constant_ownership(
            sources,
            inputs,
            &names,
            &environment,
            &typed,
        )
        .expect("real constant ownership");
        assert!(
            owned.ownership().diagnostics().is_empty(),
            "{:?}",
            owned.ownership().diagnostics()
        );
        assert!(
            owned.ownership().deferred().is_empty(),
            "{:?}",
            owned.ownership().deferred()
        );
        assert!(owned.is_compatible_with(&typed));
        let owned = owned.validate().expect("complete constant ownership");
        let view = const_owned_compilation_unit_view(
            sources,
            inputs,
            &names,
            &environment,
            &typed,
            &owned,
        )
        .expect("same-round constant Unit view");
        lower_const_owned_unit_with_entry(&view, declaration.id())
            .expect("specialized constant Unit lowering")
    } else {
        let typed = raw.validate().expect("basic Unit type capability");
        let owned = check_compilation_unit_ownership(sources, inputs, &names, &environment, &typed)
            .expect("real Unit ownership");
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        assert!(owned.is_compatible_with(&typed));
        let owned = owned.validate().expect("complete Unit ownership");
        let view =
            owned_compilation_unit_view(sources, inputs, &names, &environment, &typed, &owned)
                .expect("same-round basic Unit view");
        lower_owned_unit_with_entry(&view, declaration.id()).expect("ordinary Unit lowering")
    };
    assert_eq!(
        program.modules[0].function(entry).unwrap().origin.span(),
        entry_span
    );
    verify_program(&program).expect("complete Unit SSA verification");
    (program, entry, modes)
}
