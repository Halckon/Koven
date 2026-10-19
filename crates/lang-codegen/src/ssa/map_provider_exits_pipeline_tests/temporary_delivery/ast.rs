//! Real source-order argument Calls, named mapping and the selected declaration.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Delivery};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{
        Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParameterModeMarker,
        ParsedFile, Statement,
    },
    source::{SourceMap, Span},
    type_checking::ParameterMode,
};

#[derive(Debug)]
pub(super) struct Parameter {
    pub name: Span,
    pub span: Span,
    pub mode: ParameterMode,
}
#[derive(Debug)]
pub(super) struct Function {
    pub span: Span,
    pub name: Span,
    pub body: StatementId,
    pub parameters: Vec<Parameter>,
}
#[derive(Debug)]
pub(super) struct Argument {
    pub span: Span,
    pub expression: ExpressionId,
    pub parameter: usize,
    pub function: Function,
}
#[derive(Debug)]
pub(super) struct Site {
    pub statement: StatementId,
    pub span: Span,
    pub body: StatementId,
    pub source: ExpressionId,
    pub call: Span,
    pub bindings: [Span; 2],
    pub read: ExpressionId,
    pub arguments: [ExpressionId; 2],
}
pub(super) struct Input {
    pub entry: Function,
    pub factory: Function,
    pub reader: Function,
    pub call: ExpressionId,
    pub arguments: Vec<Argument>,
    pub site: Option<Site>,
    pub after: Span,
    pub control_root: Option<Span>,
    pub size: Option<ExpressionId>,
}

pub(super) fn expression(parsed: &ParsedFile, id: ExpressionId) -> Span {
    parsed.ast().expressions().get(id).unwrap().span()
}
pub(super) fn inside(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && inner.start() > outer.start()
        && inner.end() < outer.end()
}

pub(super) fn function(
    sources: &SourceMap,
    parsed: &ParsedFile,
    wanted: &str,
) -> Result<Function, Failure> {
    let node=one(parsed.roots().iter().filter_map(|id|parsed.ast().items().get(*id).ok())
        .filter(|n|matches!(n.payload(),Item::Function {name:NameMarker::Present(name),..} if sources.slice(*name)==Ok(wanted))),"delivery original function declaration")?;
    let Item::Function {
        name: NameMarker::Present(name),
        form,
        parameters,
        ..
    } = node.payload()
    else {
        unreachable!()
    };
    let body = match form {
        FunctionForm::ImplicitUnitBlock(body)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => *body,
        other => return Err(failure("delivery original callable block", other)),
    };
    let parameters = parameters
        .iter()
        .map(|p| {
            let NameMarker::Present(name) = p.name else {
                return Err(failure("delivery actual parameter marker", p));
            };
            let mode = match p.mode_marker {
                None | Some(ParameterModeMarker::Borrow(_)) => ParameterMode::Borrow,
                Some(ParameterModeMarker::Own(_)) => ParameterMode::Value,
                other => return Err(failure("delivery actual parameter mode", other)),
            };
            Ok(Parameter {
                name,
                span: p.span,
                mode,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    Ok(Function {
        span: node.span(),
        name: *name,
        body,
        parameters,
    })
}

fn call(
    sources: &SourceMap,
    parsed: &ParsedFile,
    scope: Span,
    wanted: &str,
) -> Result<ExpressionId, Failure> {
    one(
        parsed.ast().expressions().iter().filter_map(|(id, n)| {
            let Expression::Call { callee, .. } = n.payload() else {
                return None;
            };
            (inside(n.span(), scope) && sources.slice(expression(parsed, *callee)) == Ok(wanted))
                .then_some(id)
        }),
        "delivery actual source Call",
    )
}

pub(super) fn input(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    control: bool,
) -> Result<Input, Failure> {
    let entry = function(sources, parsed, "entry")?;
    let factory = function(sources, parsed, "produce")?;
    let reader = function(sources, parsed, "read")?;
    let call = call(sources, parsed, entry.span, "produce")?;
    let Expression::Call { arguments, .. } =
        parsed.ast().expressions().get(call).unwrap().payload()
    else {
        unreachable!()
    };
    let expected = match case.delivery {
        Delivery::BorrowAndOwnedResource => vec![("borrowedArgument", 0), ("ownedArgument", 1)],
        Delivery::SameMapBorrow => vec![("borrowedArgument", 0)],
        Delivery::ReorderedMixed => vec![
            ("rightArgument", 1),
            ("ownedArgument", 2),
            ("leftArgument", 0),
        ],
    };
    need(
        arguments.len() == expected.len() && factory.parameters.len() == arguments.len(),
        "delivery actual arity matches fixed fixture",
        arguments,
    )?;
    let arguments = arguments
        .iter()
        .zip(expected)
        .enumerate()
        .map(|(index, (argument, (wanted, expected_slot)))| {
            need(
                argument.mode_marker.is_none(),
                "delivery ordinary Value/Borrow has no Inout marker",
                argument,
            )?;
            let parameter = if let Some(prefix) = argument.named_prefix {
                one(
                    factory
                        .parameters
                        .iter()
                        .enumerate()
                        .filter_map(|(slot, p)| {
                            (sources.slice(p.name) == sources.slice(prefix.name_span))
                                .then_some(slot)
                        }),
                    "delivery original named parameter mapping",
                )?
            } else {
                index
            };
            let node = parsed.ast().expressions().get(argument.value).unwrap();
            let Expression::Call {
                callee, arguments, ..
            } = node.payload()
            else {
                return Err(failure("delivery actual argument evaluator Call", node));
            };
            need(
                parameter == expected_slot
                    && arguments.is_empty()
                    && sources.slice(expression(parsed, *callee)) == Ok(wanted)
                    && matches!(
                        parsed.ast().expressions().get(*callee).unwrap().payload(),
                        Expression::Name
                    ),
                "delivery source-order evaluator and actual named slot",
                (index, parameter, node),
            )?;
            need(
                factory.parameters[parameter].mode
                    == if wanted == "ownedArgument" {
                        ParameterMode::Value
                    } else {
                        ParameterMode::Borrow
                    },
                "delivery actual Value/Borrow parameter syntax",
                &factory.parameters[parameter],
            )?;
            let function = function(sources, parsed, wanted)?;
            need(
                function.parameters.is_empty(),
                "delivery evaluator has no arguments",
                &function,
            )?;
            Ok(Argument {
                span: argument.span,
                expression: argument.value,
                parameter,
                function,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let loops = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, n)| matches!(n.payload(), Statement::For { .. }).then_some((id, n)))
        .collect::<Vec<_>>();
    need(
        loops.len() == usize::from(!control),
        "delivery original provider/control distinction",
        &loops,
    )?;
    let site = if control {
        None
    } else {
        let (statement, node) = loops[0];
        let Statement::For {
            binding: ForBinding::Destructuring { names, .. },
            source,
            body,
            ..
        } = node.payload()
        else {
            return Err(failure("delivery original For shape", node));
        };
        let [NameMarker::Present(key), NameMarker::Present(val)] = names.as_slice() else {
            return Err(failure("delivery actual component markers", names));
        };
        let read = self::call(sources, parsed, node.span(), "read")?;
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(read).unwrap().payload()
        else {
            unreachable!()
        };
        need(
            *source == call && arguments.len() == 2,
            "delivery actual factory Call is provider source",
            node,
        )?;
        Some(Site {
            statement,
            span: node.span(),
            body: *body,
            source: *source,
            call: expression(parsed, *source),
            bindings: [*key, *val],
            read,
            arguments: [arguments[0].value, arguments[1].value],
        })
    };
    let (control_root, size) = if control {
        let root = one(
            parsed.ast().items().iter().filter_map(|(_, n)| {
                let Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } = n.payload()
                else {
                    return None;
                };
                (inside(n.span(), entry.span) && *initializer == call).then_some(*name)
            }),
            "delivery actual control result binding",
        )?;
        let size=one(parsed.ast().expressions().iter().filter_map(|(id,n)|{
            (inside(n.span(),entry.span)&&matches!(n.payload(),Expression::Member {name_span,..} if sources.slice(*name_span)==Ok("size"))).then_some(id)
        }),"delivery actual control size")?;
        (Some(root), Some(size))
    } else {
        (None, None)
    };
    let after=one(parsed.ast().expressions().iter().filter_map(|(_,n)|{
        let Expression::Call {callee,arguments,..}=n.payload() else{return None;};
        (inside(n.span(),entry.span)&&sources.slice(expression(parsed,*callee))==Ok("println")
            &&matches!(arguments.as_slice(),[arg] if sources.slice(expression(parsed,arg.value))==Ok("\"after\""))).then_some(n.span())
    }),"delivery actual after output")?;
    Ok(Input {
        entry,
        factory,
        reader,
        call,
        arguments,
        site,
        after,
        control_root,
        size,
    })
}
