//! Exact ordinary source Call and argument If; independent of expression arena slots.
use super::*;
use lang_frontend::{
    ast::StatementId,
    parser::{IntegerLiteralKind, Item, LiteralKind, NameMarker, Statement},
};
pub(super) struct Loop {
    pub span: Span,
    pub body: StatementId,
    pub read: Span,
    pub arguments: [Span; 2],
}
pub(super) struct Input {
    pub scan: Span,
    pub factory: Span,
    pub call: Span,
    pub argument: Span,
    pub flag: Span,
    pub arms: [Span; 2],
    pub loop_site: Option<Loop>,
    pub size: Option<Span>,
    pub drop_origin: Span,
    pub construct: Span,
    pub factory_return: Span,
    pub mutable: bool,
}
fn inside(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && inner.start() > outer.start()
        && inner.end() < outer.end()
}
pub(super) fn input(sources: &SourceMap, parsed: &ParsedFile) -> Result<Input, Failure> {
    let expressions = parsed.ast().expressions();
    let (call_id, call, argument) = one(
        expressions.iter().filter_map(|(id, n)| match n.payload() {
            Expression::Call {
                callee, arguments, ..
            } if sources.slice(expressions.get(*callee).ok()?.span()) == Ok("produce")
                && arguments.len() == 1 =>
            {
                Some((id, n.span(), arguments[0].value))
            }
            _ => None,
        }),
        "original unique produce Call",
    )?;
    let scan = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            (inside(call, n.span()) && matches!(n.payload(), Item::Function { .. })).then_some(n)
        }),
        "actual containing scanner declaration",
    )?;
    let Item::Function { parameters, .. } = scan.payload() else {
        unreachable!()
    };
    let [parameter] = parameters.as_slice() else {
        return Err(failure("scanner original one Boolean parameter", scan));
    };
    let NameMarker::Present(parameter_name) = parameter.name else {
        return Err(failure("scanner real parameter Name", parameter));
    };
    need(
        parameter.mode_marker.is_none()
            && sources.slice(parameter_name) == Ok("flag")
            && sources.slice(
                parsed
                    .ast()
                    .type_refs()
                    .get(parameter.type_ref)
                    .unwrap()
                    .span(),
            ) == Ok("Boolean"),
        "original scanner Borrow Boolean syntax",
        parameter,
    )?;
    let factory = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            let Item::Function {
                name: NameMarker::Present(name),
                ..
            } = n.payload()
            else {
                return None;
            };
            (sources.slice(*name) == Ok("produce")).then_some(n)
        }),
        "original selected produce declaration",
    )?;
    let Item::Function { parameters, .. } = factory.payload() else {
        unreachable!()
    };
    let [p] = parameters.as_slice() else {
        return Err(failure("factory original one Int parameter", factory));
    };
    let NameMarker::Present(factory_parameter) = p.name else {
        return Err(failure("factory parameter original Name", p));
    };
    need(
        p.mode_marker.is_none()
            && sources.slice(factory_parameter) == Ok("input")
            && sources.slice(parsed.ast().type_refs().get(p.type_ref).unwrap().span()) == Ok("Int"),
        "factory original Borrow Int syntax",
        p,
    )?;
    let argument_node = expressions.get(argument).unwrap();
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(no),
        ..
    } = argument_node.payload()
    else {
        return Err(failure("original actual argument If", argument_node));
    };
    let flag = expressions.get(*condition).unwrap();
    need(
        flag.payload() == &Expression::Name && sources.slice(flag.span()) == Ok("flag"),
        "original argument condition flag Name",
        flag,
    )?;
    let arm = |statement, wanted| -> Result<Span, Failure> {
        let n = parsed.ast().statements().get(statement).unwrap();
        // parse_if -> parse_control_body -> parse_control_block preserves a
        // ControlBody for a braced value arm; an ordinary Block is a different node.
        let Statement::ControlBody { elements } = n.payload() else {
            return Err(failure(
                "original argument arm ControlBody",
                (n, n.payload()),
            ));
        };
        let [statement] = elements.as_slice() else {
            return Err(failure("original single literal arm", n));
        };
        let n = parsed.ast().statements().get(*statement).unwrap();
        let Statement::Expression { expression } = n.payload() else {
            return Err(failure("original literal arm statement", n));
        };
        let n = expressions.get(*expression).unwrap();
        need(
            n.payload()
                == &Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed))
                && sources.slice(n.span()) == Ok(wanted),
            "original selected arm literal",
            n,
        )?;
        Ok(n.span())
    };
    let arms = [arm(*then_branch, "1")?, arm(*no, "2")?];
    let loops = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, n)| match n.payload() {
            Statement::For { source, body, .. } if inside(n.span(), scan.span()) => {
                Some((n.span(), *source, *body))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let loop_site = if let [(span, source, body)] = loops.as_slice() {
        need(
            *source == call_id,
            "original For source is the selected Call itself",
            source,
        )?;
        let (read, args) = one(
            expressions.iter().filter_map(|(_, n)| match n.payload() {
                Expression::Call {
                    callee, arguments, ..
                } if inside(n.span(), *span)
                    && sources.slice(expressions.get(*callee).ok()?.span()) == Ok("read")
                    && arguments.len() == 2 =>
                {
                    Some((n.span(), [arguments[0].value, arguments[1].value]))
                }
                _ => None,
            }),
            "actual body read Call",
        )?;
        let arguments = args.map(|id| expressions.get(id).unwrap().span());
        need(
            arguments
                .iter()
                .zip(["key", "value"])
                .all(|(s, w)| sources.slice(*s) == Ok(w)),
            "original body K/V argument order",
            arguments,
        )?;
        Some(Loop {
            span: *span,
            body: *body,
            read,
            arguments,
        })
    } else {
        need(loops.is_empty(), "exact zero or one original For", &loops)?;
        None
    };
    let (size, drop_origin) = if loop_site.is_none() {
        let marker = one(
            parsed
                .ast()
                .items()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Item::Variable {
                        name: NameMarker::Present(name),
                        initializer,
                        ..
                    } if *initializer == call_id => Some(*name),
                    _ => None,
                }),
            "ordinary control original owner binding",
        )?;
        let size = one(
            expressions.iter().filter_map(|(_, n)| match n.payload() {
                Expression::Member {
                    receiver,
                    name_span,
                    ..
                } if inside(n.span(), scan.span())
                    && sources.slice(*name_span) == Ok("size")
                    && sources.slice(expressions.get(*receiver).ok()?.span())
                        == sources.slice(marker) =>
                {
                    Some(n.span())
                }
                _ => None,
            }),
            "control actual source size",
        )?;
        (Some(size), marker)
    } else {
        (None, call)
    };
    let (construct, mutable) = one(
        expressions.iter().filter_map(|(_, n)| {
            let Expression::Call {
                callee, arguments, ..
            } = n.payload()
            else {
                return None;
            };
            if !inside(n.span(), factory.span()) || !arguments.is_empty() {
                return None;
            }
            match sources.slice(expressions.get(*callee).ok()?.span()) {
                Ok("mapOf") => Some((n.span(), false)),
                Ok("mutableMapOf") => Some((n.span(), true)),
                _ => None,
            }
        }),
        "factory original empty Map construction",
    )?;
    let factory_return = one(
        expressions.iter().filter_map(|(_, n)| match n.payload() {
            Expression::Return { value: Some(v), .. }
                if inside(n.span(), factory.span())
                    && expressions.get(*v).ok()?.span() == construct =>
            {
                Some(n.span())
            }
            _ => None,
        }),
        "factory returns actual empty construct",
    )?;
    Ok(Input {
        scan: scan.span(),
        factory: factory.span(),
        call,
        argument: argument_node.span(),
        flag: flag.span(),
        arms,
        loop_site,
        size,
        drop_origin,
        construct,
        factory_return,
        mutable,
    })
}
