//! One original named owner surviving the first call, consumed after its final use.
use super::*;
use crate::ssa::model::{EntityId, Operation, Origin};
use lang_frontend::ownership_checking::{
    LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
};

const FIRST: &str = "run(owner,owner,1,false)";
const SECOND: &str = "run(owner,owner,2,true)";

#[test]
fn shared_owner_pair_original_facts_keep_owner_until_last_call() {
    let mut sources = SourceMap::new();
    let text = format!(
        "fun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun main():Unit {{val owner=\"shared\";{FIRST};{SECOND}}}"
    );
    let id = sources.add_source("shared-probe.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "shared-probe.ko", id, &parsed)];
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
    assert_eq!(owned.drops().len(), 1);
    let UnitDropTarget::Named(owner) = owned.drops()[0].target() else {
        panic!("named owner drop")
    };
    let mut previous_span_end = 0;
    for (call_index, text) in [FIRST, SECOND].into_iter().enumerate() {
        let matches: Vec<_> = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()) == Ok(text))
            .collect();
        let [(expression, node)] = matches.as_slice() else {
            panic!("unique source call")
        };
        assert!(node.span().start() >= previous_span_end);
        previous_span_end = node.span().end();
        let loans: Vec<_> = owned
            .loans()
            .iter()
            .filter(|loan| loan.call().expression() == *expression)
            .collect();
        assert_eq!(loans.len(), 4);
        let call = loans[0].call();
        let mut named_count = 0;
        for loan in &loans {
            assert_eq!(loan.kind(), LoanKind::Shared);
            let node = parsed
                .ast()
                .expressions()
                .get(loan.argument().expression())
                .unwrap();
            if sources.slice(node.span()) == Ok("owner") {
                assert!(
                    matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == owner)
                );
                named_count += 1;
            } else {
                assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
            }
            let ends: Vec<_> = owned
                .direct_call_loan_ends()
                .iter()
                .filter(|end| end.loan() == *loan)
                .collect();
            assert_eq!(ends.len(), 1);
            assert_eq!(ends[0].point(), UnitDropPoint::CallReturn(call));
            assert_eq!(ends[0].preceding_drops(), 0);
        }
        assert_eq!(named_count, 2);
        let ends: Vec<_> = owned
            .direct_call_loan_ends()
            .iter()
            .filter(|end| end.loan().call() == call)
            .collect();
        assert_eq!(
            ends.iter()
                .map(|end| end.preceding_call_loan_ends())
                .collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|drop| drop.point() == UnitDropPoint::CallReturn(call))
            .collect();
        assert_eq!(drops.len(), call_index);
        if call_index == 1 {
            assert_eq!(drops[0].target(), UnitDropTarget::Named(owner));
        }
    }
}

#[test]
fn shared_owner_pair_emits_one_owner_across_two_complete_lifetimes() {
    check_entry(
        &format!("val owner=\"shared\";{FIRST};{SECOND}"),
        [FIRST, SECOND],
        true,
        "",
    );
    let reordered = "run(choose=true,right=owner,rounds=7,left=owner)";
    check_entry(
        &format!("val owner=\"shared\";{FIRST};{reordered}"),
        [FIRST, reordered],
        true,
        "",
    );
}

#[test]
fn shared_owner_pair_rejects_other_lifetime_shapes() {
    let original = source(false, "");
    let other = original
        .lines()
        .find(|line| line.starts_with("fun run("))
        .unwrap()
        .replace("fun run(", "fun other(");
    for (body, extra) in [
        (
            format!(
                "val owner=\"shared\";val another=\"shared\";{FIRST};run(another,another,2,true)"
            ),
            "",
        ),
        (
            format!("val owner=\"shared\";{FIRST};run(\"a\",\"b\",2,true)"),
            "",
        ),
        (
            format!("val owner=\"shared\";{FIRST};consume(owner);{SECOND}"),
            "",
        ),
        (
            format!("val owner=\"shared\";val rounds=1;run(owner,owner,rounds,false);{SECOND}"),
            "",
        ),
        (format!("val owner=\"shared\";{FIRST};{SECOND};{FIRST}"), ""),
        (
            format!("val owner=\"shared\";{FIRST};other(owner,owner,2,true)"),
            other.as_str(),
        ),
        (
            format!("val owner=\"shared\";{FIRST};run(owner,owner,(2),true)"),
            "",
        ),
    ] {
        check_entry(&body, [FIRST, SECOND], false, extra);
    }
}

fn check_entry(body: &str, expected_calls: [&str; 2], accepted: bool, extra: &str) {
    let mut sources = SourceMap::new();
    let text = source(false, extra).replace(
        "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)",
        body,
    );
    let id = sources.add_source("shared-entry.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "shared-entry.ko", id, &parsed)];
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
    if !accepted {
        assert!(
            matches!(prepared, Err(TwoSourceUnitError::UnsupportedSelection)),
            "{body}"
        );
        return;
    }
    let unit = prepared.expect("original shared owner pair");
    let f = unit.program.modules[0].function(unit.function).unwrap();
    let strings: Vec<_> = f
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
        .collect();
    let [string] = strings.as_slice() else {
        panic!("one original String owner")
    };
    let Operation::StringLiteral { bytes, .. } = &string.operation else {
        unreachable!()
    };
    assert_eq!(bytes, b"shared");
    let [EntityId::Value(owner)] = string.results.as_slice() else {
        panic!("owner value")
    };
    let roots: Vec<_> = f
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::RootPlace {owner: actual} if actual == *owner))
        .collect();
    let [root] = roots.as_slice() else {
        panic!("one original named root")
    };
    let [EntityId::Place(place)] = root.results.as_slice() else {
        panic!("root place")
    };
    let calls: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::DirectCall { .. }))
        .collect();
    assert_eq!(calls.len(), 2);
    let mut prior_end = None;
    let mut all_loans = Vec::new();
    for ((index, call), text) in calls.into_iter().zip(expected_calls) {
        let Origin::Source(span) = call.origin else {
            panic!("original call origin")
        };
        assert_eq!(sources.slice(span).unwrap(), text);
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        assert_eq!(arguments.len(), 4);
        let mut last_end = index;
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
            assert!(begin_index < index);
            if let Some(end) = prior_end {
                assert!(end < begin_index);
            }
            if role < 2 {
                assert!(
                    matches!(begin.operation, Operation::BorrowBegin {place: actual, ..} if actual == *place)
                );
            }
            let ends: Vec<_> = f.instructions.iter().enumerate().filter(|(_, i)| matches!(i.operation, Operation::BorrowEnd {loan: actual} if actual == *loan)).collect();
            assert_eq!(ends.len(), 1);
            assert!(ends[0].0 > index);
            last_end = last_end.max(ends[0].0);
        }
        prior_end = Some(last_end);
    }
    assert_eq!(all_loans.len(), 8);
    let drops: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::Drop { .. }))
        .collect();
    let [(index, drop)] = drops.as_slice() else {
        panic!("one final Drop")
    };
    assert!(*index > prior_end.unwrap());
    assert!(matches!(drop.operation, Operation::Drop {owner: actual} if actual == *owner));
    crate::ssa::verify::verify_program(&unit.program).unwrap();
    crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
}
