//! Bare Copyable Int/Boolean tokens retire through the existing pending-loan traversal.
use super::*;

fn operand<'a>(
    observed: &'a Observed,
    loan: &lang_frontend::ownership_checking::UnitLoanFact,
) -> &'a str {
    let node = observed
        .parsed
        .ast()
        .expressions()
        .get(loan.argument().expression())
        .unwrap();
    observed.sources.slice(node.span()).unwrap()
}

#[test]
fn scalar_literals_end_once_per_original_token_before_string_drops() {
    let observed = observe(
        "fun read(text:String,n:Int,flag:Boolean):Unit {}\nfun run():Unit {read(\"one\",3,false);read(\"two\",4,true)}",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        assert_eq!(unit.loans().len(), 6);
        assert_eq!(unit.direct_call_loan_ends().len(), 6);
        assert_eq!(unit.drops().len(), 2);
        assert_ne!(unit.loans()[0].call(), unit.loans()[3].call());
        for loan in unit.loans() {
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
            let ends: Vec<_> = unit
                .direct_call_loan_ends()
                .iter()
                .filter(|e| e.loan() == loan)
                .collect();
            assert_eq!(ends.len(), 1);
            assert_eq!(ends[0].point(), UnitDropPoint::CallReturn(loan.call()));
            assert_eq!(ends[0].preceding_drops(), 0);
        }
        let ends = unit.direct_call_loan_ends();
        assert_eq!(
            ends.iter()
                .map(|e| operand(&observed, e.loan()))
                .collect::<Vec<_>>(),
            ["false", "3", "\"one\"", "true", "4", "\"two\""]
        );
        for chunk in ends.chunks_exact(3) {
            assert!(
                chunk
                    .windows(2)
                    .all(|p| p[0].preceding_call_loan_ends() < p[1].preceding_call_loan_ends())
            );
        }
    }
}

#[test]
fn scalar_literal_prefix_unwinds_only_acquired_tokens() {
    for jump in ["return", "break", "continue"] {
        let call = format!("read(3,if(flag){{{jump}}}else{{0}},false)");
        let body = if jump == "return" {
            call
        } else {
            format!("loop{{{call};break}}")
        };
        let observed = observe(&format!(
            "fun read(n:Int,own gate:Int,last:Boolean):Unit {{}}\nfun run(flag:Boolean):Unit {{{body}}}"
        ));
        assert_units_admitted(&observed);
        for unit in &observed.units {
            let ends = unit.direct_call_loan_ends();
            assert_eq!(ends.len(), 3, "{jump}");
            let transfers: Vec<_> = ends
                .iter()
                .filter(|e| matches!(e.point(), UnitDropPoint::ControlTransfer(_)))
                .collect();
            assert_eq!(transfers.len(), 1);
            assert_eq!(operand(&observed, transfers[0].loan()), "3");
            let normal: Vec<_> = ends
                .iter()
                .filter(|e| matches!(e.point(), UnitDropPoint::CallReturn(_)))
                .map(|e| operand(&observed, e.loan()))
                .collect();
            assert_eq!(normal, ["false", "3"]);
            assert!(unit.drops().is_empty());
        }
    }
}

#[test]
fn scalar_observation_does_not_admit_group_long_call_or_computation() {
    let observed = observe(
        "fun value():Int=7\nfun read(a:Int,b:Long,c:Int,d:Int,e:Boolean):Unit {}\nfun run():Unit {read((3),4L,value(),1+2,(true))}",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        assert!(!unit.loans().is_empty());
        assert!(unit.direct_call_loan_ends().is_empty());
    }
}

#[test]
fn scalar_abort_and_later_ownership_error_publish_no_cleanup() {
    let observed = observe(
        "fun stop():Nothing=stop()\nfun read(n:Int,flag:Boolean,own gate:Int):Unit {}\nfun run():Unit {read(3,false,stop());read(4,true,0)}",
    );
    assert_units_admitted(&observed);
    for unit in &observed.units {
        assert!(unit.direct_call_loan_ends().is_empty());
        assert!(unit.drops().is_empty());
    }
    let observed = observe(
        "fun read(n:Int,flag:Boolean):Unit {}\nfun take(own text:String):Unit {}\nfun run():Unit {read(3,false);val text=\"held\";take(text);take(text)}",
    );
    for unit in &observed.units {
        assert!(!unit.diagnostics().is_empty());
        assert!(unit.direct_call_loan_ends().is_empty());
        assert!(unit.drops().is_empty());
    }
}

#[test]
fn scalar_literals_do_not_observe_nullable_parameter_wrappers() {
    let observed =
        observe("fun read(n:Int?,flag:Boolean?):Unit {}\nfun run():Unit {read(3,false)}");
    assert_units_admitted(&observed);
    for (unit, typed) in observed.units.iter().zip(&observed.unit_types) {
        assert_eq!(unit.loans().len(), 2);
        for loan in unit.loans() {
            let contracts: Vec<_> = unit
                .call_argument_contracts_for(loan.call())
                .filter(|contract| contract.argument() == loan.argument())
                .collect();
            assert_eq!(contracts.len(), 1);
            assert_ne!(
                typed.types().expression_type(loan.argument()),
                Some(contracts[0].parameter_type())
            );
            assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
        }
        assert!(unit.direct_call_loan_ends().is_empty());
        assert!(unit.drops().is_empty());
    }
}
