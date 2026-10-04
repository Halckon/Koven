use super::*;
use crate::ssa::model::{
    Edge, EntityType, LoanKind, Origin, Program, SequentialContainerKind, SsaTypeId, SsaTypeKind,
};
use lang_frontend::source::SourceMap;

fn fixture(loans: usize, flags: usize) -> (Function, EntityType, SsaTypeId, Origin) {
    let mut sources = SourceMap::default();
    let source = sources.add_source("provider-state.ko", "fixture").unwrap();
    let origin = Origin::Source(sources.span(source, 0, 7).unwrap());
    let mut program = Program::default();
    let module_id = program.add_module("provider-state");
    let module = program.module_mut(module_id).unwrap();
    let int = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let container = module
        .add_sequential_container_type(SequentialContainerKind::Array, int)
        .unwrap();
    let loan_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: container,
    };
    let id = module.add_function("scan", vec![], origin.clone()).unwrap();
    let function = module.function_mut(id).unwrap();
    let parameters = std::iter::repeat_n(loan_type, loans)
        .chain(std::iter::repeat_n(EntityType::Value(boolean), flags))
        .chain([EntityType::Value(int)])
        .collect();
    function.add_block(parameters, origin.clone()).unwrap();
    (module.functions.remove(0), loan_type, int, origin)
}

fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(value) = entity else {
        panic!("loan")
    };
    value
}
fn value(entity: EntityId) -> crate::ssa::model::ValueId {
    let EntityId::Value(value) = entity else {
        panic!("value")
    };
    value
}

#[test]
fn different_provider_loans_and_places_remain_correlated_on_actual_edges() {
    let (mut function, loan_type, int, origin) = fixture(2, 1);
    let entry = function.entry_block().unwrap();
    let parameters = function.blocks[0].parameters.clone();
    let left = function.add_block(vec![], origin.clone()).unwrap();
    let right = function.add_block(vec![], origin.clone()).unwrap();
    let join = function
        .add_block(vec![loan_type, EntityType::Place(int)], origin.clone())
        .unwrap();
    function
        .set_terminator(
            entry,
            TerminatorKind::Conditional {
                condition: value(parameters[2]),
                when_true: Edge {
                    target: left,
                    arguments: vec![],
                },
                when_false: Edge {
                    target: right,
                    arguments: vec![],
                },
            },
            origin.clone(),
        )
        .unwrap();
    for (block, selected, ended) in [
        (left, parameters[0], parameters[1]),
        (right, parameters[1], parameters[0]),
    ] {
        let (_, results) = function
            .append_instruction(
                block,
                Operation::ContainerElementPlace {
                    owner: selected,
                    index: value(parameters[3]),
                },
                vec![EntityType::Place(int)],
                origin.clone(),
            )
            .unwrap();
        function
            .append_instruction(
                block,
                Operation::BorrowEnd { loan: loan(ended) },
                vec![],
                origin.clone(),
            )
            .unwrap();
        function
            .set_terminator(
                block,
                TerminatorKind::Branch(Edge {
                    target: join,
                    arguments: vec![selected, results[0]],
                }),
                origin.clone(),
            )
            .unwrap();
    }
    let joined = function.blocks[join.index()].parameters.clone();
    let EntityId::Place(place) = joined[1] else {
        panic!("place")
    };
    let (_, results) = function
        .append_instruction(
            join,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: int,
            }],
            origin.clone(),
        )
        .unwrap();
    for ended in [results[0], joined[0]] {
        function
            .append_instruction(
                join,
                Operation::BorrowEnd { loan: loan(ended) },
                vec![],
                origin.clone(),
            )
            .unwrap();
    }
    function
        .set_terminator(join, TerminatorKind::Return { values: vec![] }, origin)
        .unwrap();
    let mut errors = Vec::new();
    verify(&function, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn path_state_budget_exhaustion_fails_closed() {
    const DIAMONDS: usize = 16;
    let (mut function, _, int, origin) = fixture(DIAMONDS + 1, DIAMONDS);
    let mut current = function.entry_block().unwrap();
    let parameters = function.blocks[0].parameters.clone();
    function
        .append_instruction(
            current,
            Operation::ContainerElementPlace {
                owner: parameters[0],
                index: value(*parameters.last().unwrap()),
            },
            vec![EntityType::Place(int)],
            origin.clone(),
        )
        .unwrap();
    for index in 0..DIAMONDS {
        let ended = function.add_block(vec![], origin.clone()).unwrap();
        let retained = function.add_block(vec![], origin.clone()).unwrap();
        let join = function.add_block(vec![], origin.clone()).unwrap();
        function
            .set_terminator(
                current,
                TerminatorKind::Conditional {
                    condition: value(parameters[DIAMONDS + 1 + index]),
                    when_true: Edge {
                        target: ended,
                        arguments: vec![],
                    },
                    when_false: Edge {
                        target: retained,
                        arguments: vec![],
                    },
                },
                origin.clone(),
            )
            .unwrap();
        function
            .append_instruction(
                ended,
                Operation::BorrowEnd {
                    loan: loan(parameters[index + 1]),
                },
                vec![],
                origin.clone(),
            )
            .unwrap();
        for block in [ended, retained] {
            function
                .set_terminator(
                    block,
                    TerminatorKind::Branch(Edge {
                        target: join,
                        arguments: vec![],
                    }),
                    origin.clone(),
                )
                .unwrap();
        }
        current = join;
    }
    function
        .set_terminator(current, TerminatorKind::Return { values: vec![] }, origin)
        .unwrap();
    let mut errors = Vec::new();
    verify(&function, &mut errors);
    assert!(
        errors.iter().any(|error| matches!(
            error.kind,
            VerifyErrorKind::OperationContract {
                reason: "provider provenance fixed-point budget exceeded",
            }
        )),
        "{errors:?}"
    );
}
