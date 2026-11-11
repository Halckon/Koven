//! Hand-built SSA boundary checks: CI-only; never select for local ordinary runs.
use super::*;
use crate::ssa::{
    model::{Origin, Program, SsaTypeKind},
    verify::verify_program,
};
use lang_frontend::source::SourceMap;

fn fixture(mixed: bool) -> (Program, BlockId) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("transport.ko", "borrow").unwrap();
    let origin = Origin::Source(sources.span(source, 0, 6).unwrap());
    let mut program = Program::default();
    let module_id = program.add_module("transport");
    let module = program.module_mut(module_id).unwrap();
    let string = module.intern_type(SsaTypeKind::StringOwner);
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: string,
    };
    let id = module
        .add_function("forward", vec![string], origin.clone())
        .unwrap();
    let function = module.function_mut(id).unwrap();
    function.borrow_return = Some(0);
    let entry = function
        .add_block(vec![shared, EntityType::Value(boolean)], origin.clone())
        .unwrap();
    let left = function.add_block(vec![shared], origin.clone()).unwrap();
    let right = function.add_block(vec![shared], origin.clone()).unwrap();
    let join = function.add_block(vec![shared], origin.clone()).unwrap();
    let EntityId::Loan(root) = function.blocks[0].parameters[0] else {
        unreachable!()
    };
    let EntityId::Value(condition) = function.blocks[0].parameters[1] else {
        unreachable!()
    };
    function
        .set_terminator(
            entry,
            TerminatorKind::Conditional {
                condition,
                when_true: Edge {
                    target: left,
                    arguments: vec![EntityId::Loan(root)],
                },
                when_false: Edge {
                    target: right,
                    arguments: vec![EntityId::Loan(root)],
                },
            },
            origin.clone(),
        )
        .unwrap();
    let left_loan = function.block(left).unwrap().parameters[0];
    function
        .set_terminator(
            left,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: vec![left_loan],
            }),
            origin.clone(),
        )
        .unwrap();
    let EntityId::Loan(right_loan) = function.block(right).unwrap().parameters[0] else {
        unreachable!()
    };
    let incoming = if mixed {
        function
            .append_instruction(
                right,
                Operation::SharedReborrow { source: right_loan },
                vec![shared],
                origin.clone(),
            )
            .unwrap()
            .1[0]
    } else {
        EntityId::Loan(right_loan)
    };
    function
        .set_terminator(
            right,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: vec![incoming],
            }),
            origin.clone(),
        )
        .unwrap();
    let EntityId::Loan(join_loan) = function.block(join).unwrap().parameters[0] else {
        unreachable!()
    };
    let result = function
        .append_instruction(
            join,
            Operation::SharedReborrow { source: join_loan },
            vec![shared],
            origin.clone(),
        )
        .unwrap()
        .1[0];
    let EntityId::Loan(result) = result else {
        unreachable!()
    };
    function
        .set_terminator(join, TerminatorKind::BorrowReturn { loan: result }, origin)
        .unwrap();
    (program, join)
}

#[test]
fn borrow_return_transport_ci_only_pure_entry_diamond_is_accepted() {
    let (program, _) = fixture(false);
    verify_program(&program).unwrap();
}

#[test]
fn borrow_return_transport_ci_only_mixed_entry_and_derived_is_rejected_at_return() {
    let (program, join) = fixture(true);
    let function = &program.modules[0].functions[0];
    let root = function.blocks[0].parameters[0];
    let transported = function.block(join).unwrap().parameters[0];
    let aliases = AliasRoots::compute(function);
    assert_eq!(
        aliases.roots.get(&transported),
        Some(&BTreeSet::from([root]))
    );
    let EntityId::Loan(transported) = transported else {
        unreachable!()
    };
    let flows = LoanFlowAliases::compute(function);
    assert_eq!(
        flows.origins[&transported].len(),
        2,
        "same storage root is not one capability"
    );
    let errors = verify_program(&program).expect_err("mixed capability is not entry identity");
    assert!(errors.errors.iter().any(|error| error.kind
        == VerifyErrorKind::ReturnType { index: 0 }
        && error.location == VerifyLocation::Terminator(join)));
}
