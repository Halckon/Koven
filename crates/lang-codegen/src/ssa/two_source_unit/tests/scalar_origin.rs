//! Read-only evidence probe: loan descriptors are not proof of a direct-call End event.
use super::*;
use lang_frontend::ownership_checking::{
    LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
};

#[test]
fn scalar_literal_loans_supply_original_observed_direct_call_ends() {
    for (rounds, choose) in [
        ("3", "false"),
        ("rounds", "choose"),
        ("3", "choose"),
        ("rounds", "false"),
    ] {
        // The ordinary callee deliberately has no E2 loop: isolate original ownership facts.
        let text = format!(
            "fun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\n\
             fun main():Unit {{val rounds=3;val choose=false;run(\"LEFT\",\"RIGHT\",{rounds},{choose})}}"
        );
        let mut sources = SourceMap::new();
        let id = sources.add_source("scalar-origin.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new(
            "root",
            "scalar-origin.ko",
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
        let validated = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        let ownership = validated.ownership();
        let loans = ownership.loans();
        assert_eq!(loans.len(), 4, "{rounds}/{choose}");
        let call = loans[0].call();
        for (loan, argument) in loans.iter().zip(["\"LEFT\"", "\"RIGHT\"", rounds, choose]) {
            assert_eq!(loan.call(), call);
            assert_eq!(loan.kind(), LoanKind::Shared);
            let node = parsed
                .ast()
                .expressions()
                .get(loan.argument().expression())
                .unwrap();
            assert_eq!(sources.slice(node.span()).unwrap(), argument);
            let named = matches!(argument, "rounds" | "choose");
            if named {
                assert!(matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root()));
            } else {
                assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
            }
            let ends: Vec<_> = ownership
                .direct_call_loan_ends()
                .iter()
                .filter(|end| end.loan() == loan)
                .collect();
            assert_eq!(ends.len(), 1, "{argument}");
            for end in ends {
                assert_eq!(end.point(), UnitDropPoint::CallReturn(call));
            }
        }
        assert_eq!(ownership.direct_call_loan_ends().len(), 4);
        // Only the two MoveOnly String temporaries carry owner destruction obligations.
        assert_eq!(ownership.drops().len(), 2);
        for loan in &loans[..2] {
            let drops: Vec<_> = ownership
                .drops()
                .iter()
                .filter(|drop| drop.target() == UnitDropTarget::Temporary(loan.argument()))
                .collect();
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].point(), UnitDropPoint::CallReturn(call));
        }
    }
}
