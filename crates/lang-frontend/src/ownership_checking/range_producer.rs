//! 新 descriptor producer 的有界返回依赖证明；递归环不能从签名自举出证明。
//! 候选仅在同一轮 typed 身份上建立，body dataflow 再验证根；失败时不发布任何证明。
mod unit;
use super::{OwnershipCheckingError, borrow_result::marker_span};
use crate::{
    ast::ExpressionId,
    name_resolution::{NameResolution, ReferenceTarget},
    parser::{
        BorrowReturnSource, Expression, FunctionBody, FunctionForm, FunctionResultSource, Item,
        NameMarker, ParsedFile, Statement,
    },
    source::{SourceMap, Span},
    type_checking::{BorrowReturnOrigin, CallableResultSource, CallableTarget, TypedFile},
};
pub(super) use unit::collect_unit;

struct Candidate {
    name: Span,
    dependency: Option<Span>,
}

fn finish(candidates: Vec<Candidate>) -> Vec<Span> {
    let mut proven = Vec::new();
    loop {
        let before = proven.len();
        for candidate in &candidates {
            if !proven.contains(&candidate.name)
                && candidate
                    .dependency
                    .is_none_or(|dependency| proven.contains(&dependency))
            {
                proven.push(candidate.name);
            }
        }
        if before == proven.len() {
            return proven;
        }
    }
}

// 首片只证明 expression body 或唯一 terminal return；其他控制流不靠 from 猜测。
fn returned_expression(
    parsed: &ParsedFile,
    body: FunctionBody,
) -> Result<Option<ExpressionId>, OwnershipCheckingError> {
    match body {
        FunctionBody::Expression { expression, .. } => Ok(Some(expression)),
        FunctionBody::Absent => Ok(None),
        FunctionBody::Block(body) => {
            let block = parsed.ast().statements().get(body)?;
            let Statement::Block { elements } = block.payload() else {
                return Ok(None);
            };
            let Some(last) = elements.last() else {
                return Ok(None);
            };
            let Statement::Expression { expression } =
                parsed.ast().statements().get(*last)?.payload()
            else {
                return Ok(None);
            };
            let Expression::Return {
                value: Some(value), ..
            } = parsed.ast().expressions().get(*expression)?.payload()
            else {
                return Ok(None);
            };
            let returns = parsed
                .ast()
                .expressions()
                .iter()
                .filter(|(_, node)| {
                    node.span().start() >= block.span().start()
                        && node.span().end() <= block.span().end()
                        && matches!(node.payload(), Expression::Return { .. })
                })
                .count();
            Ok((returns == 1).then_some(*value))
        }
    }
}

fn candidates(
    sources: &SourceMap,
    parsed: &ParsedFile,
    parameter_reference: impl Fn(ExpressionId) -> Option<Span>,
    receiver_declaration: impl Fn(Span) -> Option<Span>,
    construction_path: impl Fn(ExpressionId) -> Option<(ExpressionId, Option<Span>)>,
) -> Result<Vec<Candidate>, OwnershipCheckingError> {
    let mut result = Vec::new();
    for (_, item) in parsed.ast().items().iter() {
        let Item::Function {
            name,
            parameters,
            form:
                FunctionForm::Explicit {
                    result_source: Some(FunctionResultSource::Carrier(syntax)),
                    body,
                    ..
                },
            ..
        } = item.payload()
        else {
            continue;
        };
        let expected = match syntax.source {
            BorrowReturnSource::Parameter(NameMarker::Present(source)) => parameters
                .iter()
                .find(|parameter| {
                    sources.slice(marker_span(parameter.name)).ok() == sources.slice(source).ok()
                })
                .map(|parameter| marker_span(parameter.name)),
            BorrowReturnSource::Receiver(_) => receiver_declaration(marker_span(*name)),
            _ => None,
        };
        let Some(expected) = expected else {
            continue;
        };
        let Some(mut returned) = returned_expression(parsed, *body)? else {
            continue;
        };
        while let Expression::Group { expression } =
            parsed.ast().expressions().get(returned)?.payload()
        {
            returned = *expression;
        }
        let Some((argument, dependency)) = construction_path(returned) else {
            continue;
        };
        if parameter_reference(argument) == Some(expected) {
            result.push(Candidate {
                name: marker_span(*name),
                dependency,
            });
        }
    }
    Ok(result)
}

pub(super) fn source_argument(
    parsed: &ParsedFile,
    expression: ExpressionId,
    parameter: usize,
    mut arguments: impl Iterator<Item = (usize, usize)>,
) -> Option<ExpressionId> {
    let Expression::Call {
        arguments: syntax, ..
    } = parsed.ast().expressions().get(expression).ok()?.payload()
    else {
        return None;
    };
    let (_, index) = arguments.find(|(p, _)| *p == parameter)?;
    syntax.get(index).map(|argument| argument.value)
}

pub(super) fn collect_single(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
) -> Result<Vec<Span>, OwnershipCheckingError> {
    let candidates = candidates(
        sources,
        parsed,
        |expression| {
            let node = parsed.ast().expressions().get(expression).ok()?;
            if !matches!(node.payload(), Expression::Name | Expression::This) {
                return None;
            }
            let reference = names
                .references()
                .iter()
                .find(|r| r.span() == node.span())?;
            let ReferenceTarget::Symbol(symbol) = reference.target() else {
                return None;
            };
            names
                .symbols()
                .get(symbol.index())
                .map(|symbol| symbol.span())
        },
        |name| {
            let callable = typed
                .callables()
                .iter()
                .find(|c| names.symbols()[c.symbol().index()].span() == name)?;
            callable.range_extension()?;
            names
                .symbols()
                .get(callable.extension_receiver_symbol()?.index())
                .map(|s| s.span())
        },
        |expression| {
            let call = typed.call(expression)?;
            if let Some(range) = call.range_construction() {
                return Some((range.source(), None));
            }
            let CallableResultSource::Carrier(contract) = call.result_source() else {
                return None;
            };
            let source = match contract.origin() {
                BorrowReturnOrigin::Parameter(parameter) => source_argument(
                    parsed,
                    expression,
                    parameter,
                    call.arguments()
                        .iter()
                        .map(|a| (a.parameter_index(), a.argument_index())),
                )?,
                BorrowReturnOrigin::Receiver => match call.receiver()?.origin() {
                    crate::type_checking::CallReceiverOrigin::Expression(id) => id,
                    _ => return None,
                },
            };
            let CallableTarget::Source(target) = call.target() else {
                return None;
            };
            let name = names.symbols().get(target.index())?.span();
            Some((source, Some(name)))
        },
    )?;
    Ok(finish(candidates))
}
