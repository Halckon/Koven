//! The method comes from its original classifier body, never a top-level name match.
use super::super::{
    births::ast::Site,
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::Case;
use lang_frontend::{
    ast::{ExpressionId, ItemId},
    parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
};
#[derive(Debug)]
pub(super) struct Input {
    pub class: ItemId,
    pub class_name: Span,
    pub method: ItemId,
    pub method_span: Span,
    pub method_name: Span,
    pub parameter: Span,
    pub entry: Span,
    pub make: Span,
    pub reader: Span,
    pub observe: Span,
    pub receiver_name: Span,
    pub receiver_birth: ExpressionId,
    pub source_name: Span,
    pub source_birth: ExpressionId,
    pub member_call: ExpressionId,
    pub receiver: ExpressionId,
    pub argument: ExpressionId,
    pub member_name: Span,
    pub site: Option<Site>,
    pub control: Option<(ExpressionId, ExpressionId)>,
}
pub(super) fn span(p: &ParsedFile, e: ExpressionId) -> Span {
    p.ast().expressions().get(e).unwrap().span()
}
fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() < inner.start()
        && inner.end() < outer.end()
}
fn top(s: &SourceMap, p: &ParsedFile, name: &str, arity: usize) -> Result<Span, Failure> {
    one(p.roots().iter().filter_map(|id| {
        let n = p.ast().items().get(*id).ok()?;
        matches!(n.payload(), Item::Function { name: NameMarker::Present(marker), parameters, .. }
            if s.slice(*marker) == Ok(name) && parameters.len() == arity).then_some(n.span())
    }), "P24 original top-level callable AST/arity")
}
fn call(s: &SourceMap, p: &ParsedFile, scope: Span, name: &str) -> Result<ExpressionId, Failure> {
    one(
        p.ast().expressions().iter().filter_map(|(id, n)| {
            let Expression::Call { callee, .. } = n.payload() else {
                return None;
            };
            (contains(scope, n.span()) && s.slice(span(p, *callee)) == Ok(name)).then_some(id)
        }),
        "P24 original Call inside actual callable",
    )
}
fn local(
    s: &SourceMap,
    p: &ParsedFile,
    scope: Span,
    name: &str,
) -> Result<(Span, ExpressionId), Failure> {
    one(
        p.ast()
            .items()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Item::Variable {
                    name: NameMarker::Present(marker),
                    initializer,
                    ..
                } if contains(scope, n.span()) && s.slice(*marker) == Ok(name) => {
                    Some((*marker, *initializer))
                }
                _ => None,
            }),
        "P24 original caller local declaration/initializer",
    )
}
pub(super) fn input(c: &Case, s: &SourceMap, p: &ParsedFile) -> Result<Input, Failure> {
    let (class, class_name, method) = one(
        p.roots().iter().filter_map(|id| {
            let Item::Classifier(class) = p.ast().items().get(*id).ok()?.payload() else {
                return None;
            };
            let NameMarker::Present(name) = class.name else {
                return None;
            };
            if s.slice(name) != Ok("Reader") {
                return None;
            }
            let [method] = class.body.as_ref()?.members.as_slice() else {
                return None;
            };
            Some((*id, name, *method))
        }),
        "P24 actual Reader classifier and sole real member",
    )?;
    let method_node = p.ast().items().get(method).unwrap();
    let Item::Function {
        name: NameMarker::Present(method_name),
        parameters,
        extension_receiver: None,
        ..
    } = method_node.payload()
    else {
        return Err(failure("P24 original instance function AST", method_node));
    };
    let [parameter] = parameters.as_slice() else {
        return Err(failure(
            "P24 method exactly one explicit parameter",
            parameters,
        ));
    };
    let NameMarker::Present(parameter_name) = parameter.name else {
        return Err(failure("P24 real Map parameter marker", parameter));
    };
    need(
        s.slice(*method_name) == Ok("scan")
            && parameter.mode_marker.is_none()
            && s.slice(p.ast().type_refs().get(parameter.type_ref).unwrap().span())
                == Ok("Map<String,String>"),
        "P24 real method/default Borrow Map syntax",
        method_node,
    )?;
    let entry = top(s, p, "entry", 0)?;
    let (receiver_name, receiver_birth) = local(s, p, entry, "reader")?;
    let (source_name, source_birth) = local(s, p, entry, "source")?;
    let member_call = call(s, p, entry, "reader.scan")?;
    let Expression::Call {
        callee,
        arguments,
        type_arguments,
        ..
    } = p.ast().expressions().get(member_call).unwrap().payload()
    else {
        unreachable!()
    };
    let Expression::Member {
        receiver,
        name_span,
        safe: false,
        ..
    } = p.ast().expressions().get(*callee).unwrap().payload()
    else {
        return Err(failure("P24 actual nonnullable member callee", callee));
    };
    let [argument] = arguments.as_slice() else {
        return Err(failure("P24 actual member one argument", arguments));
    };
    need(
        type_arguments.is_empty()
            && argument.named_prefix.is_none()
            && argument.mode_marker.is_none(),
        "P24 original plain member argument",
        arguments,
    )?;
    let site = if c.provider {
        let (statement, n, names, source, body) = one(
            p.ast()
                .statements()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Statement::For {
                        binding: ForBinding::Destructuring { names, .. },
                        source,
                        body,
                        ..
                    } if contains(method_node.span(), n.span()) => {
                        Some((id, n, names, *source, *body))
                    }
                    _ => None,
                }),
            "P24 For is inside this actual method",
        )?;
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(failure("P24 original K/V markers", names));
        };
        let read = call(s, p, n.span(), "read")?;
        let Expression::Call { arguments, .. } = p.ast().expressions().get(read).unwrap().payload()
        else {
            unreachable!()
        };
        let [key_arg, value_arg] = arguments.as_slice() else {
            return Err(failure("P24 actual read arity", arguments));
        };
        Some(Site {
            statement,
            span: n.span(),
            source,
            body,
            bindings: [*key, *value],
            read,
            arguments: [key_arg.value, value_arg.value],
        })
    } else {
        None
    };
    let control = if c.provider {
        None
    } else {
        let observed = call(s, p, method_node.span(), "observe")?;
        let Expression::Call { arguments, .. } =
            p.ast().expressions().get(observed).unwrap().payload()
        else {
            unreachable!()
        };
        let [argument] = arguments.as_slice() else {
            return Err(failure("P24 original control observe argument", arguments));
        };
        Some((observed, argument.value))
    };
    Ok(Input {
        class,
        class_name,
        method,
        method_span: method_node.span(),
        method_name: *method_name,
        parameter: parameter_name,
        entry,
        make: top(s, p, "make", 0)?,
        reader: top(s, p, "read", 2)?,
        observe: top(s, p, "observe", 1)?,
        receiver_name,
        receiver_birth,
        source_name,
        source_birth,
        member_call,
        receiver: *receiver,
        argument: argument.value,
        member_name: *name_span,
        site,
        control,
    })
}
