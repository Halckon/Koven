//! One original shared owner across two independently closed callable instances.
use super::*;
use crate::ssa::model::{EntityId, Operation, Origin};
use lang_frontend::{
    ownership_checking::{
        LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget, check_compilation_unit_ownership,
        two_source_experiment::analyze_two_source_experiment,
    },
    type_checking::UnitCallTarget,
};

const CASES: [[&str; 2]; 2] = [
    ["run_a(owner,owner,1,false)", "run_b(owner,owner,2,true)"],
    [
        "run_b(choose=true,right=owner,rounds=7,left=owner)",
        "run_a(rounds=0,left=owner,choose=false,right=owner)",
    ],
];

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
fn shared_distinct_callees_original_facts_preserve_one_final_owner_drop() {
    for calls in CASES {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("shared-distinct-probe.ko", text(calls, false))
            .unwrap();
        let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new(
            "root",
            "shared-distinct-probe.ko",
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
        let [owner] = owned.formed_owned_locals() else {
            panic!("one formed root")
        };
        let [drop] = owned.drops() else {
            panic!("one original Drop: {:?}", owned.drops())
        };
        assert_eq!(drop.target(), UnitDropTarget::Named(*owner));
        let symbol = names.names().source_units()[owner.source_unit().index()]
            .resolution()
            .symbols()
            .get(owner.symbol().index())
            .unwrap();
        assert_eq!(drop.value_origin(), symbol.span());
        let initializers: Vec<_> = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, n)| sources.slice(n.span()) == Ok("\"shared\""))
            .collect();
        let [(_, initializer)] = initializers.as_slice() else {
            panic!("one initializer")
        };
        let mut instances = Vec::new();
        let mut all_arguments = Vec::new();
        let mut previous_end = initializer.span().end();
        for (number, call_text) in calls.into_iter().enumerate() {
            let nodes: Vec<_> = parsed
                .ast()
                .expressions()
                .iter()
                .filter(|(_, n)| sources.slice(n.span()) == Ok(call_text))
                .collect();
            let [(expression, node)] = nodes.as_slice() else {
                panic!("unique call")
            };
            assert!(previous_end < node.span().start());
            previous_end = node.span().end();
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
            let contracts: Vec<_> = owned.call_argument_contracts_for(call).collect();
            assert_eq!(contracts.len(), 4);
            for parameter in 0..4 {
                assert_eq!(
                    contracts
                        .iter()
                        .filter(|c| c.parameter_index() == parameter)
                        .count(),
                    1
                );
            }
            let mut spans = Vec::new();
            for loan in loans {
                assert_eq!(loan.kind(), LoanKind::Shared);
                assert!(!all_arguments.contains(&loan.argument()));
                all_arguments.push(loan.argument());
                let matching: Vec<_> = contracts
                    .iter()
                    .filter(|c| c.argument() == loan.argument())
                    .collect();
                let [contract] = matching.as_slice() else {
                    panic!("exactly one original contract per complete loan")
                };
                let argument = parsed
                    .ast()
                    .expressions()
                    .get(loan.argument().expression())
                    .unwrap();
                spans.push(argument.span());
                if contract.parameter_index() < 2 {
                    assert_eq!(sources.slice(argument.span()), Ok("owner"));
                    assert!(
                        matches!(loan.target(),UnitLoanTarget::Place(p) if p.is_root()&&p.root()==*owner)
                    );
                } else {
                    assert_eq!(loan.target(), &UnitLoanTarget::Temporary(loan.argument()));
                }
                let ends: Vec<_> = owned
                    .direct_call_loan_ends()
                    .iter()
                    .filter(|e| e.loan() == loan)
                    .collect();
                assert_eq!(ends.len(), 1);
                assert_eq!(ends[0].point(), UnitDropPoint::CallReturn(call));
                assert_eq!(ends[0].preceding_drops(), 0);
            }
            assert!(spans.windows(2).all(|p| p[0].end() < p[1].start()));
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
            if number == 0 {
                assert!(drops.is_empty(), "{drops:?}");
            } else {
                assert_eq!(drops, [drop]);
            }
        }
        assert_ne!(instances[0], instances[1]);
        assert_eq!(all_arguments.len(), 8);
    }
}

#[test]
fn shared_distinct_callees_closed_facts_prepare_shared_root() {
    for calls in CASES {
        check_closed(calls, None, None);
    }
}

#[test]
fn shared_distinct_callees_reject_other_shapes_and_unclaimed_shells() {
    for body in [
        "val owner=\"shared\";run_a(owner,\"a\",1,false);run_b(\"b\",owner,2,true)",
        "val owner=\"shared\";run_a(owner,owner,1,false);run_b(owner,\"b\",2,true)",
        "run_a(\"a\",\"b\",1,false);val owner=\"shared\";run_b(owner,owner,2,true)",
        "val owner=\"shared\";val other=\"shared\";run_a(owner,other,1,false);run_b(owner,owner,2,true)",
        "val owner=\"shared\";run_a(owner,owner,1,false);run_b(owner,owner,2,true);run_a(owner,owner,3,true)",
        "val owner=\"shared\";val rounds=1;run_a(owner,owner,rounds,false);run_b(owner,owner,2,true)",
        "val owner=\"shared\";run_a(owner,owner,1,false);run_b(owner,owner,1+1,true)",
        "val owner=(\"shared\");run_a(owner,owner,1,false);run_b(owner,owner,2,true)",
    ] {
        check_closed(CASES[0], Some(body), Some(0));
    }
    check_closed(CASES[0], None, Some(1));
    check_closed(CASES[0], None, Some(2));
}

// 0: valid unsupported shape, 1: unclaimed closed shell, 2: non-E2 second definition.
fn check_closed(calls: [&str; 2], body: Option<&str>, rejected: Option<u8>) {
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
        return;
    }
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
    let descriptors = experiment.entries(&typed).unwrap();
    let descriptor = descriptors.iter().find(|d| d.entry() == entry).unwrap();
    assert!(descriptor.is_different_shared_named_pair());
    assert!(!descriptor.is_different_temporary_pair());
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
    let unit = prepared.expect("two distinct closed callees with one original shared root");
    let module = &unit.program.modules[0];
    let f = module.function(unit.function).unwrap();
    let strings: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::StringLiteral { .. }))
        .collect();
    let [(string_index, string)] = strings.as_slice() else {
        panic!("one root initialization")
    };
    assert!(matches!(&string.operation,Operation::StringLiteral{bytes,..} if bytes==b"shared"));
    let [EntityId::Value(owner)] = string.results.as_slice() else {
        panic!("owner value")
    };
    let roots: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation,Operation::RootPlace{owner:actual}if actual==*owner))
        .collect();
    let [(root_index, root)] = roots.as_slice() else {
        panic!("one root place")
    };
    let [EntityId::Place(place)] = root.results.as_slice() else {
        panic!("root place")
    };
    let (_, _, initializer_span, _, final_drop) = descriptor.shared_owner().unwrap();
    assert_eq!(string.origin, Origin::Source(initializer_span));
    assert_eq!(root.origin, Origin::Source(initializer_span));
    assert!(string_index < root_index);
    let actual_calls: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::DirectCall { .. }))
        .collect();
    assert_eq!(actual_calls.len(), 2);
    let mut last_end = *root_index;
    let mut all_loans = Vec::new();
    let mut callees = Vec::new();
    for ((call_index, actual), expected) in actual_calls.iter().zip(descriptor.calls()) {
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
        let Operation::DirectCall {
            callee, arguments, ..
        } = &actual.operation
        else {
            unreachable!()
        };
        assert_eq!(
            module.function(*callee).unwrap().origin,
            Origin::Source(declaration_span)
        );
        assert!(!callees.contains(callee));
        callees.push(*callee);
        assert_eq!(
            actual.origin,
            Origin::Source(expected.contracts()[0].call_span())
        );
        let previous_end = last_end;
        let mut previous_begin = *root_index;
        for contract in expected.contracts() {
            let argument = arguments[contract.parameter_index()];
            let EntityId::Loan(loan) = argument else {
                panic!("loan")
            };
            assert!(!all_loans.contains(&loan));
            all_loans.push(loan);
            let (begin_index, begin) = f
                .instructions
                .iter()
                .enumerate()
                .find(|(_, i)| i.results == [argument])
                .unwrap();
            assert!(
                previous_end < begin_index
                    && previous_begin < begin_index
                    && begin_index < *call_index
            );
            previous_begin = begin_index;
            let arg_span = parsed
                .ast()
                .expressions()
                .get(contract.argument().expression())
                .unwrap()
                .span();
            assert_eq!(begin.origin, Origin::Source(arg_span));
            if contract.parameter_index() < 2 {
                assert!(
                    matches!(begin.operation,Operation::BorrowBegin{place:actual,..}if actual==*place)
                );
            }
            let ends: Vec<_> = f
                .instructions
                .iter()
                .enumerate()
                .filter(
                    |(_, i)| matches!(i.operation,Operation::BorrowEnd{loan:actual}if actual==loan),
                )
                .collect();
            assert_eq!(ends.len(), 1);
            assert!(ends[0].0 > *call_index);
            last_end = last_end.max(ends[0].0);
        }
    }
    assert_eq!(all_loans.len(), 8);
    let drops: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::Drop { .. }))
        .collect();
    let [(drop_index, drop)] = drops.as_slice() else {
        panic!("unique final Drop")
    };
    assert!(*drop_index > last_end);
    assert!(matches!(drop.operation,Operation::Drop{owner:actual}if actual==*owner));
    assert_eq!(drop.origin, Origin::Source(final_drop.value_origin()));
    crate::ssa::verify::verify_program(&unit.program).unwrap();
    crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
}
