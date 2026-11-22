use lang_frontend::{
    ownership_checking::{
        LoanKind, UnitLoanTarget, UnitValueDeliveryKind, UnitValueDeliverySource,
    },
    parser::Expression,
    type_checking::{Copyability, UnitExpressionId, standard_environments},
};

pub(super) fn assert_copy_while_shared(text: &str) {
    for rooted in [false, true] {
        let (sources, parsed, owned, _, typed) = super::nullable_unit::checked_with_evidence(
            text,
            "copy-while-shared.ko",
            rooted,
            standard_environments(),
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let [when] = owned.nullable_whens() else {
            panic!("the original nullable when");
        };
        let control = when.descriptor().expression();
        let source = control.source_unit();
        let exprs = parsed.ast().expressions();
        let (call, first_arg) = exprs
            .iter()
            .find_map(|(id, node)| {
                let Expression::Call { arguments, .. } = node.payload() else {
                    return None;
                };
                let [first, second] = arguments.as_slice() else {
                    return None;
                };
                (second.value == control.expression()).then_some((
                    UnitExpressionId::new(source, id),
                    UnitExpressionId::new(source, first.value),
                ))
            })
            .expect("actual enclosing borrowed/owned call");
        let span = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), source);
            exprs.get(id.expression()).unwrap().span()
        };
        assert_eq!(sources.slice(span(first_arg)), Ok("x"));
        let [loan] = owned.loans() else {
            panic!("one real Shared outer call operand");
        };
        assert_eq!((loan.call(), loan.argument()), (call, first_arg));
        assert_eq!(loan.kind(), LoanKind::Shared);
        assert_eq!(loan.begin_span(), span(first_arg));
        assert_eq!(loan.end_span(), span(call));
        assert_eq!(
            loan.target(),
            &UnitLoanTarget::Place(when.source_place().unwrap().clone())
        );
        assert_eq!(
            Some(when.source_place().unwrap().root()),
            when.descriptor().stable_symbol()
        );

        let typed = typed.types();
        let [proof] = typed.nullable_when_uses() else {
            panic!("actual non-null inner use in the else arm");
        };
        assert_eq!(proof.control(), control);
        assert_eq!(proof.subject(), when.descriptor().subject());
        assert_eq!(proof.symbol(), when.source_place().unwrap().root());
        assert_eq!(proof.entry_index(), 1);
        assert_eq!(proof.body(), when.entries()[1].body());
        assert_eq!(
            Some(proof.source_generation()),
            when.descriptor().source_generation()
        );
        assert_eq!(sources.slice(span(proof.reference())), Ok("x"));
        assert!(span(loan.argument()).end() < span(proof.reference()).start());
        assert!(span(proof.reference()).end() < span(loan.call()).end());
        assert_eq!(
            typed.expression_type(proof.expression()),
            Some(proof.inner_type())
        );
        assert_eq!(typed.copyability(proof.inner_type()), Copyability::Copyable);

        let [delivery] = owned.value_deliveries() else {
            panic!("only the when result is delivered by Value");
        };
        assert_eq!((delivery.call(), delivery.argument()), (call, control));
        assert_eq!(delivery.kind(), UnitValueDeliveryKind::Temporary);
        assert_eq!(
            delivery.source(),
            &UnitValueDeliverySource::Temporary(control)
        );
        assert!(owned.drops().is_empty());
        assert!(owned.validate().is_ok());
    }
}

pub(super) fn assert_body_copy(text: &str) {
    for rooted in [false, true] {
        let (sources, parsed, owned, _, typed) = super::nullable_unit::checked_with_evidence(
            text,
            "copy-when-body.ko",
            rooted,
            standard_environments(),
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let [when] = owned.nullable_whens() else {
            panic!("original stable nullable when");
        };
        let descriptor = when.descriptor();
        let typed = typed.types();
        let [proof] = typed.nullable_when_uses() else {
            panic!("one actual typed use");
        };
        assert_eq!(proof.control(), descriptor.expression());
        assert_eq!(proof.subject(), descriptor.subject());
        assert_eq!(Some(proof.symbol()), descriptor.stable_symbol());
        assert_eq!(proof.symbol(), when.source_place().unwrap().root());
        assert_eq!(
            Some(proof.source_generation()),
            descriptor.source_generation()
        );
        assert_eq!(proof.entry_index(), 1);
        assert_eq!(proof.body(), when.entries()[1].body());
        let [_, non_null] = descriptor.entries() else {
            panic!("actual null then else arms");
        };
        assert!(non_null.alternatives().is_empty());
        let reference = proof.reference();
        assert_eq!(proof.expression(), reference);
        assert_ne!(reference, descriptor.subject());
        assert_eq!(reference.source_unit(), descriptor.subject().source_unit());
        let exprs = parsed.ast().expressions();
        let node = exprs.get(reference.expression()).unwrap();
        assert!(matches!(node.payload(), Expression::Name));
        assert_eq!(sources.slice(node.span()), Ok("x"));
        let subject = exprs.get(descriptor.subject().expression()).unwrap();
        assert!(subject.span().end() < node.span().start());
        assert_eq!(typed.expression_type(reference), Some(proof.inner_type()));
        assert_eq!(typed.copyability(proof.inner_type()), Copyability::Copyable);
        assert!(owned.loans().is_empty());
        assert!(owned.drops().is_empty());
        assert!(owned.validate().is_ok());
    }
}
