//! Exact former Single sources through current Unit callable routing.
use super::*;

const SPARSE: &str = "fun helper(first: (Int)->Int, size: Int, last: (Int)->Int): Unit {}\n\
             fun entry(): Unit { val a: (Int)->Int = { index -> index }\n\
             val b: (Int)->Int = { index -> index + 1 }\n\
             helper(a, 1, b)\nhelper(last = b, size = 2, first = a)\nhelper(b, 3, a) }";

const RECURSIVE: &str = "fun leaf(callback: (Int)->Int): Unit {}\n\
             fun relay(forwarded: (Int)->Int): Unit { leaf((forwarded))\nrelay(forwarded) }\n\
             fun entry(): Unit { relay({ index -> index }) }";

type Inspect = fn(
    &SourceMap,
    &ParsedFile,
    &ValidatedCompilationUnitNames,
    &ValidatedCompilationUnitTypes,
    &ValidatedCompilationUnitOwnership,
    &UnitInstancePlan,
);

fn with_original(text: &str, inspect: Inspect) {
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
            declaration(&names, "entry"),
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
