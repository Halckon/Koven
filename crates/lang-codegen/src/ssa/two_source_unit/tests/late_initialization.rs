//! Owner formation after a complete earlier call, never hoisted to entry start.
use super::*;
use crate::ssa::model::{EntityId, Operation, Origin};
use lang_frontend::ownership_checking::{
    LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
};
struct Case {
    first: &'static str,
    initializer: &'static str,
    second: &'static str,
    bytes: [&'static [u8]; 3],
}
fn cases() -> [Case; 2] {
    [
        Case {
            first: "run(\"a\",\"b\",1,false)",
            initializer: "\"late\"",
            second: "run(owner,owner,2,true)",
            bytes: [b"a", b"b", b"late"],
        },
        Case {
            first: "run(choose=false,right=\"same\",rounds=1,left=\"same\")",
            initializer: "\"same\"",
            second: "run(rounds=2,right=owner,choose=true,left=owner)",
            bytes: [b"same", b"same", b"same"],
        },
    ]
}

#[test]
fn late_initialization_original_facts_keep_formation_between_calls() {
    for case in cases() {
        let mut sources = SourceMap::new();
        let text = format!(
            "fun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun main():Unit {{{};val owner={};{}}}",
            case.first, case.initializer, case.second
        );
        let id = sources.add_source("late-probe.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "late-probe.ko", id, &parsed)];
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
        let checked = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        let owned = checked.ownership();
        assert_eq!(owned.loans().len(), 8);
        assert_eq!(owned.direct_call_loan_ends().len(), 8);
        assert_eq!(owned.drops().len(), 3);
        let [owner] = owned.formed_owned_locals() else {
            panic!("one original formed named root")
        };
        let symbol = names.names().source_units()[owner.source_unit().index()]
            .resolution()
            .symbols()
            .get(owner.symbol().index())
            .unwrap();
        let mut call_spans = Vec::new();
        let mut named_drop = None;
        for (call_number, text) in [case.first, case.second].into_iter().enumerate() {
            let matches: Vec<_> = parsed
                .ast()
                .expressions()
                .iter()
                .filter(|(_, n)| sources.slice(n.span()) == Ok(text))
                .collect();
            let [(expression, node)] = matches.as_slice() else {
                panic!("original call AST")
            };
            call_spans.push(node.span());
            let loans: Vec<_> = owned
                .loans()
                .iter()
                .filter(|l| l.call().expression() == *expression)
                .collect();
            assert_eq!(loans.len(), 4);
            let call = loans[0].call();
            let contracts: Vec<_> = owned.call_argument_contracts_for(call).collect();
            assert_eq!(contracts.len(), 4);
            let mut string_arguments = Vec::new();
            for loan in &loans {
                assert_eq!(loan.kind(), LoanKind::Shared);
                let contract = contracts
                    .iter()
                    .find(|c| c.argument() == loan.argument())
                    .unwrap();
                if contract.parameter_index() < 2 {
                    string_arguments.push(loan.argument());
                    if call_number == 0 {
                        assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
                    } else {
                        assert!(
                            matches!(loan.target(),UnitLoanTarget::Place(p) if p.is_root()&&p.root()==*owner)
                        );
                    }
                } else {
                    assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
                }
                let ends: Vec<_> = owned
                    .direct_call_loan_ends()
                    .iter()
                    .filter(|e| e.loan() == *loan)
                    .collect();
                assert_eq!(ends.len(), 1);
                assert_eq!(ends[0].point(), UnitDropPoint::CallReturn(call));
                assert_eq!(ends[0].preceding_drops(), 0);
            }
            let ends: Vec<_> = owned
                .direct_call_loan_ends()
                .iter()
                .filter(|e| e.loan().call() == call)
                .collect();
            assert_eq!(
                ends.iter()
                    .map(|e| e.preceding_call_loan_ends())
                    .collect::<Vec<_>>(),
                [0, 1, 2, 3]
            );
            let drops: Vec<_> = owned
                .drops()
                .iter()
                .filter(|d| d.point() == UnitDropPoint::CallReturn(call))
                .collect();
            if call_number == 0 {
                assert_eq!(drops.len(), 2, "actual original Drops: {:?}", owned.drops());
                // This fixture checks the current planner's reverse pending-temporary traversal,
                // not a language-wide destruction rule. Keep the actual fact vector unsorted.
                let source_strings: Vec<_> = contracts
                    .iter()
                    .filter(|c| c.parameter_index() < 2)
                    .map(|c| UnitDropTarget::Temporary(c.argument()))
                    .collect();
                assert_eq!(source_strings.len(), 2);
                assert_eq!(
                    drops.iter().map(|d| d.target()).collect::<Vec<_>>(),
                    [source_strings[1], source_strings[0]],
                    "actual original Drops: {:?}",
                    owned.drops()
                );
                for argument in string_arguments {
                    assert_eq!(
                        drops
                            .iter()
                            .filter(|d| d.target() == UnitDropTarget::Temporary(argument))
                            .count(),
                        1
                    );
                    assert_eq!(
                        owned
                            .drops()
                            .iter()
                            .filter(|d| d.target() == UnitDropTarget::Temporary(argument))
                            .count(),
                        1
                    );
                }
            } else {
                assert_eq!(drops.len(), 1, "actual original Drops: {:?}", owned.drops());
                assert_eq!(drops[0].target(), UnitDropTarget::Named(*owner));
                named_drop = Some(*drops[0]);
                assert_eq!(
                    owned
                        .drops()
                        .iter()
                        .filter(|d| d.target() == UnitDropTarget::Named(*owner))
                        .count(),
                    1
                );
            }
        }
        // formed_owned_locals is membership, not a fabricated temporal event. The original AST
        // locates its actual initializer between the two complete CallReturn control points.
        assert!(
            call_spans[0].end() < symbol.span().start()
                && symbol.span().end() < call_spans[1].start()
        );
        let initializers: Vec<_> = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, n)| {
                n.span().start() > call_spans[0].end()
                    && n.span().end() < call_spans[1].start()
                    && sources.slice(n.span()) == Ok(case.initializer)
            })
            .collect();
        let [(_, initializer)] = initializers.as_slice() else {
            panic!("unique original late initializer")
        };
        assert!(symbol.span().end() < initializer.span().start());
        // The original named Drop diagnostic origin is the binding name, not its initializer.
        assert_eq!(named_drop.unwrap().value_origin(), symbol.span());
    }
}

#[test]
fn late_initialization_emits_after_first_cleanup_before_second_acquisition() {
    for case in cases() {
        let mut sources = SourceMap::new();
        let body = format!(
            "{};val owner={};{}",
            case.first, case.initializer, case.second
        );
        let text = source(false, "").replace(
            "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)",
            &body,
        );
        let id = sources.add_source("late-entry.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new("root", "late-entry.ko", id, &parsed)];
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
        let unit = prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry)
            .expect("original late initialization");
        let f = unit.program.modules[0].function(unit.function).unwrap();
        let strings: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation, Operation::StringLiteral { .. }))
            .collect();
        assert_eq!(strings.len(), 3);
        let mut owners = Vec::new();
        for ((_, i), bytes) in strings.iter().zip(case.bytes) {
            let Operation::StringLiteral { bytes: actual, .. } = &i.operation else {
                unreachable!()
            };
            assert_eq!(actual, bytes);
            let [EntityId::Value(owner)] = i.results.as_slice() else {
                panic!("owner")
            };
            assert!(!owners.contains(owner));
            owners.push(*owner);
        }
        let late_index = strings[2].0;
        let late_owner = owners[2];
        let late_roots: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation,Operation::RootPlace{owner} if owner==late_owner))
            .collect();
        let [(late_root_index, late_root)] = late_roots.as_slice() else {
            panic!("one late root")
        };
        let [EntityId::Place(late_place)] = late_root.results.as_slice() else {
            panic!("late place")
        };
        assert!(late_index < *late_root_index);
        let calls: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation, Operation::DirectCall { .. }))
            .collect();
        assert_eq!(calls.len(), 2);
        let Origin::Source(late_span) = strings[2].1.origin else {
            panic!("late initializer source")
        };
        let mut all_loans = Vec::new();
        for (call_number, (call_index, call)) in calls.into_iter().enumerate() {
            let Origin::Source(span) = call.origin else {
                panic!("call source")
            };
            assert_eq!(
                sources.slice(span).unwrap(),
                [case.first, case.second][call_number]
            );
            if call_number == 0 {
                assert!(span.end() < late_span.start());
            } else {
                assert!(late_span.end() < span.start());
            }
            let Operation::DirectCall { arguments, .. } = &call.operation else {
                unreachable!()
            };
            assert_eq!(arguments.len(), 4);
            let mut last_end = call_index;
            let mut string_owners = Vec::new();
            let mut begins = Vec::new();
            for (role, argument) in arguments.iter().enumerate() {
                let EntityId::Loan(loan) = argument else {
                    panic!("loan")
                };
                assert!(!all_loans.contains(loan));
                all_loans.push(*loan);
                let (begin_index, begin) = f
                    .instructions
                    .iter()
                    .enumerate()
                    .find(|(_, i)| i.results == vec![*argument])
                    .unwrap();
                assert!(begin_index < call_index);
                let Origin::Source(argspan) = begin.origin else {
                    panic!("arg source")
                };
                begins.push((begin_index, argspan));
                if call_number == 1 {
                    assert!(*late_root_index < begin_index);
                }
                let Operation::BorrowBegin { place, .. } = begin.operation else {
                    panic!("begin")
                };
                if role < 2 {
                    if call_number == 1 {
                        assert_eq!(place, *late_place);
                    } else {
                        let root = f
                            .instructions
                            .iter()
                            .find(|i| i.results == vec![EntityId::Place(place)])
                            .unwrap();
                        let Operation::RootPlace { owner } = root.operation else {
                            panic!("root")
                        };
                        assert_ne!(owner, late_owner);
                        assert!(!string_owners.contains(&owner));
                        string_owners.push(owner);
                    }
                }
                let ends:Vec<_>=f.instructions.iter().enumerate().filter(|(_,i)|matches!(i.operation,Operation::BorrowEnd{loan:actual} if actual==*loan)).collect();
                assert_eq!(ends.len(), 1);
                assert!(ends[0].0 > call_index);
                last_end = last_end.max(ends[0].0);
            }
            begins.sort_by_key(|(index, _)| *index);
            assert!(begins.windows(2).all(|p| p[0].1.end() <= p[1].1.start()));
            if call_number == 1 {
                string_owners.push(late_owner);
            }
            assert_eq!(string_owners.len(), 2 - call_number);
            for owner in string_owners {
                let drops:Vec<_>=f.instructions.iter().enumerate().filter(|(_,i)|matches!(i.operation,Operation::Drop{owner:actual} if actual==owner)).collect();
                assert_eq!(drops.len(), 1);
                assert!(last_end < drops[0].0);
                if call_number == 0 {
                    assert!(drops[0].0 < late_index);
                }
            }
        }
        assert_eq!(all_loans.len(), 8);
        assert_eq!(
            f.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::Drop { .. }))
                .count(),
            3
        );
        crate::ssa::verify::verify_program(&unit.program).unwrap();
        crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
    }
}

#[test]
fn late_initialization_rejects_other_formation_and_call_shapes() {
    let original = source(false, "");
    let other = original
        .lines()
        .find(|line| line.starts_with("fun run("))
        .unwrap()
        .replace("fun run(", "fun other(");
    for (body, extra) in [
        (
            "run(\"a\",\"b\",1,false);val owner=\"late\";run(owner,\"temp\",2,true)",
            "",
        ),
        (
            "run(\"a\",\"b\",1,false);val owner=\"late\";val extra=\"extra\";run(owner,owner,2,true)",
            "",
        ),
        (
            "run(\"a\",\"b\",1,false);val owner=\"late\";consume(owner);run(owner,owner,2,true)",
            "",
        ),
        (
            "run(\"a\",\"b\",1,false);val owner=\"late\";run(owner,owner,2,true);run(\"c\",\"d\",3,false)",
            "",
        ),
        (
            "run(\"a\",\"b\",1,false);val owner=(\"late\");run(owner,owner,2,true)",
            "",
        ),
        (
            "run(\"a\",\"b\",1,false);val owner=\"late\";other(owner,owner,2,true)",
            other.as_str(),
        ),
        (
            "run(\"a\",\"b\",1,false);val owner=\"late\";run(owner,owner,(2),true)",
            "",
        ),
    ] {
        let mut sources = SourceMap::new();
        let text = source(false, extra).replace(
            "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)",
            body,
        );
        let id = sources.add_source("late-rejected.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new(
            "root",
            "late-rejected.ko",
            id,
            &parsed,
        )];
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
        let prepared = prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry);

        assert!(
            matches!(prepared, Err(TwoSourceUnitError::UnsupportedSelection)),
            "{body}"
        );
    }
}
