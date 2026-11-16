//! Direct scalar arguments retain source evaluation, original End and ABI mapping.
use super::*;
use crate::ssa::model::{EntityId, Operation, Origin, ScalarConstant};

#[derive(Clone, Copy, Debug)]
enum Expected {
    Value(i32),
    Unsupported,
    TypeDiagnostic,
}

fn check(literal: &str, shape: usize, direct: [bool; 2], expected: Expected) {
    check_variant(literal, shape, direct, expected, "false", "");
}
fn check_variant(
    literal: &str,
    shape: usize,
    direct: [bool; 2],
    expected: Expected,
    boolean: &str,
    extra: &str,
) {
    let (prefix, call, drops) = match shape {
        0 => ("", "run(\"LEFT\",\"RIGHT\",rounds,choose)", 2),
        1 => (
            "val owner=\"OWNER\";",
            "run(choose=choose,right=owner,rounds=rounds,left=owner)",
            1,
        ),
        2 => (
            "val owner=\"OWNER\";",
            "run(right=\"RIGHT\",choose=choose,left=owner,rounds=rounds)",
            2,
        ),
        _ => (
            "val left=\"LEFT\";val right=\"RIGHT\";",
            "run(right=right,rounds=rounds,left=left,choose=choose)",
            2,
        ),
    };
    let call = call
        .replace("rounds=rounds", "rounds=ROUNDARG")
        .replace("choose=choose", "choose=BOOLARG");
    let call = call
        .replace("ROUNDARG", if direct[0] { literal } else { "rounds" })
        .replace("BOOLARG", if direct[1] { boolean } else { "choose" });
    let call = if shape == 0 {
        format!(
            "run(\"LEFT\",\"RIGHT\",{},{})",
            if direct[0] { literal } else { "rounds" },
            if direct[1] { boolean } else { "choose" }
        )
    } else {
        call
    };
    let declarations = format!(
        "{}{}",
        if direct[0] {
            String::new()
        } else {
            format!("val rounds={literal};")
        },
        if direct[1] { "" } else { "val choose=false;" }
    );
    let text = source(false, "").replace(
        "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)",
        &format!("{prefix}{extra}{declarations}{call}"),
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
    let source_roles: [usize; 4] = match shape {
        0 => [0, 1, 2, 3],
        1 => [3, 1, 2, 0],
        2 => [1, 3, 0, 2],
        _ => [1, 2, 0, 3],
    };
    let abi_roles = if shape == 2 {
        [3, 2, 1, 0]
    } else {
        [0, 1, 2, 3]
    };
    let begins: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(i, instruction)| {
            if let Operation::BorrowBegin { place, .. } = instruction.operation {
                let [EntityId::Loan(loan)] = instruction.results.as_slice() else {
                    panic!("loan")
                };
                Some((i, instruction, place, *loan))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(begins.len(), 4);
    let (call_position, arguments) = f
        .instructions
        .iter()
        .enumerate()
        .find_map(|(i, instruction)| {
            if let Operation::DirectCall { arguments, .. } = &instruction.operation {
                Some((i, arguments))
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        *arguments,
        abi_roles
            .iter()
            .map(|role| EntityId::Loan(
                begins[source_roles.iter().position(|r| r == role).unwrap()].3
            ))
            .collect::<Vec<_>>()
    );
    let mut previous_end = 0;
    for ((position, begin, place, loan), role) in begins.iter().zip(source_roles) {
        let Origin::Source(span) = begin.origin else {
            panic!("argument span")
        };
        assert!(span.start() >= previous_end);
        previous_end = span.end();
        assert!(*position < call_position);
        let end_positions: Vec<_> = f.instructions.iter().enumerate().filter_map(|(i, instruction)| matches!(instruction.operation, Operation::BorrowEnd { loan: actual } if actual == *loan).then_some(i)).collect();
        assert_eq!(end_positions.len(), 1);
        assert!(end_positions[0] > call_position);
        if role >= 2 && direct[role - 2] {
            let root = &f.instructions[position - 1];
            assert_eq!(root.results, vec![EntityId::Place(*place)]);
            let Operation::RootPlace { owner } = root.operation else {
                panic!("temporary root")
            };
            let constant = &f.instructions[position - 2];
            assert_eq!(constant.results, vec![EntityId::Value(owner)]);
            assert_eq!(constant.origin, begin.origin);
            assert_eq!(root.origin, begin.origin);
            assert_eq!(
                constant.operation,
                Operation::Constant(if role == 2 {
                    ScalarConstant::Integer(i128::from(expected))
                } else {
                    ScalarConstant::Boolean(boolean == "true")
                })
            );
        }
    }
    let last_end = f
        .instructions
        .iter()
        .rposition(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
        .unwrap();
    let string_owners: Vec<_> = f
        .instructions
        .iter()
        .filter_map(|i| {
            if matches!(i.operation, Operation::StringLiteral { .. }) {
                Some(i.results[0])
            } else {
                None
            }
        })
        .collect();
    for owner in string_owners {
        let positions: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| {
                if let Operation::Drop { owner: actual } = instruction.operation {
                    (EntityId::Value(actual) == owner).then_some(index)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(positions.len(), 1);
        assert!(positions[0] > last_end);
    }
    crate::ssa::verify::verify_program(&unit.program).unwrap();
    crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
}

#[test]
fn direct_scalar_arguments_preserve_real_constants_and_cleanup() {
    for direct in [[true, true], [true, false], [false, true], [false, false]] {
        for shape in 0..4 {
            check("0x2a", shape, direct, Expected::Value(42));
        }
    }
}
#[test]
fn direct_scalar_arguments_keep_restricted_expression_boundary() {
    for literal in ["(3)", "1+2", "-1"] {
        check(literal, 0, [true, true], Expected::Unsupported);
    }
    for literal in ["3L", "3u", "2147483648"] {
        check(literal, 0, [true, true], Expected::TypeDiagnostic);
    }
}

#[test]
fn direct_scalars_do_not_leave_unused_locals_or_accept_boolean_expressions() {
    check_variant("3", 2, [true, true], Expected::Value(3), "true", "");
    for extra in ["val rounds=3;", "val choose=false;"] {
        check_variant("3", 0, [true, true], Expected::Unsupported, "false", extra);
    }
    for boolean in ["(true)", "!false"] {
        check_variant("3", 0, [true, true], Expected::Unsupported, boolean, "");
    }
}
