//! SPEC-0210: unit constants are checked in dependency order, with syntax edges for SCCs.
use lang_frontend::{
    diagnostic::DiagnosticDetail,
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        BuiltinType, UnitTypeKind, check_compilation_unit_types, standard_environments,
    },
};

fn check(texts: [&str; 2], expected_codes: &[&str], expected_read: Option<BuiltinType>) {
    let mut sources = SourceMap::new();
    let a = sources.add_source("a.ko", texts[0]).unwrap();
    let b = sources.add_source("b.ko", texts[1]).unwrap();
    let fa = parse_file(&sources, &lex(&sources, a).unwrap()).unwrap();
    let fb = parse_file(&sources, &lex(&sources, b).unwrap()).unwrap();
    assert!(fa.diagnostics().is_empty(), "{:?}", fa.diagnostics());
    assert!(fb.diagnostics().is_empty(), "{:?}", fb.diagnostics());
    let inputs = [
        SourceUnitInput::new("root", "a/source.ko", a, &fa),
        SourceUnitInput::new("root", "b/source.ko", b, &fb),
    ];
    let (ne, te) = standard_environments();
    let mut previous = None;
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            expected_codes
        );
        for diagnostic in typed.diagnostics() {
            if diagnostic.code().to_string() == "L0157" {
                let primary = sources.slice(diagnostic.primary_span()).unwrap();
                let labels = diagnostic
                    .details()
                    .iter()
                    .filter_map(|detail| match detail {
                        DiagnosticDetail::Label(label) => {
                            Some(sources.slice(label.span()).unwrap())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                match primary {
                    "SELF" => assert!(labels.is_empty()),
                    "FIRST" => assert_eq!(labels, ["LAST", "MIDDLE"]),
                    "VALUE" => assert_eq!(labels, ["VALUE"]),
                    _ => panic!("unexpected cycle primary: {primary}"),
                }
            }
        }
        if let Some(expected) = expected_read {
            let (expression, _) = fa
                .ast()
                .expressions()
                .iter()
                .find(|(_, node)| sources.slice(node.span()) == Ok("a.A.VALUE"))
                .unwrap();
            let unit = names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|unit| unit.source_id() == a)
                .unwrap()
                .id();
            let id = lang_frontend::type_checking::UnitExpressionId::new(unit, expression);
            assert_eq!(
                typed.types().get(typed.expression_type(id).unwrap()),
                Some(&UnitTypeKind::Builtin(expected))
            );
        }
        let snapshot = (
            typed.types().clone(),
            typed.expression_types().clone(),
            typed.diagnostics().to_vec(),
        );
        if let Some(previous) = &previous {
            assert_eq!(&snapshot, previous);
        }
        previous = Some(snapshot);
        assert!(
            typed.validate().is_err(),
            "dependency facts cannot grant the old base capability"
        );
    }
}

#[test]
fn forward_associated_chains_infer_before_runtime_bodies() {
    check(
        [
            "package a\nimport b.B\nobject A { const val VALUE = B.VALUE + 1 }\nfun read(): Int = a.A.VALUE",
            "package b\nobject B { const val VALUE = 40 + 1 }",
        ],
        &[],
        Some(BuiltinType::Int),
    );
    check(
        [
            "package a\nimport b.B\nobject A { const val VALUE = B.VALUE + \"文\" }\nfun read(): String = a.A.VALUE",
            "package b\nobject B { const val VALUE = \"中\" }",
        ],
        &[],
        Some(BuiltinType::String),
    );
}

#[test]
fn cycles_include_short_circuit_rhs_and_emit_one_diagnostic_per_scc() {
    check(
        [
            "package a\nimport b.B\nobject A { const val VALUE = B.VALUE }",
            "package b\nimport a.A\nobject B { const val VALUE = A.VALUE }",
        ],
        &["L0157"],
        None,
    );
    check(
        [
            "package a\nimport b.B\nobject A { const val VALUE: Boolean = false && B.VALUE }",
            "package b\nimport a.A\nobject B { const val VALUE: Boolean = A.VALUE }",
        ],
        &["L0157"],
        None,
    );
}

#[test]
fn cyclic_operand_types_are_rechecked_after_indirect_inference() {
    check(
        [
            "package a\nconst val A = B && true\nconst val B = C\nconst val C: Int = A",
            "package b\nfun unused(): Unit {}",
        ],
        &["L0085"],
        None,
    );
}

#[test]
fn separate_self_and_multinode_cycles_keep_stable_diagnostics() {
    check(
        [
            "package a\nimport b.B\nobject A { const val SELF = SELF\nconst val FIRST = B.MIDDLE\nconst val LAST = FIRST }",
            "package b\nimport a.A\nobject B { const val MIDDLE = A.LAST }",
        ],
        &["L0157", "L0157"],
        None,
    );
}
