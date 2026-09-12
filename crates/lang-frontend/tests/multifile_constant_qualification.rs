//! SPEC-0210: constant type eligibility follows ordinary type diagnostics.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

fn check(texts: [&str; 2], expected_codes: &[&str], expected_span: Option<&str>) {
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
        if let Some(span) = expected_span {
            assert_eq!(
                sources
                    .slice(typed.diagnostics()[0].primary_span())
                    .unwrap(),
                span
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
        if !expected_codes.is_empty() {
            assert!(typed.validate().is_err());
        }
    }
}

#[test]
fn explicit_constant_types_are_closed_in_each_namespace() {
    for (ty, value) in [
        ("Int?", "1"),
        ("Double", "1.5"),
        ("Float", "1.5f"),
        ("() -> Int", "{ 1 }"),
    ] {
        for body in [
            format!("const val VALUE: {ty} = {value}"),
            format!("object A {{ const val VALUE: {ty} = {value} }}"),
            format!("class A {{ companion object {{ const val VALUE: {ty} = {value} }} }}"),
        ] {
            check(
                [
                    &format!("package a\n{body}"),
                    "package b\nfun unused(): Unit {}",
                ],
                &["L0155"],
                Some(ty),
            );
        }
    }
}

#[test]
fn inferred_constant_types_and_mismatches_keep_precise_diagnostics() {
    check(
        [
            "package a\nobject A { const val VALUE = 1.5 }",
            "package b\nfun unused(): Unit {}",
        ],
        &["L0155"],
        Some("1.5"),
    );
    for ty in ["Int", "Double"] {
        check(
            [
                &format!("package a\nobject A {{ const val VALUE: {ty} = true }}"),
                "package b\nfun unused(): Unit {}",
            ],
            &["L0084"],
            None,
        );
    }
}

#[test]
fn ordinary_variables_keep_nonconstant_types() {
    check(
        [
            "package a\nval sample: Double = 1.5",
            "package b\nval sample: Int? = 1",
        ],
        &[],
        None,
    );
}

#[test]
fn invalid_nested_signature_types_do_not_add_constant_diagnostics() {
    check(
        [
            "package a\nconst val VALUE: Array<Int<String>>? = null",
            "package b\nfun unused(): Unit {}",
        ],
        &["L0082"],
        None,
    );
}

#[test]
fn closed_scalar_types_remain_available() {
    for (ty, literal) in [
        ("Boolean", "true"),
        ("Byte", "1"),
        ("Short", "1"),
        ("Int", "1"),
        ("Long", "1L"),
        ("UByte", "1u"),
        ("UShort", "1u"),
        ("UInt", "1u"),
        ("ULong", "1uL"),
        ("Char", "'文'"),
        ("String", "\"中\""),
    ] {
        check(
            [
                &format!("package a\nobject A {{ const val VALUE: {ty} = {literal} }}"),
                "package b\nfun unused(): Unit {}",
            ],
            &[],
            None,
        );
    }
}

#[test]
fn invalid_cross_file_dependency_does_not_repeat_type_qualification() {
    for (ty, literal) in [("Double", "1.5"), ("Int?", "1")] {
        check(
            [
                "package a\nimport b.B\nobject A { const val VALUE = B.VALUE }",
                &format!("package b\nobject B {{ const val VALUE: {ty} = {literal} }}"),
            ],
            &["L0155"],
            Some(ty),
        );
    }
}

#[test]
fn invalid_signature_dependency_keeps_only_the_upstream_error() {
    check(
        [
            "package a\nimport b.B\nobject A { const val COPY = B.BAD }",
            "package b\nobject B { const val BAD: Array<Int<String>>? = null }",
        ],
        &["L0082"],
        None,
    );
}
