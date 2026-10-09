//! 函数结果/body 的封闭 AST 合同与错误恢复场景。
use super::*;

#[test]
fn function_payload_preserves_generic_signature_and_optional_body() {
    let text = "fun <T: pkg.Outer<Inner>> map(x: T, f: move (T) -> T): T = f(x)";
    let (sources, parsed) = parsed_ok(text);
    let Item::Function {
        name,
        extension_receiver,
        type_parameters,
        type_parameter_list_span,
        parameters,
        form,
    } = item(&parsed)
    else {
        panic!("function")
    };
    assert_eq!(sources.slice(marker_span(*name)).expect("name"), "map");
    assert!(extension_receiver.is_none());
    assert_eq!(type_parameters.len(), 1);
    assert_eq!(
        sources
            .slice(type_parameter_list_span.expect("list"))
            .expect("list"),
        "<T: pkg.Outer<Inner>>"
    );
    assert!(type_parameters[0].bound.is_some());
    assert_eq!(parameters.len(), 2);
    let FunctionForm::Explicit {
        colon_span,
        type_ref: return_type,
        result_source: None,
        body,
    } = form
    else {
        panic!("explicit function")
    };
    assert_eq!(sources.slice(*colon_span).expect("colon"), ":");
    assert!(matches!(
        type_ref(&parsed, *return_type),
        TypeRef::Qualified { .. }
    ));
    let FunctionBody::Expression {
        equals_span,
        expression: body,
    } = body
    else {
        panic!("expression body")
    };
    assert_eq!(sources.slice(*equals_span).expect("equals"), "=");
    assert!(matches!(
        expression(&parsed, *body),
        Expression::Call { .. }
    ));

    let (_, signature) = parsed_ok("fun idle(): Unit");
    let Item::Function {
        type_parameters,
        type_parameter_list_span,
        parameters,
        form,
        ..
    } = item(&signature)
    else {
        panic!("signature")
    };
    assert!(type_parameters.is_empty() && parameters.is_empty());
    assert!(type_parameter_list_span.is_none());
    assert!(matches!(
        form,
        FunctionForm::Explicit {
            body: FunctionBody::Absent,
            ..
        }
    ));
}

#[test]
fn missing_return_colon_fallback_preserves_the_expression_body() {
    let text = "fun f() = 1";
    let (_, parsed) = parsed(text);
    let Item::Function {
        form:
            FunctionForm::Explicit {
                colon_span,
                type_ref: return_type,
                result_source: None,
                body,
            },
        ..
    } = item(&parsed)
    else {
        panic!("function")
    };
    assert_eq!((colon_span.start(), colon_span.end()), (8, 8));
    assert!(matches!(type_ref(&parsed, *return_type), TypeRef::Error));
    let FunctionBody::Expression {
        expression: body, ..
    } = body
    else {
        panic!("preserved expression body")
    };
    assert!(matches!(expression(&parsed, *body), Expression::Literal(_)));
    let codes = parsed
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect::<Vec<_>>();
    assert_eq!(codes, ["L0021"]);
}
