//! Check the original predicate with genuine Unit facts, independently of lowering admission.
#[test]
fn same_offset_cleanup_facts_obey_the_real_scoped_refine_predicate() {
    use super::{RefineBody, RefineInputs, query_checker};
    use crate::ssa::{
        lowering_support::callable_instances::CallableArena,
        unit_lower_test_support::{analyze, parsed},
        unit_plan::UnitFunctionInstanceKey,
    };
    use lang_frontend::{
        name_resolution::SourceUnitInput,
        ownership_checking::{UnitDropPoint, UnitDropTarget},
        parser::{Expression, FunctionBody, FunctionForm, Item},
        source::SourceMap,
        type_checking::{UnitExpressionId, standard_environments},
    };

    // The two legal closure programs share byte offsets through the creation of `edit`.
    // Only the right returning branch must drop the environment before reading its source.
    let mut sources = SourceMap::new();
    let prefix = "fun run(flag: Boolean): Int { var n = 1; val edit: () -> Unit = { n = 2 }; if (flag) { edit() } else { ";
    let left_text = format!(
        "package left\n{prefix}edit(); return n }}; return n }}\nfun probe(): Int = run(true)"
    );
    // Keep the package line itself the same length to cause actual physical offset collision.
    let right_text =
        format!("package west\n{prefix}return n }}; return n }}\nfun probe(): Int = run(true)");
    let (left, left_file) = parsed(&mut sources, "left/probe.ko", &left_text);
    let (right, right_file) = parsed(&mut sources, "west/probe.ko", &right_text);
    let inputs = [
        SourceUnitInput::new("root", "west/probe.ko", right, &right_file),
        SourceUnitInput::new("root", "left/probe.ko", left, &left_file),
    ];
    let (env, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &env, &types);
    let drops = owned.ownership().drops();
    let blocking = drops
        .iter()
        .filter(|f| {
            f.value_origin().source_id() == right
                && matches!(f.point(), UnitDropPoint::BranchEntry { .. })
        })
        .collect::<Vec<_>>();
    let [blocked] = blocking.as_slice() else {
        panic!("original returning branch must have exactly one real BranchEntry obligation");
    };
    let origin = blocked.value_origin();
    assert!(
        drops.iter().any(|f| {
            let span = f.value_origin();
            span.source_id() == left
                && (span.start(), span.end()) == (origin.start(), origin.end())
                && !matches!(f.point(), UnitDropPoint::BranchEntry { .. })
        }),
        "same source-local offset must select distinct ordinary cleanup obligations"
    );
    let roster = names.names().index().source_units();
    let decls = names.names().index().declarations();
    for (source, file, should_accept) in [(left, &left_file, true), (right, &right_file, false)] {
        let unit = roster
            .iter()
            .find(|s| s.source_id() == source)
            .unwrap()
            .id();
        let (function, entry) = (
            decls
                .iter()
                .find(|d| d.source_unit() == unit && d.name() == "run")
                .unwrap(),
            decls
                .iter()
                .find(|d| d.source_unit() == unit && d.name() == "probe")
                .unwrap()
                .id(),
        );
        let item = file.ast().items().get(function.root()).unwrap();
        let Item::Function {
            form:
                FunctionForm::Explicit {
                    body: FunctionBody::Block(block),
                    ..
                },
            ..
        } = item.payload()
        else {
            panic!("original explicit block function");
        };
        let scope = item.span();
        // This closure probe has only outer simple drops. A real thunk plan is intentionally
        // unavailable, so check that no inspected origin requires nested-lambda filtering
        // and no drop requires querying the route plan for a pending temporary.
        assert!(!drops.iter().any(|f| {
            matches!(
                (f.point(), f.target()),
                (
                    UnitDropPoint::ControlTransfer(_),
                    UnitDropTarget::Temporary(_)
                )
            ) || matches!(f.target(), UnitDropTarget::Captured { .. })
        }));
        for (_, node) in file.ast().expressions().iter() {
            if matches!(node.payload(), Expression::Lambda { .. }) {
                let span = node.span();
                assert!(!drops.iter().any(|f| {
                    let o = f.value_origin();
                    o.source_id() == source
                        && span.start() <= o.start()
                        && o.end() <= span.end()
                        && span != o
                }));
            }
        }
        let typed_unit = typed.types();
        let signature = typed_unit
            .signatures()
            .declaration(function.id())
            .unwrap()
            .callable()
            .unwrap();
        assert!(signature.receiver().is_none() && signature.borrow_return().is_none());
        let mut token_owner = CallableArena::<UnitFunctionInstanceKey, UnitExpressionId>::default();
        let source_token = token_owner
            .intern_source(UnitFunctionInstanceKey::for_entry(function.id()))
            .unwrap();
        let checked = RefineInputs {
            sources: &sources,
            parsed: file,
            while_scopes: &Default::default(),
            source_unit: unit,
            names: &names,
            typed: typed_unit,
            owned: owned.ownership(),
            two_receiver_facts: None,
            single_receiver_receipts: None,
            single_receiver_facts: None,
            subset_drop_index: Some(&Default::default()),
            constant_owned: None,
            function_ids: &Default::default(),
            source_plan: &Default::default(), // not queried by these verified drop shapes
            source_token,
            references: &Default::default(),
            function_item: Some(function.root()),
            body: RefineBody::Block(*block),
            return_type: signature.return_type(),
            receiver: None,
            parameters: signature
                .parameters()
                .iter()
                .map(|p| p.symbol().unwrap())
                .collect(),
            modes: signature.parameters().iter().map(|p| p.mode()).collect(),
            span: scope,
            substitutions: &Default::default(),
            static_self: None,
            callables: &Default::default(),
            views: &Default::default(),
            callable_returns: &Default::default(),
            known_functions: &Default::default(),
            borrow_return: None,
            carrier_return: None,
        };
        let (decision, counts) = super::super::receiver_receipt::work_count::observe(|| {
            query_checker(&checked).drops_supported()
        });
        eprintln!(
            "scoped predicate expected={should_accept}, decision={decision}, counts={counts:?}"
        );
        assert_eq!(decision, should_accept);
        assert_eq!(counts.subset_drop_index_rows, 0);
        assert_eq!(counts.subset_drop_rows, 4);

        // This is *not* a public Refine success: ordinary lowering refuses the mutable
        // capture before the cleanup query. Its separately observed result must stay so.
        let (lowered, routes) = crate::ssa::refine::observe_routing(|| {
            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                &sources, &inputs, &names, &types, &typed, &owned, entry,
            )
        });
        let Err(error) = lowered else {
            panic!("public lowering must reject this capture");
        };
        eprintln!("public lowering error={error:?}, routes={routes:?}");
        assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
        assert_eq!(
            error.span.map(|s| (s.source_id(), s.start(), s.end())),
            Some((source, 79, 80))
        );
    }
}
