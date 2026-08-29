use lang_frontend::{
    lexer::lex,
    name_resolution::{
        DeclarationId, NameEnvironment, SourceUnitInput, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{ValidatedCompilationUnitOwnership, check_compilation_unit_ownership},
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        TypeEnvironment, ValidatedCompilationUnitTypes, check_compilation_unit_types,
        standard_environments,
    },
};

use super::{
    LoweringErrorKind, model::Operation, render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
};

fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique source");
    let lexed = lex(sources, source).expect("lexing succeeds internally");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (source, parsed)
}

fn analyze(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    name_environment: &NameEnvironment,
    type_environment: &TypeEnvironment,
) -> (
    ValidatedCompilationUnitNames,
    ValidatedCompilationUnitTypes,
    ValidatedCompilationUnitOwnership,
) {
    let index = index_compilation_unit(sources, inputs).expect("valid unit input");
    let names = resolve_compilation_unit_names(sources, inputs, &index, name_environment)
        .expect("name resolution succeeds internally")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(sources, inputs, &names, type_environment)
        .expect("type checking succeeds internally")
        .validate()
        .expect("valid types");
    let owned = check_compilation_unit_ownership(sources, inputs, &names, type_environment, &typed)
        .expect("ownership checking succeeds internally")
        .validate()
        .expect("valid ownership");
    (names, typed, owned)
}

fn declaration(names: &ValidatedCompilationUnitNames, package: &str, name: &str) -> DeclarationId {
    names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| {
            declaration.name() == name
                && names.names().index().packages()[declaration.package().index()]
                    .name()
                    .segments()
                    .iter()
                    .map(String::as_str)
                    .eq(package.split('.'))
        })
        .expect("declaration exists")
        .id()
}

#[test]
fn lowers_cross_package_generic_alias_call_to_deterministic_verified_ssa() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun <T> identity(own input: T): T = input",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nimport p.identity as id\nfun entry(): Int = id(7)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "q", "entry");

    let (forward, forward_entry) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("cross-package unit lowers to verified SSA");
    let (backward, backward_entry) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("input permutation lowers identically");

    assert_eq!(render_program(&forward), render_program(&backward));
    let module = &forward.modules[0];
    assert_eq!(module.functions.len(), 2);
    assert_eq!(
        module.functions[forward_entry.index()].name,
        "koven.q.entry.d1"
    );
    assert_eq!(
        backward.modules[0].functions[backward_entry.index()].name,
        "koven.q.entry.d1"
    );
    assert!(
        module.functions[0]
            .name
            .starts_with("koven.p.identity.d0.t")
    );
    assert_eq!(
        module
            .functions
            .iter()
            .flat_map(|function| function.instructions.iter())
            .filter(|instruction| matches!(instruction.operation, Operation::DirectCall { .. }))
            .count(),
        1
    );
}

#[test]
fn package_identity_keeps_same_named_functions_distinct_and_dead_body_unlowered() {
    let mut sources = SourceMap::new();
    let (left_source, left) = parsed(
        &mut sources,
        "left/provider.ko",
        "package left\nfun answer(): Int = 1",
    );
    let (right_source, right) = parsed(
        &mut sources,
        "right/provider.ko",
        "package right\nfun answer(): Int = 2",
    );
    let (entry_source, entry_file) = parsed(
        &mut sources,
        "main/entry.ko",
        "package main\nfun entry(): Int = left.answer()",
    );
    let inputs = [
        SourceUnitInput::new("root", "left/provider.ko", left_source, &left),
        SourceUnitInput::new("root", "right/provider.ko", right_source, &right),
        SourceUnitInput::new("root", "main/entry.ko", entry_source, &entry_file),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "main", "entry"),
    )
    .expect("reachable package function lowers");
    let names = program.modules[0]
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 2);
    assert!(
        names
            .iter()
            .any(|name| name.starts_with("koven.left.answer.d"))
    );
    assert!(
        names
            .iter()
            .any(|name| name.starts_with("koven.main.entry.d"))
    );
    assert!(!names.iter().any(|name| name.contains("right.answer")));
}

#[test]
fn unsupported_reachable_block_body_fails_before_publishing_ssa() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(&mut sources, "p/main.ko", "package p\nfun entry(): Unit {}");
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let error = match lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    ) {
        Ok(_) => panic!("unimplemented body family must fail loudly"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}
