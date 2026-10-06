//! Planner identity contracts consume successful frontend facts without emitting SSA or LLVM.

use super::*;
use lang_frontend::{
    ast::ExpressionId,
    lexer::lex,
    name_resolution::{NameResolution, resolve_names},
    ownership_checking::{
        CallableOrigin, ClosureCaptureMode, OwnershipCheckedFile, PointerCallableReturnOrigin,
        check_ownership,
    },
    parser::{Expression, Item, parse_file},
    source::SourceMap,
    type_checking::{ParameterMode, check_types, standard_environments},
};
use std::collections::BTreeSet;

struct Fixture {
    sources: SourceMap,
    parsed: ParsedFile,
    names: NameResolution,
    typed: TypedFile,
    owned: OwnershipCheckedFile,
    templates: Vec<FunctionTemplate>,
}

impl Fixture {
    /// A provenance assertion must never disguise parser, type or ownership fixture errors.
    fn analyze(text: &str) -> Self {
        let mut sources = SourceMap::new();
        let source = sources.add_source("single-callable-plan.ko", text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &parsed, &environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        assert!(owned.is_compatible_with(&names, &typed));
        // Match orchestrate's source-order templates, using selected signatures rather than names.
        let templates = parsed
            .roots()
            .iter()
            .map(|root| {
                let node = parsed.ast().items().get(*root).unwrap();
                assert!(matches!(node.payload(), Item::Function { .. }));
                let callable = typed
                    .callables()
                    .iter()
                    .find(|callable| {
                        names.symbols().iter().any(|symbol| {
                            symbol.id() == callable.symbol()
                                && symbol.span().source_id() == node.span().source_id()
                                && node.span().start() <= symbol.span().start()
                                && symbol.span().end() <= node.span().end()
                        })
                    })
                    .expect("one existing ordinary source signature per fixture declaration");
                FunctionTemplate {
                    symbol: callable.symbol(),
                    type_parameters: callable.type_parameters().to_vec(),
                    span: node.span(),
                }
            })
            .collect();
        Self {
            sources,
            parsed,
            names,
            typed,
            owned,
            templates,
        }
    }

    fn symbol(&self, name: &str) -> SymbolId {
        let symbols = self
            .names
            .symbols()
            .iter()
            .filter(|symbol| symbol.name() == name)
            .map(|symbol| symbol.id())
            .collect::<Vec<_>>();
        assert_eq!(symbols.len(), 1, "unique fixture symbol {name}");
        symbols[0]
    }

    fn expression(&self, text: &str) -> ExpressionId {
        let ids = self
            .parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                (self.sources.slice(node.span()).unwrap() == text).then_some(id)
            })
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 1, "unique fixture expression {text}: {ids:?}");
        ids[0]
    }

    fn lambdas(&self) -> Vec<ExpressionId> {
        self.parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), Expression::Lambda { .. }).then_some(id)
            })
            .collect()
    }

    fn plan(&self) -> Result<FunctionInstancePlan, LoweringError> {
        plan_instances(&self.parsed, &self.typed, &self.owned, &self.templates, &[])
    }

    fn assert_count(&self, plan: &FunctionInstancePlan, name: &str, count: usize) {
        let symbol = self.symbol(name);
        let keys = plan
            .instances()
            .iter()
            .filter(|instance| instance.key.symbol() == symbol)
            .map(|instance| &instance.key)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            keys.len(),
            count,
            "complete distinct source keys for {name}: {keys:?}"
        );
    }

    fn assert_unsupported(&self, argument: &str) {
        let expected = self
            .parsed
            .ast()
            .expressions()
            .get(self.expression(argument))
            .unwrap()
            .span();
        match self.plan() {
            Ok(plan) => panic!(
                "unsupported callback at {expected:?} was accepted as {} source instances",
                plan.instances().len()
            ),
            Err(error) => {
                assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
                assert_eq!(error.span, Some(expected));
            }
        }
    }
}

const HELPER: &str = "fun helper(callback: (Int)->Int): Unit {}\n";

#[test]
fn single_callable_plan_three_environments_keep_three_complete_helper_keys() {
    let fixture = Fixture::analyze(
        "fun helper(callback: (Int)->Boolean): Unit {}\n\
         fun entry(sharedLabel: String, own ownedLabel: String): Unit {\n\
         val pointer: (Int)->Boolean = { index -> index == 0 }\n\
         val shared: (Int)->Boolean = { index -> sharedLabel == \"shared\" && index == 0 }\n\
         val ownedCallback: (Int)->Boolean = move { index -> ownedLabel == \"owned\" && index == 0 }\n\
         helper(pointer)\nhelper(shared)\nhelper(ownedCallback)\n }",
    );
    let lambdas = fixture.lambdas();
    assert_eq!(lambdas.len(), 3);
    for (lambda, expected) in lambdas.iter().zip([
        None,
        Some(ClosureCaptureMode::Shared),
        Some(ClosureCaptureMode::Owned),
    ]) {
        let modes = fixture
            .owned
            .captures_of(*lambda)
            .map(|capture| capture.mode())
            .collect::<Vec<_>>();
        assert_eq!(modes, expected.into_iter().collect::<Vec<_>>());
    }
    let mut types = BTreeSet::new();
    for (name, lambda) in ["pointer", "shared", "ownedCallback"]
        .into_iter()
        .zip(lambdas)
    {
        let expression = fixture.expression(name);
        assert_eq!(
            fixture.owned.callable_origin(expression).unwrap().origin(),
            CallableOrigin::Lambda(lambda)
        );
        types.insert(fixture.typed.expression_type(expression).unwrap());
    }
    assert_eq!(
        types.len(),
        1,
        "one static Fn type must not merge three environments"
    );
    let plan = fixture
        .plan()
        .expect("all three concrete callback origins are supported");
    fixture.assert_count(&plan, "helper", 3);
}

#[test]
fn single_callable_plan_alias_and_group_reuse_one_origin() {
    let fixture = Fixture::analyze(&format!(
        "{HELPER}fun entry(): Unit {{ val original: (Int)->Int = {{ index -> index }}\n\
         helper(original)\nval alias = (original)\nhelper((alias)) }}"
    ));
    let lambda = fixture.lambdas()[0];
    let mut uses = 0;
    for (id, node) in fixture.parsed.ast().expressions().iter() {
        if matches!(
            fixture.sources.slice(node.span()).unwrap(),
            "original" | "(original)" | "(alias)"
        ) {
            assert_eq!(
                fixture.owned.callable_origin(id).unwrap().origin(),
                CallableOrigin::Lambda(lambda)
            );
            uses += 1;
        }
    }
    assert_eq!(
        uses, 4,
        "both original uses and their transparent groups are checked"
    );
    let plan = fixture.plan().unwrap();
    fixture.assert_count(&plan, "helper", 1);
}

#[test]
fn single_callable_plan_parameter_forwarding_and_recursion_reuse_complete_keys() {
    let fixture = Fixture::analyze(&format!(
        "{HELPER}\
         fun middle(forwarded: (Int)->Int): Unit {{ helper((forwarded)) }}\n\
         fun relay(input: (Int)->Int): Unit {{ middle(input)\nrelay(input) }}\n\
         fun entry(): Unit {{ relay({{ index -> index }}) }}"
    ));
    for (id, node) in fixture.parsed.ast().expressions().iter() {
        let text = fixture.sources.slice(node.span()).unwrap();
        if matches!(text, "input" | "forwarded") {
            assert_eq!(
                fixture.owned.callable_origin(id).unwrap().origin(),
                CallableOrigin::Parameter(fixture.symbol(text))
            );
            assert_eq!(
                fixture.typed.parameter_mode(fixture.symbol(text)),
                Some(ParameterMode::Borrow)
            );
        }
    }
    let plan = fixture
        .plan()
        .expect("parameter forwarding does not create an instance chain");
    for name in ["helper", "middle", "relay", "entry"] {
        fixture.assert_count(&plan, name, 1);
    }
}

#[test]
fn single_callable_plan_distinct_lambda_expressions_do_not_merge_equal_shapes() {
    let fixture = Fixture::analyze(&format!(
        "{HELPER}fun entry(): Unit {{ helper({{ index -> index }})\nhelper({{ index -> index }}) }}"
    ));
    assert_eq!(fixture.lambdas().len(), 2);
    assert_ne!(fixture.lambdas()[0], fixture.lambdas()[1]);
    fixture.assert_count(&fixture.plan().unwrap(), "helper", 2);
}

#[test]
fn single_callable_plan_lambda_owner_includes_concrete_generic_instance() {
    let fixture = Fixture::analyze(&format!(
        "{HELPER}fun <T> relay(own value: T): Unit {{ helper({{ index -> index }}) }}\n\
         fun entry(): Unit {{ relay<Int>(1)\nrelay<Boolean>(true) }}"
    ));
    assert_eq!(
        fixture.lambdas().len(),
        1,
        "same source expression in two owners"
    );
    let plan = fixture.plan().unwrap();
    fixture.assert_count(&plan, "relay", 2);
    fixture.assert_count(&plan, "helper", 2);
}

#[test]
fn single_callable_plan_named_reordering_uses_sparse_declaration_slots() {
    let fixture = Fixture::analyze(
        "fun helper(first: (Int)->Int, count: Int, last: (Int)->Int): Unit {}\n\
         fun entry(): Unit { val a: (Int)->Int = { index -> index }\n\
         val b: (Int)->Int = { index -> index + 1 }\n\
         helper(a, 1, b)\nhelper(last = b, count = 2, first = a)\n\
         helper(last = a, count = 3, first = b) }",
    );
    for text in [
        "helper(last = b, count = 2, first = a)",
        "helper(last = a, count = 3, first = b)",
    ] {
        let call = fixture
            .typed
            .calls()
            .iter()
            .find(|call| call.expression() == fixture.expression(text))
            .unwrap();
        assert_eq!(
            call.arguments()
                .iter()
                .map(|mapping| (mapping.argument_index(), mapping.parameter_index()))
                .collect::<Vec<_>>(),
            [(0, 2), (1, 1), (2, 0)]
        );
        assert_eq!(
            call.arguments()
                .iter()
                .filter(|argument| matches!(
                    fixture.typed.types().get(argument.parameter_type()),
                    Some(TypeKind::Function { .. })
                ))
                .map(|argument| argument.parameter_index())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([0, 2])
        );
    }
    fixture.assert_count(&fixture.plan().unwrap(), "helper", 2);
}

#[test]
fn single_callable_plan_deferred_named_operand_cannot_supply_callback_abi() {
    let fixture = Fixture::analyze(&format!(
        "{HELPER}fun identity(index: Int): Int = index\n\
         fun entry(): Unit {{ val annotated: (Int)->Int = identity\nhelper(annotated) }}"
    ));
    assert!(matches!(
        fixture
            .typed
            .symbol_type(fixture.symbol("annotated"))
            .and_then(|ty| fixture.typed.types().get(ty)),
        Some(TypeKind::Function { .. })
    ));
    assert!(matches!(
        fixture
            .typed
            .expression_type(fixture.expression("identity"))
            .and_then(|ty| fixture.typed.types().get(ty)),
        Some(TypeKind::Deferred(_))
    ));
    assert!(
        fixture
            .owned
            .callable_origin(fixture.expression("annotated"))
            .is_none()
    );
    fixture.assert_unsupported("annotated");
}

#[test]
fn single_callable_plan_unknown_cfg_origin_is_rejected_at_argument() {
    let fixture = Fixture::analyze(&format!(
        "{HELPER}fun entry(flag: Boolean): Unit {{\n\
         val first: (Int)->Int = {{ index -> index }}\n\
         val second: (Int)->Int = {{ index -> index + 1 }}\n\
         val selected: (Int)->Int = if(flag) {{ first }} else {{ second }}\nhelper(selected) }}"
    ));
    assert!(matches!(
        fixture
            .typed
            .expression_type(fixture.expression("selected"))
            .and_then(|ty| fixture.typed.types().get(ty)),
        Some(TypeKind::Function { .. })
    ));
    assert!(
        fixture
            .owned
            .callable_origin(fixture.expression("selected"))
            .is_none()
    );
    fixture.assert_unsupported("selected");
}

#[test]
fn single_callable_plan_pointer_factory_result_reuses_factory_owned_origin() {
    let fixture = Fixture::analyze(&format!(
        "{HELPER}fun entry(): Unit {{ val first = factory()\n\
         val alias = (first)\nhelper(alias)\nval second = factory()\nhelper(second) }}\n\
         fun factory(): (Int)->Int = ({{ index -> index }})"
    ));
    let summary = fixture
        .owned
        .pointer_callable_return(fixture.symbol("factory"))
        .unwrap();
    assert_eq!(
        summary.origin(),
        PointerCallableReturnOrigin::Lambda(fixture.lambdas()[0])
    );
    let mut results = BTreeSet::new();
    for text in ["alias", "second"] {
        let CallableOrigin::FactoryResult(call) = fixture
            .owned
            .callable_origin(fixture.expression(text))
            .unwrap()
            .origin()
        else {
            panic!("validated pointer factory origin");
        };
        results.insert(call.index());
    }
    assert_eq!(
        results.len(),
        2,
        "two actual calls still share one factory return source"
    );
    let plan = fixture.plan().unwrap();
    fixture.assert_count(&plan, "factory", 1);
    fixture.assert_count(&plan, "helper", 1);
}

#[test]
fn single_callable_plan_preserves_plain_roots_and_scalar_generic_deduplication() {
    let fixture = Fixture::analyze(
        "fun <T> identity(own value: T): T = value\n\
         fun <T> recurse(own input: T): T = recurse(input)\n\
         fun <T> unused(own item: T): T = item\n\
         fun uncalled(): Unit {}\n\
         fun first(input: Int): Int = identity(input)\n\
         fun second(input: Int): Int = identity<Int>(input)\n\
         fun third(input: Long): Long = identity(input)\n\
         fun recursive(input: Int): Int = recurse(input)",
    );
    let plan = fixture.plan().unwrap();
    for name in [
        "uncalled",
        "first",
        "second",
        "third",
        "recursive",
        "recurse",
    ] {
        fixture.assert_count(&plan, name, 1);
    }
    fixture.assert_count(&plan, "identity", 2);
    fixture.assert_count(&plan, "unused", 0);
    let repeated = fixture.plan().unwrap();
    assert_eq!(
        plan.instances()
            .iter()
            .map(|instance| &instance.key)
            .collect::<Vec<_>>(),
        repeated
            .instances()
            .iter()
            .map(|instance| &instance.key)
            .collect::<Vec<_>>()
    );
}

#[test]
fn single_callable_plan_ordinary_nullable_generic_argument_keeps_existing_frontier() {
    let fixture = Fixture::analyze(
        "fun <T> ignore(value: T?): Unit {}\n\
         fun entry(value: Int?): Unit { ignore<Int>(value) }",
    );
    let type_count = fixture.typed.types().len();
    let plan = fixture.plan().expect(
        "callback discovery must not add a concrete-layout gate to an ordinary non-Fn parameter",
    );
    fixture.assert_count(&plan, "ignore", 1);
    fixture.assert_count(&plan, "entry", 1);
    assert!(
        plan.instances()
            .iter()
            .all(|instance| instance.key.callable_arguments().is_empty())
    );
    assert_eq!(
        fixture.typed.types().len(),
        type_count,
        "ordinary planning remains readonly"
    );
}

#[test]
fn single_callable_plan_new_owner_lambda_growth_stops_at_source_budget() {
    let fixture = Fixture::analyze(
        "fun relay(callback: (Int)->Int): Unit { relay({ index -> index }) }\n\
         fun entry(): Unit { relay({ index -> index }) }",
    );
    assert_eq!(fixture.lambdas().len(), 2);
    assert!(
        fixture.owned.captures().is_empty(),
        "this is owner identity growth, never Fn capture layout growth"
    );
    match fixture.plan() {
        Ok(plan) => panic!(
            "fresh owner-qualified origins collapsed into {} instances",
            plan.instances().len()
        ),
        Err(error) => {
            assert_eq!(error.kind, LoweringErrorKind::InstanceLimitExceeded);
            assert_eq!(error.span, Some(fixture.templates[0].span));
        }
    }
}

#[test]
fn single_callable_plan_generic_and_callback_budget_charges_each_source_once() {
    let calls = (0..MAX_GENERIC_INSTANCES)
        .map(|_| "helper<Int>(1, { index -> index })\n")
        .collect::<String>();
    let fixture = Fixture::analyze(&format!(
        "fun <T> helper(own value: T, callback: (Int)->Int): Unit {{}}\nfun entry(): Unit {{\n{calls}}}"
    ));
    assert_eq!(fixture.lambdas().len(), MAX_GENERIC_INSTANCES);
    let plan = fixture
        .plan()
        .expect("1024 generic plus callback-specialized sources cost 1024, not 2048");
    fixture.assert_count(&plan, "helper", MAX_GENERIC_INSTANCES);
    fixture.assert_count(&plan, "entry", 1);
}

#[test]
fn single_callable_plan_repeated_origin_deduplicates_before_source_budget() {
    let calls = (0..=MAX_GENERIC_INSTANCES)
        .map(|_| "helper<Int>(1, callback)\n")
        .collect::<String>();
    let fixture = Fixture::analyze(&format!(
        "fun <T> helper(own value: T, forwarded: (Int)->Int): Unit {{}}\n\
         fun entry(): Unit {{ val callback: (Int)->Int = {{ index -> index }}\n{calls}}}"
    ));
    assert_eq!(fixture.lambdas().len(), 1);
    let plan = fixture
        .plan()
        .expect("1025 uses of one full key only occupy one source budget slot");
    fixture.assert_count(&plan, "helper", 1);
}

#[path = "callable_route_tests.rs"]
mod route_contract;
