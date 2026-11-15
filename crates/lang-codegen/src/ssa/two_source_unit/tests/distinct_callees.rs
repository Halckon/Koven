//! Every original occurrence maps to its own closed source definition, never the first callee.
use super::*;
use crate::ssa::model::{EntityId, Operation, Origin};
use lang_frontend::{
    ownership_checking::{UnitDropTarget, two_source_experiment::analyze_two_source_experiment},
    type_checking::UnitCallTarget,
};

fn definitions() -> (String, String) {
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
    (a, b)
}

#[test]
fn distinct_callees_preserve_original_instances_and_call_lifetimes() {
    for body in [
        "run_a(\"a\",\"b\",1,false);run_b(\"c\",\"d\",2,true)",
        "run_b(choose=true,right=\"same\",rounds=2,left=\"same\");run_a(rounds=1,left=\"same\",choose=false,right=\"same\")",
    ] {
        for reverse in [false, true] {
            check(body, reverse, None);
        }
    }
}

#[test]
fn distinct_callees_keep_other_lifetime_and_shape_boundaries() {
    for body in [
        "val owner=\"s\";run_a(owner,owner,1,false);run_b(owner,owner,(2),true)",
        "val owner=\"s\";run_a(owner,\"a\",1,false);run_b(\"b\",owner,2,true)",
        "run_a(\"a\",\"b\",1,false);val owner=\"s\";run_b(owner,owner,2,true)",
        "run_a(\"a\",\"b\",1,false);run_b(\"c\",\"d\",2,true);run_a(\"e\",\"f\",3,false)",
        "run_a(\"a\",\"b\",1,false);run_b(\"c\",\"d\",1+1,true)",
        "run_a(\"a\",\"b\",1,false);run_b((\"c\"),\"d\",2,true)",
    ] {
        check(body, false, Some(0));
    }
}

#[test]
fn distinct_callees_reject_unclosed_and_unclaimed_definitions() {
    check(
        "run_a(\"a\",\"b\",1,false);run_b(\"c\",\"d\",2,true)",
        false,
        Some(1),
    );
    check(
        "run_a(\"a\",\"b\",1,false);run_b(\"c\",\"d\",2,true)",
        false,
        Some(2),
    );
}

// 0: unsupported source shape; 1: unclaimed valid shell; 2: non-E2 callee.
fn check(body: &str, reverse: bool, rejected: Option<u8>) {
    let (a, mut b) = definitions();
    if rejected == Some(1) {
        b += &format!("\n{}", b.replace("fun run_b(", "fun unused("));
    }
    if rejected == Some(2) {
        b = "fun run_b(left:String,right:String,rounds:Int,choose:Boolean):Unit {}".to_owned();
    }
    b += &format!("\nfun main():Unit {{{body}}}\n");
    let mut sources = SourceMap::new();
    let a_id = sources.add_source("a.ko", a).unwrap();
    let b_id = sources.add_source("b.ko", b).unwrap();
    let a_parsed = parse_file(&sources, &lex(&sources, a_id).unwrap()).unwrap();
    let b_parsed = parse_file(&sources, &lex(&sources, b_id).unwrap()).unwrap();
    let mut inputs = vec![
        SourceUnitInput::new("root", "a.ko", a_id, &a_parsed),
        SourceUnitInput::new("root", "b.ko", b_id, &b_parsed),
    ];
    if reverse {
        inputs.reverse();
    }
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
    if let Some(extra) = rejected {
        if extra == 1 {
            assert!(matches!(prepared, Err(TwoSourceUnitError::Preflight(_))));
        } else {
            assert!(
                matches!(prepared, Err(TwoSourceUnitError::UnsupportedSelection)),
                "{body}"
            );
        }
        return;
    }
    let experiment = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
        .unwrap()
        .unwrap();
    assert_eq!(experiment.facts().len(), 2);
    let descriptor = experiment
        .entries(&typed)
        .unwrap()
        .iter()
        .find(|d| d.entry() == entry)
        .unwrap();
    assert!(descriptor.is_different_temporary_pair());
    assert!(descriptor.common_instance().is_none());
    for fact in experiment.facts() {
        assert!(experiment.function_shell(fact, &typed).is_some());
    }
    #[cfg(feature = "e2-two-source-test-support")]
    {
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
    }
    let unit = prepared.expect("two closed original callees");
    let module = &unit.program.modules[0];
    assert_eq!(module.functions.len(), 9);
    let f = module.function(unit.function).unwrap();
    let calls: Vec<_> = f
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| matches!(i.operation, Operation::DirectCall { .. }))
        .collect();
    assert_eq!(calls.len(), 2);
    let mut actual_callees = Vec::new();
    let mut loans = Vec::new();
    let mut owners = Vec::new();
    let mut last_drop = None;
    for ((call_index, actual), expected) in calls.iter().zip(descriptor.calls()) {
        let typed_call = typed.types().call(expected.contracts()[0].call()).unwrap();
        assert_eq!(typed_call.instance(), expected.instance());
        let UnitCallTarget::Declaration(declaration) = expected.instance().target() else {
            panic!("declaration");
        };
        let declaration = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.id() == declaration)
            .unwrap();
        let source = names
            .names()
            .index()
            .source_units()
            .iter()
            .find(|s| s.id() == declaration.source_unit())
            .unwrap();
        let parsed = inputs
            .iter()
            .find(|i| i.source_id() == source.source_id())
            .unwrap()
            .parsed();
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
        assert!(!actual_callees.contains(callee));
        actual_callees.push(*callee);
        let original_call = inputs
            .iter()
            .find(|i| i.source_id() == b_id)
            .unwrap()
            .parsed()
            .ast()
            .expressions()
            .get(expected.contracts()[0].call().expression())
            .unwrap();
        assert_eq!(actual.origin, Origin::Source(original_call.span()));
        let mut end_max = *call_index;
        let mut begin_previous = None;
        let mut argument_owners = Vec::new();
        for contract in expected.contracts() {
            let argument = arguments[contract.parameter_index()];
            let EntityId::Loan(loan) = argument else {
                panic!("loan");
            };
            assert!(!loans.contains(&loan));
            loans.push(loan);
            let (begin_index, begin) = f
                .instructions
                .iter()
                .enumerate()
                .find(|(_, i)| i.results == [argument])
                .unwrap();
            assert!(begin_index < *call_index);
            assert!(last_drop.is_none_or(|drop| drop < begin_index));
            assert!(begin_previous.is_none_or(|previous| previous < begin_index));
            begin_previous = Some(begin_index);
            let arg_span = b_parsed
                .ast()
                .expressions()
                .get(contract.argument().expression())
                .unwrap()
                .span();
            assert_eq!(begin.origin, Origin::Source(arg_span));
            let ends:Vec<_>=f.instructions.iter().enumerate().filter(|(_,i)|matches!(i.operation,Operation::BorrowEnd{loan:actual} if actual==loan)).collect();
            assert_eq!(ends.len(), 1);
            assert!(ends[0].0 > *call_index);
            end_max = end_max.max(ends[0].0);
            if contract.parameter_index() < 2 {
                let Operation::BorrowBegin { place, .. } = begin.operation else {
                    panic!("borrow");
                };
                let root = f
                    .instructions
                    .iter()
                    .find(|i| i.results == [EntityId::Place(place)])
                    .unwrap();
                let Operation::RootPlace { owner } = root.operation else {
                    panic!("root");
                };
                assert!(!owners.contains(&owner));
                owners.push(owner);
                argument_owners.push((contract.argument(), owner));
            }
        }
        for drop in expected.drops() {
            let UnitDropTarget::Temporary(argument) = drop.target() else {
                panic!("temporary");
            };
            let owner = argument_owners
                .iter()
                .find(|(a, _)| *a == argument)
                .unwrap()
                .1;
            let drops: Vec<_> = f
                .instructions
                .iter()
                .enumerate()
                .filter(
                    |(_, i)| matches!(i.operation,Operation::Drop{owner:actual}if actual==owner),
                )
                .collect();
            assert_eq!(drops.len(), 1);
            assert!(drops[0].0 > end_max);
            assert!(last_drop.is_none_or(|previous| previous < drops[0].0));
            // Existing temporary cleanup IR is located at the original CallReturn span;
            // the ownership Drop value_origin still identifies its original argument.
            assert_eq!(
                drop.value_origin(),
                b_parsed
                    .ast()
                    .expressions()
                    .get(argument.expression())
                    .unwrap()
                    .span()
            );
            assert_eq!(
                drops[0].1.origin,
                Origin::Source(expected.contracts()[0].call_span())
            );
            last_drop = Some(drops[0].0);
        }
    }
    assert_eq!(loans.len(), 8);
    assert_eq!(owners.len(), 4);
    crate::ssa::verify::verify_program(&unit.program).unwrap();
    crate::llvm::render_verified_program_with_entry(&unit.program, unit.function).unwrap();
}
