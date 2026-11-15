//! Original last-use evidence for two distinct named owners ending at different calls.
use super::*;
use crate::ssa::model::{EntityId, Operation, Origin};
use lang_frontend::ownership_checking::{
    LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
};

struct Case {
    declarations: &'static str,
    calls: [&'static str; 2],
    bytes: [&'static [u8]; 2],
    // ABI String roles expressed as original declaration-order root indices.
    roots: [[usize; 2]; 2],
    dropped: [usize; 2],
}
fn cases() -> [Case; 3] {
    [
        Case {
            declarations: "val a=\"A\";val b=\"B\"",
            calls: ["run(a,b,1,false)", "run(a,a,2,true)"],
            bytes: [b"A", b"B"],
            roots: [[0, 1], [0, 0]],
            dropped: [1, 0],
        },
        Case {
            declarations: "val a=\"same\";val b=\"same\"",
            calls: ["run(b,a,1,false)", "run(b,b,2,true)"],
            bytes: [b"same", b"same"],
            roots: [[1, 0], [1, 1]],
            dropped: [0, 1],
        },
        Case {
            declarations: "val b=\"B\";val a=\"A\"",
            calls: [
                "run(choose=false,right=b,rounds=1,left=a)",
                "run(right=a,rounds=2,left=a,choose=true)",
            ],
            bytes: [b"B", b"A"],
            roots: [[1, 0], [1, 1]],
            dropped: [0, 1],
        },
    ]
}

#[test]
fn staggered_owners_original_facts_observe_each_roots_last_call() {
    for case in cases() {
        let mut sources = SourceMap::new();
        let text = format!(
            "fun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun main():Unit {{{};{};{}}}",
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
        let mut source_end = 0;
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
            let contracts: Vec<_> = owned.call_argument_contracts_for(call).collect();
            assert_eq!(contracts.len(), 4);
            for loan in &loans {
                assert_eq!(loan.kind(), LoanKind::Shared);
                let contract = contracts
                    .iter()
                    .find(|c| c.argument() == loan.argument())
                    .unwrap();
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
    }
}

#[test]
fn staggered_owners_emit_distinct_initializations_and_original_last_use_drops() {
    for case in cases() {
        check_entry(&case, None, "");
    }
}

#[test]
fn staggered_owners_reject_other_root_and_call_shapes() {
    let [case, _, _] = cases();
    let original = source(false, "");
    let other = original
        .lines()
        .find(|line| line.starts_with("fun run("))
        .unwrap()
        .replace("fun run(", "fun other(");
    for (body, extra) in [
        (
            "val a=\"A\";val b=\"B\";run(a,a,1,false);run(b,b,2,true)",
            "",
        ),
        (
            "val a=\"A\";val b=\"B\";run(a,b,1,false);run(a,b,2,true)",
            "",
        ),
        (
            "val a=\"A\";val b=\"B\";run(a,\"temp\",1,false);run(a,a,2,true)",
            "",
        ),
        (
            "val a=\"A\";val b=\"B\";val c=\"C\";run(a,b,1,false);run(a,a,2,true)",
            "",
        ),
        (
            "val a=\"A\";val b=\"B\";run(a,b,1,false);consume(a);run(a,a,2,true)",
            "",
        ),
        (
            "val a=\"A\";val b=\"B\";val rounds=1;run(a,b,rounds,false);run(a,a,2,true)",
            "",
        ),
        (
            "val a=\"A\";val b=\"B\";run(a,b,1,false);other(a,a,1+1,true)",
            other.as_str(),
        ),
        (
            "val a=\"A\";val b=\"B\";run(a,b,1,false);run(a,a,(2),true)",
            "",
        ),
    ] {
        check_entry(&case, Some(body), extra);
    }
}

fn check_entry(case: &Case, rejected_body: Option<&str>, extra: &str) {
    let mut sources = SourceMap::new();
    let body = format!("{};{};{}", case.declarations, case.calls[0], case.calls[1]);
    let body = rejected_body.unwrap_or(&body);
    let text = source(false, extra).replace(
        "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)",
        body,
    );
    let id = sources.add_source("stagger-entry.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new(
        "root",
        "stagger-entry.ko",
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
    if rejected_body.is_some() {
        assert!(
            matches!(prepared, Err(TwoSourceUnitError::UnsupportedSelection)),
            "{body}"
        );
        return;
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
    let mut previous_drop = None;
    for (call_number, (call_index, call)) in calls.into_iter().enumerate() {
        let Origin::Source(span) = call.origin else {
            panic!("call origin")
        };
        assert_eq!(sources.slice(span).unwrap(), case.calls[call_number]);
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
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
