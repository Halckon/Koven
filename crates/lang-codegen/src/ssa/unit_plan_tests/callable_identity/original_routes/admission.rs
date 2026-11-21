//! Exact former Single callback admission cases through the Unit planner.
use super::*;

const FORMER_DEFERRED: &str = "fun helper(callback: (Int)->Int): Unit {}\nfun identity(index: Int): Int = index\n\
             fun entry(): Unit { val annotated: (Int)->Int = identity\nhelper(annotated) }";

const UNKNOWN_JOIN: &str = "fun helper(callback: (Int)->Int): Unit {}\nfun entry(flag: Boolean): Unit {\n\
             val first: (Int)->Int = { index -> index }\n\
             val second: (Int)->Int = { index -> index + 1 }\n\
             val selected: (Int)->Int = if(flag) { first } else { second }\nhelper(selected) }";

#[test]
fn original_named_function_is_completed_and_demands_its_selected_body() {
    with_original(
        FORMER_DEFERRED,
        |sources, file, names, typed, owned, plan| {
            let target = |name| UnitCallableTarget::Declaration(declaration(names, name));
            let instance = |name| {
                let mut instances = plan
                    .iter()
                    .filter(|item| item.key().target() == target(name));
                let exact = instances.next().expect("required source body");
                assert!(instances.next().is_none(), "one selected body for {name}");
                exact
            };
            let entry = instance("entry");
            let identity = instance("identity");
            let helper = instance("helper");
            let call = call_at(sources, file, typed, "helper(annotated)");
            let [(slot, operand)]: [_; 1] =
                function_operands(file, typed, call).try_into().unwrap();
            assert_eq!(slot, 0);
            assert_eq!(
                owned
                    .ownership()
                    .callable_origin(operand)
                    .map(|fact| fact.origin()),
                Some(UnitCallableOrigin::KnownFunction(target("identity")))
            );

            // Unit completes the original initializer itself, not just its annotated local.
            let initializers = file
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, expression)| {
                    (sources.slice(expression.span()) == Ok("identity"))
                        .then(|| UnitExpressionId::new(operand.source_unit(), id))
                })
                .collect::<Vec<_>>();
            let [initializer] = initializers.as_slice() else {
                panic!("one actual identity initializer");
            };
            let canonical = typed.types().expression_type(*initializer).unwrap();
            assert!(matches!(
                typed.types().types().get(canonical),
                Some(UnitTypeKind::Function { .. })
            ));
            assert_eq!(typed.types().expression_type(operand), Some(canonical));
            let callables = plan.callable_plan();
            let token = callables
                .borrowed_argument(entry.source_token(), operand)
                .unwrap();
            assert_eq!(
                callables.callable(token),
                Some(&CallableKey::KnownFunction {
                    function: identity.source_token(),
                })
            );
            let route = callables
                .call_site(entry.source_token(), call.expression())
                .unwrap();
            assert_eq!(route.key(), helper.key());
            assert!(route.key().callable_arguments().is_empty());
        },
    );
}

#[test]
fn original_unknown_join_remains_rejected_at_the_selected_operand() {
    const PATH: &str = "original-unknown-callback.ko";
    for rooted in [false, true] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, PATH, UNKNOWN_JOIN);
        let inputs = [if rooted {
            SourceUnitInput::new("root", PATH, source, &file)
        } else {
            SourceUnitInput::memory_document(source, &file)
        }];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        let call = call_at(&sources, &file, &typed, "helper(selected)");
        assert_eq!(
            call.target(),
            UnitCallTarget::Declaration(declaration(&names, "helper"))
        );
        let [(slot, operand)]: [_; 1] = function_operands(&file, &typed, call).try_into().unwrap();
        assert_eq!(slot, 0);
        assert!(owned.ownership().callable_origin(operand).is_none());
        let span = file
            .ast()
            .expressions()
            .get(operand.expression())
            .unwrap()
            .span();
        assert_eq!(sources.slice(span), Ok("selected"));

        // Legal frontend types do not give permission to guess one environment at the join.
        let arena = typed.types().types().len();
        let error = plan_unit_instances(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "entry"),
        )
        .expect_err("the original ambiguous source cannot supply a concrete callable view");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        assert_eq!(error.span, Some(span));
        assert_eq!(typed.types().types().len(), arena);
    }
}
