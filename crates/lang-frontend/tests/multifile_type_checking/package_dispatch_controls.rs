//! Package qualifiers must not become runtime Map receivers because a member is named consume.
use super::*;
use lang_frontend::type_checking::complete_compilation_unit_types;

#[test]
fn package_consume_calls_and_same_named_local_value_keep_source_identity() {
    const PROVIDER: &str = include_str!("package_dispatch_controls/p/provider.ko");
    const DEEP: &str = include_str!("package_dispatch_controls/p/deep/provider.ko");
    const CONSUMER: &str = include_str!("package_dispatch_controls/q/consumer.ko");
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", PROVIDER);
    let (deep_source, deep) = parsed(&mut sources, "p/deep/provider.ko", DEEP);
    let (consumer_source, consumer) = parsed(&mut sources, "q/consumer.ko", CONSUMER);
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "p/deep/provider.ko", deep_source, &deep),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let unit = source_unit(&names, consumer_source);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("ordinary package and local-value calls type check internally");

    // Source-qualified declaration spans distinguish the two deep overloads.
    let declaration_at = |source, text: &str, signature: &str| {
        let start = text.find(signature).expect("fixture declaration signature");
        let span = sources
            .span(source, start, start + "consume".len())
            .unwrap();
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.source_unit() == source_unit(&names, source)
                    && declaration.name_span() == span
            })
            .expect("exact source declaration")
            .id()
    };
    let expected_packages = [
        declaration_at(provider_source, PROVIDER, "consume(value: Int)"),
        declaration_at(deep_source, DEEP, "consume(value: Int)"),
        declaration_at(deep_source, DEEP, "consume(value: Boolean)"),
    ];
    let observations = [
        "p.consume(3)",
        "p.deep.consume(4)",
        "p.deep.consume(true)",
        "p.consume(5)",
    ]
    .map(|text| {
        let expression = expression_with_text(&sources, &consumer, text);
        let call_id = UnitExpressionId::new(unit, expression);
        let Expression::Call { callee, .. } = consumer
            .ast()
            .expressions()
            .get(expression)
            .expect("call AST")
            .payload()
        else {
            panic!("fixture expression is a call");
        };
        let Expression::Member { receiver, .. } = consumer
            .ast()
            .expressions()
            .get(*callee)
            .expect("callee AST")
            .payload()
        else {
            panic!("fixture callee has a qualifier or receiver");
        };
        let receiver_node = consumer
            .ast()
            .expressions()
            .get(*receiver)
            .expect("receiver AST");
        let receiver_span = match receiver_node.payload() {
            Expression::Name => receiver_node.span(),
            Expression::Member { name_span, .. } => *name_span,
            _ => panic!("fixture receiver is a name or package path"),
        };
        let references = names
            .names()
            .references()
            .iter()
            .filter(|reference| {
                reference.source_unit() == unit && reference.span() == receiver_span
            })
            .collect::<Vec<_>>();
        (
            text,
            call_id,
            typed.call(call_id),
            typed.expression_type(call_id),
            typed.map_consume(call_id),
            UnitExpressionId::new(unit, *receiver),
            references,
        )
    });
    let raw_ok = typed.clone().validate().is_ok();
    let complete =
        complete_compilation_unit_types(&sources, &inputs, &names, &type_environment, &typed)
            .map(|_| ());
    // Collect and report every call and both admission results before asserting behavior.
    eprintln!(
        "package dispatch controls: diagnostics={:?}; calls={observations:#?}; raw={raw_ok}; complete={complete:?}",
        typed.diagnostics()
    );

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(raw_ok, "ordinary calls retain raw type admission");
    complete.expect("ordinary package and local-value calls retain final admission");
    let int = typed.types().builtin(BuiltinType::Int).expect("Int");
    for (index, (text, id, call, expression_type, map_consume, receiver, references)) in
        observations.iter().enumerate()
    {
        let call = call.expect("ordinary source call descriptor");
        assert_eq!(call.expression(), *id, "{text}");
        assert_eq!(call.return_type(), int, "{text}");
        assert_eq!(*expression_type, Some(int), "{text}");
        assert!(
            map_consume.is_none(),
            "ordinary source call is not Map.consume: {text}"
        );
        assert_eq!(call.arguments().len(), 1, "{text}");
        let parameter = if index == 2 {
            BuiltinType::Boolean
        } else {
            BuiltinType::Int
        };
        assert_eq!(
            typed.types().get(call.arguments()[0].parameter_type()),
            Some(&UnitTypeKind::Builtin(parameter)),
            "{text}"
        );
        if index < expected_packages.len() {
            assert_eq!(
                call.target(),
                UnitCallTarget::Declaration(expected_packages[index]),
                "{text}"
            );
            assert!(
                call.receiver().is_none(),
                "package qualifier has no runtime receiver: {text}"
            );
            assert!(
                references.iter().any(|reference| {
                    reference.namespace().is_none()
                        && matches!(reference.target(), UnitReferenceTarget::Package(_))
                }),
                "real package reference: {text}"
            );
        } else {
            let resolution = names.names().source_units()[unit.index()].resolution();
            let method_start = CONSUMER.find("consume(value: Int)").unwrap();
            let method_span = sources
                .span(
                    consumer_source,
                    method_start,
                    method_start + "consume".len(),
                )
                .unwrap();
            let method = resolution
                .symbols()
                .iter()
                .find(|symbol| {
                    symbol.span() == method_span && symbol.namespace() == Namespace::Value
                })
                .expect("UserValue.consume source symbol");
            let UnitCallTarget::Symbol(target) = call.target() else {
                panic!("UserValue.consume is a source-local callable");
            };
            assert_eq!(target.source_unit(), unit);
            assert_eq!(target.symbol(), method.id());
            assert_eq!(method.kind(), SymbolKind::Function);
            assert_eq!(
                call.receiver().expect("value receiver").origin(),
                UnitCallReceiverOrigin::Expression(*receiver)
            );
            let local_start = CONSUMER.find("val p =").unwrap() + "val ".len();
            let local_span = sources
                .span(consumer_source, local_start, local_start + 1)
                .unwrap();
            let local = resolution
                .symbols()
                .iter()
                .find(|symbol| {
                    symbol.span() == local_span && symbol.namespace() == Namespace::Value
                })
                .expect("local p declaration symbol");
            assert_eq!(local.kind(), SymbolKind::Variable);
            assert!(
                references.iter().any(|reference| {
                    reference.namespace() == Some(Namespace::Value)
                        && matches!(reference.target(), UnitReferenceTarget::Symbol(symbol)
                            if symbol.source_unit() == unit && symbol.symbol() == local.id())
                }),
                "receiver p resolves to the actual local value"
            );
            assert!(
                references.iter().all(|reference| !matches!(
                    reference.target(),
                    UnitReferenceTarget::Package(_)
                ))
            );
        }
    }
    assert!(typed.map_consumes().is_empty());
}
