//! 普通 expected Function 接收 move literal，同时保留 canonical 类型与参数契约。

use super::*;

#[test]
fn ordinary_expected_move_literals_keep_canonical_type_and_borrow_parameters() {
    let text = "fun apply(callback: (Int) -> Boolean): Unit {}\n\
                fun make(own returnedLabel: String): (Int) -> Boolean = move { returnedIndex -> returnedLabel == \"return\" && returnedIndex == 0 }\n\
                fun use(own localLabel: String, own argumentLabel: String): Unit {\n\
                    val contextual: (Int) -> Boolean = move { localIndex -> localLabel == \"local\" && localIndex == 0 }\n\
                    apply(move { argumentIndex -> argumentLabel == \"argument\" && argumentIndex == 0 })\n\
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    assert!(resolution.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let contextual = resolution
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "contextual")
        .expect("contextual binding");
    let expected = typed
        .symbol_type(contextual.id())
        .expect("ordinary expected type");
    assert!(
        matches!(typed.types().get(expected), Some(TypeKind::Function {
        move_only: false, parameters, return_type,
    }) if parameters.len() == 1 && parameters[0].mode == ParameterMode::Borrow
        && typed.types().get(parameters[0].ty) == Some(&TypeKind::Builtin(BuiltinType::Int))
        && typed.types().get(*return_type) == Some(&TypeKind::Builtin(BuiltinType::Boolean)))
    );
    let lambdas = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Expression::Lambda { .. }).then_some(id))
        .collect::<Vec<_>>();
    assert_eq!(lambdas.len(), 3);
    for lambda in lambdas {
        assert_eq!(
            typed.expression_type(lambda),
            Some(expected),
            "local, argument and return literals adopt the same canonical ordinary Function"
        );
    }
    for name in ["returnedIndex", "localIndex", "argumentIndex"] {
        let symbol = resolution
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == name)
            .expect("lambda parameter");
        assert_eq!(
            typed.parameter_mode(symbol.id()),
            Some(ParameterMode::Borrow)
        );
        assert_eq!(
            typed
                .symbol_type(symbol.id())
                .and_then(|ty| typed.types().get(ty)),
            Some(&TypeKind::Builtin(BuiltinType::Int))
        );
    }
}

#[test]
fn contextual_move_literal_acceptance_preserves_named_identity_mode_and_arity_errors() {
    let text = "fun take(callback: (Int) -> Int): Unit {}\n\
                fun strong(callback: move (Int) -> Int): Unit {}\n\
                fun use(named: move (Int) -> Int, ordinary: (Int) -> Int): Unit {\n\
                    val rejected: (Int) -> Int = named\n\
                    take(named)\n\
                    val wrongMode: (own Int) -> Int = ordinary\n\
                    strong({ shared -> shared })\n\
                    val wrongArity: (Int) -> Int = move { first, second -> first }\n\
                    val inferred = move { 1 }\n\
                    val wrongInferred: () -> Int = inferred\n\
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    let resolution = resolve_names(&sources, &parsed, &names).expect("names");
    assert!(resolution.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &resolution, &types).expect("types");
    assert_eq!(codes(typed.diagnostics()), ["L0084"; 6]);
    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("primary"))
            .collect::<Vec<_>>(),
        ["named", "named", "ordinary", "->", "->", "inferred"]
    );
    for name in ["shared", "first", "second"] {
        let symbol = resolution
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == name)
            .expect("rejected parameter");
        assert_eq!(
            typed.parameter_mode(symbol.id()),
            None,
            "rejected lambda structure must not publish an adopted parameter mode"
        );
    }
    let inferred = resolution
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "inferred")
        .expect("inferred binding");
    assert!(
        matches!(typed.symbol_type(inferred.id()).and_then(|ty| typed.types().get(ty)),
        Some(TypeKind::Function { move_only: true, parameters, return_type }) if parameters.is_empty()
            && typed.types().get(*return_type) == Some(&TypeKind::Builtin(BuiltinType::Int))),
        "a move literal without expected context retains its move Function identity"
    );
}
