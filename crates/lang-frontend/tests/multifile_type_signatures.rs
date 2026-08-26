//! SPEC-0197 Stage 1 compilation-unit 类型身份与签名收集契约。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        CompilationUnitNames, NameEnvironment, SourceUnitInput, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        CompilationUnitSignatures, CompilationUnitTypeError, UnitTypeKind,
        collect_compilation_unit_signatures, standard_environments,
    },
};

fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique test source");
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

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn recursive_nominal_signatures_are_canonical_under_input_permutation() {
    let mut sources = SourceMap::new();
    let (node_id, node) = parsed(
        &mut sources,
        "node.ko",
        "package graph\npublic class Node(val edge: Edge?)\npublic fun node(edge: Edge): Node",
    );
    let (edge_id, edge) = parsed(
        &mut sources,
        "edge.ko",
        "package graph\npublic class Edge(val node: Node)\npublic fun edge(node: Node): Edge",
    );
    let forward = [
        SourceUnitInput::new("root", "graph/node.ko", node_id, &node),
        SourceUnitInput::new("root", "graph/edge.ko", edge_id, &edge),
    ];
    let reverse = [forward[1], forward[0]];
    let (name_environment, type_environment) = standard_environments();

    let forward_names = names(&sources, &forward, &name_environment)
        .validate()
        .expect("valid names");
    let reverse_names = names(&sources, &reverse, &name_environment)
        .validate()
        .expect("valid names");
    let forward =
        collect_compilation_unit_signatures(&sources, &forward, &forward_names, &type_environment)
            .expect("signature collection succeeds");
    let reverse =
        collect_compilation_unit_signatures(&sources, &reverse, &reverse_names, &type_environment)
            .expect("signature collection succeeds");

    assert_eq!(forward, reverse);
    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(
        forward
            .declarations()
            .iter()
            .filter(|declaration| declaration.nominal().is_some())
            .count(),
        2
    );
}

#[test]
fn source_local_type_parameter_identities_do_not_collide() {
    let mut sources = SourceMap::new();
    let (left_id, left) = parsed(
        &mut sources,
        "left.ko",
        "package p\nclass Left<T>(val item: T)",
    );
    let (right_id, right) = parsed(
        &mut sources,
        "right.ko",
        "package p\nclass Right<T>(val item: T)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/left.ko", left_id, &left),
        SourceUnitInput::new("root", "p/right.ko", right_id, &right),
    ];
    let (name_environment, type_environment) = standard_environments();
    let validated = names(&sources, &inputs, &name_environment)
        .validate()
        .expect("valid names");
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &validated, &type_environment)
            .expect("signature collection succeeds");
    let nominals = signatures
        .declarations()
        .iter()
        .filter_map(|declaration| declaration.nominal())
        .collect::<Vec<_>>();

    assert_eq!(nominals.len(), 2);
    let left_parameter = nominals[0].type_parameters()[0];
    let right_parameter = nominals[1].type_parameters()[0];
    assert_eq!(left_parameter.symbol(), right_parameter.symbol());
    assert_ne!(left_parameter.source_unit(), right_parameter.source_unit());
    assert_ne!(left_parameter, right_parameter);
    for nominal in nominals {
        let field_type = nominal.fields()[0].ty();
        assert_eq!(
            signatures.types().get(field_type),
            Some(&UnitTypeKind::TypeParameter(nominal.type_parameters()[0]))
        );
    }
}

#[test]
fn alpha_equivalent_top_level_shapes_across_sources_emit_l0097() {
    let mut sources = SourceMap::new();
    let (first_id, first) = parsed(
        &mut sources,
        "first.ko",
        "package p\nfun <T> pick(input: T): Int = 1",
    );
    let (second_id, second) = parsed(
        &mut sources,
        "second.ko",
        "package p\nfun <U> pick(other: U): Long = 1L",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/first.ko", first_id, &first),
        SourceUnitInput::new("root", "p/second.ko", second_id, &second),
    ];
    let (name_environment, type_environment) = standard_environments();
    let validated = names(&sources, &inputs, &name_environment)
        .validate()
        .expect("overloads are valid names");
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &validated, &type_environment)
            .expect("signature collection succeeds");

    assert_eq!(codes(signatures.diagnostics()), ["L0097"]);
    assert_eq!(signatures.diagnostics()[0].details().len(), 1);
    assert!(signatures.clone().validate().is_err());
}

#[test]
fn top_level_parameter_modes_do_not_distinguish_overload_shapes() {
    let mut sources = SourceMap::new();
    let (borrow_id, borrow) = parsed(
        &mut sources,
        "borrow.ko",
        "package p\nfun send(input: Int): Unit {}",
    );
    let (own_id, own) = parsed(
        &mut sources,
        "own.ko",
        "package p\nfun send(own input: Int): Unit {}",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/borrow.ko", borrow_id, &borrow),
        SourceUnitInput::new("root", "p/own.ko", own_id, &own),
    ];
    let (name_environment, type_environment) = standard_environments();
    let validated = names(&sources, &inputs, &name_environment)
        .validate()
        .expect("overloads are valid names");
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &validated, &type_environment)
            .expect("signature collection succeeds");

    assert_eq!(codes(signatures.diagnostics()), ["L0097"]);
}

#[test]
fn arity_and_mixed_analysis_inputs_are_rejected_at_the_right_boundary() {
    let mut sources = SourceMap::new();
    let (model_id, model) = parsed(
        &mut sources,
        "model.ko",
        "package p\nclass Holder<T>\nfun use(input: Holder<Int, Long>): Unit",
    );
    let inputs = [SourceUnitInput::new("root", "p/model.ko", model_id, &model)];
    let (name_environment, type_environment) = standard_environments();
    let validated = names(&sources, &inputs, &name_environment)
        .validate()
        .expect("valid names");
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &validated, &type_environment)
            .expect("signature collection succeeds");
    assert_eq!(codes(signatures.diagnostics()), ["L0091"]);

    let (_, unrelated_types) = standard_environments();
    assert!(matches!(
        collect_compilation_unit_signatures(&sources, &inputs, &validated, &unrelated_types),
        Err(CompilationUnitTypeError::MismatchedNameEnvironment)
    ));
    let changed_inputs = [SourceUnitInput::new(
        "root",
        "other/model.ko",
        model_id,
        &model,
    )];
    assert!(matches!(
        collect_compilation_unit_signatures(
            &sources,
            &changed_inputs,
            &validated,
            &type_environment,
        ),
        Err(CompilationUnitTypeError::MismatchedInputs)
    ));
}

#[allow(dead_code)]
fn assert_signature_product_is_public(_: &CompilationUnitSignatures) {}
