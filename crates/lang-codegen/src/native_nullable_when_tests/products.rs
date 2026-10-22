//! Private real frontend product contracts for the four existing nullable-When wholes.
//! No test entry, source fixture, synthesized product, or native operation lives here.

use super::{
    OwnedProducts, UnitSource,
    support::{Control, contains, root, span},
};
use lang_frontend::{
    ownership_checking::{
        LoanKind as FrontendLoanKind, UnitCallArgumentOwnershipKind, UnitLoanTarget,
        UnitValueDeliveryKind,
    },
    parser::Expression,
    type_checking::{ParameterMode, UnitCallTarget, UnitExpressionId, UnitTypeKind},
};

pub(super) fn assert_existing_call_facts(source: &UnitSource, control: &Control) {
    let OwnedProducts::Basic(typed, owned) = &source.products else {
        panic!("real Basic capability")
    };
    let typed = typed.types();
    let owned = owned.ownership();
    for call in typed
        .calls()
        .iter()
        .filter(|call| contains(control.entries[1].1, span(source, call.expression())))
    {
        let UnitCallTarget::Declaration(target) = call.target() else {
            continue;
        };
        let target_name = source.names.names().index().declarations()[target.index()].name();
        if !["observe", "consume", "delayed"].contains(&target_name) {
            continue;
        }
        assert!(call.receiver().is_none());
        let node = source
            .parsed
            .ast()
            .expressions()
            .get(call.expression().expression())
            .unwrap();
        let Expression::Call { arguments, .. } = node.payload() else {
            panic!("actual source call")
        };
        let argument = UnitExpressionId::new(control.id.source_unit(), arguments[0].value);
        let expected_root = root(source, argument);
        let proof = typed
            .non_null_use(argument)
            .expect("actual narrowed argument including transparent Group");
        assert_eq!(proof.expression(), argument);
        assert_eq!(proof.symbol(), expected_root);
        assert_eq!(
            proof.declared_type(),
            typed.expression_type(control.subject).unwrap()
        );
        let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(proof.declared_type()) else {
            panic!("nullable declaration")
        };
        assert_eq!(proof.narrowed_type(), *inner);
        let contract = owned
            .call_argument_contracts_for(call.expression())
            .find(|fact| fact.argument() == argument)
            .unwrap();
        let mapping = call.arguments()[0];
        assert_eq!(contract.call_span(), node.span());
        assert_eq!(contract.argument_span(), arguments[0].span);
        assert_eq!(contract.parameter_index(), mapping.parameter_index());
        assert_eq!(contract.parameter_type(), mapping.parameter_type());
        assert_eq!(mapping.argument_index(), 0);
        assert!(!contract.crosses_thread());
        let consume = target_name != "observe";
        assert_eq!(
            mapping.mode(),
            if consume {
                ParameterMode::Value
            } else {
                ParameterMode::Borrow
            }
        );
        assert_eq!(
            contract.kind(),
            if consume {
                UnitCallArgumentOwnershipKind::Value
            } else {
                UnitCallArgumentOwnershipKind::SharedLoan
            }
        );
        let deliveries = owned
            .value_deliveries()
            .iter()
            .filter(|fact| fact.call() == call.expression() && fact.argument() == argument)
            .collect::<Vec<_>>();
        let loans = owned
            .loans()
            .iter()
            .filter(|fact| fact.call() == call.expression() && fact.argument() == argument)
            .collect::<Vec<_>>();
        if consume {
            let [delivery] = deliveries.as_slice() else {
                panic!("one actual Value delivery")
            };
            assert!(loans.is_empty());
            assert_eq!(delivery.kind(), UnitValueDeliveryKind::Move);
            assert_eq!(delivery.span(), arguments[0].span);
            let place = delivery.place().unwrap();
            assert!(place.is_root());
            assert_eq!(place.root(), expected_root);
            assert!(delivery.parameter_span().is_some());
        } else {
            assert!(deliveries.is_empty());
            let [loan] = loans.as_slice() else {
                panic!("one actual shared argument loan")
            };
            assert_eq!(loan.kind(), FrontendLoanKind::Shared);
            assert_eq!(loan.begin_span(), arguments[0].span);
            assert_eq!(loan.end_span(), node.span());
            let UnitLoanTarget::Place(place) = loan.target() else {
                panic!("observe actual whole root")
            };
            assert!(place.is_root());
            assert_eq!(place.root(), expected_root);
        }
    }
}

pub(super) fn assert_new_products(source: &UnitSource, control: &Control) {
    use lang_frontend::{
        ownership_checking::UnitNullableWhenUseKind,
        type_checking::{NullableWhenSubjectCategory, UnitNullableWhenDomain as Domain},
    };
    let OwnedProducts::Basic(typed, owned) = &source.products else { panic!("Basic products") };
    let typed = typed.types();
    let owned = owned.ownership();
    let descriptor = typed.nullable_when(control.id).expect("real checker-produced descriptor");
    assert_eq!(descriptor.expression(), control.id);
    assert_eq!(descriptor.subject(), control.subject);
    assert_eq!(descriptor.nullable_type(), typed.expression_type(control.subject).unwrap());
    assert_eq!(typed.types().get(descriptor.nullable_type()), Some(&UnitTypeKind::Nullable(descriptor.inner_type())));
    assert_eq!(descriptor.entries().len(), 2);
    let first = &descriptor.entries()[0];
    let last = &descriptor.entries()[1];
    for (index, entry) in descriptor.entries().iter().enumerate() {
        assert_eq!(entry.index(), index);
        assert_eq!(entry.body(), control.entries[index].0);
        assert_eq!(entry.span(), control.entries[index].1);
        assert_eq!(entry.is_else(), index == 1);
    }
    assert_eq!(first.input_domain(), Domain::Nullable);
    assert_eq!(first.body_domain(), Domain::Null);
    assert_eq!(first.remaining_domain(), Domain::NonNull);
    let [alternative] = first.alternatives() else { panic!("one actual null alternative") };
    assert_eq!(source.sources.slice(alternative.span()), Ok("null"));
    assert_eq!(span(source, alternative.expression()), alternative.span());
    assert_eq!(alternative.input_domain(), Domain::Nullable);
    assert_eq!(alternative.matched_domain(), Domain::Null);
    assert_eq!(alternative.remaining_domain(), Domain::NonNull);
    assert_eq!(last.input_domain(), Domain::NonNull);
    assert_eq!(last.body_domain(), Domain::NonNull);
    assert_eq!(last.remaining_domain(), Domain::Empty);
    assert!(last.alternatives().is_empty());
    let plan = owned.nullable_when(control.id).expect("same-analysis executable owned plan");
    assert_eq!(plan.descriptor(), descriptor);
    assert_eq!(plan.entries().len(), descriptor.entries().len());
    for (entry, expected) in plan.entries().iter().zip(descriptor.entries()) {
        assert_eq!((entry.index(), entry.body()), (expected.index(), expected.body()));
        assert_eq!(entry.falls_through(), expected.falls_through());
    }
    let subject = source.parsed.ast().expressions().get(control.subject.expression()).unwrap();
    if matches!(subject.payload(), Expression::Call { .. }) {
        assert_eq!(descriptor.source_category(), NullableWhenSubjectCategory::Temporary);
        assert!(descriptor.stable_symbol().is_none() && descriptor.subject_reference().is_none() && descriptor.source_generation().is_none());
        assert!(plan.source_place().is_none() && plan.uses().is_empty());
    } else {
        assert_eq!(descriptor.source_category(), NullableWhenSubjectCategory::OwnedRoot);
        assert_eq!(descriptor.stable_symbol(), Some(root(source, control.subject)));
        assert_eq!(descriptor.subject_reference(), Some(control.subject));
        assert!(descriptor.source_generation().is_some());
        let place = plan.source_place().unwrap();
        assert!(place.is_root());
        assert_eq!(Some(place.root()), descriptor.stable_symbol());
    }
    for fact in plan.uses() {
        assert_eq!(owned.nullable_when_use(fact.argument()), Some(fact));
        let witness = fact.witness();
        assert_eq!(typed.nullable_when_use(fact.argument()), Some(witness));
        assert_eq!(witness.control(), control.id);
        assert_eq!(witness.subject(), control.subject);
        assert_eq!(witness.entry_index(), 1);
        assert_eq!(witness.body(), control.entries[1].0);
        assert_eq!(Some(witness.symbol()), descriptor.stable_symbol());
        assert_eq!(Some(witness.source_generation()), descriptor.source_generation());
        assert_eq!((witness.nullable_type(), witness.inner_type()), (descriptor.nullable_type(), descriptor.inner_type()));
        let mut current = fact.argument();
        loop {
            let chain = typed.nullable_when_use(current).expect("every real transparent Group/Name carries the same provenance");
            assert_eq!(chain.expression(), current);
            assert_eq!(chain.reference(), fact.reference());
            assert_eq!((chain.control(), chain.subject(), chain.entry_index(), chain.body(), chain.symbol(), chain.source_generation()),
                (witness.control(), witness.subject(), witness.entry_index(), witness.body(), witness.symbol(), witness.source_generation()));
            let node = source.parsed.ast().expressions().get(current.expression()).unwrap();
            match node.payload() {
                Expression::Group { expression } => current = UnitExpressionId::new(current.source_unit(), *expression),
                Expression::Name => { assert_eq!(current, fact.reference()); break; }
                _ => panic!("no invented canonical reference"),
            }
        }
        match fact.kind() {
            UnitNullableWhenUseKind::Observe => {
                assert!(fact.delivery().is_none() && fact.moved_span().is_none());
                let loan = fact.loan().unwrap();
                assert!(owned.loans().contains(loan));
                assert_eq!((loan.call(), loan.argument()), (fact.call(), fact.argument()));
                assert_eq!(loan.kind(), FrontendLoanKind::Shared);
            }
            UnitNullableWhenUseKind::Consume => {
                assert!(fact.loan().is_none());
                let delivery = fact.delivery().unwrap();
                assert!(owned.value_deliveries().contains(delivery));
                assert_eq!((delivery.call(), delivery.argument()), (fact.call(), fact.argument()));
                assert_eq!(delivery.kind(), UnitValueDeliveryKind::Move);
                assert_eq!(fact.moved_span(), Some(span(source, fact.reference())));
            }
        }
    }
    let expected_calls = typed.calls().iter().filter(|call| {
        contains(control.entries[1].1, span(source, call.expression())) &&
        matches!(call.target(), UnitCallTarget::Declaration(id) if ["observe", "consume", "delayed"].contains(&source.names.names().index().declarations()[id.index()].name()))
    }).count();
    assert_eq!(plan.uses().len(), expected_calls, "no missing or duplicate outer-argument use rows");
}
