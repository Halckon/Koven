//! Signature-role permutations from real sources; no injected facts or native execution.
use super::*;
use crate::ssa::model::{
    EntityId, EntityType, LoanKind, Operation, Origin, ScalarConstant, SsaTypeKind,
};

fn check(order: &[usize], prefix: &str, operands: &[&str; 4], named: bool) {
    let parameters = [
        "left:String",
        "right:String",
        "rounds:Int",
        "choose:Boolean",
    ];
    let arguments = if named {
        "choose=choose,rounds=rounds,right=\"RIGHT\",left=\"LEFT\"".to_owned()
    } else {
        order
            .iter()
            .map(|role| operands[*role])
            .collect::<Vec<_>>()
            .join(",")
    };
    let text = source(false, "")
        .replace(
            "left:String,right:String,rounds:Int,choose:Boolean",
            &order
                .iter()
                .map(|role| parameters[*role])
                .collect::<Vec<_>>()
                .join(","),
        )
        .replace(
            "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)",
            &format!("val choose=false;{prefix}val rounds=3;run({arguments})"),
        );
    let mut sources = SourceMap::new();
    let id = sources.add_source("roles.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "roles.ko", id, &parsed)];
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
    if named {
        assert!(matches!(
            result,
            Err(TwoSourceUnitError::UnsupportedSelection)
        ));
        return;
    }
    let unit = result.unwrap_or_else(|error| panic!("{order:?} {prefix} {operands:?}: {error:?}"));
    let module = &unit.program.modules[0];
    let f = module.function(unit.function).unwrap();
    let (call_position, callee, call_arguments) = f
        .instructions
        .iter()
        .enumerate()
        .find_map(|(i, instruction)| {
            if let Operation::DirectCall {
                callee, arguments, ..
            } = &instruction.operation
            {
                Some((i, *callee, arguments))
            } else {
                None
            }
        })
        .unwrap();
    let callee = module.function(callee).unwrap();
    let parameters = &callee.blocks[0].parameters;
    assert_eq!(parameters.len(), 4);
    let begins: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(i, instruction)| {
            if let Operation::BorrowBegin { place, kind } = instruction.operation {
                assert_eq!(kind, LoanKind::Shared);
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
    assert_eq!(
        *call_arguments,
        begins
            .iter()
            .map(|(_, _, _, loan)| EntityId::Loan(*loan))
            .collect::<Vec<_>>()
    );
    let mut last_end = 0;
    for (position, role) in order.iter().copied().enumerate() {
        let (begin_position, begin, place, loan) = begins[position];
        assert!(begin_position < call_position);
        let expected = match role {
            0 | 1 => SsaTypeKind::StringOwner,
            2 => SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            },
            _ => SsaTypeKind::Boolean,
        };
        for entity in [
            f.entity(EntityId::Loan(loan)).unwrap(),
            callee.entity(parameters[position]).unwrap(),
        ] {
            let EntityType::Loan { kind, target } = entity.ty else {
                panic!("borrow parameter")
            };
            assert_eq!(kind, LoanKind::Shared);
            assert_eq!(module.types[target.index], expected);
        }
        let Origin::Source(span) = begin.origin else {
            panic!("argument origin")
        };
        assert_eq!(sources.slice(span).unwrap(), operands[role]);
        let owner = f
            .instructions
            .iter()
            .find_map(|i| match i.operation {
                Operation::RootPlace { owner } if i.results == [EntityId::Place(place)] => {
                    Some(owner)
                }
                _ => None,
            })
            .unwrap();
        let initializer = f
            .instructions
            .iter()
            .find(|i| i.results == [EntityId::Value(owner)])
            .unwrap();
        match (&initializer.operation, role) {
            (Operation::Constant(ScalarConstant::Integer(value)), 2) => assert_eq!(*value, 3),
            (Operation::Constant(ScalarConstant::Boolean(value)), 3) => assert!(!value),
            (Operation::StringLiteral { bytes, .. }, 0 | 1) => {
                let expected = if operands[role] == "left" || operands[role] == "\"LEFT\"" {
                    b"LEFT".as_slice()
                } else {
                    b"RIGHT".as_slice()
                };
                assert_eq!(bytes, expected);
            }
            actual => panic!("role mismatch {actual:?}"),
        }
        let ends: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(
                |(_, i)| matches!(i.operation, Operation::BorrowEnd { loan: ended } if ended==loan),
            )
            .collect();
        assert_eq!(ends.len(), 1);
        assert!(ends[0].0 > call_position);
        last_end = last_end.max(ends[0].0);
    }
    let string_owners: Vec<_> = f
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
        .map(|i| i.results[0])
        .collect();
    let drops: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, i)| match i.operation {
            Operation::Drop { owner } => Some((index, EntityId::Value(owner))),
            _ => None,
        })
        .collect();
    assert_eq!(drops.len(), string_owners.len());
    for owner in string_owners {
        assert_eq!(drops.iter().filter(|(_, value)| *value == owner).count(), 1);
    }
    assert!(drops.iter().all(|(position, _)| *position > last_end));
    crate::ssa::verify::verify_program(&unit.program).unwrap();
    crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
}

fn permutations(items: &mut [usize; 4], at: usize, out: &mut Vec<[usize; 4]>) {
    if at == 4 {
        out.push(*items);
        return;
    }
    for next in at..4 {
        items.swap(at, next);
        permutations(items, at + 1, out);
        items.swap(at, next);
    }
}

#[test]
fn entry_arguments_preserve_all_signature_role_permutations() {
    let mut orders = Vec::new();
    permutations(&mut [3, 0, 1, 2], 0, &mut orders);
    for order in orders {
        for (prefix, operands) in [
            ("", ["\"LEFT\"", "\"RIGHT\"", "rounds", "choose"]),
            ("val left=\"LEFT\";", ["left", "left", "rounds", "choose"]),
            (
                "val right=\"RIGHT\";val left=\"LEFT\";",
                ["left", "right", "rounds", "choose"],
            ),
            (
                "val left=\"LEFT\";",
                ["left", "\"RIGHT\"", "rounds", "choose"],
            ),
            (
                "val right=\"RIGHT\";",
                ["\"LEFT\"", "right", "rounds", "choose"],
            ),
        ] {
            check(&order, prefix, &operands, false);
        }
    }
}
#[test]
fn entry_role_permutations_do_not_admit_reordered_named_arguments() {
    check(
        &[0, 1, 2, 3],
        "",
        &["\"LEFT\"", "\"RIGHT\"", "rounds", "choose"],
        true,
    );
}
