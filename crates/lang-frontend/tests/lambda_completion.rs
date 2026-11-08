//! Creating a lambda must not execute its return or discard the enclosing flow path.

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SourceUnitKey, index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::{Expression, parse_file},
    source::SourceMap,
    type_checking::{UnitExpressionId, check_compilation_unit_types, standard_environments},
};

fn check(text: &str, expected_code: Option<&str>) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("main.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (name_environment, type_environment) = standard_environments();
    let expected_codes = expected_code.into_iter().collect::<Vec<_>>();

    for (mode, input) in [
        ("memory", SourceUnitInput::memory_document(source, &parsed)),
        (
            "rooted",
            SourceUnitInput::new("root", "main.ko", source, &parsed),
        ),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let [indexed] = index.source_units() else {
            panic!("one original source");
        };
        assert_eq!(indexed.source_id(), source);
        assert!(matches!(
            (mode, indexed.key()),
            ("memory", SourceUnitKey::MemoryDocument) | ("rooted", SourceUnitKey::Rooted { .. })
        ));
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed =
            check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
        let codes = typed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(codes, expected_codes, "{mode}: {text}");
        let mut lambdas = 0;
        let mut returns = 0;
        for (id, node) in parsed.ast().expressions().iter() {
            let id = UnitExpressionId::new(indexed.id(), id);
            if matches!(node.payload(), Expression::Lambda { .. }) {
                lambdas += 1;
                assert_eq!(
                    typed.expression_falls_through(id),
                    Some(true),
                    "creating a lambda continues even when its body returns: {text}"
                );
            }
            if matches!(node.payload(), Expression::Return { .. }) {
                returns += 1;
                assert_eq!(
                    typed.expression_falls_through(id),
                    Some(false),
                    "a checked local return still terminates its own callable path: {text}"
                );
            }
        }
        assert_eq!(lambdas, 1, "{mode}: {text}");
        assert!(returns > 0, "{mode}: {text}");
    }
}

#[test]
fn lambda_return_does_not_satisfy_the_enclosing_functions_return() {
    for body in ["({ -> return })", "val callback: () -> Unit = { return }"] {
        check(&format!("fun missing(): Int {{ {body} }}"), Some("L0088"));
    }
}

#[test]
fn lambda_creation_keeps_nullable_assignment_paths_in_the_branch_join() {
    for literal in ["({ -> return })", "val callback: () -> Unit = { return }"] {
        check(
            &format!(
                "fun bad(flag: Boolean): Int {{\n\
                 var x: Int? = 1\n\
                 if (x != null) {{\n\
                   if (flag) {{ x = null; {literal} }}\n\
                   return x\n\
                 }}\n\
                 return 0\n\
                 }}"
            ),
            Some("L0084"),
        );
    }
}

#[test]
fn lambda_body_return_remains_local_when_outer_function_returns_explicitly() {
    check(
        "fun valid(): Int { val callback: () -> Unit = { return }; return 1 }",
        None,
    );
}

#[test]
fn lambda_body_is_still_checked_against_its_own_return_type() {
    check(
        "fun invalid(): Int { val callback: () -> Unit = { return 1 }; return 1 }",
        Some("L0087"),
    );
}
