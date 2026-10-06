//! Callback identity belongs to a concrete source instance, never just its Function signature.

use super::*;
use crate::ssa::lowering_support::callable_instances::CallableKey;
use lang_frontend::{
    name_resolution::SourceUnitId,
    ownership_checking::{ClosureCaptureMode, UnitCallableOrigin, UnitPointerCallableReturnOrigin},
    parser::Expression,
    type_checking::{ParameterMode, UnitExpressionId},
};

struct Fixture {
    sources: SourceMap,
    provider_source: SourceId,
    provider: ParsedFile,
    caller_source: SourceId,
    caller: ParsedFile,
    names: ValidatedCompilationUnitNames,
    typed: ValidatedCompilationUnitTypes,
    owned: ValidatedCompilationUnitOwnership,
    environment: TypeEnvironment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CallbackUse {
    parameter: usize,
    expression: UnitExpressionId,
    origin: Option<UnitCallableOrigin>,
}

impl Fixture {
    /// Complete all frontend gates before any planner identity assertion.
    fn new(provider: &str, caller: &str) -> Self {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(&mut sources, "q/api.ko", provider);
        let (caller_source, caller) = parsed(&mut sources, "p/use.ko", caller);
        let inputs = [
            SourceUnitInput::new("root", "q/api.ko", provider_source, &provider),
            SourceUnitInput::new("root", "p/use.ko", caller_source, &caller),
        ];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
        Self {
            sources,
            provider_source,
            provider,
            caller_source,
            caller,
            names,
            typed,
            owned,
            environment,
        }
    }

    fn inputs(&self) -> [SourceUnitInput<'_>; 2] {
        [
            SourceUnitInput::new("root", "q/api.ko", self.provider_source, &self.provider),
            SourceUnitInput::new("root", "p/use.ko", self.caller_source, &self.caller),
        ]
    }

    fn result(&self) -> Result<UnitInstancePlan, super::super::LoweringError> {
        plan_unit_instances(
            &self.sources,
            &self.inputs(),
            &self.names,
            &self.environment,
            &self.typed,
            &self.owned,
            declaration(&self.names, "entry"),
        )
    }

    fn plan(&self) -> UnitInstancePlan {
        let arena = self.typed.types().types().len();
        let result = self.result().expect("legal callable instance plan");
        assert_eq!(
            self.typed.types().types().len(),
            arena,
            "backend only reads canonical types"
        );
        result
    }

    fn reversed_plan(&self) -> UnitInstancePlan {
        let inputs = self.inputs();
        let reversed = [inputs[1], inputs[0]];
        let (name_environment, environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&self.sources, &reversed, &name_environment, &environment);
        plan_unit_instances(
            &self.sources,
            &reversed,
            &names,
            &environment,
            &typed,
            &owned,
            declaration(&names, "entry"),
        )
        .expect("fresh reversed frontend chain plans the same identities")
    }

    fn target(&self, name: &str) -> UnitCallableTarget {
        UnitCallableTarget::Declaration(declaration(&self.names, name))
    }

    fn counts(&self, plan: &UnitInstancePlan, names: &[&str]) -> Vec<usize> {
        names
            .iter()
            .map(|name| {
                plan.iter()
                    .filter(|instance| instance.key().target() == self.target(name))
                    .count()
            })
            .collect()
    }

    fn file(&self, unit: SourceUnitId) -> &ParsedFile {
        let source = self.names.names().index().source_units()[unit.index()].source_id();
        if source == self.provider_source {
            &self.provider
        } else {
            assert_eq!(source, self.caller_source);
            &self.caller
        }
    }

    /// Follow actual typed argument-to-declaration mappings; AST only locates each operand.
    fn callback_uses(&self, target: &str) -> Vec<Vec<CallbackUse>> {
        self.typed
            .types()
            .calls()
            .iter()
            .filter(|call| {
                call.target()
                    == match self.target(target) {
                        UnitCallableTarget::Declaration(id) => UnitCallTarget::Declaration(id),
                        _ => unreachable!("fixture helper is a top-level declaration"),
                    }
            })
            .map(|call| {
                let expression = call.expression();
                let Expression::Call { arguments, .. } = self
                    .file(expression.source_unit())
                    .ast()
                    .expressions()
                    .get(expression.expression())
                    .unwrap()
                    .payload()
                else {
                    panic!("typed source call has a Call operand");
                };
                call.arguments()
                    .iter()
                    .filter(|mapping| {
                        matches!(
                            self.typed.types().types().get(mapping.parameter_type()),
                            Some(UnitTypeKind::Function { .. })
                        )
                    })
                    .map(|mapping| {
                        assert_eq!(mapping.mode(), ParameterMode::Borrow);
                        let expression = UnitExpressionId::new(
                            expression.source_unit(),
                            arguments[mapping.argument_index()].value,
                        );
                        assert!(matches!(
                            self.typed
                                .types()
                                .expression_type(expression)
                                .and_then(|ty| self.typed.types().types().get(ty)),
                            Some(UnitTypeKind::Function { .. })
                        ));
                        CallbackUse {
                            parameter: mapping.parameter_index(),
                            expression,
                            origin: self
                                .owned
                                .ownership()
                                .callable_origin(expression)
                                .map(|fact| fact.origin()),
                        }
                    })
                    .collect()
            })
            .collect()
    }
}

const GENERATE: &str = "package q\nfun <T> generate(size: Int, initializer: (Int)->T): List<T> = List<T>(size, initializer)";
const ENVIRONMENTS: &str = "val scale = 7\n\
    val pointer: (Int)->Int = { index -> index }\n\
    val shared: (Int)->Int = { index -> index + scale }\n\
    val owned: (Int)->Int = move { index -> index + scale }";

#[test]
fn unit_callable_plan_function_valued_capture_has_no_selected_environment_layout() {
    let fixture = Fixture::new(
        "package q\nfun accept(f: (Int)->Int): Unit {}",
        "package p\nimport q.accept\nfun entry(): Unit {\n\
         val outer: (Int)->Int = { index -> index }\n\
         accept({ index -> outer(index) }) }",
    );
    let uses = fixture.callback_uses("accept");
    let [call] = uses.as_slice() else {
        panic!("one actual source callback use")
    };
    let Some(UnitCallableOrigin::Lambda(lambda)) = call[0].origin else {
        panic!("the passed lambda has committed frontend provenance")
    };
    let captures = fixture
        .owned
        .ownership()
        .captures_of(lambda)
        .collect::<Vec<_>>();
    let [capture] = captures.as_slice() else {
        panic!("the inner callback captures exactly the outer Function")
    };
    assert!(matches!(
        fixture.typed.types().types().get(capture.ty()),
        Some(UnitTypeKind::Function { .. })
    ));
    let error = fixture
        .result()
        .expect_err("callback identity cannot supply a layout for its captured Function");
    assert_eq!(error.kind, super::super::LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(capture.reference_span()));
}

#[test]
fn unit_callable_plan_three_environments_have_three_full_keys_and_stable_input_order() {
    let fixture = Fixture::new(
        GENERATE,
        &format!(
            "package p\nimport q.generate\nfun entry(): Unit {{ {ENVIRONMENTS}\n\
        generate<Int>(3, pointer)\ngenerate<Int>(3, shared)\ngenerate<Int>(3, owned) }}"
        ),
    );
    let uses = fixture.callback_uses("generate");
    assert_eq!(uses.len(), 3);
    let mut origins = Vec::new();
    let mut capture_modes = Vec::new();
    for call in uses {
        let [operand] = call.as_slice() else {
            panic!("one callback argument")
        };
        assert_eq!(operand.parameter, 1);
        let Some(UnitCallableOrigin::Lambda(lambda)) = operand.origin else {
            panic!("actual lambda fact")
        };
        assert!(
            !origins.contains(&lambda),
            "three distinct source callbacks"
        );
        origins.push(lambda);
        let captures = fixture
            .owned
            .ownership()
            .captures_of(lambda)
            .collect::<Vec<_>>();
        capture_modes.push(captures.first().map(|capture| capture.mode()));
        assert_eq!(
            fixture
                .owned
                .ownership()
                .closure(lambda)
                .unwrap()
                .move_owned(),
            captures
                .first()
                .is_some_and(|capture| capture.mode() == ClosureCaptureMode::Owned)
        );
    }
    assert_eq!(
        capture_modes,
        [
            None,
            Some(ClosureCaptureMode::Shared),
            Some(ClosureCaptureMode::Owned)
        ]
    );
    let forward = fixture.plan();
    let reversed = fixture.reversed_plan();
    assert_eq!(
        forward, reversed,
        "normalized source order determines the entire plan"
    );
    assert_eq!(
        fixture.counts(&forward, &["generate"]),
        [3],
        "one semantic Fn signature cannot merge pointer/shared/owned environments"
    );
}

#[test]
fn unit_callable_plan_same_origin_alias_and_recursive_parameter_reuse_one_instance() {
    let fixture = Fixture::new(
        "package q\nfun relay(f: (Int)->Int): Unit { relay((f)) }",
        "package p\nimport q.relay\nfun entry(): Unit {\n\
        val original: (Int)->Int = { index -> index }\nrelay(original)\n\
        val alias = (original)\nrelay((alias)) }",
    );
    let uses = fixture.callback_uses("relay");
    let origins = uses
        .iter()
        .map(|call| call[0].origin.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        origins
            .iter()
            .filter(|origin| matches!(origin, UnitCallableOrigin::Parameter(_)))
            .count(),
        1
    );
    let lambdas = origins
        .iter()
        .filter(|origin| matches!(origin, UnitCallableOrigin::Lambda(_)))
        .collect::<Vec<_>>();
    assert_eq!(lambdas.len(), 2);
    assert_eq!(
        lambdas[0], lambdas[1],
        "Group/alias forwards the same source"
    );
    let plan = fixture.plan();
    assert_eq!(
        fixture.counts(&plan, &["relay", "entry"]),
        [1, 1],
        "Parameter recursion must return the same full key instead of creating a call-chain identity"
    );
    assert!(
        plan.iter()
            .filter(|instance| instance.key().target() == fixture.target("relay"))
            .all(|instance| instance.key().is_specialized()),
        "nongeneric callback slots still use the existing specialization budget"
    );
}

#[test]
fn unit_callable_plan_three_parameter_forwarders_preserve_each_actual_environment() {
    let fixture = Fixture::new(
        "package q\nfun inner(f: (Int)->Int): Unit {}\n\
        fun middle(f: (Int)->Int): Unit { inner((f)) }\n\
        fun outer(f: (Int)->Int): Unit { middle(f) }",
        &format!(
            "package p\nimport q.outer\nfun entry(): Unit {{ {ENVIRONMENTS}\n\
        outer(pointer)\nouter(shared)\nouter(owned) }}"
        ),
    );
    for name in ["inner", "middle"] {
        let uses = fixture.callback_uses(name);
        assert_eq!(uses.len(), 1);
        assert!(matches!(
            uses[0][0].origin,
            Some(UnitCallableOrigin::Parameter(_))
        ));
    }
    let plan = fixture.plan();
    assert_eq!(
        fixture.counts(&plan, &["outer", "middle", "inner"]),
        [3, 3, 3],
        "each forwarding instance transports its current parameter token, including the environment"
    );
}

#[test]
fn unit_callable_plan_named_arguments_use_declaration_slots_instead_of_evaluation_order() {
    let fixture = Fixture::new(
        "package q\nfun route(size: Int, first: (Int)->Int, tag: Int, second: (Int)->Int): Unit {}",
        "package p\nimport q.route\nfun entry(): Unit {\n\
        val left: (Int)->Int = { index -> index }\nval right: (Int)->Int = { index -> index + 1 }\n\
        route(size = 3, first = left, tag = 0, second = right)\n\
        route(second = right, tag = 1, first = left, size = 4)\n\
        route(second = left, size = 5, first = right, tag = 2) }",
    );
    let uses = fixture.callback_uses("route");
    assert_eq!(uses.len(), 3);
    assert_eq!(
        uses.iter()
            .map(|call| call
                .iter()
                .map(|operand| operand.parameter)
                .collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        [vec![1, 3], vec![3, 1], vec![3, 1]]
    );
    let mut slots = uses
        .iter()
        .map(|call| {
            call.iter()
                .map(|operand| (operand.parameter, operand.origin.unwrap()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    for call in &mut slots {
        call.sort_by_key(|(slot, _)| *slot);
    }
    assert_eq!(
        slots[0], slots[1],
        "reordered equivalent mapping reuses one identity"
    );
    assert_ne!(
        slots[0], slots[2],
        "swapping source callbacks changes declaration slots"
    );
    let plan = fixture.plan();
    assert_eq!(
        fixture.counts(&plan, &["route"]),
        [2],
        "ordered sparse Fn slots are 1/3, with no entries for scalar arguments"
    );
}

#[test]
fn unit_callable_plan_pointer_factory_return_identity_is_memoized_per_source_instance() {
    let fixture = Fixture::new(
        "package q\nfun generate(size: Int, initializer: (Int)->Int): Unit {}\n\
        fun factory(): (Int)->Int { println(\"factory\")\nreturn ({ index -> index }) }\n\
        fun otherFactory(): (Int)->Int = { index -> index + 1 }",
        "package p\nimport q.generate\nimport q.factory\nimport q.otherFactory\n\
        fun entry(): Unit { generate(3, factory())\ngenerate(4, factory())\ngenerate(5, otherFactory()) }",
    );
    let mut returned = Vec::new();
    for name in ["factory", "otherFactory"] {
        let summary = fixture
            .owned
            .ownership()
            .pointer_callable_return(fixture.target(name))
            .unwrap();
        let UnitPointerCallableReturnOrigin::Lambda(lambda) = summary.origin() else {
            panic!("pointer factory lambda")
        };
        assert_eq!(fixture.owned.ownership().captures_of(lambda).count(), 0);
        returned.push(lambda);
    }
    assert_ne!(returned[0], returned[1]);
    let uses = fixture.callback_uses("generate");
    assert_eq!(uses.len(), 3);
    assert!(
        uses.iter()
            .all(|call| matches!(call[0].origin, Some(UnitCallableOrigin::FactoryResult(_))))
    );
    let plan = fixture.plan();
    assert_eq!(
        fixture.counts(&plan, &["generate", "factory", "otherFactory"]),
        [2, 1, 1],
        "two evaluations of the same pointer factory share its return token; different factory bodies remain distinct"
    );
}

#[test]
fn unit_callable_plan_known_function_reference_requires_the_selected_source_body() {
    let fixture = Fixture::new(
        "package q\nfun helper(f: (Int)->Int): Unit {}\n\
        fun identity(index: Int): Int = index\nfun other(index: Int): Int = index + 1",
        "package p\nimport q.helper\nimport q.identity\nimport q.other\n\
        fun entry(): Unit { val named = identity\nval alias = (named)\n\
        helper((alias))\nhelper(identity)\nhelper(other) }",
    );
    let uses = fixture.callback_uses("helper");
    let origins = uses
        .iter()
        .map(|call| call[0].origin.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        origins,
        [
            UnitCallableOrigin::KnownFunction(fixture.target("identity")),
            UnitCallableOrigin::KnownFunction(fixture.target("identity")),
            UnitCallableOrigin::KnownFunction(fixture.target("other"))
        ]
    );
    let plan = fixture.plan();
    assert_eq!(
        fixture.counts(&plan, &["helper", "identity", "other"]),
        [2, 1, 1],
        "function value references demand their selected bodies even without a direct source call"
    );
}

#[test]
fn unit_callable_plan_unknown_legal_cfg_join_does_not_guess_a_pointer_environment() {
    let fixture = Fixture::new(
        "package q\nfun helper(f: (Int)->Int): Unit {}",
        "package p\nimport q.helper\nfun entry(flag: Boolean): Unit {\n\
        val first: (Int)->Int = { index -> index }\nval second: (Int)->Int = { index -> index + 1 }\n\
        val merged: (Int)->Int = if(flag) { first } else { second }\nhelper(merged) }",
    );
    let uses = fixture.callback_uses("helper");
    let [call] = uses.as_slice() else {
        panic!("one helper call")
    };
    assert_eq!(
        call[0].origin, None,
        "frontend withholds an ambiguous source after a legal join"
    );
    let span = fixture
        .file(call[0].expression.source_unit())
        .ast()
        .expressions()
        .get(call[0].expression.expression())
        .unwrap()
        .span();
    let arena = fixture.typed.types().types().len();
    let error = fixture
        .result()
        .expect_err("valid Function type is insufficient to select one concrete environment");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(
        error.span,
        Some(span),
        "unsupported source is diagnosed at the actual callback operand"
    );
    assert_eq!(fixture.typed.types().types().len(), arena);
}

#[test]
fn unit_callable_plan_named_unknown_reports_only_the_actual_callback_operand() {
    let fixture = Fixture::new(
        "package q\nfun helper(f: (Int)->Int): Unit {}",
        "package p\nimport q.helper\nfun entry(flag: Boolean): Unit {\n\
         val first: (Int)->Int = { index -> index }\n\
         val second: (Int)->Int = { index -> index + 1 }\n\
         val merged: (Int)->Int = if(flag) { first } else { second }\n\
         helper(f = merged) }",
    );
    let uses = fixture.callback_uses("helper");
    let [call] = uses.as_slice() else {
        panic!("one named callback use")
    };
    assert_eq!(call[0].parameter, 0);
    assert_eq!(call[0].origin, None, "the legal join has no unique source");
    let operand_span = fixture
        .file(call[0].expression.source_unit())
        .ast()
        .expressions()
        .get(call[0].expression.expression())
        .unwrap()
        .span();
    assert_eq!(fixture.sources.slice(operand_span).unwrap(), "merged");
    let error = fixture
        .result()
        .expect_err("a named argument cannot make an Unknown environment concrete");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(
        error.span,
        Some(operand_span),
        "the named prefix belongs to argument mapping, not to the unknown callback value"
    );
}

#[test]
fn unit_callable_plan_preserves_earlier_helper_error_before_later_recipe_frontier() {
    let fixture = Fixture::new(
        "package q\n\
        interface Readable { fun read(): Int }\n\
        class First(val second: Second): Readable by second {}\n\
        class Second(val first: First): Readable by first {}\n\
        fun helper(first: First, f: (Int)->Int): Int = first.read()\n\
        interface Base<A> { fun readGrowth(): Int = 7 }\n\
        class GrowA<T>(val next: GrowB<List<T>>)\n\
        class GrowB<U>(val next: GrowA<U>)\n\
        class Host<T>: Base<T> {}\n\
        fun <T> relay(host: Host<T>, f: (Int)->Int): Int = host.readGrowth()",
        "package p\nimport q.First\nimport q.Host\nimport q.GrowA\nimport q.helper\nimport q.relay\n\
        fun entry(first: First, host: Host<GrowA<Int>>): Int {\n\
        val callback: (Int)->Int = { index -> index }\n\
        return helper(first, callback) + relay(host, callback) }",
    );
    assert!(declaration(&fixture.names, "helper") < declaration(&fixture.names, "relay"));
    let helper_uses = fixture.callback_uses("helper");
    let relay_uses = fixture.callback_uses("relay");
    assert_eq!(helper_uses[0][0].origin, relay_uses[0][0].origin);
    assert!(matches!(
        helper_uses[0][0].origin,
        Some(UnitCallableOrigin::Lambda(_))
    ));
    let read = fixture
        .typed
        .types()
        .calls()
        .iter()
        .find(|call| {
            let parsed = fixture.file(call.expression().source_unit());
            fixture
                .sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(call.expression().expression())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                == "first.read()"
        })
        .expect("the earlier helper has its ordinary unsupported delegation call");
    let span = fixture
        .file(read.expression().source_unit())
        .ast()
        .expressions()
        .get(read.expression().expression())
        .unwrap()
        .span();
    let error = fixture
        .result()
        .expect_err("callback specialization cannot reorder the existing recipe frontier");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(span));
}

#[test]
fn unit_callable_plan_frozen_named_routes_preserve_sparse_slots_and_owner_tokens() {
    let fixture = Fixture::new(
        "package q\nfun route(size: Int, first: (Int)->Int, tag: Int, second: (Int)->Int): Unit {}",
        "package p\nimport q.route\nfun entry(): Unit {\n\
        val left: (Int)->Int = { index -> index }\nval right: (Int)->Int = { index -> index + 1 }\n\
        route(size = 3, first = left, tag = 0, second = right)\n\
        route(second = right, tag = 1, first = left, size = 4)\n\
        route(second = left, size = 5, first = right, tag = 2) }",
    );
    let plan = fixture.plan();
    let source = plan
        .iter()
        .find(|instance| instance.key().target() == fixture.target("entry"))
        .unwrap();
    let callables = plan.callable_plan();
    assert_eq!(callables.source(source.source_token()), Some(source.key()));
    let calls = fixture
        .typed
        .types()
        .calls()
        .iter()
        .filter(|call| {
            call.target() == UnitCallTarget::Declaration(declaration(&fixture.names, "route"))
        })
        .collect::<Vec<_>>();
    let uses = fixture.callback_uses("route");
    let routes = calls
        .iter()
        .map(|call| {
            callables
                .call_site(source.source_token(), call.expression())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(routes[0].key(), routes[1].key());
    assert_ne!(routes[0].key(), routes[2].key());
    for (route, operands) in routes.iter().zip(uses) {
        assert!(route.delegation().is_empty());
        assert_eq!(
            route
                .key()
                .callable_arguments()
                .iter()
                .map(|(slot, _)| *slot)
                .collect::<Vec<_>>(),
            [1, 3]
        );
        for &(slot, token) in route.key().callable_arguments() {
            let Some(UnitCallableOrigin::Lambda(lambda)) = operands
                .iter()
                .find(|operand| operand.parameter == slot)
                .unwrap()
                .origin
            else {
                panic!("actual source lambda");
            };
            assert_eq!(
                callables.callable(token),
                Some(&CallableKey::Lambda {
                    owner: source.source_token(),
                    expression: lambda
                })
            );
        }
        let target = plan
            .iter()
            .find(|instance| instance.key() == route.key())
            .unwrap();
        assert_eq!(callables.source(target.source_token()), Some(route.key()));
    }
}

#[test]
fn unit_callable_plan_factory_memo_and_routes_share_the_returned_pointer_token() {
    let fixture = Fixture::new(
        "package q\nfun helper(f: (Int)->Int): Unit {}\n\
        fun factory(): (Int)->Int = { index -> index }",
        "package p\nimport q.helper\nimport q.factory\nfun entry(): Unit { helper(factory())\nhelper(factory()) }",
    );
    let plan = fixture.plan();
    let callables = plan.callable_plan();
    let source = plan
        .iter()
        .find(|instance| instance.key().target() == fixture.target("entry"))
        .unwrap();
    let factory = plan
        .iter()
        .find(|instance| instance.key().target() == fixture.target("factory"))
        .unwrap();
    let result = callables.callable_return(factory.source_token()).unwrap();
    let summary = fixture
        .owned
        .ownership()
        .pointer_callable_return(fixture.target("factory"))
        .unwrap();
    let UnitPointerCallableReturnOrigin::Lambda(lambda) = summary.origin() else {
        panic!("pointer return")
    };
    assert_eq!(result.function_type(), summary.function_type());
    assert_eq!(
        callables.callable(result.callable()),
        Some(&CallableKey::Lambda {
            owner: factory.source_token(),
            expression: lambda
        })
    );
    for call in fixture.typed.types().calls() {
        let route = callables
            .call_site(source.source_token(), call.expression())
            .unwrap();
        if call.target() == UnitCallTarget::Declaration(declaration(&fixture.names, "helper")) {
            assert_eq!(route.key().callable_arguments(), [(0, result.callable())]);
        } else {
            assert_eq!(route.key(), factory.key());
        }
    }
}

#[test]
fn unit_callable_plan_keeps_static_routes_inside_lambda_and_hidden_deinit_bodies() {
    let fixture = Fixture::new(
        "package q\nfun helper(): Unit {}\n\
        class Ticket { deinit() { helper() } }\nfun make(): Ticket = Ticket()",
        "package p\nimport q.helper\nimport q.make\nfun entry(): Unit {\n\
        val callback: ()->Unit = { helper() }\ncallback()\nval ticket = make() }",
    );
    let plan = fixture.plan();
    let source = plan
        .iter()
        .find(|instance| instance.key().target() == fixture.target("entry"))
        .unwrap();
    let deinit = plan
        .iter()
        .find(|instance| {
            instance.key().deinit_owner() == Some(declaration(&fixture.names, "Ticket"))
        })
        .unwrap();
    let calls = fixture
        .typed
        .types()
        .calls()
        .iter()
        .filter(|call| {
            call.target() == UnitCallTarget::Declaration(declaration(&fixture.names, "helper"))
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    for call in calls {
        let span = fixture
            .file(call.expression().source_unit())
            .ast()
            .expressions()
            .get(call.expression().expression())
            .unwrap()
            .span();
        let owner = if span.source_id() == fixture.caller_source {
            source
        } else {
            deinit
        };
        let route = plan
            .callable_plan()
            .call_site(owner.source_token(), call.expression())
            .unwrap();
        assert_eq!(route.key().target(), fixture.target("helper"));
        assert!(route.key().callable_arguments().is_empty());
        assert!(route.delegation().is_empty());
    }
}
