//! Same-key behavior and future multi-callee evidence, without widening admission.
use super::*;
use crate::ssa::model::Operation;
use lang_frontend::ownership_checking::{
    LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
    two_source_experiment::analyze_two_source_experiment,
};
const FIRST: &str = "run_a(\"a\",\"b\",1,false)";
const SECOND: &str = "run_b(\"c\",\"d\",2,true)";

fn fixture(calls: &str, second_shell: bool) -> String {
    let base = source(false, "").replace("run", "run_a");
    let other = if second_shell {
        base.lines()
            .find(|line| line.starts_with("fun run_a("))
            .unwrap()
            .replace("fun run_a(", "fun run_b(")
    } else {
        String::new()
    };
    base.replace(
        "val rounds=3;val choose=false;run_a(\"left\",\"right\",rounds,choose)",
        calls,
    ) + "\n"
        + &other
}

#[test]
fn occurrence_bindings_keep_single_callee_and_repeated_occurrences() {
    for (text, count) in [
        (fixture("run_a(\"a\",\"b\",1,false)", false), 1),
        (
            fixture("run_a(\"a\",\"b\",1,false)", false).replace("run_a", "run_b"),
            1,
        ),
        (
            fixture(
                "run_a(\"a\",\"b\",1,false);run_a(choose=true,right=\"d\",rounds=2,left=\"c\")",
                false,
            ),
            2,
        ),
    ] {
        check(&text, count, false);
    }
}

#[test]
fn occurrence_bindings_reject_named_owner_across_distinct_callees() {
    check(
        &fixture(
            "val owner=\"s\";run_a(owner,owner,1,false);run_b(owner,owner,2,true)",
            true,
        ),
        2,
        true,
    );
}

fn check(text: &str, count: usize, unsupported: bool) {
    let mut sources = SourceMap::new();
    let id = sources.add_source("bindings.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "bindings.ko", id, &parsed)];
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
    let experiment = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
        .unwrap()
        .unwrap();
    assert_eq!(experiment.facts().len(), if unsupported { 2 } else { 1 });
    let mut declarations = Vec::new();
    for fact in experiment.facts() {
        let callee = experiment.callable(fact).unwrap();
        assert!(!declarations.contains(&callee));
        declarations.push(callee);
        assert!(experiment.function_shell(fact, &typed).is_some());
    }
    let prepared = prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry);
    if unsupported {
        assert!(matches!(
            prepared,
            Err(TwoSourceUnitError::UnsupportedSelection)
        ));
        return;
    }
    let d = experiment
        .entries(&typed)
        .unwrap()
        .iter()
        .find(|d| d.entry() == entry)
        .unwrap();
    let keys = crate::ssa::e2_occurrences::EntryKeys::checked(d, &typed).unwrap();
    assert_eq!(keys.as_slice().len(), count);
    assert!(
        keys.as_slice()
            .iter()
            .all(|key| Some(key) == d.common_instance())
    );
    let unit = prepared.unwrap();
    let f = unit.program.modules[0].function(unit.function).unwrap();
    let calls: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(i, instruction)| {
            if let Operation::DirectCall { callee, .. } = instruction.operation {
                Some((i, callee))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(calls.len(), count);
    assert!(calls.iter().all(|(_, callee)| *callee == calls[0].1));
    assert_eq!(
        f.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
            .count(),
        4 * count
    );
    assert_eq!(
        f.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::Drop { .. }))
            .count(),
        2 * count
    );
    if count == 2 {
        let first_drops: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(i, n)| {
                *i > calls[0].0 && *i < calls[1].0 && matches!(n.operation, Operation::Drop { .. })
            })
            .collect();
        assert_eq!(first_drops.len(), 2);
        let next_begin = f
            .instructions
            .iter()
            .enumerate()
            .find(|(i, n)| *i > calls[0].0 && matches!(n.operation, Operation::BorrowBegin { .. }))
            .unwrap()
            .0;
        assert!(first_drops[1].0 < next_begin);
    }
    crate::ssa::verify::verify_program(&unit.program).unwrap();
}

#[test]
fn occurrence_bindings_distinct_callers_keep_original_temporary_cleanup() {
    let mut sources = SourceMap::new();
    let id = sources.add_source("probe.ko", format!("fun run_a(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun run_b(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun main():Unit {{{FIRST};{SECOND}}}")).unwrap();
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
        let string_targets: Vec<_> = owned
            .call_argument_contracts_for(call)
            .filter(|contract| contract.parameter_index() < 2)
            .map(|contract| UnitDropTarget::Temporary(contract.argument()))
            .collect();
        assert_eq!(
            drops.iter().map(|drop| drop.target()).collect::<Vec<_>>(),
            [string_targets[1], string_targets[0]]
        );
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
fn occurrence_bindings_preserve_pure_multi_entry_mapping() {
    let text = fixture("run_a(\"a\",\"b\",1,false)", false).replace("fun main()", "fun main_a()")
        + "\nfun main_b():Unit {run_a(choose=true,right=\"d\",rounds=2,left=\"c\")}\n";
    let mut sources = SourceMap::new();
    let id = sources.add_source("multi-bindings.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new(
        "root",
        "multi-bindings.ko",
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
    let named = |name| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == name)
            .unwrap()
            .id()
    };
    let a = named("main_a");
    let b = named("main_b");
    let units =
        prepare_two_source_units(&sources, &inputs, &names, &types, &typed, &[b, a]).unwrap();
    assert_eq!(
        units
            .entries
            .iter()
            .map(|(entry, _)| *entry)
            .collect::<Vec<_>>(),
        [b, a]
    );
    assert_ne!(units.entries[0].1, units.entries[1].1);
    let mut callees = Vec::new();
    for (entry, expected) in [
        (a, [b"a".as_slice(), b"b".as_slice()]),
        (b, [b"d".as_slice(), b"c".as_slice()]),
    ] {
        let (_, program, function) = units.parts(entry).unwrap();
        let f = program.modules[0].function(function).unwrap();
        let calls: Vec<_> = f
            .instructions
            .iter()
            .filter_map(|i| {
                if let Operation::DirectCall { callee, .. } = i.operation {
                    Some(callee)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(calls.len(), 1);
        callees.push(calls[0]);
        let strings: Vec<_> = f
            .instructions
            .iter()
            .filter_map(|i| {
                if let Operation::StringLiteral { bytes, .. } = &i.operation {
                    Some(bytes.as_slice())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(strings, expected);
        assert_eq!(
            f.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
                .count(),
            4
        );
        assert_eq!(
            f.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::Drop { .. }))
                .count(),
            2
        );
        crate::llvm::render_verified_program_with_entry(program, function).unwrap();
    }
    assert_eq!(callees[0], callees[1]);
    crate::ssa::verify::verify_program(&units.program).unwrap();
}
