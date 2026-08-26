//! SPEC-0197 compilation-unit signature identity 与诊断确定性回归。

use lang_frontend::{
    diagnostic::DiagnosticDetail,
    lexer::lex,
    name_resolution::{
        CompilationUnitNames, NameEnvironment, Namespace, SourceUnitInput,
        ValidatedCompilationUnitNames, index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        CompilationUnitSignatures, UnitCallableTarget, collect_compilation_unit_signatures,
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

fn names<'a>(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'a>],
    environment: &NameEnvironment,
) -> CompilationUnitNames {
    let index = index_compilation_unit(sources, inputs).expect("valid unit input");
    resolve_compilation_unit_names(sources, inputs, &index, environment)
        .expect("unit names resolve internally")
}

fn signatures(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &lang_frontend::type_checking::TypeEnvironment,
) -> CompilationUnitSignatures {
    collect_compilation_unit_signatures(sources, inputs, names, environment)
        .expect("signature collection succeeds")
}

#[test]
fn source_local_field_member_and_enum_case_ids_do_not_collide() {
    let mut sources = SourceMap::new();
    let (a_id, a) = parsed(
        &mut sources,
        "a.ko",
        "package a\nprivate fun helperA(): Unit {}\nclass ModelA(val fieldA: Int) { fun memberA(): Unit {} }\nenum class ChoiceA { PickA(valueA: Int) }",
    );
    let (b_id, b) = parsed(
        &mut sources,
        "b.ko",
        "package b\nprivate fun helperB(): Unit {}\nclass ModelB(val fieldB: Int) { fun memberB(): Unit {} }\nenum class ChoiceB { PickB(valueB: Int) }",
    );
    let inputs = [
        SourceUnitInput::new("root", "a/a.ko", a_id, &a),
        SourceUnitInput::new("root", "b/b.ko", b_id, &b),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = names(&sources, &inputs, &name_environment)
        .validate()
        .expect("valid names");
    let signatures = signatures(&sources, &inputs, &names, &type_environment);

    let nominal = |name: &str| {
        let declaration = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.namespace() == Namespace::Type && declaration.name() == name
            })
            .expect("nominal declaration");
        signatures
            .declaration(declaration.id())
            .and_then(|signature| signature.nominal())
            .expect("nominal signature")
    };
    let model_a = nominal("ModelA");
    let model_b = nominal("ModelB");
    let choice_a = nominal("ChoiceA");
    let choice_b = nominal("ChoiceB");
    let member_symbol = |target| match target {
        UnitCallableTarget::Symbol(symbol) => symbol,
        UnitCallableTarget::Declaration(_) => panic!("instance member must use a unit symbol"),
    };

    let pairs = [
        (model_a.fields()[0].symbol(), model_b.fields()[0].symbol()),
        (
            member_symbol(model_a.members()[0].target()),
            member_symbol(model_b.members()[0].target()),
        ),
        (
            choice_a.enum_cases()[0].value_symbol(),
            choice_b.enum_cases()[0].value_symbol(),
        ),
        (
            choice_a.enum_cases()[0].type_symbol(),
            choice_b.enum_cases()[0].type_symbol(),
        ),
        (
            choice_a.enum_cases()[0].payloads()[0].symbol(),
            choice_b.enum_cases()[0].payloads()[0].symbol(),
        ),
    ];
    for (left, right) in pairs {
        assert_eq!(left.symbol().index(), right.symbol().index());
        assert_ne!(left, right);
    }

    for helper in ["helperA", "helperB"] {
        let declaration = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == helper)
            .expect("private helper declaration");
        assert!(
            signatures
                .declaration(declaration.id())
                .and_then(|signature| signature.callable())
                .is_some(),
            "private top-level callable must remain in its unit signature graph",
        );
    }
}

#[test]
fn cross_source_member_diagnostics_are_equal_under_input_permutation() {
    let mut sources = SourceMap::new();
    let (contract_id, contract) = parsed(
        &mut sources,
        "contract.ko",
        "package p\ninterface Contract { fun act(input: Int): Int }",
    );
    let (implementation_id, implementation) = parsed(
        &mut sources,
        "implementation.ko",
        "package p\nclass Implementation : Contract { override fun act(input: Int): Long = 1L }",
    );
    let forward = [
        SourceUnitInput::new("root", "p/contract.ko", contract_id, &contract),
        SourceUnitInput::new(
            "root",
            "p/implementation.ko",
            implementation_id,
            &implementation,
        ),
    ];
    let reverse = [forward[1], forward[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = names(&sources, &forward, &name_environment)
        .validate()
        .expect("valid forward names");
    let reverse_names = names(&sources, &reverse, &name_environment)
        .validate()
        .expect("valid reverse names");
    let forward = signatures(&sources, &forward, &forward_names, &type_environment);
    let reverse = signatures(&sources, &reverse, &reverse_names, &type_environment);

    assert_eq!(forward, reverse);
    assert_eq!(forward.diagnostics().len(), 1);
    assert_eq!(forward.diagnostics()[0].code().to_string(), "L0100");
    assert_eq!(
        sources
            .slice(forward.diagnostics()[0].primary_span())
            .expect("primary span"),
        "override"
    );
    let label = forward.diagnostics()[0]
        .details()
        .iter()
        .find_map(|detail| match detail {
            DiagnosticDetail::Label(label) => Some(label.span()),
            DiagnosticDetail::Note(_) | DiagnosticDetail::Help(_) => None,
        })
        .expect("cross-source label");
    assert_eq!(label.source_id(), contract_id);
    assert_eq!(sources.slice(label).expect("label span"), "act");
}
