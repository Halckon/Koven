//! Mutations attack the public producer contract rather than re-running its builder.
use super::{
    CompilationUnitOwnership, UnitDropPoint, UnitIterationCleanupAction as Action,
    UnitIterationExitKind as Exit,
};

fn fixture() -> CompilationUnitOwnership {
    let owned = super::constants_tests::analyze(
        "package a\nfun make(): List<Int> = listOf(1)\nfun run() { for (outer in make()) { for (inner in make()) {return} } }",
    );
    assert!(owned.clone().validate().is_ok());
    owned
}

fn return_point(owned: &CompilationUnitOwnership) -> UnitDropPoint {
    owned
        .iterations()
        .iter()
        .flat_map(|plan| plan.exits())
        .find(|exit| matches!(exit.kind(), Exit::Return(_)))
        .unwrap()
        .point()
}

fn mutate_point(
    owned: &mut CompilationUnitOwnership,
    point: UnitDropPoint,
    change: impl Fn(&mut Vec<Action>),
) {
    for plan in &mut owned.iterations {
        for exit in &mut plan.exits {
            if exit.point == point {
                change(&mut exit.actions);
            }
        }
    }
}

#[test]
fn iteration_schema_rejects_missing_provider_return_and_foreign_source() {
    let valid = fixture();
    let mut missing_provider = valid.clone();
    missing_provider.iterations.pop();
    assert!(missing_provider.validate().is_err());
    let mut missing_return = valid.clone();
    missing_return.iterations[0]
        .exits
        .retain(|exit| !matches!(exit.kind, Exit::Return(_)));
    assert!(missing_return.validate().is_err());
    let mut source = valid.clone();
    source.iterations[0].source = valid.iterations[1].source.clone();
    assert!(source.validate().is_err());
    let mut binding = valid.clone();
    binding.iterations[0].bindings = valid.iterations[1].bindings.clone();
    assert!(binding.validate().is_err());
}

#[test]
fn iteration_schema_rejects_extra_actions_and_outer_before_inner_cleanup() {
    let valid = fixture();
    let point = return_point(&valid);
    let first = valid.iterations[0].descriptor.statement();
    let symbol = valid.iterations[0].bindings[0].symbol();
    for extra in [
        Action::EndSource(first),
        Action::EndBinding {
            statement: first,
            symbol,
        },
    ] {
        let mut mutated = valid.clone();
        mutate_point(&mut mutated, point, |actions| actions.push(extra.clone()));
        assert!(mutated.validate().is_err());
    }
    let mut reversed = valid;
    mutate_point(&mut reversed, point, |actions| {
        // Rotate the two complete provider control sequences. Each provider's own
        // EndElement/Finish/EndSource order stays correct, but lexical unwind reverses.
        let mut control = actions
            .iter()
            .filter(|action| !matches!(action, Action::Drop(_)))
            .cloned()
            .collect::<Vec<_>>();
        let boundary = control
            .iter()
            .position(|action| matches!(action, Action::EndSource(_)))
            .unwrap()
            + 1;
        control.rotate_left(boundary);
        let mut control = control.into_iter();
        for action in actions
            .iter_mut()
            .filter(|action| !matches!(action, Action::Drop(_)))
        {
            *action = control.next().unwrap();
        }
    });
    assert!(reversed.validate().is_err());
}

#[test]
fn iteration_schema_rejects_a_valid_provider_belonging_to_another_exit() {
    let valid = super::constants_tests::analyze(
        "package a\nfun run() { for (first in listOf(1)) {break}\nfor (second in listOf(2)) {} }",
    );
    assert!(valid.clone().validate().is_ok());
    let (owner, point) = valid
        .iterations
        .iter()
        .find_map(|plan| {
            plan.exits
                .iter()
                .find(|exit| matches!(exit.kind, Exit::Break(_)))
                .map(|exit| (plan.descriptor.statement(), exit.point))
        })
        .unwrap();
    let foreign = valid
        .iterations
        .iter()
        .find(|plan| plan.descriptor.statement() != owner)
        .unwrap()
        .descriptor
        .statement();
    let mut mutated = valid;
    mutate_point(&mut mutated, point, |actions| {
        actions.push(Action::EndSource(foreign))
    });
    assert!(mutated.validate().is_err());
}

#[test]
fn iteration_schema_rejects_early_source_release_and_late_pending_loan_end() {
    let mut valid = fixture();
    let point = return_point(&valid);
    mutate_point(&mut valid, point, |actions| {
        let drop = actions
            .iter()
            .position(|action| matches!(action, Action::Drop(_)))
            .unwrap();
        let value = actions.remove(drop);
        actions.insert(0, value);
    });
    assert!(valid.validate().is_err());
    let mut loan = super::constants_tests::analyze(
        "package a\nfun use(text: String, n: Int) {}\nfun run(flag: Boolean) { for (item in listOf(\"text\")) { use(item, if(flag) {return} else 1) } }",
    );
    assert!(loan.clone().validate().is_ok());
    let point = return_point(&loan);
    mutate_point(&mut loan, point, |actions| {
        let index = actions
            .iter()
            .position(|action| matches!(action, Action::EndCallLoan(_)))
            .unwrap();
        let end = actions.remove(index);
        actions.push(end);
    });
    assert!(loan.validate().is_err());
}

#[test]
fn iteration_schema_rejects_body_owner_after_element_end() {
    let mut owned = super::constants_tests::analyze(
        "package a\nclass Guard { deinit() {} }\nfun run() { for (item in listOf(1)) { val guard = Guard()\nreturn } }",
    );
    assert!(owned.clone().validate().is_ok());
    let point = return_point(&owned);
    mutate_point(&mut owned, point, |actions| {
        let index = actions.iter().position(|action| matches!(action, Action::Drop(fact) if matches!(fact.target(), super::UnitDropTarget::Named(_)))).unwrap();
        let drop = actions.remove(index);
        let after_source = actions
            .iter()
            .position(|action| matches!(action, Action::EndSource(_)))
            .unwrap()
            + 1;
        actions.insert(after_source, drop);
    });
    assert!(
        owned.validate().is_err(),
        "body owner release must precede its element end even when all drops keep their relative order"
    );
}

#[test]
fn iteration_schema_rejects_pending_temporary_after_element_end() {
    let mut owned = super::constants_tests::analyze(
        "package a\nfun use(text: String, n: Int) {}\nfun run(flag: Boolean) { for (_ in listOf(1)) { use(\"a\" + \"b\", if(flag) {return} else 1) } }",
    );
    assert!(owned.clone().validate().is_ok());
    let point = return_point(&owned);
    let source = match owned.iterations()[0].source() {
        super::UnitLoanTarget::Temporary(source) => *source,
        _ => panic!("temporary source"),
    };
    mutate_point(&mut owned, point, |actions| {
        let index = actions.iter().position(|action| matches!(action, Action::Drop(fact) if matches!(fact.target(), super::UnitDropTarget::Temporary(owner) if owner != source))).unwrap();
        let drop = actions.remove(index);
        let after_source = actions
            .iter()
            .position(|action| matches!(action, Action::EndSource(_)))
            .unwrap()
            + 1;
        actions.insert(after_source, drop);
    });
    assert!(
        owned.validate().is_err(),
        "pending body owner release must precede element end"
    );
}

#[test]
fn iteration_schema_rejects_loan_end_after_its_temporary_owner_drop() {
    let mut owned = super::constants_tests::analyze(
        "package a\nfun use(text: String, n: Int) {}\nfun run(flag: Boolean) { for (_ in listOf(1)) { use(\"a\" + \"b\", if(flag) {return} else 1) } }",
    );
    assert!(owned.clone().validate().is_ok());
    let point = return_point(&owned);
    mutate_point(&mut owned, point, |actions| {
        let index = actions
            .iter()
            .position(|action| matches!(action, Action::EndCallLoan(_)))
            .unwrap();
        let end = actions.remove(index);
        let after_drop = actions
            .iter()
            .position(|action| matches!(action, Action::Drop(_)))
            .unwrap()
            + 1;
        actions.insert(after_drop, end);
    });
    assert!(
        owned.validate().is_err(),
        "ending before element is insufficient after the borrowed owner was already dropped"
    );
}

#[test]
fn iteration_schema_rejects_missing_duplicate_and_wrong_scope_conditional_receiver_drop() {
    for (body, inside) in [
        (
            "for (_ in xs) { consume(if(flag) {return} else {return}) }",
            true,
        ),
        (
            "consume(if(flag) { for (_ in xs) {return}\n1 } else 1)",
            false,
        ),
    ] {
        let valid = super::constants_tests::analyze(&format!(
            "package a\ninterface Relay {{ own fun consume(n: Int): Unit {{}}\nown fun run(xs: List<Int>, flag: Boolean): Unit {{ {body} }} }}"
        ));
        assert!(valid.clone().validate().is_ok());
        let point = return_point(&valid);
        let conditional = valid
            .conditional_receiver_drops()
            .iter()
            .find(|fact| fact.point() == point)
            .copied()
            .unwrap();
        assert_eq!(conditional.preceding_drops(), 0);
        let mut missing = valid.clone();
        mutate_point(&mut missing, point, |actions| {
            actions.retain(|action| !matches!(action, Action::DropConditionalReceiver(_)))
        });
        assert!(missing.validate().is_err());
        let mut duplicate = valid.clone();
        mutate_point(&mut duplicate, point, |actions| {
            actions.push(Action::DropConditionalReceiver(conditional))
        });
        assert!(duplicate.validate().is_err());
        let mut wrong_scope = valid.clone();
        mutate_point(&mut wrong_scope, point, |actions| {
            let index = actions
                .iter()
                .position(|action| matches!(action, Action::DropConditionalReceiver(_)))
                .unwrap();
            let action = actions.remove(index);
            if inside {
                actions.push(action);
            } else {
                actions.insert(0, action);
            }
        });
        assert!(
            wrong_scope.validate().is_err(),
            "zero preceding ordinary drops must not hide a provider-boundary inversion"
        );
        let foreign = valid
            .conditional_receiver_drops()
            .iter()
            .find(|fact| fact.point() != point)
            .copied()
            .unwrap();
        let mut foreign_point = valid;
        mutate_point(&mut foreign_point, point, |actions| {
            let action = actions
                .iter_mut()
                .find(|action| matches!(action, Action::DropConditionalReceiver(_)))
                .unwrap();
            *action = Action::DropConditionalReceiver(foreign);
        });
        assert!(foreign_point.validate().is_err());
    }
}

#[test]
fn iteration_schema_requires_conditional_receiver_between_its_nested_provider_boundaries() {
    let valid = super::constants_tests::analyze(
        "package a\ninterface Relay { own fun consume(n: Int): Unit {}\nown fun run(xs: List<Int>, flag: Boolean): Unit { for (_ in xs) { consume(if(flag) { for (_ in xs) {return}\nreturn } else {return}) } } }",
    );
    assert!(valid.clone().validate().is_ok());
    let point = valid
        .iterations()
        .iter()
        .flat_map(|plan| plan.exits())
        .find(|exit| {
            matches!(exit.kind(), Exit::Return(_))
                && exit
                    .actions()
                    .iter()
                    .filter(|action| matches!(action, Action::EndSource(_)))
                    .count()
                    == 2
        })
        .unwrap()
        .point();
    for before_all in [true, false] {
        let mut invalid = valid.clone();
        mutate_point(&mut invalid, point, |actions| {
            let index = actions
                .iter()
                .position(|action| matches!(action, Action::DropConditionalReceiver(_)))
                .unwrap();
            let conditional = actions.remove(index);
            if before_all {
                actions.insert(0, conditional);
            } else {
                actions.push(conditional);
            }
        });
        assert!(
            invalid.validate().is_err(),
            "neither before the inner source end nor after the outer element end is a valid receiver cleanup slot"
        );
    }
}

#[test]
fn temporary_range_unit_schema_requires_the_actual_source_loan_continuation() {
    use crate::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        parser::parse_file,
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "range.ko",
            "fun run():Unit{for(item in rangeView(listOf(\"a\"),0,1)){println(item)}}",
        )
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("std", "range.ko", source, &parsed)];
    let (env, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let valid =
        super::check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(valid.clone().validate().is_ok());
    let mut missing = valid.clone();
    missing.borrow_results.range_uses.clear();
    assert!(missing.validate().is_err());
    let mut missing_loan = valid.clone();
    let source = valid.borrow_results.range_uses[0].source_loan();
    missing_loan
        .loans
        .retain(|loan| loan.call() != source.call() || loan.argument() != source.argument());
    assert!(missing_loan.validate().is_err());
    let mut wrong = valid;
    wrong.borrow_results.range_uses[0].origin = super::UnitLoanTarget::Temporary(source.call());
    assert!(wrong.validate().is_err());
}
