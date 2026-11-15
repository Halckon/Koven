//! Real-source static literal decoding; no fact or SSA mutation and no native execution.
use super::*;
use crate::ssa::model::Operation;

fn check(body: &str, expected: Option<Vec<Vec<u8>>>) {
    let original = source(false, "");
    let old = "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)";
    assert!(original.contains(old));
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("static.ko", original.replace(old, body))
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "static.ko", id, &parsed)];
    let (env, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "main")
        .unwrap()
        .id();
    let result = prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry);
    let Some(expected) = expected else {
        assert!(
            matches!(result, Err(TwoSourceUnitError::UnsupportedSelection)),
            "{body}"
        );
        return;
    };
    let unit = result.unwrap_or_else(|error| panic!("{body}: {error:?}"));
    let function = unit.program.modules[0].function(unit.function).unwrap();
    let literals: Vec<_> = function
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::StringLiteral { bytes, .. } = &instruction.operation {
                Some(bytes.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(literals, expected, "{body}");
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::Drop { .. }))
            .count(),
        expected.len()
    );
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
            .count(),
        4
    );
    crate::ssa::verify::verify_program(&unit.program).unwrap();
    crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
}

fn bodies(literal: &str) -> Vec<(String, usize)> {
    let scalars = "val rounds=3;val choose=false;";
    vec![
        (
            format!("{scalars}run({literal},{literal},rounds,choose)"),
            2,
        ),
        (
            format!("val owner={literal};{scalars}run(owner,owner,rounds,choose)"),
            1,
        ),
        (
            format!(
                "val left={literal};val right={literal};{scalars}run(right,left,rounds,choose)"
            ),
            2,
        ),
        (
            format!("val owner={literal};{scalars}run(owner,{literal},rounds,choose)"),
            2,
        ),
        (
            format!("val owner={literal};{scalars}run({literal},owner,rounds,choose)"),
            2,
        ),
    ]
}

#[test]
fn static_string_escapes_preserve_bytes_in_every_owner_shape() {
    // All supported escapes, multibyte UTF-8, embedded NUL and a literal interpolation opener.
    let literal = r#""中\n\r\t\0\$\"\'\\\${""#;
    let bytes = "中\n\r\t\0$\"'\\${".as_bytes().to_vec();
    for (literal, bytes) in [
        (literal, bytes),
        (r#""""#, vec![]),
        (r#""中🙂""#, "中🙂".as_bytes().to_vec()),
    ] {
        for (body, count) in bodies(literal) {
            check(&body, Some(vec![bytes.clone(); count]));
        }
    }
}

#[test]
fn static_string_entries_still_reject_real_interpolation() {
    for (body, _) in bodies(r#""${1}""#) {
        check(&body, None);
    }
}
