use lang_frontend::{
    name_resolution::{SourceUnitInput, UnitSymbolId},
    ownership_checking::{ElementIndexIdentity, LoanKind, UnitLoanTarget},
    parser::Expression,
    type_checking::{
        NullableWhenSubjectCategory, UnitNullableWhenDomain, complete_compilation_unit_types,
        standard_environments,
    },
};

pub(super) fn assert_unit_projection(text: &str, field: bool) {
    let (path, syntax, root_name) = if field {
        ("unstable-field.ko", "h.item", "h")
    } else {
        ("unstable-index.ko", "xs[0]", "xs")
    };
    for rooted in [false, true] {
        let environments = standard_environments();
        let type_environment = environments.1.clone();
        let (sources, parsed, owned, names, typed) =
            super::nullable_unit::checked_with_evidence(text, path, rooted, environments);
        let source = names.names().index().source_units()[0].source_id();
        let input = if rooted {
            SourceUnitInput::new("root", path, source, &parsed)
        } else {
            SourceUnitInput::memory_document(source, &parsed)
        };
        complete_compilation_unit_types(
            &sources,
            &[input],
            &names,
            &type_environment,
            typed.types(),
        )
        .expect("complete Unit type admission for the unchanged source");
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let [plan] = owned.nullable_whens() else {
            panic!("one exact nullable plan")
        };
        let descriptor = plan.descriptor();
        assert_eq!(
            descriptor.source_category(),
            if field {
                NullableWhenSubjectCategory::OrdinaryField
            } else {
                NullableWhenSubjectCategory::ContainerElement
            }
        );
        let [null, present] = descriptor.entries() else {
            panic!("the original two entries")
        };
        assert_eq!(
            (null.body_domain(), present.body_domain()),
            (
                UnitNullableWhenDomain::Null,
                UnitNullableWhenDomain::NonNull
            )
        );
        assert_eq!(descriptor.stable_symbol(), None);
        assert_eq!(descriptor.subject_reference(), None);
        assert_eq!(descriptor.source_generation(), None);
        let source_place = plan.source_place().unwrap();
        let unit = descriptor.expression().source_unit();
        let symbol_text = |id: UnitSymbolId| {
            assert_eq!(id.source_unit(), unit);
            let definitions = names.names().source_units()[unit.index()]
                .resolution()
                .symbols();
            sources
                .slice(definitions.get(id.symbol().index()).unwrap().span())
                .unwrap()
        };
        assert_eq!(symbol_text(source_place.root()), root_name);
        if field {
            let [item] = source_place.fields() else {
                panic!("actual item field path")
            };
            assert_eq!(symbol_text(*item), "item");
            assert!(source_place.elements().is_empty());
        } else {
            assert!(source_place.fields().is_empty());
            assert_eq!(source_place.elements(), [ElementIndexIdentity::Known(0)]);
        }

        let [loan] = owned.loans() else {
            panic!("one Shared borrow of the second projection")
        };
        assert_eq!(loan.target(), &UnitLoanTarget::Place(source_place.clone()));
        assert_eq!(loan.kind(), LoanKind::Shared);
        let subject = descriptor.subject();
        let repeated = loan.argument();
        assert_ne!(subject, repeated);
        let exprs = parsed.ast().expressions();
        for id in [subject, repeated] {
            assert_eq!(id.source_unit(), unit);
            let expression = exprs.get(id.expression()).unwrap();
            assert_eq!(sources.slice(expression.span()), Ok(syntax));
            assert!(if field {
                matches!(expression.payload(), Expression::Member { .. })
            } else {
                matches!(expression.payload(), Expression::Index { .. })
            });
            assert_eq!(
                typed.types().expression_type(id),
                Some(descriptor.nullable_type())
            );
        }
        assert_eq!(
            loan.begin_span(),
            exprs.get(repeated.expression()).unwrap().span()
        );
        let call = loan.call();
        assert_eq!(call.source_unit(), unit);
        let call = exprs.get(call.expression()).unwrap();
        let Expression::Call { arguments, .. } = call.payload() else {
            panic!("actual read call")
        };
        assert_eq!(arguments.len(), 1);
        assert_eq!(arguments[0].value, repeated.expression());
        assert_eq!(loan.end_span(), call.span());
        assert_eq!(
            sources.slice(call.span()).unwrap(),
            format!("read({syntax})")
        );
        assert_eq!(
            sources.slice(loan.parameter_span().unwrap()),
            Ok("x: Node?")
        );
        assert!(typed.types().nullable_when_uses().is_empty());
        assert!(plan.uses().is_empty());
        assert!(owned.value_deliveries().is_empty());
        assert!(owned.drops().is_empty());
        assert!(owned.validate().is_ok());
    }
}
