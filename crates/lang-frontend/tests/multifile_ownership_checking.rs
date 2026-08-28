//! SPEC-0198 compilation-unit ownership product integration tests.

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        NameEnvironment, SourceUnitId, SourceUnitInput, UnitSymbolId,
        ValidatedCompilationUnitNames, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipBindingKind, OwnershipCheckingError,
        check_compilation_unit_ownership,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        TypeEnvironment, ValidatedCompilationUnitTypes, check_compilation_unit_types,
        standard_environments,
    },
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

fn validated_names<'a>(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'a>],
    environment: &NameEnvironment,
) -> ValidatedCompilationUnitNames {
    let index = index_compilation_unit(sources, inputs).expect("valid unit input");
    resolve_compilation_unit_names(sources, inputs, &index, environment)
        .expect("name resolution succeeds internally")
        .validate()
        .expect("valid names")
}

fn validated_types(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
) -> ValidatedCompilationUnitTypes {
    check_compilation_unit_types(sources, inputs, names, environment)
        .expect("type checking succeeds internally")
        .validate()
        .expect("valid compilation-unit types")
}

fn source_unit(names: &ValidatedCompilationUnitNames, source: SourceId) -> SourceUnitId {
    names
        .names()
        .index()
        .source_units()
        .iter()
        .find(|unit| unit.source_id() == source)
        .expect("source belongs to unit")
        .id()
}

fn symbol_named(
    ownership: &CompilationUnitOwnership,
    names: &ValidatedCompilationUnitNames,
    source: SourceUnitId,
    name: &str,
) -> UnitSymbolId {
    let resolution = names.names().source_units()[source.index()].resolution();
    let symbol = resolution
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .expect("symbol exists");
    ownership
        .bindings()
        .iter()
        .map(|binding| binding.symbol())
        .find(|candidate| candidate.source_unit() == source && candidate.symbol() == symbol.id())
        .expect("source-qualified ownership binding exists")
}

#[test]
fn unit_parameter_bindings_cover_cross_file_member_and_lambda_modes() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun accept(own calleeOwned: String, calleeShared: String, inout calleeExclusive: String): Unit {}\n\
         class Worker {\n\
             fun run(own memberOwned: String, memberShared: String): Unit {}\n\
             companion object {\n\
                 fun configure(inout companionExclusive: String): Unit {}\n\
             }\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun use(own first: String, second: String, inout third: String): Unit {\n\
             p.accept(first, second, &third)\n\
             val callback: (own String, borrow String, inout String) -> Unit =\n\
                 { lambdaOwned, lambdaShared, lambdaExclusive -> }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    let provider_unit = source_unit(&names, provider_source);
    let consumer_unit = source_unit(&names, consumer_source);
    for (source, name, expected) in [
        (provider_unit, "calleeOwned", OwnershipBindingKind::Owned),
        (provider_unit, "calleeShared", OwnershipBindingKind::Shared),
        (
            provider_unit,
            "calleeExclusive",
            OwnershipBindingKind::Exclusive,
        ),
        (provider_unit, "memberOwned", OwnershipBindingKind::Owned),
        (provider_unit, "memberShared", OwnershipBindingKind::Shared),
        (
            provider_unit,
            "companionExclusive",
            OwnershipBindingKind::Exclusive,
        ),
        (consumer_unit, "first", OwnershipBindingKind::Owned),
        (consumer_unit, "second", OwnershipBindingKind::Shared),
        (consumer_unit, "third", OwnershipBindingKind::Exclusive),
        (consumer_unit, "lambdaOwned", OwnershipBindingKind::Owned),
        (consumer_unit, "lambdaShared", OwnershipBindingKind::Shared),
        (
            consumer_unit,
            "lambdaExclusive",
            OwnershipBindingKind::Exclusive,
        ),
    ] {
        let symbol = symbol_named(&ownership, &names, source, name);
        assert_eq!(ownership.binding_kind(symbol), Some(expected), "{name}");
        let descriptor = ownership
            .bindings()
            .iter()
            .find(|binding| binding.symbol() == symbol)
            .expect("binding descriptor");
        assert!(
            sources
                .slice(descriptor.declaration_span())
                .is_ok_and(|text| text.contains(name)),
            "{name} declaration span"
        );
    }
    assert!(ownership.diagnostics().is_empty());
    assert!(ownership.is_compatible_with(&typed));
    assert!(ownership.is_same_analysis(&ownership.clone()));

    let reversed_inputs = [inputs[1], inputs[0]];
    let reversed_names = validated_names(&sources, &reversed_inputs, &name_environment);
    let reversed_typed = validated_types(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
    );
    let reversed_ownership = check_compilation_unit_ownership(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
        &reversed_typed,
    )
    .expect("reversed unit ownership product");
    assert_eq!(ownership.bindings(), reversed_ownership.bindings());
    assert_eq!(ownership.diagnostics(), reversed_ownership.diagnostics());
}

#[test]
fn unit_ownership_rejects_mixed_analysis_and_duplicate_inputs() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\nfun read(input: String): Unit {}",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("first ownership product");

    let second_names = validated_names(&sources, &inputs, &name_environment);
    assert!(matches!(
        check_compilation_unit_ownership(
            &sources,
            &inputs,
            &second_names,
            &type_environment,
            &typed,
        ),
        Err(OwnershipCheckingError::MismatchedCompilationUnitTypes)
    ));

    let second_typed = validated_types(&sources, &inputs, &names, &type_environment);
    assert!(!ownership.is_compatible_with(&second_typed));
    let duplicate_inputs = [inputs[0], inputs[0]];
    assert!(matches!(
        check_compilation_unit_ownership(
            &sources,
            &duplicate_inputs,
            &names,
            &type_environment,
            &typed,
        ),
        Err(OwnershipCheckingError::MismatchedCompilationUnitTypes)
    ));
}
