//! Original evidence for two named roots jointly surviving until the second call.
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
    calls: [&'static str; 2],
    // ABI String roles expressed as original declaration-order root indices.
    roots: [[usize; 2]; 2],
}
fn cases() -> [Case; 2] {
    [
        Case {
            declarations: "val a=\"A\";val b=\"B\"",
            calls: ["run_a(a,b,1,false)", "run_b(b,a,2,true)"],
            roots: [[0, 1], [1, 0]],
        },
        Case {
            declarations: "val b=\"same\";val a=\"same\"",
            calls: [
                "run_b(choose=false,right=b,rounds=1,left=a)",
                "run_a(right=a,rounds=2,left=b,choose=true)",
            ],
            roots: [[1, 0], [0, 1]],
        },
    ]
}

#[test]
fn joint_final_distinct_original_facts_preserve_joint_last_use() {
    for case in cases() {
        let mut sources = SourceMap::new();
        let text = format!(
            "fun run_a(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun run_b(left:String,right:String,rounds:Int,choose:Boolean):Unit {{}}\nfun main():Unit {{{};{};{}}}",
            case.declarations, case.calls[0], case.calls[1]
        );
        let id = sources.add_source("joint-final-probe.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new(
            "root",
            "joint-final-probe.ko",
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
            // End ordinals belong to original cleanup order, not argument evaluation order.
            // Preserve it while independently anchoring every identity to its unique source operand.
            let end_spans: Vec<_> = ends
                .iter()
                .map(|end| {
                    parsed
                        .ast()
                        .expressions()
                        .get(end.loan().argument().expression())
                        .unwrap()
                        .span()
                })
                .collect();
            assert!(
                end_spans.iter().all(
                    |span| span.start() > node.span().start() && span.end() < node.span().end()
                )
            );
            for (index, span) in end_spans.iter().enumerate() {
                assert!(!end_spans[..index].contains(span));
            }
            eprintln!("original End argument spans, cleanup ordinal order: {end_spans:?}");
            // Inspect and assert the entire original point-local vector, not a guessed synthetic Drop.
            let drops: Vec<_> = owned
                .drops()
                .iter()
                .filter(|d| d.point() == UnitDropPoint::CallReturn(call))
                .collect();
            if call_number == 0 {
                assert!(drops.is_empty(), "first original vector: {drops:?}");
            } else {
                assert_eq!(drops.len(), 2, "terminal original vector: {drops:?}");
                assert_ne!(drops[0].target(), drops[1].target());
                // Preserve and expose the original sequence; never sort it into a guessed order.
                eprintln!("joint final original terminal vector: {drops:?}");
                for drop in &drops {
                    let UnitDropTarget::Named(root) = drop.target() else {
                        panic!("only original named roots may retire: {drops:?}")
                    };
                    let matches: Vec<_> = bindings.iter().filter(|(r, _)| *r == root).collect();
                    let [(_, span)] = matches.as_slice() else {
                        panic!("unique formed root")
                    };
                    assert_eq!(drop.value_origin(), *span);
                    assert_eq!(
                        owned
                            .drops()
                            .iter()
                            .filter(|d| d.target() == drop.target())
                            .count(),
                        1
                    );
                }
            }
        }
        assert_ne!(instances[0], instances[1]);
        assert_eq!(all_arguments.len(), 8);
    }
}

#[test]
fn joint_final_distinct_closed_facts_prepare_joint_retirement() {
    for case in cases() {
        let calls = case.calls;
        let definitions = source(false, "")
            .lines()
            .filter(|line| !line.starts_with("fun main("))
            .collect::<Vec<_>>()
            .join("\n");
        let second = definitions
            .lines()
            .find(|line| line.starts_with("fun run("))
            .unwrap()
            .replace("fun run(", "fun run_b(");
        let definitions = definitions.replace("fun run(", "fun run_a(") + "\n" + &second;
        let source_text = format!(
            "{definitions}\nfun main():Unit {{{};{};{}}}",
            case.declarations, calls[0], calls[1]
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
        assert!(descriptor.is_different_joint_final_named_pair());
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
        let (declared, terminal) = descriptor
            .joint_final_owners()
            .expect("private joint-final root");
        assert!(descriptor.staggered_owners().is_none());
        assert!(descriptor.shared_owner().is_none());
        let unit = prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry)
            .expect("joint final entry");
        let f = unit.program.modules[0].function(unit.function).unwrap();
        let strings: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation, Operation::StringLiteral { .. }))
            .collect();
        assert_eq!(strings.len(), 2);
        let mut actual = Vec::new();
        for ((position, instruction), owner) in strings.iter().zip(declared) {
            assert_eq!(instruction.origin, Origin::Source(owner.span()));
            let Operation::StringLiteral { bytes, .. } = &instruction.operation else {
                unreachable!()
            };
            assert_eq!(bytes, owner.bytes());
            let [EntityId::Value(value)] = instruction.results.as_slice() else {
                panic!("owner")
            };
            let roots: Vec<_> = f.instructions.iter().enumerate().filter(|(_,i)| matches!(i.operation,Operation::RootPlace { owner } if owner == *value)).collect();
            assert_eq!(roots.len(), 1);
            let [EntityId::Place(place)] = roots[0].1.results.as_slice() else {
                panic!("root")
            };
            assert!(*position < roots[0].0);
            actual.push((owner.root(), *value, *place, roots[0].0));
        }
        assert_ne!(actual[0].1, actual[1].1);
        assert!(actual[0].3 < strings[1].0);
        let calls: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation, Operation::DirectCall { .. }))
            .collect();
        assert_eq!(calls.len(), 2);
        let mut previous_end = actual[1].3;
        let mut loan_ids = Vec::new();
        let mut callees = Vec::new();
        for (index, (position, instruction)) in calls.iter().enumerate() {
            let expected = &descriptor.calls()[index];
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
            assert!(!callees.contains(callee));
            callees.push(*callee);
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
            assert_eq!(
                unit.program.modules[0].function(*callee).unwrap().origin,
                Origin::Source(parsed.ast().items().get(declaration.root()).unwrap().span())
            );
            let mut end_max = *position;
            let mut source_begins = Vec::new();
            for (column, argument) in arguments.iter().enumerate() {
                let EntityId::Loan(loan) = argument else {
                    panic!("loan")
                };
                assert!(!loan_ids.contains(loan));
                loan_ids.push(*loan);
                let begins: Vec<_> = f
                    .instructions
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| i.results == vec![*argument])
                    .collect();
                assert_eq!(begins.len(), 1);
                let (begin_index, begin) = begins[0];
                assert!(previous_end < begin_index && begin_index < *position);
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
                source_begins.push((begin_index, span));
                if column < 2 {
                    let Operation::BorrowBegin { place, .. } = begin.operation else {
                        panic!("begin")
                    };
                    assert_eq!(place, actual[case.roots[index][column]].2);
                }
                let ends: Vec<_> = f.instructions.iter().enumerate().filter(|(_,i)| matches!(i.operation, Operation::BorrowEnd { loan: l } if l == *loan)).collect();
                assert_eq!(ends.len(), 1);
                assert!(ends[0].0 > *position);
                end_max = end_max.max(ends[0].0);
            }
            source_begins.sort_by_key(|(i, _)| *i);
            assert!(
                source_begins
                    .windows(2)
                    .all(|p| p[0].1.end() < p[1].1.start())
            );
            previous_end = end_max;
        }
        assert_eq!(loan_ids.len(), 8);
        let drops: Vec<_> = f
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.operation, Operation::Drop { .. }))
            .collect();
        assert_eq!(drops.len(), 2);
        for ((position, instruction), fact) in drops.iter().zip(terminal) {
            assert!(*position > previous_end);
            previous_end = *position;
            let UnitDropTarget::Named(root) = fact.target() else {
                panic!("named")
            };
            let expected = actual
                .iter()
                .find(|(symbol, ..)| *symbol == root)
                .unwrap()
                .1;
            assert!(matches!(instruction.operation,Operation::Drop { owner } if owner==expected));
            assert_eq!(instruction.origin, Origin::Source(fact.value_origin()));
        }
        crate::ssa::verify::verify_program(&unit.program).unwrap();
        crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
    }
}

#[test]
fn joint_final_distinct_rejects_other_shapes_and_unclaimed_shells() {
    let normal = "val a=\"a\";val b=\"b\";run_a(a,b,1,false);run_b(b,a,2,true)";
    for (body, kind) in [
        (
            "val a=\"a\";val b=\"b\";val c=\"c\";run_a(a,b,1,false);run_b(b,c,2,true)",
            0,
        ),
        (
            "val a=\"a\";val b=\"b\";run_a(a,b,1,false);run_b(b,a,2,true);run_a(a,b,3,true)",
            0,
        ),
        (
            "val a=\"a\";run_a(a,\"t\",1,false);val b=\"b\";run_b(b,a,2,true)",
            0,
        ),
        (
            "val a=\"a\";val b=\"b\";run_a(a,\"t\",1,false);run_b(b,a,2,true)",
            0,
        ),
        (
            "val a=\"a\";val b=\"b\";run_a(a,b,1,false);run_b(b,a,1+1,true)",
            0,
        ),
        (
            "val a=\"a\";val b=\"b\";run_a(a,b,1,false);consume(a);run_b(b,a,2,true)",
            0,
        ),
        (
            "val a=\"a\";val b=\"b\";val n=1;run_a(a,b,n,false);run_b(b,a,2,true)",
            0,
        ),
        (normal, 1),
        (normal, 2),
    ] {
        let definitions = source(false, "")
            .lines()
            .filter(|line| !line.starts_with("fun main("))
            .collect::<Vec<_>>()
            .join("\n");
        let second = definitions
            .lines()
            .find(|line| line.starts_with("fun run("))
            .unwrap()
            .replace("fun run(", "fun run_b(");
        let mut definitions = definitions.replace("fun run(", "fun run_a(");
        if kind == 1 {
            definitions += &format!("\n{}", second.replace("fun run_b(", "fun unused("));
        }
        let second = if kind == 2 {
            "fun run_b(left:String,right:String,rounds:Int,choose:Boolean):Unit {}".to_string()
        } else {
            second
        };
        let source_text = format!("{definitions}\n{second}\nfun main():Unit {{{body}}}");
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

        let result = prepare_two_source_unit(&sources, &inputs, &names, &types, &typed, entry);
        if kind == 1 {
            assert!(matches!(result, Err(TwoSourceUnitError::Preflight(_))));
        } else {
            assert!(matches!(
                result,
                Err(TwoSourceUnitError::UnsupportedSelection)
            ));
        }
    }
}
