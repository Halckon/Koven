//! SPEC-0291: ordinary local receivers retain both concrete Map type arguments.

use lang_frontend::{
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    type_checking::{
        BuiltinType, IntrinsicTypeConstructor, UnitExpressionId, UnitTypeKind,
        check_compilation_unit_types, standard_environments,
    },
};

#[test]
fn mutable_map_consume_local_roots_preserve_exact_key_value_types() {
    let cases = [
        (
            "val-empty-int-int",
            "fun run(): Unit {\n\
             val source = mutableMapOf<Int, Int>()\n\
             val result = source.consume()\n\
             }",
            BuiltinType::Int,
            BuiltinType::Int,
        ),
        (
            "var-empty-string-string",
            "fun run(): Unit {\n\
             var source = mutableMapOf<String, String>()\n\
             val result = source.consume()\n\
             }",
            BuiltinType::String,
            BuiltinType::String,
        ),
        (
            "val-nonempty-string-int",
            "fun run(): Unit {\n\
             var seed = mutableMapOf<String, Int>()\n\
             seed.put(\"key\", 1)\n\
             val source = seed\n\
             val result = source.consume()\n\
             }",
            BuiltinType::String,
            BuiltinType::Int,
        ),
        (
            "var-nonempty-int-string",
            "fun run(): Unit {\n\
             var source = mutableMapOf<Int, String>()\n\
             source.put(1, \"value\")\n\
             val result = source.consume()\n\
             }",
            BuiltinType::Int,
            BuiltinType::String,
        ),
    ];
    let mut observations = Vec::with_capacity(cases.len() * 2);
    for (label, source, key, value) in cases {
        observations.extend(observe_both(label, source, key, value));
    }

    assert_eq!(observations.len(), 8, "all four sources need both entries");
    assert!(
        observations.iter().all(|(_, accepted)| *accepted),
        "all eight ordinary-source observations:\n{observations:#?}"
    );
}

fn observe_both(
    label: &str,
    text: &str,
    key: BuiltinType,
    value: BuiltinType,
) -> [(String, bool); 2] {
    let (sources, file) = super::parsed(text);
    let calls = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            (sources.slice(node.span()) == Ok("source.consume()")).then_some(id)
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 1, "{label}: one actual consume expression");
    let call = calls[0];
    let (environment, types) = standard_environments();
    super::ENTRIES.map(|entry| {
        let inputs = [entry.input(&file, "map_test.ko")];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        entry.assert_identity(&index, &file, "map_test.ko");
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("unit names")
            .validate()
            .expect("valid unit names");
        let source_unit = names.names().index().source_units()[0].id();
        let unit =
            check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit raw types");
        let unit_expected = UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Map,
            arguments: vec![
                unit.types().builtin(key).expect("standard key type"),
                unit.types().builtin(value).expect("standard value type"),
            ],
        };
        let unit_actual = unit
            .expression_type(UnitExpressionId::new(source_unit, call))
            .and_then(|ty| unit.types().get(ty));
        (
            format!(
                "{label}/{entry:?}: expected {unit_expected:?}, actual {unit_actual:?}, diagnostics {:?}",
                unit.diagnostics()
            ),
            unit.diagnostics().is_empty() && unit_actual == Some(&unit_expected),
        )
    })
}

#[test]
fn user_named_consume_keeps_ordinary_callable_identity() {
    let cases = [
        (
            "local-user-method",
            r#"class UserValue { fun consume(): Int { return 7 } }
fun run(): Unit {
  val receiver = UserValue()
  val result = receiver.consume()
}"#,
            "receiver.consume()",
            false,
        ),
        (
            "factory-user-method",
            r#"class UserValue { fun consume(): Int { return 7 } }
fun make(): UserValue { return UserValue() }
fun run(): Unit {
  val result = make().consume()
}"#,
            "make().consume()",
            true,
        ),
    ];
    let mut observations = Vec::with_capacity(4);
    for (label, text, call_text, factory) in cases {
        let mut sources = super::SourceMap::new();
        let source = sources
            .add_source("user_consume.ko", text)
            .expect("fixed source");
        let file = super::parse_file_twice(&sources, source, label);
        for entry in super::ENTRIES {
            observations.push((
                format!("{label}/{entry:?}"),
                observe_user_unit(&sources, &file, call_text, factory, entry),
            ));
        }
    }
    assert_eq!(observations.len(), 4);
    assert!(
        observations
            .iter()
            .all(|(_, result)| matches!(result, Ok((true, _)))),
        "all ordinary callable observations:\n{observations:#?}"
    );
}

fn user_calls(
    sources: &super::SourceMap,
    file: &super::ParsedFile,
    text: &str,
) -> Vec<lang_frontend::ast::ExpressionId> {
    file.ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            (matches!(
                node.payload(),
                lang_frontend::parser::Expression::Call { .. }
            ) && sources.slice(node.span()) == Ok(text))
            .then_some(id)
        })
        .collect()
}

fn user_symbol(
    names: &super::NameResolution,
    name: &str,
) -> Option<lang_frontend::name_resolution::SymbolId> {
    names
        .symbols()
        .iter()
        .find(|symbol| {
            symbol.name() == name
                && symbol.kind() == lang_frontend::name_resolution::SymbolKind::Function
        })
        .map(|symbol| symbol.id())
}

fn observe_user_unit(
    sources: &super::SourceMap,
    file: &super::ParsedFile,
    call_text: &str,
    factory: bool,
    entry: super::Entry,
) -> Result<(bool, String), String> {
    use lang_frontend::type_checking::{UnitCallTarget, complete_compilation_unit_types};
    if !file.diagnostics().is_empty() {
        return Err(format!(
            "parse {:?}; names/raw/complete not reached",
            file.diagnostics()
        ));
    }
    let (environment, types) = standard_environments();
    let inputs = [entry.input(file, "user_consume.ko")];
    let index = index_compilation_unit(sources, &inputs)
        .map_err(|error| format!("index internal: {error:?}"))?;
    entry.assert_identity(&index, file, "user_consume.ko");
    let names = resolve_compilation_unit_names(sources, &inputs, &index, &environment)
        .map_err(|error| format!("names internal: {error:?}"))?
        .validate()
        .map_err(|names| format!("names {:?}; raw/complete not reached", names.diagnostics()))?;
    let source = names.names().source_units()[0].source_unit();
    let local_names = names.names().source_units()[0].resolution();
    let typed = check_compilation_unit_types(sources, &inputs, &names, &types)
        .map_err(|error| format!("raw internal: {error:?}"))?;
    let calls = user_calls(sources, file, call_text)
        .into_iter()
        .map(|call| UnitExpressionId::new(source, call))
        .collect::<Vec<_>>();
    let method = user_symbol(local_names, "consume");
    let method_descriptors = typed
        .calls()
        .iter()
        .filter(|descriptor| calls.contains(&descriptor.expression()))
        .collect::<Vec<_>>();
    let mut accepted = typed.diagnostics().is_empty() && calls.len() == 1 && method_descriptors.len() == 1
        && method.is_some() && method_descriptors.iter().all(|descriptor| {
            matches!(descriptor.target(), UnitCallTarget::Symbol(symbol) if symbol.source_unit() == source && Some(symbol.symbol()) == method)
                && typed.expression_type(descriptor.expression()) == typed.types().builtin(BuiltinType::Int)
        });
    let mut details = format!(
        "parse/names qualified; raw {:?}; method symbol {method:?}, calls {calls:?}, descriptors {method_descriptors:?}",
        typed.diagnostics()
    );
    if factory {
        let calls = user_calls(sources, file, "make()")
            .into_iter()
            .map(|call| UnitExpressionId::new(source, call))
            .collect::<Vec<_>>();
        let target = user_symbol(local_names, "make");
        let descriptors = typed
            .calls()
            .iter()
            .filter(|descriptor| calls.contains(&descriptor.expression()))
            .collect::<Vec<_>>();
        accepted &= calls.len() == 1
            && descriptors.len() == 1
            && target.is_some()
            && descriptors
                .iter()
                .all(|descriptor| match descriptor.target() {
                    UnitCallTarget::Declaration(declaration) => names
                        .names()
                        .declaration_symbol(declaration)
                        .is_some_and(|symbol| {
                            symbol.source_unit() == source && Some(symbol.symbol()) == target
                        }),
                    _ => false,
                });
        details.push_str(&format!(
            "; factory symbol {target:?}, calls {calls:?}, descriptors {descriptors:?}"
        ));
    }
    let map_counts = [
        typed.map_constructions().len(),
        typed.map_sizes().len(),
        typed.map_contains_calls().len(),
        typed.map_gets().len(),
        typed.map_require_values().len(),
        typed.map_with_values().len(),
        typed.map_puts().len(),
        typed.map_removes().len(),
        typed.map_consumes().len(),
    ];
    accepted &= map_counts.iter().all(|count| *count == 0)
        && typed.calls().len() == (if factory { 2 } else { 1 })
        && typed.constructions().len() == 1;
    details.push_str(&format!(
        "; Map counts {map_counts:?}; calls {:?}; constructions {:?}",
        typed.calls(),
        typed.constructions()
    ));
    match complete_compilation_unit_types(sources, &inputs, &names, &types, &typed) {
        Ok(complete) => {
            accepted &= complete.is_same_analysis(&typed);
            details.push_str("; complete original analysis");
        }
        Err(error) => {
            accepted = false;
            details.push_str(&format!("; complete {error:?}"));
        }
    }
    Ok((accepted, details))
}
