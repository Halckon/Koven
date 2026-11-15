//! Original expression-bodied Unit entry: real ownership facts, no synthetic wrapper.
use super::*;
use crate::ssa::model::{Operation, Origin, TerminatorKind};
use lang_frontend::ownership_checking::{
    LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
};

#[test]
fn expression_entry_original_call_finishes_all_loans_and_string_owners() {
    let mut sources = SourceMap::new();
    let id = sources.add_source("probe.ko", "fun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {}\nfun main():Unit=run(\"a\",\"b\",3,false)").unwrap();
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
    assert_eq!(owned.loans().len(), 4);
    assert_eq!(owned.direct_call_loan_ends().len(), 4);
    assert_eq!(owned.drops().len(), 2);
    let call = owned.loans()[0].call();
    for loan in owned.loans() {
        assert_eq!(loan.call(), call);
        assert_eq!(loan.kind(), LoanKind::Shared);
        assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
        let matching: Vec<_> = owned
            .direct_call_loan_ends()
            .iter()
            .filter(|e| e.loan() == loan)
            .collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].point(), UnitDropPoint::CallReturn(call));
        assert_eq!(matching[0].preceding_drops(), 0);
    }
    for loan in &owned.loans()[..2] {
        assert_eq!(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Temporary(loan.argument()))
                .count(),
            1
        );
    }
    assert!(
        owned
            .drops()
            .iter()
            .all(|d| d.point() == UnitDropPoint::CallReturn(call))
    );
}

fn check(call: &str, accepted: bool) {
    let mut sources = SourceMap::new();
    let original = source(false, "");
    let text = original.replace(
        "fun main():Unit {val rounds=3;val choose=false;run(\"left\",\"right\",rounds,choose)}",
        &format!("fun main():Unit={call}"),
    );
    assert_ne!(text, original);
    let id = sources.add_source("expression.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "expression.ko", id, &parsed)];
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
            "{call}"
        );
        return;
    }
    let prepared = prepared.unwrap_or_else(|e| panic!("{call}: {e:?}"));
    let f = prepared.program.modules[0]
        .function(prepared.function)
        .unwrap();
    let Origin::Source(declaration) = f.origin else {
        panic!("original declaration")
    };
    assert!(sources.slice(declaration).unwrap().contains("main"));
    assert_eq!(f.blocks.len(), 1);
    assert_eq!(f.blocks[0].origin, f.origin);
    let terminator = f.blocks[0].terminator.as_ref().unwrap();
    assert_eq!(terminator.kind, TerminatorKind::Return { values: vec![] });
    let Origin::Source(body) = terminator.origin else {
        panic!("expression body")
    };
    assert_eq!(sources.slice(body).unwrap(), call);
    assert_eq!(
        f.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
            .count(),
        4
    );
    let last_end = f
        .instructions
        .iter()
        .rposition(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
        .unwrap();
    let drops: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::Drop { .. }))
        .collect();
    assert_eq!(drops.len(), 2);
    assert!(drops.iter().all(|(i, _)| *i > last_end));
    crate::ssa::verify::verify_program(&prepared.program).unwrap();
    crate::llvm::render_verified_program_with_entry(&prepared.program, prepared.function).unwrap();
}

#[test]
fn expression_entry_lowers_only_the_original_direct_call() {
    check("run(\"a\",\"b\",3,false)", true);
    check("run(choose=true,right=\"b\",rounds=0x2a,left=\"a\")", true);
    check("(run(\"a\",\"b\",3,false))", false);
    check(
        "if(true){run(\"a\",\"b\",3,false)}else{run(\"a\",\"b\",3,false)}",
        false,
    );
}
