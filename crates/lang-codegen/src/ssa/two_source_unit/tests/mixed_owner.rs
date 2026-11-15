use super::*;
#[test]
fn mixed_owner_entry_preserves_temporary_argument_and_named_cleanup() {
    let original = source(false, "");
    let literal = "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)";
    let named =
        "val owner=\"NAMED\";val rounds=3;val choose=false;run(owner,\"TEMP\",rounds,choose)";
    for (body, accepted, reversed) in [
        (named.to_owned(), true, false),
        (literal.to_owned(), true, false),
        (named.replace("run(owner,", "run(\"UNUSED\","), false, false),
        (
            named.replace("val rounds", "val unused=\"EXTRA\";val rounds"),
            false,
            false,
        ),
        (
            named.replace("run(owner,\"TEMP\"", "run(\"TEMP\",owner"),
            true,
            true,
        ),
        (named.replace("TEMP", "NAMED"), true, false),
        (named.replace("run(owner,", "run((owner),"), false, false),
        (named.replace("val owner", "var owner"), false, false),
        (format!("{named};consume(owner)"), false, false),
    ] {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("named.ko", original.replace(literal, &body))
            .unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "named.ko", id, &parsed)];
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
            assert!(matches!(
                result,
                Err(TwoSourceUnitError::UnsupportedSelection)
            ));
            continue;
        }
        let unit = result.unwrap();
        use super::super::super::model::{EntityId, Operation};
        let f = unit.program.modules[0].function(unit.function).unwrap();
        let literals: Vec<_> = f
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
            .collect();
        assert_eq!(literals.len(), 2);
        assert_ne!(literals[0].results, literals[1].results);
        let owners: Vec<_> = literals
            .iter()
            .map(|i| {
                let [EntityId::Value(v)] = i.results.as_slice() else {
                    panic!("owner")
                };
                *v
            })
            .collect();
        let roots: Vec<_> = owners
            .iter()
            .map(|owner| {
                let i = f
                    .instructions
                    .iter()
                    .find(|i| matches!(i.operation,Operation::RootPlace{owner:o} if o==*owner))
                    .unwrap();
                let [EntityId::Place(p)] = i.results.as_slice() else {
                    panic!("root")
                };
                *p
            })
            .collect();
        let borrowed: Vec<_> = f
            .instructions
            .iter()
            .filter_map(|i| {
                if let Operation::BorrowBegin { place, .. } = i.operation {
                    Some(place)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            &borrowed[..2],
            if reversed {
                vec![roots[1], roots[0]]
            } else {
                roots.clone()
            }
        );
        let drops: Vec<_> = f
            .instructions
            .iter()
            .filter_map(|i| {
                if let Operation::Drop { owner } = i.operation {
                    Some(owner)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(drops.len(), 2);
        for owner in owners {
            assert_eq!(drops.iter().filter(|o| **o == owner).count(), 1);
        }
        assert_eq!(
            f.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
                .count(),
            4
        );
        super::super::super::verify::verify_program(&unit.program).unwrap();
        crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
    }
}
