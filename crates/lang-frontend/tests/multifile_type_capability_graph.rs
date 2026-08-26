//! SPEC-0197 compilation-unit capability、interface 与 inline layout graph 契约。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        CompilationUnitNames, NameEnvironment, SourceUnitInput, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{collect_compilation_unit_signatures, standard_environments},
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

fn primary_texts<'a>(sources: &'a SourceMap, diagnostics: &[Diagnostic]) -> Vec<&'a str> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span")
        })
        .collect()
}

#[test]
fn interfaces_are_rejected_in_cross_file_runtime_signature_positions() {
    let mut sources = SourceMap::new();
    let (protocol_id, protocol) = parsed(
        &mut sources,
        "protocol.ko",
        "package p\npublic interface Protocol",
    );
    let (api_id, api) = parsed(
        &mut sources,
        "api.ko",
        "package p\nvalue class Holder(val item: Protocol)\nfun use(input: Protocol): Protocol",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/protocol.ko", protocol_id, &protocol),
        SourceUnitInput::new("root", "p/api.ko", api_id, &api),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = names(&sources, &inputs, &name_environment)
        .validate()
        .expect("valid names");
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
            .expect("signature collection succeeds");

    assert_eq!(codes(signatures.diagnostics()), ["L0094", "L0094", "L0094"]);
    assert_eq!(
        primary_texts(&sources, signatures.diagnostics()),
        ["Protocol", "Protocol", "Protocol"]
    );
    assert!(
        signatures
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.details().len() == 1)
    );
}

#[test]
fn inherited_interface_instances_reject_conflicting_invariant_arguments() {
    let mut sources = SourceMap::new();
    let (base_id, base) = parsed(
        &mut sources,
        "base.ko",
        "package p\ninterface Base<T>\ninterface ValidOnly { fun required(): Unit }\ninterface InvalidOnly { fun skipped(): Unit }",
    );
    let (left_id, left) = parsed(
        &mut sources,
        "left.ko",
        "package p\ninterface Left : Base<Int>, ValidOnly",
    );
    let (right_id, right) = parsed(
        &mut sources,
        "right.ko",
        "package p\ninterface Right : Base<String>, InvalidOnly",
    );
    let (both_id, both) = parsed(
        &mut sources,
        "both.ko",
        "package p\nclass Both : Left, Right",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/base.ko", base_id, &base),
        SourceUnitInput::new("root", "p/left.ko", left_id, &left),
        SourceUnitInput::new("root", "p/right.ko", right_id, &right),
        SourceUnitInput::new("root", "p/both.ko", both_id, &both),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = names(&sources, &inputs, &name_environment)
        .validate()
        .expect("valid names");
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
            .expect("signature collection succeeds");

    assert_eq!(codes(signatures.diagnostics()), ["L0101", "L0095"]);
    assert_eq!(
        primary_texts(&sources, signatures.diagnostics()),
        ["Both", "Right"]
    );
    assert_eq!(signatures.diagnostics()[0].details().len(), 1);
    let requirement = signatures.diagnostics()[0]
        .details()
        .iter()
        .find_map(|detail| match detail {
            lang_frontend::diagnostic::DiagnosticDetail::Label(label) => {
                sources.slice(label.span()).ok()
            }
            lang_frontend::diagnostic::DiagnosticDetail::Note(_)
            | lang_frontend::diagnostic::DiagnosticDetail::Help(_) => None,
        })
        .expect("missing requirement label");
    assert_eq!(requirement, "required");
    let both = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "Both")
        .expect("Both declaration");
    let both = signatures
        .declaration(both.id())
        .and_then(|declaration| declaration.nominal())
        .expect("Both nominal signature");
    assert_eq!(both.direct_interfaces().len(), 1);
    assert_eq!(both.interfaces().len(), 3);
}

#[test]
fn copyable_and_transferable_bounds_follow_cross_file_structural_types() {
    let mut sources = SourceMap::new();
    let (bounds_id, bounds) = parsed(
        &mut sources,
        "bounds.ko",
        "package p\nvalue class NeedsCopy<T: Copyable>(val item: T)\nvalue class NeedsTransfer<T: Transferable>(val item: T)",
    );
    let (models_id, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\nvalue class CopyValue(val item: Int)\nclass Ref\nclass TransferRef(val text: String)",
    );
    let (api_id, api) = parsed(
        &mut sources,
        "api.ko",
        "package p\nfun inspect(okCopy: NeedsCopy<CopyValue>, badCopy: NeedsCopy<Ref>, okTransfer: NeedsTransfer<TransferRef>, badTransfer: NeedsTransfer<(Int) -> Int>): Unit",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/bounds.ko", bounds_id, &bounds),
        SourceUnitInput::new("root", "p/models.ko", models_id, &models),
        SourceUnitInput::new("root", "p/api.ko", api_id, &api),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = names(&sources, &inputs, &name_environment)
        .validate()
        .expect("valid names");
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
            .expect("signature collection succeeds");

    assert_eq!(codes(signatures.diagnostics()), ["L0115", "L0141"]);
    assert_eq!(
        primary_texts(&sources, signatures.diagnostics()),
        ["Ref", "(Int) -> Int"]
    );
    assert!(signatures.clone().validate().is_err());
}

#[test]
fn inline_value_and_enum_cycles_are_diagnosed_once_per_component() {
    let mut sources = SourceMap::new();
    let (a_id, a) = parsed(&mut sources, "a.ko", "package p\nvalue class A(val b: B)");
    let (b_id, b) = parsed(&mut sources, "b.ko", "package p\nvalue class B(val a: A)");
    let (loop_id, loop_file) = parsed(
        &mut sources,
        "loop.ko",
        "package p\nenum class Loop { Next(item: Loop) }\nclass Node(var next: Node?)\nvalue class Finite(val node: Node)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/a.ko", a_id, &a),
        SourceUnitInput::new("root", "p/b.ko", b_id, &b),
        SourceUnitInput::new("root", "p/loop.ko", loop_id, &loop_file),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = names(&sources, &inputs, &name_environment)
        .validate()
        .expect("valid names");
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &names, &type_environment)
            .expect("signature collection succeeds");

    assert_eq!(codes(signatures.diagnostics()), ["L0116", "L0116"]);
    assert_eq!(
        primary_texts(&sources, signatures.diagnostics()),
        ["A", "Loop"]
    );
    assert!(signatures.clone().validate().is_err());
}
