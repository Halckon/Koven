//! Actual declaration ordering without widening the closed initializer or body grammar.
use super::*;
use crate::ssa::model::{Operation, Origin, ScalarConstant};

fn check(declarations: &[&str], call: &str, accepted: bool) {
    let original = source(false, "");
    let old = "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)";
    let body = format!("{};{call}", declarations.join(";"));
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("order.ko", original.replace(old, &body))
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "order.ko", id, &parsed)];
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
    if !accepted {
        assert!(
            matches!(result, Err(TwoSourceUnitError::UnsupportedSelection)),
            "{body}"
        );
        return;
    }
    let unit = result.unwrap_or_else(|error| panic!("{body}: {error:?}"));
    let f = unit.program.modules[0].function(unit.function).unwrap();
    let mut previous_end = 0;
    for (index, declaration) in declarations.iter().enumerate() {
        let literal = declaration.split_once('=').unwrap().1;
        let instruction = &f.instructions[index * 2];
        let Origin::Source(span) = instruction.origin else {
            panic!("source origin")
        };
        assert!(
            span.start() >= previous_end,
            "initializers reordered: {body}"
        );
        previous_end = span.end();
        assert_eq!(sources.slice(span).unwrap(), literal, "{body}");
        match (&instruction.operation, literal) {
            (Operation::Constant(ScalarConstant::Integer(value)), "3") => assert_eq!(*value, 3),
            (Operation::Constant(ScalarConstant::Boolean(value)), "false") => assert!(!value),
            (Operation::StringLiteral { bytes, .. }, "\"LEFT\"") => assert_eq!(bytes, b"LEFT"),
            (Operation::StringLiteral { bytes, .. }, "\"RIGHT\"") => assert_eq!(bytes, b"RIGHT"),
            actual => panic!("unexpected initializer {actual:?}: {body}"),
        }
        assert!(matches!(
            f.instructions[index * 2 + 1].operation,
            Operation::RootPlace { .. }
        ));
    }
    crate::ssa::verify::verify_program(&unit.program).unwrap();
    crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
}

fn permutations<'a>(items: &mut [&'a str], index: usize, out: &mut Vec<Vec<&'a str>>) {
    if index == items.len() {
        out.push(items.to_vec());
        return;
    }
    for next in index..items.len() {
        items.swap(index, next);
        permutations(items, index + 1, out);
        items.swap(index, next);
    }
}

#[test]
fn entry_declarations_preserve_source_order_and_scalar_roles() {
    let rounds = "val rounds=3";
    let choose = "val choose=false";
    for (mut declarations, call) in [
        (
            vec![choose, rounds],
            "run(\"LEFT\",\"RIGHT\",rounds,choose)",
        ),
        (
            vec![choose, "val left=\"LEFT\"", rounds],
            "run(left,left,rounds,choose)",
        ),
        (
            vec![choose, "val left=\"LEFT\"", rounds, "val right=\"RIGHT\""],
            "run(right,left,rounds,choose)",
        ),
        (
            vec![choose, "val left=\"LEFT\"", rounds],
            "run(left,\"RIGHT\",rounds,choose)",
        ),
        (
            vec![choose, "val left=\"LEFT\"", rounds],
            "run(\"RIGHT\",left,rounds,choose)",
        ),
    ] {
        let mut orders = Vec::new();
        permutations(&mut declarations, 0, &mut orders);
        for order in orders {
            check(&order, call, true);
        }
    }
}

#[test]
fn entry_declaration_permutations_do_not_admit_extra_or_computed_locals() {
    for declarations in [
        vec!["val choose=false", "val unused=1", "val rounds=3"],
        vec!["val choose=false", "val extra=true", "val rounds=3"],
        vec!["val choose=false", "val rounds=1+2"],
        vec!["val choose=false", "var rounds=3"],
        vec!["val choose=false", "val unused=\"LEFT\"", "val rounds=3"],
    ] {
        check(
            &declarations,
            "run(\"LEFT\",\"RIGHT\",rounds,choose)",
            false,
        );
    }
}
