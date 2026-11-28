use lang_frontend::{
    name_resolution::{Namespace, UnitReferenceTarget},
    parser::{AssignmentOperator, Expression},
    type_checking::{Copyability, UnitExpressionId, UnitTypeKind, standard_environments},
};

pub(super) fn assert_nullable_subject(text: &str, replaced: bool) {
    for rooted in [false, true] {
        let (sources, parsed, owned, names, typed) = super::nullable_unit::checked_with_evidence(
            text,
            "nullable-ownership.ko",
            rooted,
            standard_environments(),
        );
        assert!(
            owned.diagnostics().is_empty(),
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.deferred().is_empty(),
            "{text}: {:?}",
            owned.deferred()
        );
        let [plan] = owned.nullable_whens() else {
            panic!("one actual typed and checked when");
        };
        let typed = typed.types();
        let descriptor = plan.descriptor();
        assert_eq!(
            typed.nullable_when(descriptor.expression()),
            Some(descriptor)
        );
        let symbol = descriptor.stable_symbol().expect("original stable subject");
        assert!(descriptor.source_generation().is_some());
        let subject = descriptor.subject_reference().expect("original reference");
        assert_eq!(descriptor.subject(), subject);
        let nullable = descriptor.nullable_type();
        assert!(
            matches!(typed.types().get(nullable), Some(UnitTypeKind::Nullable(inner)) if *inner == descriptor.inner_type())
        );
        assert_eq!(typed.copyability(nullable), Copyability::Copyable);
        let expressions = parsed.ast().expressions();
        let span = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), subject.source_unit());
            expressions.get(id.expression()).unwrap().span()
        };
        let resolve = |expression| {
            let expression_span = span(expression);
            let mut matches = names.names().references().iter().filter_map(|reference| {
                if reference.source_unit() != subject.source_unit()
                    || reference.span() != expression_span
                    || reference.namespace() != Some(Namespace::Value)
                {
                    return None;
                }
                match reference.target() {
                    UnitReferenceTarget::Symbol(symbol) => Some(*symbol),
                    _ => None,
                }
            });
            let found = matches.next().expect("actual resolved reference");
            assert!(matches.next().is_none());
            found
        };
        assert_eq!(resolve(subject), symbol);
        let after = if replaced {
            let [write] = typed.assignments() else {
                panic!("the actual assignment that invalidates the proof");
            };
            assert_eq!(
                sources.slice(span(write.expression())),
                Ok("x = replacement")
            );
            assert_eq!(write.operator(), AssignmentOperator::Assign);
            assert!(write.falls_through());
            assert_eq!(resolve(write.target()), symbol);
            assert_ne!(resolve(write.value()), symbol);
            assert_eq!(write.target_type(), nullable);
            assert_eq!(typed.expression_type(write.value()), Some(nullable));

            span(write.expression()).end()
        } else {
            assert!(typed.assignments().is_empty());
            span(subject).end()
        };

        let mut reads = expressions.iter().filter_map(|(id, node)| {
            (node.span().start() > after
                && matches!(node.payload(), Expression::Name)
                && sources.slice(node.span()) == Ok("x"))
            .then_some(UnitExpressionId::new(subject.source_unit(), id))
        });
        for _ in 0..2 {
            let read = reads.next().expect("the actual x read in the nested arm");
            assert_eq!(resolve(read), symbol);
            if replaced {
                assert_eq!(typed.expression_type(read), Some(nullable));
                assert!(typed.non_null_use(read).is_none());
                assert!(typed.nullable_when_use(read).is_none());
            } else {
                let proof = typed
                    .nullable_when_use(read)
                    .expect("unmodified x stays narrowed");
                assert_eq!(proof.reference(), read);
                assert_eq!(proof.subject(), subject);
                assert_eq!(proof.control(), descriptor.expression());
                assert_eq!(proof.symbol(), symbol);
                assert_eq!(proof.entry_index(), 1);
                assert_eq!(
                    Some(proof.source_generation()),
                    descriptor.source_generation()
                );
                assert_eq!(typed.expression_type(read), Some(descriptor.inner_type()));
                assert!(typed.non_null_use(read).is_some());
            }
        }
        assert!(reads.next().is_none());
        assert!(plan.uses().is_empty());
        assert!(owned.loans().is_empty());
        assert!(owned.drops().is_empty());
        assert!(owned.validate().is_ok());
    }
}
