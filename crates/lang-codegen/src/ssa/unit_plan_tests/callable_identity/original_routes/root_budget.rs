//! Selected Unit roots and current budget behavior on exact former Single sources.
use super::*;

const ROOTS: &str = "fun <T> identity(own value: T): T = value\n\
             fun <T> recurse(own input: T): T = recurse(input)\n\
             fun <T> unused(own item: T): T = item\n\
             fun uncalled(): Unit {}\n\
             fun first(input: Int): Int = identity(input)\n\
             fun second(input: Int): Int = identity<Int>(input)\n\
             fun third(input: Long): Long = identity(input)\n\
             fun recursive(input: Int): Int = recurse(input)";

#[test]
fn original_generic_dedup_uses_only_explicitly_selected_roots() {
    use crate::ssa::unit_plan::{UnitFunctionInstanceKey, plan_unit_root_set_from_facts};
    use lang_frontend::{
        ownership_checking::owned_compilation_unit_view, type_checking::MAX_UNIT_GENERIC_INSTANCES,
    };

    const PATH: &str = "original-generic-roots.ko";
    for rooted in [false, true] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, PATH, ROOTS);
        let inputs = [if rooted {
            SourceUnitInput::new("root", PATH, source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let view =
            owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
                .expect("actual admitted Unit facts");
        let entry_names = ["first", "second", "third", "recursive"];
        let entries = entry_names.map(|name| declaration(&names, name));
        let run = |roots: &[DeclarationId]| {
            plan_unit_root_set_from_facts(
                &[&file],
                view.names(),
                view.types(),
                view.ownership(),
                roots,
                MAX_UNIT_GENERIC_INSTANCES,
                true,
            )
            .expect("four explicitly selected ordinary roots")
        };
        let plan = run(&entries);
        let mut reversed = entries;
        reversed.reverse();
        assert_eq!(
            plan,
            run(&reversed),
            "the same explicit root set is deterministic"
        );
        assert_eq!(plan.len(), 7);
        for id in entries {
            assert_eq!(
                plan.iter()
                    .filter(|instance| instance.key() == &UnitFunctionInstanceKey::for_entry(id))
                    .count(),
                1
            );
        }
        let target = |name| UnitCallableTarget::Declaration(declaration(&names, name));
        for name in ["uncalled", "unused"] {
            assert!(
                plan.iter()
                    .all(|instance| instance.key().target() != target(name))
            );
        }
        let identity_keys = plan
            .iter()
            .filter(|instance| instance.key().target() == target("identity"))
            .map(|instance| instance.key())
            .collect::<Vec<_>>();
        let int = typed.types().types().builtin(BuiltinType::Int).unwrap();
        let long = typed.types().types().builtin(BuiltinType::Long).unwrap();
        assert_eq!(identity_keys.len(), 2);
        for ty in [int, long] {
            assert_eq!(
                identity_keys
                    .iter()
                    .filter(|key| key.type_arguments() == [ty])
                    .count(),
                1
            );
        }
        let recursive = plan
            .iter()
            .filter(|instance| instance.key().target() == target("recurse"))
            .collect::<Vec<_>>();
        let [recursive] = recursive.as_slice() else {
            panic!("exactly one concrete recursive source");
        };
        assert_eq!(recursive.key().type_arguments(), [int]);
        let routes = typed
            .types()
            .calls()
            .iter()
            .filter_map(|call| {
                plan.callable_plan()
                    .call_site(recursive.source_token(), call.expression())
            })
            .collect::<Vec<_>>();
        let [recursive_call] = routes.as_slice() else {
            panic!("one real recursive call inside the concrete source");
        };
        assert_eq!(recursive_call.key(), recursive.key());
    }
}

#[test]
#[ignore = "CI-only exact 1025-call source; use the normal Unit source budget"]
fn original_repeated_callback_origin_deduplicates_before_standard_budget() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    // Exact historical generator expanded 0..=1024; keep the source stable if the Unit limit changes.
    const ORIGINAL_REPEAT_COUNT: usize = 1025;
    let calls = "helper<Int>(1, callback)\n".repeat(ORIGINAL_REPEAT_COUNT);
    let text = format!(
        "fun <T> helper(own value: T, forwarded: (Int)->Int): Unit {{}}\n\
             fun entry(): Unit {{ val callback: (Int)->Int = {{ index -> index }}\n{calls}}}"
    );
    with_original(&text, |sources, file, names, typed, owned, plan| {
        let entry = UnitCallableTarget::Declaration(declaration(names, "entry"));
        let helper = declaration(names, "helper");
        let source = plan
            .iter()
            .find(|item| item.key().target() == entry)
            .unwrap();
        let mut helpers = plan
            .iter()
            .filter(|item| item.key().target() == UnitCallableTarget::Declaration(helper));
        let callee = helpers.next().expect("one generic helper<Int>");
        assert!(helpers.next().is_none());
        assert_eq!(plan.len(), 2);
        assert_eq!(
            callee.key().type_arguments(),
            [typed.types().types().builtin(BuiltinType::Int).unwrap()]
        );
        assert!(callee.key().callable_arguments().is_empty());

        let mut observed_calls = BTreeSet::new();
        let mut observed_operands = BTreeSet::new();
        let mut origins = BTreeSet::new();
        let mut tokens = BTreeSet::new();
        for call in typed
            .types()
            .calls()
            .iter()
            .filter(|call| call.target() == UnitCallTarget::Declaration(helper))
        {
            let span = file
                .ast()
                .expressions()
                .get(call.expression().expression())
                .unwrap()
                .span();
            assert_eq!(sources.slice(span), Ok("helper<Int>(1, callback)"));
            assert!(observed_calls.insert(call.expression()));
            let route = plan
                .callable_plan()
                .call_site(source.source_token(), call.expression())
                .unwrap();
            assert_eq!(route.key(), callee.key());
            let [(slot, operand)]: [_; 1] =
                function_operands(file, typed, call).try_into().unwrap();
            assert_eq!(slot, 1);
            assert!(observed_operands.insert(operand));
            let (origin, token) = frozen_lambda(plan, owned, source.source_token(), operand);
            origins.insert(origin);
            tokens.insert(token);
        }
        assert_eq!(observed_calls.len(), ORIGINAL_REPEAT_COUNT);
        assert_eq!(observed_operands.len(), ORIGINAL_REPEAT_COUNT);
        assert_eq!(origins.len(), 1);
        assert_eq!(tokens.len(), 1);
    });
}

#[test]
#[ignore = "CI-only exact former recursive owner pressure; use the normal Unit source budget"]
fn original_recursive_borrow_view_closes_before_standard_budget() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let text = "fun relay(callback: (Int)->Int): Unit { relay({ index -> index }) }\n\
             fun entry(): Unit { relay({ index -> index }) }";
    with_original(text, |sources, file, names, typed, owned, plan| {
        let relay = declaration(names, "relay");
        let (entry, callee) = quota_instances(names, plan, relay);
        assert!(callee.key().type_arguments().is_empty());
        assert!(owned.ownership().captures().is_empty());

        let mut operands = BTreeSet::new();
        let mut origins = BTreeSet::new();
        let mut tokens = BTreeSet::new();
        let mut calls = BTreeSet::new();
        for owner in [entry, callee] {
            let routes: Vec<_> = typed
                .types()
                .calls()
                .iter()
                .filter(|call| call.target() == UnitCallTarget::Declaration(relay))
                .filter_map(|call| {
                    plan.callable_plan()
                        .call_site(owner.source_token(), call.expression())
                        .map(|route| (call, route))
                })
                .collect();
            let [(call, route)] = routes.as_slice() else {
                panic!("one original call per reachable source instance");
            };
            assert_eq!(route.key(), callee.key());
            assert_eq!(
                sources.slice(
                    file.ast()
                        .expressions()
                        .get(call.expression().expression())
                        .unwrap()
                        .span()
                ),
                Ok("relay({ index -> index })"),
            );
            assert!(calls.insert(call.expression()));
            let [(slot, operand)]: [_; 1] =
                function_operands(file, typed, call).try_into().unwrap();
            assert_eq!(slot, 0);
            assert!(operands.insert(operand));
            let (origin, token) = frozen_lambda(plan, owned, owner.source_token(), operand);
            assert!(origins.insert(origin));
            assert!(tokens.insert(token));
        }
        assert_eq!(calls.len(), 2);
    });
}

#[test]
#[ignore = "CI-only exact 1024-call source; use the normal Unit source budget"]
fn original_distinct_callback_operands_share_one_standard_budget_key() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    const ORIGINAL_COUNT: usize = 1024;
    let calls = "helper<Int>(1, { index -> index })\n".repeat(ORIGINAL_COUNT);
    let text = format!(
        "fun <T> helper(own value: T, callback: (Int)->Int): Unit {{}}\nfun entry(): Unit {{\n{calls}}}"
    );
    with_original(&text, |sources, file, names, typed, owned, plan| {
        let helper = declaration(names, "helper");
        let (entry, callee) = quota_instances(names, plan, helper);
        assert_eq!(
            callee.key().type_arguments(),
            [typed.types().types().builtin(BuiltinType::Int).unwrap()],
        );
        assert!(owned.ownership().captures().is_empty());
        let mut calls = BTreeSet::new();
        let mut operands = BTreeSet::new();
        let mut origins = BTreeSet::new();
        let mut tokens = BTreeSet::new();
        for call in typed
            .types()
            .calls()
            .iter()
            .filter(|call| call.target() == UnitCallTarget::Declaration(helper))
        {
            assert_eq!(
                sources.slice(
                    file.ast()
                        .expressions()
                        .get(call.expression().expression())
                        .unwrap()
                        .span()
                ),
                Ok("helper<Int>(1, { index -> index })"),
            );
            assert!(calls.insert(call.expression()));
            let route = plan
                .callable_plan()
                .call_site(entry.source_token(), call.expression())
                .unwrap();
            assert_eq!(route.key(), callee.key());
            let [(slot, operand)]: [_; 1] =
                function_operands(file, typed, call).try_into().unwrap();
            assert_eq!(slot, 1);
            assert!(operands.insert(operand));
            let (origin, token) = frozen_lambda(plan, owned, entry.source_token(), operand);
            assert!(origins.insert(origin));
            assert!(tokens.insert(token));
        }
        assert_eq!(calls.len(), ORIGINAL_COUNT);
    });
}

fn quota_instances<'a>(
    names: &ValidatedCompilationUnitNames,
    plan: &'a UnitInstancePlan,
    target: lang_frontend::name_resolution::DeclarationId,
) -> (
    &'a crate::ssa::unit_plan::UnitPlannedInstance,
    &'a crate::ssa::unit_plan::UnitPlannedInstance,
) {
    use crate::ssa::unit_plan::UnitFunctionInstanceKey;

    assert_eq!(plan.len(), 2);
    let entry_key = UnitFunctionInstanceKey::for_entry(declaration(names, "entry"));
    let entry = plan.iter().find(|item| item.key() == &entry_key).unwrap();
    let callee = plan
        .iter()
        .find(|item| item.key().target() == UnitCallableTarget::Declaration(target))
        .unwrap();
    assert_ne!(entry.key(), callee.key());
    assert!(callee.key().callable_arguments().is_empty());
    (entry, callee)
}

#[test]
#[ignore = "CI-only exact standard-quota positive and negative controls"]
fn standard_budget_rejects_one_extra_specialized_key() {
    use crate::ssa::unit_plan::MAX_UNIT_GENERIC_INSTANCES;
    use std::fmt::Write as _;

    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    const SLOTS: usize = 11;
    const LIMIT: usize = 1024;
    assert_eq!(MAX_UNIT_GENERIC_INSTANCES, LIMIT);
    let parameters = (0..SLOTS)
        .map(|slot| format!("T{slot}"))
        .collect::<Vec<_>>()
        .join(", ");
    let type_slots = |bits: usize| (0..SLOTS).map(move |slot| bits & (1 << slot) != 0);
    let call_text = |bits| {
        let arguments = type_slots(bits)
            .map(|long| if long { "Long" } else { "Int" })
            .collect::<Vec<_>>()
            .join(", ");
        format!("helper<{arguments}>()")
    };
    let mut text = format!("fun <{parameters}> helper(): Unit {{}}\nfun atLimit(): Unit {{\n");
    for bits in 0..LIMIT {
        writeln!(text, "{}", call_text(bits)).unwrap();
    }
    writeln!(
        text,
        "}}\nfun overLimit(): Unit {{\natLimit()\n{}\n}}",
        call_text(LIMIT)
    )
    .unwrap();

    const PATH: &str = "standard-specialized-quota.ko";
    for rooted in [false, true] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, PATH, &text);
        let inputs = [if rooted {
            SourceUnitInput::new("root", PATH, source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let helper = declaration(&names, "helper");
        let at_limit = declaration(&names, "atLimit");
        let over_limit = declaration(&names, "overLimit");
        let types = typed.types().types();
        let int = types.builtin(BuiltinType::Int).unwrap();
        let long = types.builtin(BuiltinType::Long).unwrap();
        let expected = |bits| {
            type_slots(bits)
                .map(|is_long| if is_long { long } else { int })
                .collect::<Vec<_>>()
        };
        let all_expected = (0..=LIMIT).map(expected).collect::<BTreeSet<_>>();
        assert_eq!(all_expected.len(), LIMIT + 1);
        let calls = typed
            .types()
            .calls()
            .iter()
            .filter(|call| call.target() == UnitCallTarget::Declaration(helper))
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), LIMIT + 1);
        let actual = calls
            .iter()
            .map(|call| call.instance().type_arguments().to_vec())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, all_expected, "distinct qualified source call types");
        let size = types.len();
        let plan = |entry| {
            plan_unit_instances(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
                &owned,
                entry,
            )
        };

        let admitted = plan(at_limit).expect("exactly 1024 real specialized keys fit");
        assert_eq!(admitted.len(), LIMIT + 1, "root is uncharged");
        let admitted_keys = admitted
            .iter()
            .filter(|instance| instance.key().is_specialized())
            .map(|instance| {
                assert_eq!(
                    instance.key().target(),
                    UnitCallableTarget::Declaration(helper)
                );
                assert!(instance.key().static_self().is_none());
                assert!(instance.key().callable_arguments().is_empty());
                instance.key().type_arguments().to_vec()
            })
            .collect::<BTreeSet<_>>();
        let prefix = (0..LIMIT).map(expected).collect::<BTreeSet<_>>();
        assert_eq!(admitted_keys, prefix);

        let template_span = file
            .ast()
            .items()
            .get(names.names().index().declarations()[helper.index()].root())
            .unwrap()
            .span();
        let error = plan(over_limit).expect_err("1025 real specialized keys exceed the quota");
        assert_eq!(error.kind, LoweringErrorKind::InstanceLimitExceeded);
        assert_eq!(error.span, Some(template_span));
        assert_eq!(
            types.len(),
            size,
            "planning cannot change the admitted type arena"
        );
    }
}
