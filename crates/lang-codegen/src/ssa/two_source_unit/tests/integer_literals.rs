//! Real-source integer spelling shares the frontend decoder; no injected facts or native runs.
use super::*;
use crate::ssa::model::{Operation, Origin, ScalarConstant};

#[derive(Clone, Copy, Debug)]
enum Expected {
    Value(i32),
    Unsupported,
    TypeDiagnostic,
}

fn check(literal: &str, shape: usize, expected: Expected) {
    let (prefix, call, drops) = match shape {
        0 => ("", "run(\"LEFT\",\"RIGHT\",rounds,choose)", 2),
        1 => (
            "val owner=\"OWNER\";",
            "run(choose=choose,right=owner,rounds=rounds,left=owner)",
            1,
        ),
        _ => (
            "val owner=\"OWNER\";",
            "run(right=\"RIGHT\",choose=choose,left=owner,rounds=rounds)",
            2,
        ),
    };
    let text = source(false, "").replace(
        "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)",
        &format!("val choose=false;{prefix}val rounds={literal};{call}"),
    );
    let text = if shape == 2 {
        text.replace(
            "left:String,right:String,rounds:Int,choose:Boolean",
            "choose:Boolean,rounds:Int,right:String,left:String",
        )
    } else {
        text
    };
    let mut sources = SourceMap::new();
    let id = sources.add_source("integer.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "integer.ko", id, &parsed)];
    let (env, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let checked = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .expect("type checker infrastructure");
    let typed = match checked.validate() {
        Ok(typed) => {
            assert!(
                !matches!(expected, Expected::TypeDiagnostic),
                "expected type diagnostic: {literal}"
            );
            typed
        }
        Err(error) => {
            assert!(
                matches!(expected, Expected::TypeDiagnostic),
                "unexpected type diagnostic {literal}: {error:?}"
            );
            return;
        }
    };
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "main")
        .unwrap()
        .id();
    let result = prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry);
    let Expected::Value(expected) = expected else {
        assert!(
            matches!(result, Err(TwoSourceUnitError::UnsupportedSelection)),
            "{literal}"
        );
        return;
    };
    let unit = result.unwrap_or_else(|e| panic!("{literal} shape={shape}: {e:?}"));
    let f = unit.program.modules[0].function(unit.function).unwrap();
    let integers: Vec<_> = f
        .instructions
        .iter()
        .filter_map(|i| {
            if let Operation::Constant(ScalarConstant::Integer(value)) = i.operation {
                let Origin::Source(span) = i.origin else {
                    panic!("literal source span")
                };
                Some((value, sources.slice(span).unwrap()))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(integers, vec![(i128::from(expected), literal)]);
    assert_eq!(
        f.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
            .count(),
        4
    );
    assert_eq!(
        f.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::Drop { .. }))
            .count(),
        drops
    );
    crate::ssa::verify::verify_program(&unit.program).unwrap();
    crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
}

#[test]
fn entry_integer_spellings_preserve_value_and_source_span() {
    for (literal, value) in [
        ("0x2a", 42),
        ("0X2A", 42),
        ("0b10_1010", 42),
        ("0B101010", 42),
        ("1_024", 1024),
        ("00042", 42),
        ("0", 0),
        ("0x0", 0),
        ("2147483647", i32::MAX),
        ("0x7fff_ffff", i32::MAX),
    ] {
        for shape in 0..3 {
            check(literal, shape, Expected::Value(value));
        }
    }
}

#[test]
fn entry_integer_spellings_keep_expression_and_type_boundaries() {
    for literal in ["-1", "1+2", "(3)"] {
        check(literal, 0, Expected::Unsupported);
    }
    for literal in ["3L", "3u", "3.0", "true", "2147483648", "0x8000_0000"] {
        check(literal, 0, Expected::TypeDiagnostic);
    }
}
