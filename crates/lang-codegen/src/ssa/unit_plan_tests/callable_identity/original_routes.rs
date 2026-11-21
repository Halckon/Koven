//! Exact former Single sources through current Unit callable routing.
use super::*;

mod admission;

const SPARSE: &str = "fun helper(first: (Int)->Int, size: Int, last: (Int)->Int): Unit {}\n\
             fun entry(): Unit { val a: (Int)->Int = { index -> index }\n\
             val b: (Int)->Int = { index -> index + 1 }\n\
             helper(a, 1, b)\nhelper(last = b, size = 2, first = a)\nhelper(b, 3, a) }";

const RECURSIVE: &str = "fun leaf(callback: (Int)->Int): Unit {}\n\
             fun relay(forwarded: (Int)->Int): Unit { leaf((forwarded))\nrelay(forwarded) }\n\
             fun entry(): Unit { relay({ index -> index }) }";

const CONCRETE_OWNERS: &str = "fun leaf(callback: (Int)->Int): Unit {}\n\
             fun <T> relay(own value: T): Unit { leaf({ index -> index }) }\n\
             fun entry(): Unit { relay<Int>(1)\nrelay<Boolean>(true) }";

const FACTORY: &str = "fun leaf(callback: (Int)->Int): Unit {}\n\
             fun first(): Unit { val result = factory()\nval alias = (result)\nleaf(alias) }\n\
             fun second(): Unit { leaf(factory()) }\n\
             fun factory(): (Int)->Int = ({ index -> index })";

const ENVIRONMENTS: &str = "fun leaf(callback: (Int)->Boolean): Unit {}\n\
             fun entry(label: String, own taken: String): Unit {\n\
             leaf({ index -> index == 0 })\n\
             leaf({ index -> label == \"shared\" && index == 0 })\n\
             leaf(move { index -> taken == \"owned\" && index == 0 }) }";

const NAMED_ENVIRONMENTS: &str = "fun helper(callback: (Int)->Boolean): Unit {}\n\
             fun entry(sharedLabel: String, own ownedLabel: String): Unit {\n\
             val pointer: (Int)->Boolean = { index -> index == 0 }\n\
             val shared: (Int)->Boolean = { index -> sharedLabel == \"shared\" && index == 0 }\n\
             val ownedCallback: (Int)->Boolean = move { index -> ownedLabel == \"owned\" && index == 0 }\n\
             helper(pointer)\nhelper(shared)\nhelper(ownedCallback)\n }";

const SAME_ROOT_FACTORY: &str = "fun helper(callback: (Int)->Int): Unit {}\nfun entry(): Unit { val first = factory()\n\
             val alias = (first)\nhelper(alias)\nval second = factory()\nhelper(second) }\n\
             fun factory(): (Int)->Int = ({ index -> index })";

const EQUAL_SHAPES: &str = "fun helper(callback: (Int)->Int): Unit {}\nfun entry(): Unit { helper({ index -> index })\nhelper({ index -> index }) }";

const GENERIC_HELPER: &str = "fun helper(callback: (Int)->Int): Unit {}\nfun <T> relay(own value: T): Unit { helper({ index -> index }) }\n\
             fun entry(): Unit { relay<Int>(1)\nrelay<Boolean>(true) }";

const NAMED_SWAPPED: &str = "fun helper(first: (Int)->Int, count: Int, last: (Int)->Int): Unit {}\n\
             fun entry(): Unit { val a: (Int)->Int = { index -> index }\n\
             val b: (Int)->Int = { index -> index + 1 }\n\
             helper(a, 1, b)\nhelper(last = b, count = 2, first = a)\n\
             helper(last = a, count = 3, first = b) }";

type Inspect = fn(
    &SourceMap,
    &ParsedFile,
    &ValidatedCompilationUnitNames,
    &ValidatedCompilationUnitTypes,
    &ValidatedCompilationUnitOwnership,
    &UnitInstancePlan,
);

fn with_original(text: &str, inspect: Inspect) {
    with_selected_original(text, "entry", inspect);
}

fn with_selected_original(
    text: &str,
    entry: &str,
    inspect: impl Fn(
        &SourceMap,
        &ParsedFile,
        &ValidatedCompilationUnitNames,
        &ValidatedCompilationUnitTypes,
        &ValidatedCompilationUnitOwnership,
        &UnitInstancePlan,
    ),
) {
    const PATH: &str = "original-callable-routes.ko";
    for rooted in [false, true] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, PATH, text);
        let inputs = [if rooted {
            SourceUnitInput::new("root", PATH, source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let plan = plan_unit_instances(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, entry),
        )
        .unwrap_or_else(|error| panic!("rooted={rooted}: original source planning: {error:?}"));
        inspect(&sources, &file, &names, &typed, &owned, &plan);
    }
}

fn call_at<'a>(
    sources: &SourceMap,
    file: &ParsedFile,
    typed: &'a ValidatedCompilationUnitTypes,
    text: &str,
) -> &'a lang_frontend::type_checking::UnitCallDescriptor {
    let mut matches = typed.types().calls().iter().filter(|call| {
        let span = file
            .ast()
            .expressions()
            .get(call.expression().expression())
            .unwrap()
            .span();
        sources.slice(span) == Ok(text)
    });
    let call = matches.next().expect("exact original source call");
    assert!(matches.next().is_none());
    call
}

fn function_operands(
    file: &ParsedFile,
    typed: &ValidatedCompilationUnitTypes,
    call: &lang_frontend::type_checking::UnitCallDescriptor,
) -> Vec<(usize, UnitExpressionId)> {
    let node = file
        .ast()
        .expressions()
        .get(call.expression().expression())
        .unwrap();
    let Expression::Call { arguments, .. } = node.payload() else {
        panic!("typed call is a source Call");
    };
    call.arguments()
        .iter()
        .filter(|mapping| {
            matches!(
                typed.types().types().get(mapping.parameter_type()),
                Some(UnitTypeKind::Function { .. })
            )
        })
        .map(|mapping| {
            assert_eq!(mapping.mode(), ParameterMode::Borrow);
            (
                mapping.parameter_index(),
                UnitExpressionId::new(
                    call.expression().source_unit(),
                    arguments[mapping.argument_index()].value,
                ),
            )
        })
        .collect()
}

#[test]
fn original_sparse_named_calls_keep_operand_identity_without_specializing_borrow_slots() {
    with_original(SPARSE, |sources, file, names, typed, owned, plan| {
        let target = |name| UnitCallableTarget::Declaration(declaration(names, name));
        let entry = plan
            .iter()
            .find(|instance| instance.key().target() == target("entry"))
            .unwrap();
        let helper = plan
            .iter()
            .find(|instance| instance.key().target() == target("helper"))
            .unwrap();
        let callable_plan = plan.callable_plan();
        let mut pairs = Vec::new();
        for call in [
            "helper(a, 1, b)",
            "helper(last = b, size = 2, first = a)",
            "helper(b, 3, a)",
        ] {
            let call = call_at(sources, file, typed, call);
            let route = callable_plan
                .call_site(entry.source_token(), call.expression())
                .unwrap();
            assert_eq!(route.key(), helper.key());
            assert_eq!(
                callable_plan.source(helper.source_token()),
                Some(route.key())
            );
            // ADR-0034: borrowed operands are not part of the shared callee key.
            assert!(route.key().callable_arguments().is_empty());
            let mut operands = function_operands(file, typed, call);
            operands.sort_by_key(|(parameter, _)| *parameter);
            assert_eq!(
                operands
                    .iter()
                    .map(|(parameter, _)| *parameter)
                    .collect::<Vec<_>>(),
                [0, 2]
            );
            pairs.push(
                operands
                    .into_iter()
                    .map(|(_, operand)| {
                        let origin = owned.ownership().callable_origin(operand).unwrap();
                        let UnitCallableOrigin::Lambda(expression) = origin.origin() else {
                            panic!("actual Lambda operand origin");
                        };
                        let token = callable_plan
                            .borrowed_argument(entry.source_token(), operand)
                            .unwrap();
                        assert_eq!(
                            callable_plan.callable(token),
                            Some(&CallableKey::Lambda {
                                owner: entry.source_token(),
                                expression
                            })
                        );
                        token
                    })
                    .collect::<Vec<_>>(),
            );
        }
        assert_ne!(pairs[0][0], pairs[0][1]);
        assert_eq!(pairs[0], pairs[1]);
        assert_eq!(pairs[2], [pairs[0][1], pairs[0][0]]);
    });
}

#[test]
fn original_recursive_forwarding_keeps_parameter_origin_and_one_route() {
    with_original(RECURSIVE, |sources, file, names, typed, owned, plan| {
        let target = |name| UnitCallableTarget::Declaration(declaration(names, name));
        let entry = plan
            .iter()
            .find(|instance| instance.key().target() == target("entry"))
            .unwrap();
        let relay = plan
            .iter()
            .find(|instance| instance.key().target() == target("relay"))
            .unwrap();
        let leaf = plan
            .iter()
            .find(|instance| instance.key().target() == target("leaf"))
            .unwrap();
        assert_eq!(
            plan.iter()
                .filter(|instance| instance.key().target() == target("relay"))
                .count(),
            1
        );
        let callables = plan.callable_plan();
        let outer = call_at(sources, file, typed, "relay({ index -> index })");
        assert_eq!(
            callables
                .call_site(entry.source_token(), outer.expression())
                .unwrap()
                .key(),
            relay.key()
        );
        let [(slot, operand)]: [_; 1] = function_operands(file, typed, outer).try_into().unwrap();
        assert_eq!(slot, 0);
        let Some(UnitCallableOrigin::Lambda(expression)) = owned
            .ownership()
            .callable_origin(operand)
            .map(|fact| fact.origin())
        else {
            panic!("entry has an actual Lambda operand");
        };
        let token = callables
            .borrowed_argument(entry.source_token(), operand)
            .unwrap();
        assert_eq!(
            callables.callable(token),
            Some(&CallableKey::Lambda {
                owner: entry.source_token(),
                expression,
            })
        );

        let origins = [
            ("leaf((forwarded))", leaf.key()),
            ("relay(forwarded)", relay.key()),
        ]
        .map(|(text, key)| {
            let call = call_at(sources, file, typed, text);
            let route = callables
                .call_site(relay.source_token(), call.expression())
                .unwrap();
            assert_eq!(route.key(), key);
            assert!(route.key().callable_arguments().is_empty());
            let [(slot, operand)]: [_; 1] =
                function_operands(file, typed, call).try_into().unwrap();
            assert_eq!(slot, 0);
            let origin = owned.ownership().callable_origin(operand).unwrap().origin();
            assert!(matches!(origin, UnitCallableOrigin::Parameter(_)));
            origin
        });
        assert_eq!(
            origins[0], origins[1],
            "grouping and recursion re-borrow the same parameter"
        );
    });
}

#[test]
fn original_same_lambda_belongs_to_each_concrete_relay() {
    with_original(
        CONCRETE_OWNERS,
        |sources, file, names, typed, owned, plan| {
            let target = |name| UnitCallableTarget::Declaration(declaration(names, name));
            let entry = plan
                .iter()
                .find(|item| item.key().target() == target("entry"))
                .unwrap();
            let leaf = plan
                .iter()
                .find(|item| item.key().target() == target("leaf"))
                .unwrap();
            assert_eq!(
                plan.iter()
                    .filter(|item| item.key().target() == target("relay"))
                    .count(),
                2
            );

            let callback_call = call_at(sources, file, typed, "leaf({ index -> index })");
            let [(slot, operand)]: [_; 1] = function_operands(file, typed, callback_call)
                .try_into()
                .unwrap();
            assert_eq!(slot, 0);
            let UnitCallableOrigin::Lambda(expression) =
                owned.ownership().callable_origin(operand).unwrap().origin()
            else {
                panic!("one actual lambda operand within the generic declaration");
            };
            let callables = plan.callable_plan();
            let routes = ["relay<Int>(1)", "relay<Boolean>(true)"].map(|text| {
                let call = call_at(sources, file, typed, text);
                let route = callables
                    .call_site(entry.source_token(), call.expression())
                    .unwrap();
                let relay = plan.iter().find(|item| item.key() == route.key()).unwrap();
                assert_eq!(relay.key().target(), target("relay"));
                let leaf_route = callables
                    .call_site(relay.source_token(), callback_call.expression())
                    .unwrap();
                assert_eq!(leaf_route.key(), leaf.key());
                assert!(leaf_route.key().callable_arguments().is_empty());
                let token = callables
                    .borrowed_argument(relay.source_token(), operand)
                    .unwrap();
                assert_eq!(
                    callables.callable(token),
                    Some(&CallableKey::Lambda {
                        owner: relay.source_token(),
                        expression
                    }),
                );
                (relay.source_token(), token)
            });
            assert_ne!(routes[0].0, routes[1].0, "separate concrete generic owners");
            assert_ne!(
                routes[0].1, routes[1].1,
                "same AST does not imply the same closure token"
            );
        },
    );
}

#[test]
fn original_factory_memo_feeds_each_selected_real_caller() {
    // No artificial wrapper: each original callable is independently selected as a real root.
    for (selected, unselected, leaf_call) in [
        ("first", "second", "leaf(alias)"),
        ("second", "first", "leaf(factory())"),
    ] {
        with_selected_original(
            FACTORY,
            selected,
            |sources, file, names, typed, owned, plan| {
                let target = |name| UnitCallableTarget::Declaration(declaration(names, name));
                let caller = plan
                    .iter()
                    .find(|item| item.key().target() == target(selected))
                    .unwrap();
                assert!(
                    plan.iter()
                        .all(|item| item.key().target() != target(unselected))
                );
                let factory = plan
                    .iter()
                    .find(|item| item.key().target() == target("factory"))
                    .unwrap();
                let leaf = plan
                    .iter()
                    .find(|item| item.key().target() == target("leaf"))
                    .unwrap();
                let callables = plan.callable_plan();
                let returned = callables
                    .callable_return(factory.source_token())
                    .expect("factory return ABI");
                assert!(callables.callable_return(caller.source_token()).is_none());
                let summary = owned
                    .ownership()
                    .pointer_callable_return(target("factory"))
                    .unwrap();
                assert_eq!(returned.function_type(), summary.function_type());
                let UnitPointerCallableReturnOrigin::Lambda(expression) = summary.origin() else {
                    panic!("the selected factory actually returns a lambda");
                };
                assert_eq!(
                    callables.callable(returned.callable()),
                    Some(&CallableKey::Lambda {
                        owner: factory.source_token(),
                        expression
                    })
                );
                let call = call_at(sources, file, typed, leaf_call);
                let [(slot, operand)]: [_; 1] =
                    function_operands(file, typed, call).try_into().unwrap();
                assert_eq!(slot, 0);
                let route = callables
                    .call_site(caller.source_token(), call.expression())
                    .unwrap();
                assert_eq!(route.key(), leaf.key());
                assert!(route.key().callable_arguments().is_empty());
                assert_eq!(
                    callables.borrowed_argument(caller.source_token(), operand),
                    Some(returned.callable()),
                    "actual direct or alias caller shares this plan's factory memo"
                );
            },
        );
    }
}

#[test]
fn original_three_capture_modes_keep_distinct_operands_and_one_borrow_callee() {
    with_original(ENVIRONMENTS, |sources, file, names, typed, owned, plan| {
        let target = |name| UnitCallableTarget::Declaration(declaration(names, name));
        let entry = plan
            .iter()
            .find(|item| item.key().target() == target("entry"))
            .unwrap();
        let leaf = plan
            .iter()
            .find(|item| item.key().target() == target("leaf"))
            .unwrap();
        assert_eq!(
            plan.iter()
                .filter(|item| item.key().target() == target("leaf"))
                .count(),
            1
        );
        let callables = plan.callable_plan();
        let mut tokens = BTreeSet::new();
        for (text, expected) in [
            ("leaf({ index -> index == 0 })", None),
            (
                "leaf({ index -> label == \"shared\" && index == 0 })",
                Some(ClosureCaptureMode::Shared),
            ),
            (
                "leaf(move { index -> taken == \"owned\" && index == 0 })",
                Some(ClosureCaptureMode::Owned),
            ),
        ] {
            let call = call_at(sources, file, typed, text);
            let route = callables
                .call_site(entry.source_token(), call.expression())
                .unwrap();
            assert_eq!(route.key(), leaf.key());
            assert!(route.key().callable_arguments().is_empty());
            let [(slot, operand)]: [_; 1] =
                function_operands(file, typed, call).try_into().unwrap();
            assert_eq!(slot, 0);
            let UnitCallableOrigin::Lambda(expression) =
                owned.ownership().callable_origin(operand).unwrap().origin()
            else {
                panic!("actual lambda at {text}");
            };
            let captures = owned
                .ownership()
                .captures_of(expression)
                .map(|capture| capture.mode())
                .collect::<Vec<_>>();
            assert_eq!(captures, expected.into_iter().collect::<Vec<_>>());
            let token = callables
                .borrowed_argument(entry.source_token(), operand)
                .unwrap();
            assert!(
                tokens.insert(token),
                "pointer/shared/owned retain distinct tokens"
            );
            assert_eq!(
                callables.callable(token),
                Some(&CallableKey::Lambda {
                    owner: entry.source_token(),
                    expression
                })
            );
        }
    });
}

#[test]
fn original_named_three_environments_keep_one_fn_type_and_distinct_operands() {
    with_original(
        NAMED_ENVIRONMENTS,
        |sources, file, names, typed, owned, plan| {
            let target = |name| UnitCallableTarget::Declaration(declaration(names, name));
            let entry = plan
                .iter()
                .find(|item| item.key().target() == target("entry"))
                .unwrap();
            let mut helpers = plan
                .iter()
                .filter(|item| item.key().target() == target("helper"));
            let helper = helpers.next().expect("real borrowed callee");
            assert!(
                helpers.next().is_none(),
                "ADR-0034: one Borrow callback ABI"
            );
            let callables = plan.callable_plan();
            let mut tokens = BTreeSet::new();
            let mut types = BTreeSet::new();
            for (text, capture_mode) in [
                ("helper(pointer)", None),
                ("helper(shared)", Some(ClosureCaptureMode::Shared)),
                ("helper(ownedCallback)", Some(ClosureCaptureMode::Owned)),
            ] {
                let call = call_at(sources, file, typed, text);
                let route = callables
                    .call_site(entry.source_token(), call.expression())
                    .unwrap();
                assert_eq!(route.key(), helper.key());
                assert!(route.key().callable_arguments().is_empty());
                let [(slot, operand)]: [_; 1] =
                    function_operands(file, typed, call).try_into().unwrap();
                assert_eq!(slot, 0);
                types.insert(
                    typed
                        .types()
                        .expression_type(operand)
                        .expect("actual Fn operand type"),
                );
                let UnitCallableOrigin::Lambda(expression) =
                    owned.ownership().callable_origin(operand).unwrap().origin()
                else {
                    panic!("{text} must preserve its bound lambda origin");
                };
                assert_eq!(
                    owned
                        .ownership()
                        .captures_of(expression)
                        .map(|capture| capture.mode())
                        .collect::<Vec<_>>(),
                    capture_mode.into_iter().collect::<Vec<_>>()
                );
                let token = callables
                    .borrowed_argument(entry.source_token(), operand)
                    .unwrap();
                assert!(
                    tokens.insert(token),
                    "{text} retains its own callable token"
                );
                assert_eq!(
                    callables.callable(token),
                    Some(&CallableKey::Lambda {
                        owner: entry.source_token(),
                        expression,
                    })
                );
            }
            assert_eq!(
                types.len(),
                1,
                "one static Fn type does not erase environment identity"
            );
        },
    );
}

#[test]
fn original_same_root_factory_calls_are_distinct_but_share_one_memo() {
    with_original(
        SAME_ROOT_FACTORY,
        |sources, file, names, typed, owned, plan| {
            let target = |name| UnitCallableTarget::Declaration(declaration(names, name));
            let selected = |name| {
                let mut instances = plan
                    .iter()
                    .filter(|item| item.key().target() == target(name));
                let instance = instances.next().unwrap();
                assert!(
                    instances.next().is_none(),
                    "one planned source instance for {name}"
                );
                instance
            };
            let entry = selected("entry");
            let helper = selected("helper");
            let factory = selected("factory");
            let callables = plan.callable_plan();
            let memo = callables.callable_return(factory.source_token()).unwrap();
            let summary = owned
                .ownership()
                .pointer_callable_return(target("factory"))
                .unwrap();
            assert_eq!(memo.function_type(), summary.function_type());
            let UnitPointerCallableReturnOrigin::Lambda(expression) = summary.origin() else {
                panic!("actual factory lambda");
            };
            assert_eq!(owned.ownership().captures_of(expression).count(), 0);
            assert_eq!(
                callables.callable(memo.callable()),
                Some(&CallableKey::Lambda {
                    owner: factory.source_token(),
                    expression,
                })
            );

            let operands = ["helper(alias)", "helper(second)"].map(|text| {
                let call = call_at(sources, file, typed, text);
                let route = callables
                    .call_site(entry.source_token(), call.expression())
                    .unwrap();
                assert_eq!(route.key(), helper.key());
                assert!(route.key().callable_arguments().is_empty());
                let [(slot, operand)]: [_; 1] =
                    function_operands(file, typed, call).try_into().unwrap();
                assert_eq!(slot, 0);
                let UnitCallableOrigin::FactoryResult(origin) =
                    owned.ownership().callable_origin(operand).unwrap().origin()
                else {
                    panic!("{text} must keep its original factory evaluation");
                };
                let factory_call = typed
                    .types()
                    .call(origin)
                    .expect("actual typed factory call");
                assert_eq!(
                    factory_call.target(),
                    UnitCallTarget::Declaration(declaration(names, "factory"))
                );
                assert_eq!(
                    callables
                        .call_site(entry.source_token(), origin)
                        .unwrap()
                        .key(),
                    factory.key()
                );
                assert_eq!(
                    callables.borrowed_argument(entry.source_token(), operand),
                    Some(memo.callable())
                );
                origin
            });
            assert_ne!(
                operands[0], operands[1],
                "do not merge the two real factory evaluations"
            );
        },
    );
}

fn frozen_lambda(
    plan: &UnitInstancePlan,
    owned: &ValidatedCompilationUnitOwnership,
    owner: crate::ssa::lowering_support::callable_instances::SourceToken,
    operand: UnitExpressionId,
) -> (
    UnitExpressionId,
    crate::ssa::lowering_support::callable_instances::CallableToken,
) {
    let UnitCallableOrigin::Lambda(expression) =
        owned.ownership().callable_origin(operand).unwrap().origin()
    else {
        panic!("real original lambda operand");
    };
    let callables = plan.callable_plan();
    let token = callables.borrowed_argument(owner, operand).unwrap();
    assert_eq!(
        callables.callable(token),
        Some(&CallableKey::Lambda { owner, expression })
    );
    (expression, token)
}

#[test]
fn original_equal_shaped_lambdas_keep_distinct_callsite_origins() {
    with_original(EQUAL_SHAPES, |sources, file, names, typed, owned, plan| {
        let entry = UnitCallableTarget::Declaration(declaration(names, "entry"));
        let helper = declaration(names, "helper");
        let mut instances = plan
            .iter()
            .filter(|item| item.key().target() == UnitCallableTarget::Declaration(helper));
        let callee = instances.next().expect("one borrowed helper");
        assert!(instances.next().is_none());
        let owner = plan
            .iter()
            .find(|item| item.key().target() == entry)
            .unwrap()
            .source_token();
        let mut observations = Vec::new();
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
            assert_eq!(sources.slice(span), Ok("helper({ index -> index })"));
            let route = plan
                .callable_plan()
                .call_site(owner, call.expression())
                .unwrap();
            assert_eq!(route.key(), callee.key());
            assert!(route.key().callable_arguments().is_empty());
            let [(slot, operand)]: [_; 1] =
                function_operands(file, typed, call).try_into().unwrap();
            assert_eq!(slot, 0);
            observations.push((
                call.expression(),
                frozen_lambda(plan, owned, owner, operand),
            ));
        }
        let [first, second] = observations.as_slice() else {
            panic!("exactly two real same-spelled calls");
        };
        assert_ne!(first.0, second.0);
        assert_ne!(
            first.1.0, second.1.0,
            "equal bodies still have distinct AST nodes"
        );
        assert_ne!(
            first.1.1, second.1.1,
            "equal bodies still carry different tokens"
        );
    });
}

#[test]
fn original_generic_helper_routes_keep_owner_qualified_same_ast_lambda() {
    with_original(
        GENERIC_HELPER,
        |sources, file, names, typed, owned, plan| {
            let target = |name| UnitCallableTarget::Declaration(declaration(names, name));
            let entry = plan
                .iter()
                .find(|item| item.key().target() == target("entry"))
                .unwrap();
            let mut helpers = plan
                .iter()
                .filter(|item| item.key().target() == target("helper"));
            let helper = helpers.next().expect("one borrowed helper");
            assert!(helpers.next().is_none());
            assert_eq!(
                plan.iter()
                    .filter(|item| item.key().target() == target("relay"))
                    .count(),
                2
            );
            let actual = call_at(sources, file, typed, "helper({ index -> index })");
            let [(slot, operand)]: [_; 1] =
                function_operands(file, typed, actual).try_into().unwrap();
            assert_eq!(slot, 0);
            let callables = plan.callable_plan();
            let result = ["relay<Int>(1)", "relay<Boolean>(true)"].map(|text| {
                let call = call_at(sources, file, typed, text);
                let key = callables
                    .call_site(entry.source_token(), call.expression())
                    .unwrap()
                    .key();
                let relay = plan.iter().find(|item| item.key() == key).unwrap();
                assert_eq!(relay.key().target(), target("relay"));
                let owner = relay.source_token();
                let helper_route = callables.call_site(owner, actual.expression()).unwrap();
                assert_eq!(helper_route.key(), helper.key());
                assert!(helper_route.key().callable_arguments().is_empty());
                (owner, frozen_lambda(plan, owned, owner, operand))
            });
            assert_ne!(result[0].0, result[1].0);
            assert_eq!(result[0].1.0, result[1].1.0, "same lambda AST");
            assert_ne!(
                result[0].1.1, result[1].1.1,
                "different concrete relay owners"
            );
        },
    );
}

#[test]
fn original_named_swapped_calls_keep_source_order_and_declaration_slots() {
    with_original(NAMED_SWAPPED, |sources, file, names, typed, owned, plan| {
        let target = |name| UnitCallableTarget::Declaration(declaration(names, name));
        let entry = plan
            .iter()
            .find(|item| item.key().target() == target("entry"))
            .unwrap();
        let mut helpers = plan
            .iter()
            .filter(|item| item.key().target() == target("helper"));
        let helper = helpers.next().expect("one borrowed helper");
        assert!(helpers.next().is_none());
        let callables = plan.callable_plan();
        let routes = [
            "helper(a, 1, b)",
            "helper(last = b, count = 2, first = a)",
            "helper(last = a, count = 3, first = b)",
        ]
        .map(|text| {
            let call = call_at(sources, file, typed, text);
            let mapping = call
                .arguments()
                .iter()
                .map(|argument| (argument.argument_index(), argument.parameter_index()))
                .collect::<Vec<_>>();
            assert_eq!(
                mapping,
                if text.contains("last =") {
                    [(0, 2), (1, 1), (2, 0)]
                } else {
                    [(0, 0), (1, 1), (2, 2)]
                }
            );
            let route = callables
                .call_site(entry.source_token(), call.expression())
                .unwrap();
            assert_eq!(route.key(), helper.key());
            assert!(route.key().callable_arguments().is_empty());
            let mut operands = function_operands(file, typed, call);
            operands.sort_by_key(|(slot, _)| *slot);
            let [(first, a), (last, b)]: [_; 2] = operands.try_into().unwrap();
            assert_eq!([first, last], [0, 2]);
            [a, b].map(|operand| frozen_lambda(plan, owned, entry.source_token(), operand).1)
        });
        assert_ne!(routes[0][0], routes[0][1]);
        assert_eq!(routes[0], routes[1]);
        assert_eq!(routes[2], [routes[0][1], routes[0][0]]);
    });
}
