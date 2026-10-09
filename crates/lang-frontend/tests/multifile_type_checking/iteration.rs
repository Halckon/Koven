use super::*;

fn analyze(provider_text: &str, consumer_text: &str) -> (SourceMap, CompilationUnitTypes) {
    analyze_order(provider_text, consumer_text, false)
}

fn analyze_order(
    provider_text: &str,
    consumer_text: &str,
    reverse: bool,
) -> (SourceMap, CompilationUnitTypes) {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", provider_text);
    let (consumer_source, consumer) = parsed(&mut sources, "q/consumer.ko", consumer_text);
    let mut inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    if reverse {
        inputs.reverse();
    }
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unit type checking");
    (sources, typed)
}

#[test]
fn binding_is_concrete_before_body_type_checking() {
    let (_, typed) = analyze(
        "package p\nfun source(): Array<Int> = arrayOf(1)",
        "package q\nfun run() { for (item in p.source()) { val n: Int = item } }",
    );
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(
        typed.body_symbol_types().values().all(|ty| !matches!(
            typed.signatures().types().get(*ty),
            Some(UnitTypeKind::Deferred(_))
        )),
        "a verified provider gives its binding the element type"
    );
}

#[test]
fn nominal_provider_lookalike_is_rejected_at_source_expression() {
    let (sources, typed) = analyze(
        "package p\nclass Array()",
        "package q\nfun run(xs: p.Array) { for (item in xs) {} }",
    );
    let diagnostics = typed.diagnostics();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code().to_string(), "L0159");
    assert_eq!(sources.slice(diagnostics[0].primary_span()), Ok("xs"));
}

#[test]
fn three_providers_cover_five_sources_and_source_qualified_plans() {
    for (container, factory, kind) in [
        ("Array", "arrayOf", SequentialContainerKind::Array),
        ("List", "listOf", SequentialContainerKind::List),
        (
            "MutableList",
            "mutableListOf",
            SequentialContainerKind::MutableList,
        ),
    ] {
        let provider = format!(
            "package p\nclass Holder(val xs: {container}<Int>)\nfun make(): {container}<Int> = {factory}(1, 2)"
        );
        for (parameter, expression) in [
            (format!("own xs: {container}<Int>"), "xs"),
            (format!("xs: {container}<Int>"), "xs"),
            (format!("inout xs: {container}<Int>"), "xs"),
            ("holder: p.Holder".to_owned(), "holder.xs"),
            (String::new(), "p.make()"),
        ] {
            let (_, typed) = analyze(
                &provider,
                &format!(
                    "package q\nfun run({parameter}) {{ for (item in {expression}) {{ val n: Int = item }} }}"
                ),
            );
            assert!(
                typed.diagnostics().is_empty(),
                "{container}/{expression}: {:?}",
                typed.diagnostics()
            );
            let plans = typed.sequential_iterations();
            assert_eq!(plans.len(), 1);
            let plan = &plans[0];
            assert_eq!(
                plan.provider(),
                lang_frontend::type_checking::IterationProvider::Sequential(kind)
            );
            assert_eq!(plan.delivery(), ParameterMode::Borrow);
            assert_eq!(
                typed.types().get(plan.element_type()),
                Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            );
            assert_eq!(plan.statement().source_unit(), plan.source().source_unit());
            assert_eq!(typed.sequential_iteration(plan.statement()), Some(plan));
            assert!(
                plan.binding()
                    .symbols()
                    .all(|symbol| symbol.source_unit() == plan.statement().source_unit())
            );
        }
    }
}

#[test]
fn cross_file_generic_components_keep_declaration_identity_and_discard() {
    use lang_frontend::type_checking::UnitSequentialIterationBinding;
    let (_, typed) = analyze(
        "package p\nvalue class Pair<T>(val first: T, val second: Int)",
        "package q\nfun run(xs: List<p.Pair<String>>) { for ((text, _) in xs) { println(text) }\nfor (_ in xs) {} }",
    );
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.sequential_iterations().len(), 2);
    let plan = typed
        .sequential_iterations()
        .iter()
        .find(|plan| {
            matches!(
                plan.binding(),
                UnitSequentialIterationBinding::Destructure(_)
            )
        })
        .unwrap();
    let UnitSequentialIterationBinding::Destructure(parts) = plan.binding() else {
        unreachable!()
    };
    assert_eq!(parts.len(), 2);
    assert_ne!(
        parts[0].field().source_unit(),
        plan.statement().source_unit()
    );
    assert_eq!(
        parts[0].symbol().unwrap().source_unit(),
        plan.statement().source_unit()
    );
    assert_eq!(
        typed.types().get(parts[0].ty()),
        Some(&UnitTypeKind::Builtin(BuiltinType::String))
    );
    assert_eq!(
        typed.types().get(parts[1].ty()),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
    assert!(parts[1].symbol().is_none());
    assert!(
        typed
            .sequential_iterations()
            .iter()
            .any(|plan| matches!(plan.binding(), UnitSequentialIterationBinding::Discard))
    );
    assert!(
        typed.destructurings().is_empty(),
        "borrowed projections are not consuming local destructuring"
    );
}

#[test]
fn invalid_patterns_and_body_errors_atomically_withhold_plans() {
    for (parameter, binding, body, code) in [
        ("xs: List<p.Pair>", "(x)", "", "L0118"),
        ("xs: List<Int>", "(x, y)", "", "L0160"),
        ("xs: List<Int>", "x", "val invalid: String = x", "L0084"),
    ] {
        let (_, typed) = analyze(
            "package p\nvalue class Pair(val a: Int, val b: Int)",
            &format!("package q\nfun run({parameter}) {{ for ({binding} in xs) {{ {body} }} }}"),
        );
        assert!(
            typed
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == code),
            "{code}: {:?}",
            typed.diagnostics()
        );
        assert!(typed.sequential_iterations().is_empty());
        assert!(typed.validate().is_err());
    }
}

#[test]
fn input_permutation_preserves_duplicate_local_ids_and_projection_identity() {
    let first = "package p\nvalue class Pair(val first: Int, val second: String)\nfun left(xs: List<Pair>) { for ((a, b) in xs) { println(b) } }";
    let second = "package q\nfun right(xs: List<p.Pair>) { for ((a, b) in xs) { println(b) } }";
    let (_, forward) = analyze_order(first, second, false);
    let (_, reverse) = analyze_order(first, second, true);
    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert!(
        reverse.diagnostics().is_empty(),
        "{:?}",
        reverse.diagnostics()
    );
    assert_eq!(
        forward.sequential_iterations(),
        reverse.sequential_iterations()
    );
    assert_eq!(forward.sequential_iterations().len(), 2);
    assert_ne!(
        forward.sequential_iterations()[0].statement().source_unit(),
        forward.sequential_iterations()[1].statement().source_unit()
    );
    for plan in forward.sequential_iterations() {
        assert_eq!(forward.sequential_iteration(plan.statement()), Some(plan));
    }
}

#[test]
fn overload_trial_commits_only_the_selected_iteration_and_source_call() {
    let provider = "package p\nfun choose(callback: (Int) -> Int): Int = 1\nfun choose(callback: (String) -> String): String = \"text\"\nfun needInt(n: Int): Int = n\nfun source(): List<Int> = listOf(1)";
    let consumer = "package q\nfun run() { val result = p.choose { for (item in p.source()) { val n: Int = item }\np.needInt(it) } }";
    let (_, typed) = analyze(provider, consumer);
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.sequential_iterations().len(), 1);
    let source = typed.sequential_iterations()[0].source();
    assert_eq!(
        typed
            .calls()
            .iter()
            .filter(|call| call.expression() == source)
            .count(),
        1
    );
    for tail in ["it", "true"] {
        let (_, failed) = analyze(provider, &consumer.replace("p.needInt(it)", tail));
        assert!(!failed.diagnostics().is_empty());
        assert!(failed.sequential_iterations().is_empty());
    }
}
