//! A shared named owner interleaved with two call-local temporary String lifetimes.
use super::*;
use crate::ssa::model::{EntityId, Operation, Origin};
use lang_frontend::ownership_checking::{
    LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
};
const FIRST: &str = "run(owner,\"first\",1,false)";
const SECOND: &str = "run(\"second\",owner,2,true)";

#[test]
fn shared_mixed_pair_original_facts_order_temporary_before_final_named_drop() {
    let mut sources = SourceMap::new();
    let id = sources.add_source("mixed-probe.ko", format!("fun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun main():Unit {{val owner=\"shared\";{FIRST};{SECOND}}}")).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "mixed-probe.ko", id, &parsed)];
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
    let named_drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(|d| matches!(d.target(), UnitDropTarget::Named(_)))
        .collect();
    let [named_drop] = named_drops.as_slice() else {
        panic!("unique original named Drop")
    };
    let UnitDropTarget::Named(owner) = named_drop.target() else {
        unreachable!()
    };
    let mut previous_end = 0;
    for (call_index, text) in [FIRST, SECOND].into_iter().enumerate() {
        let matches: Vec<_> = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, n)| sources.slice(n.span()) == Ok(text))
            .collect();
        let [(expression, node)] = matches.as_slice() else {
            panic!("unique original call")
        };
        assert!(node.span().start() >= previous_end);
        previous_end = node.span().end();
        let loans: Vec<_> = owned
            .loans()
            .iter()
            .filter(|l| l.call().expression() == *expression)
            .collect();
        assert_eq!(loans.len(), 4);
        let call = loans[0].call();
        let mut temporary_string = None;
        let mut shared_count = 0;
        for loan in &loans {
            assert_eq!(loan.kind(), LoanKind::Shared);
            let node = parsed
                .ast()
                .expressions()
                .get(loan.argument().expression())
                .unwrap();
            let argument_text = sources.slice(node.span()).unwrap();
            if argument_text == "owner" {
                shared_count += 1;
                assert!(
                    matches!(loan.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root()==owner)
                );
            } else {
                assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
                if argument_text.starts_with('"') {
                    assert!(temporary_string.replace(loan.argument()).is_none());
                }
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
        assert_eq!(shared_count, 1);
        let temporary = temporary_string.unwrap();
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
        // Preserve actual planner order at this exact point, not a sorted or synthetic target set.
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|d| d.point() == UnitDropPoint::CallReturn(call))
            .collect();
        assert_eq!(drops.len(), call_index + 1);
        assert_eq!(drops[0].target(), UnitDropTarget::Temporary(temporary));
        assert_eq!(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Temporary(temporary))
                .count(),
            1
        );
        if call_index == 1 {
            assert_eq!(*drops[1], **named_drop);
        }
    }
}

#[test]
fn shared_mixed_pair_replays_interleaved_owner_lifetimes() {
    for (calls, bytes, shared_roles) in [
        (
            [FIRST, SECOND],
            [b"first".as_slice(), b"second".as_slice()],
            [0, 1],
        ),
        (
            [
                "run(\"first\",owner,1,false)",
                "run(rounds=7,left=owner,choose=true,right=\"second\")",
            ],
            [b"first".as_slice(), b"second".as_slice()],
            [1, 0],
        ),
        (
            [
                "run(owner,\"shared\",1,false)",
                "run(\"shared\",owner,2,true)",
            ],
            [b"shared".as_slice(), b"shared".as_slice()],
            [0, 1],
        ),
    ] {
        let body = format!("val owner=\"shared\";{};{}", calls[0], calls[1]);
        check_entry(
            &body,
            Some(Expected {
                calls,
                bytes,
                shared_roles,
            }),
            "",
        );
    }
}

#[test]
fn shared_mixed_pair_rejects_asymmetric_or_uncovered_lifetimes() {
    let original = source(false, "");
    let other = original
        .lines()
        .find(|line| line.starts_with("fun run("))
        .unwrap()
        .replace("fun run(", "fun other(");
    for (body, extra) in [
        (
            format!("val owner=\"shared\";run(owner,owner,1,false);{SECOND}"),
            "",
        ),
        (
            format!("val owner=\"shared\";{FIRST};run(owner,owner,2,true)"),
            "",
        ),
        (
            format!("val owner=\"shared\";run(\"a\",\"b\",1,false);{SECOND}"),
            "",
        ),
        (
            format!(
                "val owner=\"shared\";val another=\"shared\";{FIRST};run(\"second\",another,2,true)"
            ),
            "",
        ),
        (
            format!("val owner=\"shared\";{FIRST};consume(owner);{SECOND}"),
            "",
        ),
        (
            format!("val owner=\"shared\";val rounds=1;run(owner,\"first\",rounds,false);{SECOND}"),
            "",
        ),
        (format!("val owner=\"shared\";{FIRST};{SECOND};{FIRST}"), ""),
        (
            format!("val owner=\"shared\";{FIRST};other(\"second\",owner,1+1,true)"),
            other.as_str(),
        ),
        (
            format!("val owner=\"shared\";{FIRST};run(\"second\",owner,(2),true)"),
            "",
        ),
    ] {
        check_entry(&body, None, extra);
    }
}

struct Expected<'a> {
    calls: [&'a str; 2],
    bytes: [&'a [u8]; 2],
    shared_roles: [usize; 2],
}
fn check_entry(body: &str, expected: Option<Expected<'_>>, extra: &str) {
    let mut sources = SourceMap::new();
    let text = source(false, extra).replace(
        "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)",
        body,
    );
    let id = sources.add_source("mixed-entry.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "mixed-entry.ko", id, &parsed)];
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
    let Some(expected) = expected else {
        assert!(
            matches!(prepared, Err(TwoSourceUnitError::UnsupportedSelection)),
            "{body}"
        );
        return;
    };
    let unit = prepared.expect("original shared/mixed owner pair");
    let f = unit.program.modules[0].function(unit.function).unwrap();
    let strings: Vec<_> = f
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
        .collect();
    assert_eq!(strings.len(), 3);
    let [EntityId::Value(shared_owner)] = strings[0].results.as_slice() else {
        panic!("named owner value")
    };
    let shared_owner = *shared_owner;
    let mut string_owners = Vec::new();
    for (i, bytes) in
        strings
            .iter()
            .zip([b"shared".as_slice(), expected.bytes[0], expected.bytes[1]])
    {
        let Operation::StringLiteral { bytes: actual, .. } = &i.operation else {
            unreachable!()
        };
        assert_eq!(actual, bytes);
        let [EntityId::Value(owner)] = i.results.as_slice() else {
            panic!("String owner value")
        };
        assert!(!string_owners.contains(owner));
        string_owners.push(*owner);
    }
    let calls: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::DirectCall { .. }))
        .collect();
    assert_eq!(calls.len(), 2);
    let mut all_loans = Vec::new();
    let mut previous_temp_drop = None;
    let mut final_temp_drop = None;
    for (call_number, ((call_index, call), text)) in
        calls.into_iter().zip(expected.calls).enumerate()
    {
        let Origin::Source(span) = call.origin else {
            panic!("call origin")
        };
        assert_eq!(sources.slice(span).unwrap(), text);
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        assert_eq!(arguments.len(), 4);
        let mut last_end = call_index;
        let mut temporary_owner = None;
        let mut source_begins = Vec::new();
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
            let Origin::Source(argument_span) = begin.origin else {
                panic!("argument source")
            };
            assert!(argument_span.start() >= span.start() && argument_span.end() <= span.end());
            source_begins.push((begin_index, argument_span));
            if let Some(drop) = previous_temp_drop {
                assert!(drop < begin_index);
            }
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
                if role == expected.shared_roles[call_number] {
                    assert_eq!(owner, shared_owner);
                } else {
                    assert_ne!(owner, shared_owner);
                    temporary_owner = Some(owner);
                }
            }
            let ends:Vec<_>=f.instructions.iter().enumerate().filter(|(_,i)|matches!(i.operation,Operation::BorrowEnd{loan:actual} if actual==*loan)).collect();
            assert_eq!(ends.len(), 1);
            assert!(ends[0].0 > call_index);
            last_end = last_end.max(ends[0].0);
        }
        source_begins.sort_by_key(|(index, _)| *index);
        assert!(
            source_begins
                .windows(2)
                .all(|pair| pair[0].1.end() <= pair[1].1.start())
        );
        let temporary_owner = temporary_owner.unwrap();
        let drops: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation,Operation::Drop{owner} if owner==temporary_owner))
            .collect();
        assert_eq!(drops.len(), 1);
        assert!(drops[0].0 > last_end);
        previous_temp_drop = Some(drops[0].0);
        final_temp_drop = Some(drops[0].0);
    }
    assert_eq!(all_loans.len(), 8);
    let drops: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::Drop { .. }))
        .collect();
    assert_eq!(drops.len(), 3);
    let named: Vec<_> = drops
        .iter()
        .filter(|(_, i)| matches!(i.operation,Operation::Drop{owner} if owner==shared_owner))
        .collect();
    assert_eq!(named.len(), 1);
    assert!(named[0].0 > final_temp_drop.unwrap());
    crate::ssa::verify::verify_program(&unit.program).unwrap();
    crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
}
