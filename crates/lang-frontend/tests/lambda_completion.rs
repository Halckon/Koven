//! Creating a lambda must not execute its return or discard the enclosing flow path.

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::{Expression, parse_file},
    source::SourceMap,
    type_checking::{
        UnitExpressionId, check_compilation_unit_types, check_types, standard_environments,
    },
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
    let names = resolve_names(&sources, &parsed, &name_environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &type_environment).unwrap();
    let single_codes: Vec<_> = typed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect();

    let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    let unit_codes: Vec<_> = typed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect();
    for (mode, codes) in [("single", single_codes), ("unit", unit_codes)] {
        if let Some(expected) = expected_code {
            assert!(
                codes.iter().any(|code| code == expected),
                "{mode}: {text}: {codes:?}"
            );
        } else {
            assert!(codes.is_empty(), "{mode}: {text}: {codes:?}");
        }
    }
    let unit = names.names().index().source_units()[0].id();
    for (id, node) in parsed.ast().expressions().iter() {
        if matches!(node.payload(), Expression::Lambda { .. }) {
            assert_eq!(
                typed.expression_falls_through(UnitExpressionId::new(unit, id)),
                Some(true),
                "creating a lambda continues even when its body returns: {text}"
            );
        }
        if matches!(node.payload(), Expression::Return { .. }) {
            assert_eq!(
                typed.expression_falls_through(UnitExpressionId::new(unit, id)),
                Some(false),
                "a checked local return still terminates its own callable path: {text}"
            );
        }
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
