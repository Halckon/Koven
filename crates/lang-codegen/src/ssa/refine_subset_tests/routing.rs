//! Shared normal-Unit routing observation; no lowering mode override.
use super::*;

fn lower(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    entry_name: &str,
) -> (Result<Program, LoweringError>, Vec<String>) {
    let (name_environment, mut type_environment) = standard_environments();
    for input in inputs {
        let _ = type_environment.authorize_range_source(sources, input.source_id());
        let _ = type_environment.authorize_range_extension_source(sources, input.source_id());
    }
    let (names, typed, owned) = analyze(sources, inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "p", entry_name);
    let (result, routed) = observe_routing(|| {
        lower_scalar_unit_with_entry(
            sources,
            inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
    });
    (result.map(|(program, _)| program), routed)
}

pub(super) fn routes_from_entry(
    source: &str,
    entry_name: &str,
) -> (Result<Program, LoweringError>, Vec<String>) {
    let mut sources = SourceMap::new();
    let text = format!("package p\n{source}");
    let (id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
    lower(&sources, &inputs, entry_name)
}

pub(super) fn routes_with_sibling(
    source: &str,
    sibling: &str,
    reverse: bool,
) -> (Result<Program, LoweringError>, Vec<String>) {
    let mut sources = SourceMap::new();
    let (id, file) = parsed(&mut sources, "p/main.ko", &format!("package p\n{source}"));
    let (other_id, other) = parsed(
        &mut sources,
        "q/unused.ko",
        &format!("package q\n{sibling}"),
    );
    let mut inputs = [
        SourceUnitInput::new("root", "p/main.ko", id, &file),
        SourceUnitInput::new("root", "q/unused.ko", other_id, &other),
    ];
    if reverse {
        inputs.reverse();
    }
    lower(&sources, &inputs, "entry")
}

pub(super) fn route(source: &str) -> (Result<Program, LoweringError>, bool) {
    let (result, routed) = routes_from_entry(source, "f");
    let refined = routed.iter().any(|name| name.starts_with("koven.p.f.d"));
    (result, refined)
}
