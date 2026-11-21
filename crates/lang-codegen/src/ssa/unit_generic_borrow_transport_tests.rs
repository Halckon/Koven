//! Exact synchronous borrow results: child and lender retire before Move or owner cleanup.
mod field;
mod parent;
use lang_frontend::{
    name_resolution::{SourceUnitInput, SourceUnitKey},
    source::SourceMap,
    type_checking::{complete_compilation_unit_types, standard_environments},
};

use super::{
    model::{EntityId, EntityType, LoanKind, Operation, SsaTypeKind, TerminatorKind},
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, parsed},
};

#[test]
fn original_generic_synchronous_borrow_transports_then_moves_the_same_owner() {
    let text = include_str!(
        "../../../lang-frontend/tests/ownership_nested_borrow_arguments/fixtures/generic-transport.ko"
    );
    assert_original_transport(text, Lender::Generic);
}

#[test]
fn original_wrapper_synchronous_borrow_forwards_before_moving_the_original_owner() {
    let text = include_str!(
        "../../../lang-frontend/tests/ownership_nested_borrow_arguments/fixtures/wrapper-transport.ko"
    );
    assert_original_transport(text, Lender::Wrapper);
}

#[derive(Clone, Copy)]
enum Lender {
    Generic,
    Wrapper,
}

fn assert_original_transport(text: &str, lender_kind: Lender) {
    let (call_text, declaration_prefix) = match lender_kind {
        Lender::Generic => ("genericView(source)", "fun <T> genericView("),
        Lender::Wrapper => ("wrap(source)", "fun wrap("),
    };
    let path = "nested_borrow.ko";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, path, text);
    let (ne, te) = standard_environments();
    for rooted in [false, true] {
        let inputs = [if rooted {
            SourceUnitInput::new("root", path, source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
        complete_compilation_unit_types(&sources, &inputs, &names, &te, typed.types()).unwrap();
        let index = names.names().index();
        let [input] = index.source_units() else {
            panic!("one real input source")
        };
        assert_eq!(input.source_id(), source);
        assert_eq!(sources.source_text(source), Ok(text));
        assert_eq!(
            matches!(input.key(), SourceUnitKey::MemoryDocument),
            !rooted
        );
        let declaration = index
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        let (program, entry) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &te,
            &typed,
            &owned,
            declaration,
        )
        .expect("the original source reaches real Unit SSA");
        super::verify::verify_program(&program).unwrap();
        let module = program.module(entry.module()).unwrap();
        let caller = module.function(entry).unwrap();
        let [block] = caller.blocks.as_slice() else {
            panic!("real linear call sequence")
        };
        let [
            literal,
            root,
            begin,
            generic,
            observe,
            end_child,
            end_lender,
            consume,
        ] = caller.instructions.as_slice()
        else {
            panic!("exact caller actions")
        };
        let [EntityId::Value(owner)] = literal.results.as_slice() else {
            panic!("own source")
        };
        let owner_ty = caller.entity(EntityId::Value(*owner)).unwrap().ty;
        let EntityType::Value(owner_type) = owner_ty else {
            panic!("caller owns source")
        };
        assert_eq!(
            module.type_kind(owner_type),
            Some(&SsaTypeKind::StringOwner)
        );
        assert_eq!(sources.slice(literal.origin.span()), Ok("\"kept\""));
        assert!(matches!(literal.operation, Operation::StringLiteral { .. }));
        assert_eq!(root.operation, Operation::RootPlace { owner: *owner });
        let [EntityId::Place(place)] = root.results.as_slice() else {
            panic!("same owner place")
        };
        assert_eq!(
            begin.operation,
            Operation::BorrowBegin {
                place: *place,
                kind: LoanKind::Shared
            }
        );
        let [EntityId::Loan(lender)] = begin.results.as_slice() else {
            panic!("actual lender")
        };
        let Operation::BorrowCall {
            callee,
            arguments,
            source: witness,
        } = &generic.operation
        else {
            panic!("real generic borrow transport")
        };
        assert_eq!(sources.slice(generic.origin.span()), Ok(call_text));
        assert_eq!(*witness, *lender);
        assert_eq!(arguments.as_slice(), [EntityId::Loan(*lender)]);
        let actual = module.function(*callee).unwrap();
        assert!(
            sources
                .slice(actual.origin.span())
                .unwrap()
                .starts_with(declaration_prefix)
        );
        let generic_entry = actual.block(actual.entry_block().unwrap()).unwrap();
        let [EntityId::Loan(argument)] = generic_entry.parameters.as_slice() else {
            panic!("actual instantiated parameter")
        };
        assert_eq!(
            actual.entity(EntityId::Loan(*argument)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner_type
            }
        );
        let forwarded = match lender_kind {
            Lender::Generic => {
                assert!(actual.instructions.is_empty());
                *argument
            }
            Lender::Wrapper => {
                let [forward] = actual.instructions.as_slice() else {
                    panic!("wrapper must forward the original parameter, not copy or replace it")
                };
                assert_eq!(sources.slice(forward.origin.span()), Ok("view(source)"));
                let Operation::BorrowCall {
                    callee,
                    arguments,
                    source,
                } = &forward.operation
                else {
                    panic!("real inner borrow call")
                };
                assert_eq!(*source, *argument);
                assert_eq!(arguments.as_slice(), [EntityId::Loan(*argument)]);
                let inner = module.function(*callee).unwrap();
                assert!(
                    sources
                        .slice(inner.origin.span())
                        .unwrap()
                        .starts_with("fun view(")
                );
                let inner_entry = inner.block(inner.entry_block().unwrap()).unwrap();
                let [EntityId::Loan(inner_argument)] = inner_entry.parameters.as_slice() else {
                    panic!("actual borrowed String ABI")
                };
                assert_eq!(
                    inner.entity(EntityId::Loan(*inner_argument)).unwrap().ty,
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: owner_type
                    }
                );
                assert!(inner.instructions.is_empty());
                assert!(matches!(inner_entry.terminator.as_ref().unwrap().kind,
                    TerminatorKind::BorrowReturn { loan } if loan == *inner_argument));
                let [EntityId::Loan(returned)] = forward.results.as_slice() else {
                    panic!("the wrapper receives a distinct child to forward")
                };
                assert_ne!(returned, argument);
                *returned
            }
        };
        assert_eq!(
            actual.entity(EntityId::Loan(forwarded)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner_type
            }
        );
        assert!(matches!(generic_entry.terminator.as_ref().unwrap().kind,
            TerminatorKind::BorrowReturn { loan } if loan == forwarded));
        let [EntityId::Loan(child)] = generic.results.as_slice() else {
            panic!("actual borrow result")
        };
        assert_ne!(child, lender);
        let Operation::DirectCall {
            callee: observer,
            arguments,
            receiver: None,
        } = &observe.operation
        else {
            panic!("real synchronous consumer")
        };
        assert_eq!(arguments.as_slice(), [EntityId::Loan(*child)]);
        assert!(
            sources
                .slice(module.function(*observer).unwrap().origin.span())
                .unwrap()
                .starts_with("fun observe(")
        );
        for end in [end_child, end_lender] {
            assert_eq!(end.origin, observe.origin);
        }
        assert_eq!(end_child.operation, Operation::BorrowEnd { loan: *child });
        assert_eq!(end_lender.operation, Operation::BorrowEnd { loan: *lender });
        let Operation::DirectCall {
            callee: consumer,
            arguments,
            receiver: None,
        } = &consume.operation
        else {
            panic!("subsequent own delivery")
        };
        assert_eq!(sources.slice(consume.origin.span()), Ok("consume(source)"));
        assert_eq!(arguments.as_slice(), [EntityId::Value(*owner)]);
        let consumer = module.function(*consumer).unwrap();
        assert!(
            sources
                .slice(consumer.origin.span())
                .unwrap()
                .starts_with("fun consume(")
        );
        let consume_entry = consumer.block(consumer.entry_block().unwrap()).unwrap();
        let [EntityId::Value(received)] = consume_entry.parameters.as_slice() else {
            panic!("real own parameter")
        };
        let [drop] = consumer.instructions.as_slice() else {
            panic!("one retirement of transferred owner")
        };
        assert_eq!(drop.operation, Operation::Drop { owner: *received });
        assert!(matches!(&block.terminator.as_ref().unwrap().kind,
            TerminatorKind::Return { values } if values.is_empty()));
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        super::unit_field_replace_tests::link_and_run(&llvm, "", b"kept\n");
    }
}
