use lang_frontend::{
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitLoanTarget, UnitValueDeliveryKind,
        UnitValueDeliverySource,
    },
    type_checking::UnitExpressionId,
};

#[test]
fn exact_parent_borrow_survives_observe_until_its_own_last_read_and_then_moves_the_root() {
    assert_parent_transport(include_str!("fixtures/parent-transport.ko"), Finish::Move);
}

pub(super) fn assert_parent_drop_transport() {
    assert_parent_transport(
        include_str!("fixtures/parent-drop-transport.ko"),
        Finish::Drop,
    );
}

#[derive(Clone, Copy)]
enum Finish {
    Move,
    Drop,
}

fn assert_parent_transport(text: &str, finish: Finish) {
    let c = super::unit_checked(text);
    let id = c.parsed.source_id();
    assert_eq!(c.sources.source_text(id), Ok(text));
    for ((unit, unit_id), names) in [&c.memory, &c.rooted]
        .into_iter()
        .zip(c.source_units)
        .zip(&c.unit_names)
    {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(unit.deferred().is_empty(), "{:?}", unit.deferred());
        let span = |expr: UnitExpressionId| {
            assert_eq!(expr.source_unit(), unit_id);
            c.parsed
                .ast()
                .expressions()
                .get(expr.expression())
                .unwrap()
                .span()
        };
        let token = |expr| c.sources.slice(span(expr)).unwrap();
        let result = unit.borrow_results();
        let [binding] = result.bindings() else {
            panic!("exact parent binding")
        };
        let [use_fact] = result.call_uses() else {
            panic!("exact nested child operand")
        };
        let [end] = result.ends() else {
            panic!("exact parent last use")
        };
        let UnitLoanTarget::Place(root) = binding.origin() else {
            panic!("named source owner")
        };
        assert!(root.is_root());
        let resolution = names.names().source_units()[unit_id.index()].resolution();
        let symbol = |s: lang_frontend::name_resolution::UnitSymbolId| {
            assert_eq!(s.source_unit(), unit_id);
            resolution.symbols().get(s.symbol().index()).unwrap().span()
        };
        let owner = symbol(root.root());
        let parent = symbol(binding.binding());
        assert_eq!(c.sources.slice(owner), Ok("source"));
        assert_eq!(
            owner.start(),
            text.find("val source").unwrap() + "val ".len()
        );
        assert_eq!(c.sources.slice(parent), Ok("parent"));
        assert_eq!(
            parent.start(),
            text.find("val parent").unwrap() + "val ".len()
        );
        assert_eq!(token(binding.initializer()), "view(source)");
        assert!(binding.parent().is_none());
        let origin = binding
            .source_loan()
            .expect("parent initialized from a real borrowed call");
        assert_eq!(origin.call(), binding.initializer());
        assert_eq!(token(origin.argument()), "source");
        assert_eq!(use_fact.parent(), Some(binding.binding()));
        assert_eq!(use_fact.origin(), binding.origin());
        assert_eq!(token(use_fact.call()), "observe(view(parent))");
        assert_eq!(token(use_fact.argument()), "view(parent)");
        assert_eq!(use_fact.expression(), use_fact.argument());
        assert_eq!(use_fact.source_loan().call(), use_fact.argument());
        assert_eq!(token(use_fact.source_loan().argument()), "parent");
        assert_eq!(
            use_fact.end_point(),
            UnitDropPoint::CallReturn(use_fact.call())
        );
        assert_eq!(end.binding(), binding.binding());
        let UnitDropPoint::AfterStatement(last) = end.point() else {
            panic!("parent lasts through its own read")
        };
        assert_eq!(last.source_unit(), unit_id);
        let last_span = c
            .parsed
            .ast()
            .statements()
            .get(last.statement())
            .unwrap()
            .span();
        assert_eq!(c.sources.slice(last_span), Ok("println(parent)"));
        assert!(span(use_fact.call()).end() < last_span.start());
        assert!(
            unit.drops()
                .iter()
                .all(|f| !matches!(f.target(), UnitDropTarget::Temporary(_)))
        );
        if matches!(finish, Finish::Drop) {
            assert!(unit.value_deliveries().is_empty());
            let drops: Vec<_> = unit
                .drops()
                .iter()
                .filter(|f| f.target() == UnitDropTarget::Named(root.root()))
                .collect();
            let [drop] = drops.as_slice() else {
                panic!("exactly one Drop of the original named root")
            };
            assert_eq!(drop.point(), end.point());
            assert_eq!(drop.value_origin(), owner);
            assert!(!drop.is_conditional_temporary());
        } else {
            let [moved] = unit.value_deliveries() else {
                panic!("one eventual root Move")
            };
            assert_eq!(token(moved.call()), "consume(source)");
            assert_eq!(token(moved.argument()), "source");
            assert_eq!(moved.kind(), UnitValueDeliveryKind::Move);
            assert_eq!(
                moved.source(),
                &UnitValueDeliverySource::Place(root.clone())
            );
            assert!(last_span.end() < span(moved.call()).start());
            assert!(
                unit.drops()
                    .iter()
                    .all(|f| f.target() != UnitDropTarget::Named(root.root()))
            );
        }
        assert!(unit.clone().validate().is_ok());
    }
}
