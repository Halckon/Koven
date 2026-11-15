//! Owner formation after a complete earlier call, never hoisted to entry start.
use super::*;
use crate::ssa::model::{EntityId, Operation, Origin};
use lang_frontend::ownership_checking::{
    LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
};
use lang_frontend::{
    ownership_checking::two_source_experiment::analyze_two_source_experiment,
    type_checking::UnitCallTarget,
};
struct Case {
    first: &'static str,
    bytes: [&'static [u8]; 3],
    initializer: &'static str,
    second: &'static str,
}
fn cases() -> [Case; 2] {
    [
        Case {
            first: "run_a(\"a\",\"b\",1,false)",
            bytes: [b"a", b"b", b"late"],
            initializer: "\"late\"",
            second: "run_b(owner,owner,2,true)",
        },
        Case {
            first: "run_b(choose=false,right=\"same\",rounds=1,left=\"same\")",
            bytes: [b"same", b"same", b"same"],
            initializer: "\"same\"",
            second: "run_a(rounds=2,right=owner,choose=true,left=owner)",
        },
    ]
}

#[test]
fn late_distinct_original_facts_keep_formation_between_calls() {
    for case in cases() {
        let mut sources = SourceMap::new();
        let text = format!(
            "fun run_a(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun run_b(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun main():Unit {{{};val owner={};{}}}",
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
        let mut keys = Vec::new();
        let mut all_arguments = Vec::new();
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
            let key = typed.types().call(call).unwrap().instance();
            assert!(key.type_arguments().is_empty());
            keys.push(key.clone());
            let contracts: Vec<_> = owned.call_argument_contracts_for(call).collect();
            assert_eq!(contracts.len(), 4);
            for column in 0..4 {
                assert_eq!(
                    contracts
                        .iter()
                        .filter(|c| c.parameter_index() == column)
                        .count(),
                    1
                );
            }
            let mut string_arguments = Vec::new();
            for loan in &loans {
                assert_eq!(loan.kind(), LoanKind::Shared);
                assert!(!all_arguments.contains(&loan.argument()));
                all_arguments.push(loan.argument());
                let matching: Vec<_> = contracts
                    .iter()
                    .filter(|c| c.argument() == loan.argument())
                    .collect();
                let [contract] = matching.as_slice() else {
                    panic!("unique original contract")
                };
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
        assert_ne!(keys[0], keys[1]);
        assert_eq!(all_arguments.len(), 8);
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
fn late_distinct_closed_facts_prepare_late_owner() {
    for case in cases() {
        check_closed(&case, None, None);
    }
}
#[test]
fn late_distinct_rejects_other_shapes_and_unclaimed_shells() {
    let [case, _] = cases();
    for body in [
        "run_a(\"a\",\"b\",1,false);val owner=\"late\";run_b(owner,\"temp\",2,true)",
        "run_a(\"a\",\"b\",1,false);val owner=\"late\";val other=\"extra\";run_b(owner,other,2,true)",
        "val owner=\"late\";run_a(\"a\",\"b\",1,false);run_b(owner,owner,2,true)",
        "run_a(\"a\",\"b\",1,false);val owner=\"late\";consume(owner);run_b(owner,owner,2,true)",
        "run_a(\"a\",\"b\",1,false);val owner=\"late\";run_b(owner,owner,2,true);run_a(\"a\",\"b\",1,false)",
        "run_a(\"a\",\"b\",1,false);val owner=(\"late\");run_b(owner,owner,2,true)",
        "run_a(\"a\",\"b\",1,false);val owner=\"late\";run_b(owner,owner,1+1,true)",
        "run_a(\"a\",\"b\",1,false);val owner=\"late\";val rounds=2;run_b(owner,owner,rounds,true)",
    ] {
        check_closed(&case, Some(body), Some(0));
    }
    check_closed(&case, None, Some(1));
    check_closed(&case, None, Some(2));
}
fn check_closed(case: &Case, body: Option<&str>, rejected: Option<u8>) {
    let calls = [case.first, case.second];
    let original = source(false, "");
    let a = original
        .lines()
        .filter(|line| !line.starts_with("fun main("))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("fun run(", "fun run_a(");
    let b = original
        .lines()
        .find(|line| line.starts_with("fun run("))
        .unwrap()
        .replace("fun run(", "fun run_b(");
    let mut source_text = format!(
        "{a}\n{b}\nfun main():Unit {{{};val owner={};{}}}",
        case.first, case.initializer, case.second
    );
    if let Some(body) = body {
        source_text = source_text.replace(
            &format!(
                "{};val owner={};{}",
                case.first, case.initializer, case.second
            ),
            body,
        );
    }
    if rejected == Some(1) {
        source_text += &format!("\n{}", b.replace("fun run_b(", "fun unused("));
    }
    if rejected == Some(2) {
        source_text = source_text.replace(
            &b,
            "fun run_b(left:String,right:String,rounds:Int,choose:Boolean):Unit {}",
        );
    }
    let mut sources = SourceMap::new();
    let id = sources
        .add_source("shared-distinct-entry.ko", source_text)
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new(
        "root",
        "shared-distinct-entry.ko",
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
    if let Some(kind) = rejected {
        if kind == 1 {
            assert!(matches!(prepared, Err(TwoSourceUnitError::Preflight(_))));
        } else {
            assert!(matches!(
                prepared,
                Err(TwoSourceUnitError::UnsupportedSelection)
            ));
        }
        return;
    }
    let experiment = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
        .unwrap()
        .unwrap();
    assert_eq!(experiment.facts().len(), 2);
    let mut keys = Vec::new();
    for call_text in calls {
        let nodes: Vec<_> = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, n)| sources.slice(n.span()) == Ok(call_text))
            .collect();
        let [(expression, _)] = nodes.as_slice() else {
            panic!("unique original call")
        };
        let typed_calls: Vec<_> = typed
            .types()
            .calls()
            .iter()
            .filter(|call| call.expression().expression() == *expression)
            .collect();
        let [typed_call] = typed_calls.as_slice() else {
            panic!("original typed call")
        };
        let key = typed_call.instance();
        assert!(key.type_arguments().is_empty());
        let matching: Vec<_> = experiment
            .facts()
            .iter()
            .filter(|fact| {
                experiment.callable(fact).map(UnitCallTarget::Declaration) == Some(key.target())
            })
            .collect();
        assert_eq!(matching.len(), 1);
        let shell = experiment.function_shell(matching[0], &typed).unwrap();
        assert_eq!(UnitCallTarget::Declaration(shell.callable()), key.target());
        keys.push(key.clone());
    }
    assert_ne!(keys[0], keys[1]);

    let descriptor = experiment
        .entries(&typed)
        .unwrap()
        .iter()
        .find(|d| d.entry() == entry)
        .unwrap();
    assert!(descriptor.is_different_late_initialized_pair());
    assert!(descriptor.common_instance().is_none());
    assert!(descriptor.shared_owner().is_none() && descriptor.staggered_owners().is_none());
    #[cfg(feature = "e2-two-source-test-support")]
    for fact in experiment.facts() {
        assert!(matches!(
            crate::ssa::e2_entry_publication::publish(
                &experiment,
                descriptor,
                fact,
                &[fact],
                &typed
            ),
            Err(crate::ssa::e2_entry_publication::Error::Required)
        ));
        assert!(crate::ssa::e2_entry::emit(&experiment, fact, descriptor, &typed).is_none());
    }
    let unit = prepared.expect("distinct late original source");
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
    let mut callees = Vec::new();
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
        let expected = &descriptor.calls()[call_number];
        assert_eq!(
            typed
                .types()
                .call(expected.contracts()[0].call())
                .unwrap()
                .instance(),
            expected.instance()
        );
        let UnitCallTarget::Declaration(declaration) = expected.instance().target() else {
            panic!("declaration")
        };
        let declaration = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.id() == declaration)
            .unwrap();
        let declaration_span = parsed.ast().items().get(declaration.root()).unwrap().span();
        let Operation::DirectCall {
            callee, arguments, ..
        } = &call.operation
        else {
            unreachable!()
        };
        assert_eq!(
            unit.program.modules[0].function(*callee).unwrap().origin,
            Origin::Source(declaration_span)
        );
        assert!(!callees.contains(callee));
        callees.push(*callee);
        assert_eq!(arguments.len(), 4);
        let mut last_end = call_index;
        let mut string_owners = Vec::new();
        let mut argument_owners = Vec::new();
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
            let contracts: Vec<_> = expected
                .contracts()
                .iter()
                .filter(|c| c.parameter_index() == role)
                .collect();
            let [contract] = contracts.as_slice() else {
                panic!("unique ABI column")
            };
            assert_eq!(
                argspan,
                parsed
                    .ast()
                    .expressions()
                    .get(contract.argument().expression())
                    .unwrap()
                    .span()
            );
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
                    argument_owners.push((contract.argument(), owner));
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
        let mut prior_drop = None;
        for original in expected.drops() {
            let (owner, origin) = match original.target() {
                UnitDropTarget::Temporary(argument) => (
                    argument_owners
                        .iter()
                        .find(|(a, _)| *a == argument)
                        .unwrap()
                        .1,
                    Origin::Source(expected.contracts()[0].call_span()),
                ),
                UnitDropTarget::Named(_) => (late_owner, Origin::Source(original.value_origin())),
                _ => panic!("bounded Drop"),
            };
            let drops: Vec<_> = f
                .instructions
                .iter()
                .enumerate()
                .filter(
                    |(_, i)| matches!(i.operation,Operation::Drop{owner:actual}if actual==owner),
                )
                .collect();
            assert_eq!(drops.len(), 1);
            assert!(last_end < drops[0].0);
            assert_eq!(drops[0].1.origin, origin);
            assert!(prior_drop.is_none_or(|previous| previous < drops[0].0));
            prior_drop = Some(drops[0].0);
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
