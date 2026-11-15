//! Two sequential original calls: independent temporary lifetimes, not descriptor concatenation.
use super::*;
use crate::ssa::model::{EntityId, Operation, Origin};
use lang_frontend::ownership_checking::{
    LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
};

const FIRST: &str = "run(\"a\",\"b\",1,false)";
const SECOND: &str = "run(\"c\",\"d\",2,true)";

#[test]
fn two_call_entry_original_facts_keep_call_local_cleanup() {
    let mut sources = SourceMap::new();
    let id = sources.add_source("probe.ko", format!("fun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun main():Unit {{{FIRST};{SECOND}}}")).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "probe.ko", id, &parsed)];
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
    let validated = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
        .unwrap()
        .validate()
        .unwrap();
    let owned = validated.ownership();
    assert_eq!(owned.loans().len(), 8);
    assert_eq!(owned.direct_call_loan_ends().len(), 8);
    assert_eq!(owned.drops().len(), 4);
    // Select each original call by AST text, never infer cross-call execution from loan-array order.
    let mut source_end = 0;
    for text in [FIRST, SECOND] {
        let expressions: Vec<_> = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()) == Ok(text))
            .collect();
        let [(expression, node)] = expressions.as_slice() else {
            panic!("unique original call")
        };
        assert!(node.span().start() >= source_end);
        source_end = node.span().end();
        let loans: Vec<_> = owned
            .loans()
            .iter()
            .filter(|loan| loan.call().expression() == *expression)
            .collect();
        assert_eq!(loans.len(), 4);
        let call = loans[0].call();
        for loan in &loans {
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
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
        assert_eq!(drops.len(), 2);
        for loan in loans {
            let node = parsed
                .ast()
                .expressions()
                .get(loan.argument().expression())
                .unwrap();
            let is_string = sources.slice(node.span()).unwrap().starts_with('"');
            assert_eq!(
                drops
                    .iter()
                    .filter(|d| d.target() == UnitDropTarget::Temporary(loan.argument()))
                    .count(),
                usize::from(is_string)
            );
        }
    }
}

#[test]
fn two_call_entry_finishes_first_lifetime_before_second_acquisition() {
    check_entry(&format!("{FIRST};{SECOND}"), true);
}

#[test]
fn two_call_entry_preserves_second_named_argument_order_and_values() {
    check_entry(
        &format!("{FIRST};run(choose=true,right=\"second-right\",rounds=7,left=\"second-left\")"),
        true,
    );
}

#[test]
fn two_call_entry_rejects_different_supported_callees() {
    let fixture = source(false, "");
    let other = fixture
        .lines()
        .find(|line| line.starts_with("fun run("))
        .unwrap()
        .replace("fun run(", "fun other(");
    // Both definitions are legal E2 shells: each separately supports an exact pair.
    let other_first = FIRST.replace("run(", "other(");
    let other_second = SECOND.replace("run(", "other(");
    check_entry_source(
        &format!("{other_first};{other_second}"),
        true,
        fixture.replace("fun run(", "fun other("),
    );
    check_entry_with_extra(&format!("{FIRST};{other_second}"), false, &other);
}

#[test]
fn two_call_entry_rejects_uncovered_statements_and_shared_named_lifetimes() {
    for body in [
        format!("{FIRST};{SECOND};{FIRST}"),
        format!("{FIRST};val rounds=2;run(\"c\",\"d\",rounds,true)"),
        format!("val owner=\"a\";run(owner,owner,1,false);{SECOND}"),
        format!("{FIRST};run(\"c\",\"d\",1+1,true)"),
        format!("{FIRST};({SECOND})"),
    ] {
        check_entry(&body, false);
    }
}

fn check_entry(body: &str, accepted: bool) {
    check_entry_with_extra(body, accepted, "");
}

fn check_entry_with_extra(body: &str, accepted: bool, extra: &str) {
    check_entry_source(body, accepted, source(false, extra));
}

fn check_entry_source(body: &str, accepted: bool, original: String) {
    let mut sources = SourceMap::new();
    let text = original.replace(
        "val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)",
        body,
    );
    assert_ne!(text, original);
    let id = sources.add_source("sequence.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "sequence.ko", id, &parsed)];
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
    let unit = prepared.unwrap_or_else(|e| panic!("two original calls: {e:?}"));
    let f = unit.program.modules[0].function(unit.function).unwrap();
    let calls: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, i)| {
            if let Operation::DirectCall {
                callee, arguments, ..
            } = &i.operation
            {
                Some((index, i, callee, arguments))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].2, calls[1].2);
    let mut previous_cleanup = None;
    let mut all_loans = Vec::new();
    let mut all_owners = Vec::new();
    for ((call_index, instruction, _, arguments), source_call) in calls.iter().zip(body.split(';'))
    {
        let Origin::Source(call_span) = instruction.origin else {
            panic!("call source")
        };
        assert_eq!(sources.slice(call_span).unwrap(), source_call);
        let mut last_end = *call_index;
        let mut earliest_begin = *call_index;
        let mut owners = Vec::new();
        for argument in *arguments {
            let EntityId::Loan(loan) = argument else {
                panic!("loan ABI")
            };
            assert!(!all_loans.contains(loan));
            all_loans.push(*loan);
            let (begin_index, begin) = f
                .instructions
                .iter()
                .enumerate()
                .find(|(_, i)| i.results == vec![*argument])
                .unwrap();
            earliest_begin = earliest_begin.min(begin_index);
            let Operation::BorrowBegin { place, .. } = begin.operation else {
                panic!("begin")
            };
            assert!(begin_index < *call_index);
            let root = f
                .instructions
                .iter()
                .find(|i| i.results == vec![EntityId::Place(place)])
                .unwrap();
            let Operation::RootPlace { owner } = root.operation else {
                panic!("root")
            };
            let value = f
                .instructions
                .iter()
                .find(|i| i.results == vec![EntityId::Value(owner)])
                .unwrap();
            if let Operation::StringLiteral { bytes, .. } = &value.operation {
                let Origin::Source(span) = value.origin else {
                    panic!("literal source")
                };
                let literal = sources.slice(span).unwrap();
                assert_eq!(bytes.as_slice(), literal.trim_matches('"').as_bytes());
                assert!(span.start() >= call_span.start() && span.end() <= call_span.end());
                owners.push(owner);
            }
            let ends: Vec<_> = f.instructions.iter().enumerate().filter(|(_, i)| matches!(i.operation, Operation::BorrowEnd {loan: actual} if actual == *loan)).collect();
            assert_eq!(ends.len(), 1);
            assert!(ends[0].0 > *call_index);
            last_end = last_end.max(ends[0].0);
        }
        if let Some(previous) = previous_cleanup {
            assert!(previous < earliest_begin);
        }
        assert_eq!(owners.len(), 2);
        let mut last_drop = last_end;
        for owner in owners {
            assert!(!all_owners.contains(&owner));
            all_owners.push(owner);
            let drops: Vec<_> = f.instructions.iter().enumerate().filter(|(_, i)| matches!(i.operation, Operation::Drop {owner: actual} if actual == owner)).collect();
            assert_eq!(drops.len(), 1);
            assert!(drops[0].0 > last_end);
            last_drop = last_drop.max(drops[0].0);
        }
        previous_cleanup = Some(last_drop);
    }
    assert_eq!(
        f.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
            .count(),
        8
    );
    assert_eq!(
        f.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::Drop { .. }))
            .count(),
        4
    );
    crate::ssa::verify::verify_program(&unit.program).unwrap();
    crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
}
