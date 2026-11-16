use super::*;
use crate::ssa::model::{EntityId, Operation};

#[test]
fn named_shared_entries_keep_cleanup_and_whole_unit_diagnostics() {
    let main_body = "val text=\"first\";val rounds=2;val choose=false;run(text,text,rounds,choose)";
    let alias = "fun alias():Unit {val text=\"second\";val rounds=1;val choose=true;run(text,text,rounds,choose)}";
    for bad in [false, true] {
        let extra = if bad {
            "fun take(own value:String):Unit {}\nfun bad():Unit {val bad_owner=\"bad\";take(bad_owner);consume(bad_owner)}"
        } else {
            ""
        };
        let text = source(false, &format!("{alias}\n{extra}")).replace(
            "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)",
            main_body,
        );
        let mut sources = SourceMap::new();
        let id = sources.add_source("combined.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let inputs = [SourceUnitInput::new("root", "combined.ko", id, &parsed)];
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
        let entry = |name| {
            names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == name)
                .unwrap()
                .id()
        };
        let requested = [entry("alias"), entry("main")];
        let result =
            prepare_two_source_units(&sources, &inputs, &names, &types, &typed, &requested);
        if bad {
            let Err(TwoSourceUnitError::OwnershipValidation(failure)) = result else {
                panic!("whole Unit failure must never issue partial capability")
            };
            assert!(
                failure
                    .diagnostics()
                    .iter()
                    .any(|d| d.code().to_string() == "L0131"
                        && sources.slice(d.primary_span()).unwrap() == "bad_owner")
            );
            continue;
        }
        let units = result.unwrap();
        assert_eq!(units.program.modules[0].functions.len(), 7);
        let mut callers = Vec::new();
        let mut functions = Vec::new();
        for selected in requested {
            let (_, program, function) = units.parts(selected).unwrap();
            functions.push(function);
            let f = program.modules[0].function(function).unwrap();
            let literals: Vec<_> = f
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
                .collect();
            let [literal] = literals.as_slice() else {
                panic!("one source owner per entry")
            };
            let [EntityId::Value(owner)] = literal.results.as_slice() else {
                panic!("one owner value")
            };
            let roots: Vec<_> = f
                .instructions
                .iter()
                .filter(|i| matches!(i.operation,Operation::RootPlace{owner:o} if o==*owner))
                .collect();
            let [root] = roots.as_slice() else {
                panic!("one owner root")
            };
            let [EntityId::Place(place)] = root.results.as_slice() else {
                panic!("one root result")
            };
            let loans: Vec<_> = f
                .instructions
                .iter()
                .filter(|i| matches!(i.operation,Operation::BorrowBegin{place:p,..} if p==*place))
                .collect();
            assert_eq!(loans.len(), 2);
            assert_ne!(loans[0].results, loans[1].results);
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
            assert_eq!(drops, [*owner]);
            let calls: Vec<_> = f
                .instructions
                .iter()
                .filter_map(|i| {
                    if let Operation::DirectCall { callee, .. } = i.operation {
                        Some(callee)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(calls.len(), 1);
            callers.push(calls[0]);
        }
        assert_ne!(functions[0], functions[1]);
        assert_eq!(callers[0], callers[1]);
        crate::ssa::verify::verify_program(&units.program).unwrap();
    }
}
