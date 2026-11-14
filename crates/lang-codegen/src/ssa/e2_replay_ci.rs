//! CI-only mutations of real experimental artifacts; never execute faults locally.
use super::model::*;
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::two_source_experiment::analyze_two_source_experiment,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};
fn require_ci() {
    assert_eq!(
        std::env::var("CI").as_deref(),
        Ok("true"),
        "CI-only; local compile/list only"
    );
    assert_eq!(
        std::env::var("GITHUB_ACTIONS").as_deref(),
        Ok("true"),
        "GitHub Actions only; local compile/list only"
    );
}
fn check(mutate: impl Fn(&mut super::e2_function::EmittedFunction<'_>), structurally_valid: bool) {
    require_ci();
    let source = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}\nfun run(left:String,right:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(left);consume(a)}else{borrow val b=view(right);other(b)};remaining=remaining-1}}";
    let mut sources = SourceMap::new();
    let id = sources.add_source("e2-ci.ko", source).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, id).unwrap()).unwrap();
    let inputs = [SourceUnitInput::new("root", "e2-ci.ko", id, &parsed)];
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
    let experiment = analyze_two_source_experiment(&sources, &inputs, &names, &types, &typed)
        .unwrap()
        .unwrap();
    let fact = &experiment.facts()[0];
    let mut emitted = super::e2_function::emit(&experiment, fact, &typed).unwrap();
    assert!(super::e2_replay::replay(&emitted, &experiment, fact, &typed).is_some());
    mutate(&mut emitted);
    if structurally_valid {
        super::verify::verify_program(&emitted.fragments.leaves.program)
            .expect("semantic mutation must remain valid SSA");
    }
    assert!(super::e2_replay::replay(&emitted, &experiment, fact, &typed).is_none());
    // A fresh ordinary experimental publication still succeeds after this discarded candidate.
    assert!(super::e2_publication::publish(&experiment, fact, &[fact], &typed).is_ok());
}
#[test]
#[ignore = "CI-only E2 signature/role corruption; local compile/list only"]
fn e2_signature_and_source_roles_reject() {
    require_ci();
    check(
        |e| {
            let f = e.fragments.leaves.program.modules[0]
                .function_mut(e.caller)
                .unwrap();
            f.receiver = Some(f.entity(f.blocks[0].parameters[0]).unwrap().ty);
        },
        true,
    );
    check(
        |e| {
            let f = e.fragments.leaves.program.modules[0]
                .function_mut(e.caller)
                .unwrap();
            let id = f.blocks[3].instructions[0];
            let Operation::DirectCall { arguments, .. } = &mut f.instructions[id.index()].operation
            else {
                panic!("actual outline")
            };
            arguments.swap(0, 1);
        },
        true,
    );
}
#[test]
#[ignore = "CI-only E2 checked transport corruption; local compile/list only"]
fn e2_checked_counter_edges_reject() {
    require_ci();
    check(
        |e| {
            let f = e.fragments.leaves.program.modules[0]
                .function_mut(e.caller)
                .unwrap();
            let old = f.blocks[5].parameters[4];
            let TerminatorKind::Conditional { when_false, .. } =
                &mut f.blocks[5].terminator.as_mut().unwrap().kind
            else {
                panic!("checked")
            };
            when_false.arguments[4] = old;
        },
        true,
    );
    check(
        |e| {
            let f = e.fragments.leaves.program.modules[0]
                .function_mut(e.caller)
                .unwrap();
            let TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } = &mut f.blocks[5].terminator.as_mut().unwrap().kind
            else {
                panic!("checked")
            };
            std::mem::swap(when_true, when_false);
        },
        true,
    );
}
#[test]
#[ignore = "CI-only E2 leaf/child-end corruption; local compile/list only"]
fn e2_callee_identity_and_child_end_reject() {
    require_ci();
    check(
        |e| {
            let module = &mut e.fragments.leaves.program.modules[0];
            let other = module.function(e.fragments.arms[1]).unwrap();
            let Operation::DirectCall {
                callee: other_target,
                ..
            } = other.instructions[1].operation
            else {
                panic!("actual consumer")
            };
            let first = module.function_mut(e.fragments.arms[0]).unwrap();
            let Operation::DirectCall { callee, .. } = &mut first.instructions[1].operation else {
                panic!("actual consumer")
            };
            assert_ne!(*callee, other_target);
            *callee = other_target;
        },
        true,
    );
    check(
        |e| {
            let f = e.fragments.leaves.program.modules[0]
                .function_mut(e.fragments.arms[0])
                .unwrap();
            let id = *f.blocks[0].instructions.last().unwrap();
            assert!(matches!(
                f.instructions[id.index()].operation,
                Operation::BorrowEnd { .. }
            ));
            // Keep a valid instruction shape but leave the actual child un-ended.
            let EntityId::Loan(parent) = f.blocks[0].parameters[0] else {
                panic!("Shared root")
            };
            f.instructions[id.index()].operation = Operation::BorrowEnd { loan: parent };
        },
        false,
    );
}
