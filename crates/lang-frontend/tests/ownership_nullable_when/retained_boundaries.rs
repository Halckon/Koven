//! Original Single/Unit contracts whose distinct downstream limits remain explicit.
use super::{checked, nullable_unit};
use lang_frontend::{
    ownership_checking::{
        LoanKind, NullableWhenExtractionKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
    },
    type_checking::{UnitExpressionId, UnitNullableWhenDomain, standard_environments},
};

pub(super) fn named_nullable_read_borrows_without_extracting() {
    use lang_frontend::ownership_checking::UnitNullableWhenUseKind;
    use lang_frontend::type_checking::NullableWhenSubjectCategory;

    let text = include_str!("fixtures/named-borrow-read.ko");
    let old = checked(text);
    assert!(old.diagnostics().is_empty(), "{:?}", old.diagnostics());
    let [plan] = old.nullable_whens() else {
        panic!("one original nullable when")
    };
    assert!(plan.extractions().is_empty());
    for rooted in [false, true] {
        let (sources, parsed, owned, _, typed) = nullable_unit::checked_with_evidence(
            text,
            "named-borrow-read.ko",
            rooted,
            standard_environments(),
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let [plan] = owned.nullable_whens() else {
            panic!("one actual Unit plan")
        };
        let descriptor = plan.descriptor();
        let [_, non_null] = descriptor.entries() else {
            panic!("actual non-null edge")
        };
        assert_eq!(
            descriptor.source_category(),
            NullableWhenSubjectCategory::BorrowRoot
        );
        assert_eq!(non_null.body_domain(), UnitNullableWhenDomain::NonNull);
        let [witness] = typed.types().nullable_when_uses() else {
            panic!("one real typed use")
        };
        let [use_fact] = plan.uses() else {
            panic!("one real ownership action")
        };
        assert_eq!(use_fact.witness(), *witness);
        assert_eq!(use_fact.kind(), UnitNullableWhenUseKind::Observe);
        assert_eq!(use_fact.delivery(), None);
        assert_eq!(use_fact.moved_span(), None);
        assert_eq!(
            (witness.control(), witness.subject()),
            (descriptor.expression(), descriptor.subject())
        );
        assert_eq!(
            (witness.entry_index(), witness.body()),
            (1, non_null.body())
        );
        assert_eq!(
            Some(witness.source_generation()),
            descriptor.source_generation()
        );
        let root = plan.source_place().unwrap();
        assert_eq!(Some(root.root()), descriptor.stable_symbol());
        assert_eq!(root.root(), witness.symbol());
        let [loan] = owned.loans() else {
            panic!("one actual Shared loan")
        };
        assert_eq!(use_fact.loan(), Some(loan));
        assert_eq!(
            (loan.call(), loan.argument()),
            (use_fact.call(), use_fact.argument())
        );
        assert_eq!(loan.kind(), LoanKind::Shared);
        assert_eq!(loan.target(), &UnitLoanTarget::Place(root.clone()));
        assert_eq!(use_fact.reference(), loan.argument());
        let ast_span = |id: UnitExpressionId| {
            assert_eq!(id.source_unit(), descriptor.expression().source_unit());
            parsed
                .ast()
                .expressions()
                .get(id.expression())
                .unwrap()
                .span()
        };
        assert_eq!(loan.begin_span(), ast_span(loan.argument()));
        assert_eq!(loan.end_span(), ast_span(loan.call()));
        assert_eq!(sources.slice(loan.begin_span()), Ok("x"));
        assert_eq!(sources.slice(loan.end_span()), Ok("read(x)"));
        assert_eq!(sources.slice(loan.parameter_span().unwrap()), Ok("x: Node"));
        assert!(owned.value_deliveries().is_empty());
        assert!(owned.drops().is_empty());
        assert!(owned.rc_effects().is_empty());
        assert!(owned.validate().is_ok());
    }
}

pub(super) fn owned_nullable_node_moves_the_original_root() {
    let text = include_str!("fixtures/move-node-root.ko");
    let legacy = checked(text);
    assert!(legacy.diagnostics().is_empty());
    assert!(legacy.rc_effects().is_empty());
    let [legacy_plan] = legacy.nullable_whens() else {
        panic!("the original nullable when");
    };
    let [extraction] = legacy_plan.extractions() else {
        panic!("only one original payload consume");
    };
    assert_eq!(extraction.kind(), NullableWhenExtractionKind::Consume);
    assert_eq!((extraction.entry(), extraction.alternative()), (1, None));
    for rooted in [false, true] {
        use lang_frontend::parser::Expression;
        use lang_frontend::type_checking::{Copyability, UnitNullableWhenDomain};

        let (sources, parsed, owned, _, typed) = nullable_unit::checked_with_evidence(
            text,
            "move-node-root.ko",
            rooted,
            standard_environments(),
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let [plan] = owned.nullable_whens() else {
            panic!("one real Unit plan");
        };
        let descriptor = plan.descriptor();
        let [null, non_null] = descriptor.entries() else {
            panic!("original complementary entries");
        };
        assert_eq!(null.body_domain(), UnitNullableWhenDomain::Null);
        assert_eq!(non_null.body_domain(), UnitNullableWhenDomain::NonNull);
        assert!(non_null.alternatives().is_empty());
        let typed = typed.types();
        let [proof] = typed.nullable_when_uses() else {
            panic!("only the original else body extracts the payload");
        };
        assert_eq!(proof.control(), descriptor.expression());
        assert_eq!(proof.subject(), descriptor.subject());
        assert_ne!(proof.reference(), proof.subject());
        assert_eq!(proof.symbol(), plan.source_place().unwrap().root());
        assert_eq!(Some(proof.symbol()), descriptor.stable_symbol());
        assert_eq!(
            Some(proof.source_generation()),
            descriptor.source_generation()
        );
        assert_eq!((proof.entry_index(), proof.body()), (1, non_null.body()));
        let exprs = parsed.ast().expressions();
        let reference = exprs.get(proof.reference().expression()).unwrap();
        assert!(matches!(reference.payload(), Expression::Name));
        assert_eq!(sources.slice(reference.span()), Ok("x"));
        assert_eq!(
            typed.expression_type(proof.expression()),
            Some(proof.inner_type())
        );
        assert_eq!(typed.copyability(proof.inner_type()), Copyability::MoveOnly);
        let [drop] = owned.drops() else {
            panic!(
                "only the null edge retires the empty root: {:?}",
                owned.drops()
            )
        };
        assert_eq!(
            drop.point(),
            UnitDropPoint::BranchExit {
                control: descriptor.expression(),
                branch: 0
            }
        );
        assert_eq!(drop.target(), UnitDropTarget::Named(proof.symbol()));
        assert_eq!(sources.slice(drop.value_origin()), Ok("x"));
        assert!(owned.loans().is_empty());
        assert!(owned.rc_effects().is_empty());
        assert!(owned.validate().is_ok());
    }
}
