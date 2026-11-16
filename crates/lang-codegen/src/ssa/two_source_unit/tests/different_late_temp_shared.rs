//! Original asymmetric shared-root facts across two distinct closed callable instances.
use super::*;
use crate::ssa::model::{EntityId, Operation, Origin};
use lang_frontend::ownership_checking::{
    LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
};
use lang_frontend::{
    ownership_checking::two_source_experiment::analyze_two_source_experiment,
    type_checking::UnitCallTarget,
};
fn cases() -> [[&'static str; 2]; 2] {
    [
        ["run_a(owner,owner,1,false)", "run_b(\"temp\",owner,2,true)"],
        [
            "run_b(choose=false,right=owner,rounds=1,left=owner)",
            "run_a(rounds=2,left=owner,choose=true,right=\"shared\")",
        ],
    ]
}
fn text(calls: [&str; 2], closed: bool) -> String {
    let definitions = if closed {
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
        format!("{a}\n{b}")
    } else {
        ["run_a", "run_b"]
            .map(|name| {
                format!("fun {name}(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}")
            })
            .join("\n")
    };
    format!(
        "{definitions}\nfun main():Unit {{val owner=\"shared\";{};{}}}",
        calls[0], calls[1]
    )
}

#[test]
fn different_late_temp_shared_original_facts_preserve_second_temporary() {
    for [first, second] in cases() {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("mixed-probe.ko", text([first, second], false))
            .unwrap();
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
        assert_eq!(owned.drops().len(), 2);
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
        assert_eq!(owned.formed_owned_locals(), &[owner]);
        let name_span = names.names().source_units()[owner.source_unit().index()]
            .resolution()
            .symbols()
            .get(owner.symbol().index())
            .unwrap()
            .span();
        assert_eq!(named_drop.value_origin(), name_span);
        let mut all_arguments = Vec::new();
        let mut keys = Vec::new();
        let mut previous_end = 0;
        for (call_index, text) in [first, second].into_iter().enumerate() {
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
            keys.push(typed.types().call(call).unwrap().instance().clone());
            assert!(keys.last().unwrap().type_arguments().is_empty());
            if call_index == 0 {
                let literals: Vec<_> = parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter(|(_, n)| {
                        n.span().end() < node.span().start()
                            && sources.slice(n.span()).is_ok_and(|t| t.starts_with('"'))
                    })
                    .collect();
                assert_eq!(literals.len(), 1);
                assert!(name_span.end() < literals[0].1.span().start());
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
            let mut temporary_string = None;
            let mut shared_count = 0;
            for loan in &loans {
                assert!(!all_arguments.contains(&loan.argument()));
                all_arguments.push(loan.argument());
                assert_eq!(
                    contracts
                        .iter()
                        .filter(|c| c.argument() == loan.argument())
                        .count(),
                    1
                );
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
            assert_eq!(shared_count, if call_index == 0 { 2 } else { 1 });
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
            if call_index == 0 {
                assert!(temporary_string.is_none());
                assert!(drops.is_empty());
            } else {
                let temporary = temporary_string.unwrap();
                assert_eq!(drops.len(), 2, "original point vector {drops:?}");
                assert_eq!(drops[0].target(), UnitDropTarget::Temporary(temporary));
                assert_eq!(*drops[1], **named_drop);
                assert_eq!(
                    owned
                        .drops()
                        .iter()
                        .filter(|d| d.target() == UnitDropTarget::Temporary(temporary))
                        .count(),
                    1
                );
                eprintln!("late temporary shared original terminal vector: {drops:?}");
            }
        }
        assert_ne!(keys[0], keys[1]);
        assert_eq!(all_arguments.len(), 8);
    }
}

#[test]
fn different_late_temp_shared_closed_facts_prepare_asymmetric_lifetime() {
    for calls in cases() {
        let source_text = text(calls, true);
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
        assert!(descriptor.is_different_late_temporary_shared_pair());
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
        let (symbol, _, init_span, bytes, named_drop) = descriptor.shared_owner().unwrap();
        let unit = prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry)
            .expect("late temporary shared entry");
        let f = unit.program.modules[0].function(unit.function).unwrap();
        let literals: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation, Operation::StringLiteral { .. }))
            .collect();
        assert_eq!(literals.len(), 2);
        assert_eq!(literals[0].1.origin, Origin::Source(init_span));
        let Operation::StringLiteral { bytes: actual, .. } = &literals[0].1.operation else {
            unreachable!()
        };
        assert_eq!(actual, bytes);
        let mut owners = Vec::new();
        let mut root_positions = Vec::new();
        for (position, literal) in &literals {
            let [EntityId::Value(owner)] = literal.results.as_slice() else {
                panic!("owner")
            };
            assert!(!owners.contains(owner));
            owners.push(*owner);
            let roots: Vec<_> = f
                .instructions
                .iter()
                .enumerate()
                .filter(|(_, i)| matches!(i.operation,Operation::RootPlace{owner:o} if o==*owner))
                .collect();
            assert_eq!(roots.len(), 1);
            assert!(*position < roots[0].0);
            root_positions.push(roots[0].0);
        }
        let ir_calls: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation, Operation::DirectCall { .. }))
            .collect();
        assert_eq!(ir_calls.len(), 2);
        let mut all_loans = Vec::new();
        let mut previous_end = root_positions[0];
        let mut callee_id = None;
        for (call_index, (position, instruction)) in ir_calls.iter().enumerate() {
            let expected = &descriptor.calls()[call_index];
            assert_eq!(
                instruction.origin,
                Origin::Source(
                    parsed
                        .ast()
                        .expressions()
                        .get(expected.contracts()[0].call().expression())
                        .unwrap()
                        .span()
                )
            );
            let Operation::DirectCall {
                callee, arguments, ..
            } = &instruction.operation
            else {
                unreachable!()
            };
            if let Some(original) = callee_id {
                assert_ne!(original, *callee);
            } else {
                callee_id = Some(*callee);
            }
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
            assert_eq!(
                unit.program.modules[0].function(*callee).unwrap().origin,
                Origin::Source(declaration_span)
            );
            let mut max_end = *position;
            let mut begin_spans = Vec::new();
            for (column, argument) in arguments.iter().enumerate() {
                let EntityId::Loan(loan) = argument else {
                    panic!("loan")
                };
                assert!(!all_loans.contains(loan));
                all_loans.push(*loan);
                let begins: Vec<_> = f
                    .instructions
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| i.results == vec![*argument])
                    .collect();
                assert_eq!(begins.len(), 1);
                let (begin_position, begin) = begins[0];
                assert!(previous_end < begin_position && begin_position < *position);
                let contract = expected
                    .contracts()
                    .iter()
                    .find(|c| c.parameter_index() == column)
                    .unwrap();
                let span = parsed
                    .ast()
                    .expressions()
                    .get(contract.argument().expression())
                    .unwrap()
                    .span();
                assert_eq!(begin.origin, Origin::Source(span));
                begin_spans.push((begin_position, span));
                if column < 2 {
                    let Operation::BorrowBegin { place, .. } = begin.operation else {
                        panic!("borrow")
                    };
                    let root = f
                        .instructions
                        .iter()
                        .find(|i| i.results == vec![EntityId::Place(place)])
                        .unwrap();
                    let Operation::RootPlace { owner } = root.operation else {
                        panic!("place")
                    };
                    let source_position = expected
                        .contracts()
                        .iter()
                        .position(|c| c.argument() == contract.argument())
                        .unwrap();
                    if expected.named_argument_root(source_position) == Some(symbol) {
                        assert_eq!(owner, owners[0]);
                    } else {
                        assert_eq!(call_index, 1);
                        assert_eq!(owner, owners[1]);
                        assert_eq!(literals[1].1.origin, Origin::Source(span));
                        let Operation::StringLiteral { bytes, .. } = &literals[1].1.operation
                        else {
                            unreachable!()
                        };
                        assert_eq!(bytes, expected.temporary_string(source_position).unwrap());
                        assert!(previous_end < literals[1].0 && root_positions[1] < begin_position);
                    }
                }
                let ends: Vec<_> = f
                    .instructions
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| matches!(i.operation,Operation::BorrowEnd{loan:l} if l==*loan))
                    .collect();
                assert_eq!(ends.len(), 1);
                assert!(ends[0].0 > *position);
                max_end = max_end.max(ends[0].0);
            }
            begin_spans.sort_by_key(|(i, _)| *i);
            assert!(
                begin_spans
                    .windows(2)
                    .all(|p| p[0].1.end() < p[1].1.start())
            );
            previous_end = max_end;
        }
        assert_eq!(all_loans.len(), 8);
        let drops: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation, Operation::Drop { .. }))
            .collect();
        assert_eq!(drops.len(), 2);
        assert!(previous_end < drops[0].0 && drops[0].0 < drops[1].0);
        assert!(matches!(drops[0].1.operation,Operation::Drop{owner} if owner==owners[1]));
        assert!(matches!(drops[1].1.operation,Operation::Drop{owner} if owner==owners[0]));
        assert_eq!(drops[0].1.origin, ir_calls[1].1.origin);
        assert_eq!(drops[1].1.origin, Origin::Source(named_drop.value_origin()));
        crate::ssa::verify::verify_program(&unit.program).unwrap();
        crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
    }
}

#[test]
fn different_late_temp_shared_rejects_reverse_and_uncovered_shapes() {
    check_rejected_closure(cases()[0], None, Some(1));
    check_rejected_closure(cases()[0], None, Some(2));
    for body in [
        "val owner=\"shared\";run(owner,\"t\",1,false);run(owner,owner,2,true)",
        "val owner=\"shared\";run(owner,owner,1,false);run(\"a\",\"b\",2,true)",
        "val owner=\"shared\";val other=\"o\";run(owner,owner,1,false);run(\"t\",other,2,true)",
        "val owner=\"shared\";run(owner,owner,1,false);consume(owner);run(\"t\",owner,2,true)",
        "val owner=\"shared\";run(owner,owner,1,false);run(\"t\",owner,2,true);run(owner,owner,3,true)",
        "val owner=\"shared\";val n=1;run(owner,owner,n,false);run(\"t\",owner,2,true)",
        "val owner=\"shared\";run(owner,owner,1,false);run(\"t\",owner,1+1,true)",
        "val owner=\"shared\";run(owner,owner,1,false);run((\"t\"),owner,2,true)",
    ] {
        let (first, rest) = body.split_once("run(").unwrap();
        let body = format!("{first}run_a({}", rest.replace("run(", "run_b("));
        let source_text = text(cases()[0], true).replace(
            &format!("val owner=\"shared\";{};{}", cases()[0][0], cases()[0][1]),
            &body,
        );
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

        assert!(matches!(
            prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry),
            Err(TwoSourceUnitError::UnsupportedSelection)
        ));
    }
}

fn check_rejected_closure(calls: [&str; 2], body: Option<&str>, rejected: Option<u8>) {
    let mut sources = SourceMap::new();
    let mut source_text = text(calls, true);
    if let Some(body) = body {
        source_text = source_text.replace(
            &format!("val owner=\"shared\";{};{}", calls[0], calls[1]),
            body,
        );
    }
    let original = source(false, "");
    let run = original
        .lines()
        .find(|line| line.starts_with("fun run("))
        .unwrap();
    if rejected == Some(1) {
        source_text += &format!("\n{}", run.replace("fun run(", "fun unused("));
    }
    if rejected == Some(2) {
        source_text = source_text.replace(
            &run.replace("fun run(", "fun run_b("),
            "fun run_b(left:String,right:String,rounds:Int,choose:Boolean):Unit {}",
        );
    }
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
    }
}
