//! Original last-use evidence for two distinct named owners ending at different calls.
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
    declarations: &'static str,
    bytes: [&'static [u8]; 2],
    calls: [&'static str; 2],
    // ABI String roles expressed as original declaration-order root indices.
    roots: [[usize; 2]; 2],
    dropped: [usize; 2],
}
fn cases() -> [Case; 2] {
    [
        Case {
            declarations: "val a=\"A\";val b=\"B\"",
            bytes: [b"A", b"B"],
            calls: ["run_a(a,b,1,false)", "run_b(a,a,2,true)"],
            roots: [[0, 1], [0, 0]],
            dropped: [1, 0],
        },
        Case {
            declarations: "val a=\"same\";val b=\"same\"",
            bytes: [b"same", b"same"],
            calls: [
                "run_b(choose=false,right=a,rounds=1,left=b)",
                "run_a(right=b,rounds=2,left=b,choose=true)",
            ],
            roots: [[1, 0], [1, 1]],
            dropped: [0, 1],
        },
    ]
}

#[test]
fn staggered_distinct_original_facts_preserve_separate_last_use() {
    for case in cases() {
        let mut sources = SourceMap::new();
        let text = format!(
            "fun run_a(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun run_b(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun main():Unit {{{};{};{}}}",
            case.declarations, case.calls[0], case.calls[1]
        );
        let id = sources.add_source("stagger-probe.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new(
            "root",
            "stagger-probe.ko",
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
        let checked = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        let owned = checked.ownership();
        assert_eq!(owned.loans().len(), 8);
        assert_eq!(owned.direct_call_loan_ends().len(), 8);
        assert_eq!(owned.drops().len(), 2);
        let mut roots = Vec::new();
        // Derive declaration order from resolved source spans, never from Drop/loan vector order.
        let mut bindings: Vec<_> = owned
            .formed_owned_locals()
            .iter()
            .filter_map(|root| {
                let symbol = names
                    .names()
                    .source_units()
                    .get(root.source_unit().index())?
                    .resolution()
                    .symbols()
                    .get(root.symbol().index())?;
                Some((*root, symbol.span()))
            })
            .collect();
        bindings.sort_by_key(|(_, span)| span.start());
        assert_eq!(bindings.len(), 2);
        for (root, _) in &bindings {
            roots.push(*root);
        }
        assert_ne!(roots[0], roots[1]);
        let mut source_end = bindings[1].1.end();
        let mut instances = Vec::new();
        let mut all_arguments = Vec::new();
        for (call_number, text) in case.calls.iter().enumerate() {
            let found: Vec<_> = parsed
                .ast()
                .expressions()
                .iter()
                .filter(|(_, n)| sources.slice(n.span()) == Ok(*text))
                .collect();
            let [(expression, node)] = found.as_slice() else {
                panic!("unique call AST")
            };
            assert!(node.span().start() >= source_end);
            source_end = node.span().end();
            let loans: Vec<_> = owned
                .loans()
                .iter()
                .filter(|l| l.call().expression() == *expression)
                .collect();
            assert_eq!(loans.len(), 4);
            let call = loans[0].call();
            let instance = typed.types().call(call).unwrap().instance();
            assert!(instance.type_arguments().is_empty());
            instances.push(instance.clone());
            if call_number == 0 {
                let initializers: Vec<_> = parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter(|(_, n)| {
                        n.span().end() < node.span().start()
                            && sources.slice(n.span()).is_ok_and(|t| t.starts_with('"'))
                    })
                    .collect();
                assert_eq!(initializers.len(), 2);
                let mut spans = initializers
                    .iter()
                    .map(|(_, n)| n.span())
                    .collect::<Vec<_>>();
                spans.sort_by_key(|s| s.start());
                assert!(
                    bindings[0].1.end() < spans[0].start()
                        && spans[0].end() < bindings[1].1.start()
                        && bindings[1].1.end() < spans[1].start()
                );
            }
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
            for loan in &loans {
                assert!(!all_arguments.contains(&loan.argument()));
                all_arguments.push(loan.argument());
                assert_eq!(loan.kind(), LoanKind::Shared);
                let matching: Vec<_> = contracts
                    .iter()
                    .filter(|c| c.argument() == loan.argument())
                    .collect();
                let [contract] = matching.as_slice() else {
                    panic!("one original contract per loan")
                };
                if contract.parameter_index() < 2 {
                    let expected = roots[case.roots[call_number][contract.parameter_index()]];
                    assert!(
                        matches!(loan.target(),UnitLoanTarget::Place(p) if p.is_root()&&p.root()==expected)
                    );
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
            // Inspect and assert the entire original point-local vector, not a guessed synthetic Drop.
            let drops: Vec<_> = owned
                .drops()
                .iter()
                .filter(|d| d.point() == UnitDropPoint::CallReturn(call))
                .collect();
            assert_eq!(drops.len(), 1, "original Drop facts: {:?}", owned.drops());
            let target = UnitDropTarget::Named(roots[case.dropped[call_number]]);
            assert_eq!(
                drops[0].value_origin(),
                bindings[case.dropped[call_number]].1
            );
            assert_eq!(
                drops[0].target(),
                target,
                "original Drop facts: {:?}",
                owned.drops()
            );
            assert_eq!(
                owned
                    .drops()
                    .iter()
                    .filter(|d| d.target() == target)
                    .count(),
                1
            );
        }
        assert_ne!(instances[0], instances[1]);
        assert_eq!(all_arguments.len(), 8);
    }
}

#[test]
fn staggered_distinct_closed_facts_prepare_two_owner_root() {
    for case in cases() {
        check_closed(&case, None, None);
    }
}

#[test]
fn staggered_distinct_rejects_other_shapes_and_unclaimed_shells() {
    let [case, _] = cases();
    for body in [
        "val a=\"A\";val b=\"B\";run_a(a,a,1,false);run_b(b,b,2,true)",
        "val a=\"A\";val b=\"B\";run_a(a,b,1,false);run_b(a,b,2,true)",
        "val a=\"A\";val b=\"B\";run_a(a,\"temp\",1,false);run_b(b,b,2,true)",
        "val a=\"A\";val b=\"B\";val c=\"C\";run_a(a,b,1,false);run_b(c,c,2,true)",
        "val a=\"A\";run_a(a,a,1,false);val b=\"B\";run_b(b,b,2,true)",
        "val a=\"A\";val b=\"B\";run_a(a,b,1,false);run_b(a,a,2,true);run_a(a,a,3,false)",
        "val a=\"A\";val b=\"B\";val rounds=1;run_a(a,b,rounds,false);run_b(a,a,2,true)",
        "val a=\"A\";val b=\"B\";run_a(a,b,1,false);run_b(a,a,1+1,true)",
        "val a=(\"A\");val b=\"B\";run_a(a,b,1,false);run_b(a,a,2,true)",
    ] {
        check_closed(&case, Some(body), Some(0));
    }
    check_closed(&case, None, Some(1));
    check_closed(&case, None, Some(2));
}

// 0: type-valid unsupported shape; 1: unclaimed shell; 2: non-E2 callee.
fn check_closed(case: &Case, body: Option<&str>, rejected: Option<u8>) {
    let calls = case.calls;
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
        "{a}\n{b}\nfun main():Unit {{{};{};{}}}",
        case.declarations, calls[0], calls[1]
    );
    if let Some(body) = body {
        source_text = source_text.replace(
            &format!("{};{};{}", case.declarations, calls[0], calls[1]),
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
    assert!(descriptor.is_different_staggered_named_pair());
    assert!(descriptor.common_instance().is_none());
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
    let unit = prepared.expect("staggered original owners");
    let f = unit.program.modules[0].function(unit.function).unwrap();
    let strings: Vec<_> = f
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
        .collect();
    assert_eq!(strings.len(), 2);
    let mut owners = Vec::new();
    let mut init_end = 0;
    for (i, bytes) in strings.iter().zip(case.bytes) {
        let Operation::StringLiteral { bytes: actual, .. } = &i.operation else {
            unreachable!()
        };
        assert_eq!(actual, bytes);
        let Origin::Source(span) = i.origin else {
            panic!("initializer origin")
        };
        assert!(span.start() >= init_end);
        init_end = span.end();
        let [EntityId::Value(owner)] = i.results.as_slice() else {
            panic!("owner")
        };
        assert!(!owners.contains(owner));
        owners.push(*owner);
    }
    let first_begin = f
        .instructions
        .iter()
        .position(|i| matches!(i.operation, Operation::BorrowBegin { .. }))
        .expect("first original argument acquisition");
    let mut previous_root = None;
    for owner in &owners {
        let initializers: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| i.results == vec![EntityId::Value(*owner)])
            .collect();
        let [(initializer_index, _)] = initializers.as_slice() else {
            panic!("unique initializer")
        };
        let roots: Vec<_> = f.instructions.iter().enumerate().filter(|(_, i)| matches!(i.operation, Operation::RootPlace { owner: actual } if actual == *owner)).collect();
        let [(root_index, _)] = roots.as_slice() else {
            panic!("unique named RootPlace")
        };
        assert!(*initializer_index < *root_index && *root_index < first_begin);
        if let Some(previous) = previous_root {
            assert!(previous < *initializer_index);
        }
        previous_root = Some(*root_index);
    }
    let calls: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::DirectCall { .. }))
        .collect();
    assert_eq!(calls.len(), 2);
    let mut all_loans = Vec::new();
    let mut callees = Vec::new();
    let mut previous_drop = None;
    for (call_number, (call_index, call)) in calls.into_iter().enumerate() {
        let Origin::Source(span) = call.origin else {
            panic!("call origin")
        };
        assert_eq!(sources.slice(span).unwrap(), case.calls[call_number]);
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
            panic!("original declaration")
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
        let mut begin_spans = Vec::new();
        for (role, argument) in arguments.iter().enumerate() {
            let EntityId::Loan(loan) = argument else {
                panic!("actual loan")
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
            if let Some(drop) = previous_drop {
                assert!(drop < begin_index);
            }
            let Origin::Source(argument_span) = begin.origin else {
                panic!("argument origin")
            };
            let contracts: Vec<_> = expected
                .contracts()
                .iter()
                .filter(|c| c.parameter_index() == role)
                .collect();
            let [contract] = contracts.as_slice() else {
                panic!("exact original ABI column")
            };
            assert_eq!(
                argument_span,
                parsed
                    .ast()
                    .expressions()
                    .get(contract.argument().expression())
                    .unwrap()
                    .span()
            );
            begin_spans.push((begin_index, argument_span));
            let Operation::BorrowBegin { place, .. } = begin.operation else {
                panic!("begin")
            };
            if role < 2 {
                let root = f
                    .instructions
                    .iter()
                    .find(|i| i.results == vec![EntityId::Place(place)])
                    .unwrap();
                let Operation::RootPlace { owner } = root.operation else {
                    panic!("root")
                };
                assert_eq!(owner, owners[case.roots[call_number][role]]);
            }
            let ends:Vec<_>=f.instructions.iter().enumerate().filter(|(_,i)|matches!(i.operation,Operation::BorrowEnd{loan:actual} if actual==*loan)).collect();
            assert_eq!(ends.len(), 1);
            assert!(ends[0].0 > call_index);
            last_end = last_end.max(ends[0].0);
        }
        begin_spans.sort_by_key(|(index, _)| *index);
        assert!(
            begin_spans
                .windows(2)
                .all(|p| p[0].1.end() <= p[1].1.start())
        );
        let owner = owners[case.dropped[call_number]];
        let drops: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation,Operation::Drop{owner:actual} if actual==owner))
            .collect();
        assert_eq!(drops.len(), 1);
        assert!(drops[0].0 > last_end);
        let point = UnitDropPoint::CallReturn(expected.contracts()[0].call());
        let originals: Vec<_> = descriptor
            .staggered_owners()
            .unwrap()
            .iter()
            .filter(|d| d.drop_fact().point() == point)
            .collect();
        let [original] = originals.as_slice() else {
            panic!("one original point drop")
        };
        assert_eq!(
            drops[0].1.origin,
            Origin::Source(original.drop_fact().value_origin())
        );
        previous_drop = Some(drops[0].0);
    }
    assert_eq!(all_loans.len(), 8);
    assert_eq!(
        f.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::Drop { .. }))
            .count(),
        2
    );
    crate::ssa::verify::verify_program(&unit.program).unwrap();
    crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
}
