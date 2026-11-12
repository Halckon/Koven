//! Same-analysis intrinsic descriptors, represented types and named/temporary owner obligations.
mod callable {
    //! Unit FunctionValue resolves the actual noncapturing lambda and selected Int overload.
    use super::*;
    use crate::ssa::map_provider_exits_pipeline_tests::temporary_arguments;
    pub(super) fn unit(
        input: Input<'_>,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
    ) -> Result<UnitDropFact, Failure> {
        let (_, sources, parsed, i) = input;
        let unit = one(
            names.names().source_units().iter(),
            "birth callable original unit",
        )?
        .source_unit();
        let e = |id| UnitExpressionId::new(unit, id);
        let lambda = i.lambda.unwrap();
        let callee = i.callee.unwrap();
        let factory = unit_root(names, i.factory.unwrap())?;
        let descriptor = owned
            .closure(e(lambda))
            .ok_or_else(|| failure("birth original qualified closure", lambda))?;
        let origin = owned
            .callable_origin(e(callee))
            .ok_or_else(|| failure("birth actual callable source provenance", callee))?;
        let call = typed
            .call(e(i.birth))
            .ok_or_else(|| failure("birth actual FunctionValue descriptor", i.birth))?;
        let reference = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, callee)),
            "birth original callable Name reference",
        )?;
        let fun = typed
            .expression_type(e(callee))
            .ok_or_else(|| failure("birth original function type", callee))?;
        need(
            descriptor.expression() == e(lambda)
                && !descriptor.move_owned()
                && owned.captures_of(e(lambda)).next().is_none()
                && origin.expression() == e(callee)
                && origin.span() == ast::span(parsed, callee)
                && origin.origin() == UnitCallableOrigin::Lambda(e(lambda))
                && reference.target() == &UnitReferenceTarget::Symbol(factory)
                && typed.symbol_type(factory) == Some(fun)
                && matches!(typed.types().get(fun),Some(UnitTypeKind::Function{move_only:false,parameters,return_type})if parameters.is_empty()&&*return_type==call.return_type())
                && call.target() == UnitCallTarget::FunctionValue
                && call.receiver().is_none()
                && call.arguments().is_empty()
                && call.instance().type_arguments().is_empty()
                && call.result_source() == CallableResultSource::Owned
                && call.borrow_return().is_none()
                && typed.call_borrow_return(e(i.birth)).is_none(),
            "birth exact Fn0 noncapturing mode/original lambda/owned Map return",
            (call, descriptor, origin),
        )?;
        let selected = temporary_arguments::ast::function(sources, parsed, "produce", Some("Int"))?;
        let rejected = temporary_arguments::ast::function(sources, parsed, "produce", Some("Boolean"))?;
        let evaluator = temporary_arguments::ast::function(sources, parsed, "argument", None)?;
        let declaration = |marker| {
            one(
                names
                    .names()
                    .index()
                    .declarations()
                    .iter()
                    .filter(|d| d.source_unit() == unit && d.name_span() == marker)
                    .map(|d| d.id()),
                "birth lambda actual callable declaration",
            )
        };
        let produce = i.produce.unwrap();
        let argument = i.argument.unwrap();
        let inner = typed
            .call(e(produce))
            .ok_or_else(|| failure("birth lambda actual producer Call", produce))?;
        let evaluate = typed
            .call(e(argument))
            .ok_or_else(|| failure("birth lambda actual argument Call", argument))?;
        let signature = typed
            .signatures()
            .declaration(declaration(selected.name)?)
            .and_then(|d| d.callable())
            .ok_or_else(|| failure("birth lambda original Int signature", selected.name))?;
        need(
            inner.target() == UnitCallTarget::Declaration(declaration(selected.name)?)
                && inner.target() != UnitCallTarget::Declaration(declaration(rejected.name)?)
                && inner.result_source() == CallableResultSource::Owned
                && inner.borrow_return().is_none()
                && inner.return_type() == call.return_type()
                && signature.return_type() == call.return_type()
                && signature.parameters().len() == 1
                && signature.parameters()[0].mode() == ParameterMode::Borrow
                && typed.types().get(signature.parameters()[0].ty())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                && evaluate.target() == UnitCallTarget::Declaration(declaration(evaluator.name)?)
                && evaluate.arguments().is_empty()
                && evaluate.receiver().is_none()
                && evaluate.return_type() == signature.parameters()[0].ty()
                && matches!(inner.arguments(),[a]if a.argument_index()==0&&a.parameter_index()==0&&a.mode()==ParameterMode::Borrow&&a.category()==ExpressionCategory::Temporary&&a.parameter_type()==evaluate.return_type()),
            "birth actual lambda selected Int overload and ordered Borrow evaluator",
            (inner, evaluate, signature),
        )?;
        let loan = one(
            owned
                .loans()
                .iter()
                .filter(|l| l.call() == e(produce) && l.argument() == e(argument)),
            "birth lambda actual Borrow loan",
        )?;
        let contract = one(
            owned
                .call_argument_contracts_for(e(produce))
                .filter(|a| a.argument() == e(argument)),
            "birth lambda actual argument contract",
        )?;
        need(
            loan.kind() == LoanKind::Shared
                && loan.target() == &UnitLoanTarget::Temporary(e(argument))
                && loan.begin_span() == ast::span(parsed, argument)
                && loan.end_span() == ast::span(parsed, produce)
                && contract.parameter_index() == 0
                && contract.parameter_type() == evaluate.return_type()
                && contract.parameter_span() == Some(signature.parameters()[0].span())
                && contract.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                && !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Temporary(e(produce))),
            "birth lambda synchronous loan and transferred owned tail",
            (loan, contract),
        )?;
        let drop = *one(
            owned
                .drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(factory)),
            "birth unique actual callable binding Drop",
        )?;
        need(
            drop.value_origin() == i.factory.unwrap()
                && drop.point() == UnitDropPoint::CallReturn(e(i.birth))
                && !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Temporary(e(lambda))),
            "birth original callable last-use endpoint and no duplicate lambda cleanup",
            drop,
        )?;
        Ok(drop)
    }
}
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Kind, Shape, ast};
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::*,
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::*,
};
pub(super) type Input<'a> = (&'a Case, &'a SourceMap, &'a ParsedFile, &'a ast::Input);
#[derive(Debug, Default)]
pub(super) struct Proof {
    pub callable_drop: Option<UnitDropFact>,
}

fn unit_root(names: &ValidatedCompilationUnitNames, marker: Span) -> Result<UnitSymbolId, Failure> {
    one(
        names.names().source_units().iter().flat_map(|u| {
            u.resolution()
                .symbols()
                .iter()
                .filter(move |s| s.span() == marker && !s.is_synthetic())
                .map(move |s| (u.source_unit(), s.id()))
        }),
        "birth original qualified declaration",
    )
    .and_then(|(unit, symbol)| {
        one(
            names
                .names()
                .references()
                .iter()
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(id)
                        if id.source_unit() == unit && id.symbol() == symbol =>
                    {
                        Some(*id)
                    }
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter(),
            "birth actual qualified declaration identity",
        )
    })
}
pub(super) fn unit(
    input: Input<'_>,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Proof, Failure> {
    let (c, _, parsed, i) = input;
    let unit = one(names.names().source_units().iter(), "birth original unit")?.source_unit();
    let e = |id| UnitExpressionId::new(unit, id);
    let ty = typed
        .expression_type(e(i.birth))
        .ok_or_else(|| failure("birth qualified result type", i.birth))?;
    need(
        typed.expression_category(e(i.birth)) == Some(ExpressionCategory::Temporary)
            && typed.is_resource_type(ty) == Some(false)
            && matches!(typed.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor,arguments})if *constructor==if c.mutable{IntrinsicTypeConstructor::MutableMap}else{IntrinsicTypeConstructor::Map}&&arguments.len()==2&&arguments.iter().all(|t|typed.types().get(*t)==Some(&UnitTypeKind::Builtin(BuiltinType::String)))),
        "birth qualified exact represented result",
        ty,
    )?;
    let mut proof = Proof::default();
    if c.kind == Kind::FunctionValue {
        proof.callable_drop = Some(callable::unit(input, names, typed, owned)?);
    } else {
        let construct = i.construction.unwrap();
        let d = typed
            .map_construction(e(construct))
            .ok_or_else(|| failure("birth qualified typed MapNew", construct))?;
        need(
            d.expression() == e(construct)
                && d.callable()
                    == if c.kind == Kind::Consume || c.mutable {
                        IntrinsicCallable::MutableMapOf
                    } else {
                        IntrinsicCallable::MapOf
                    }
                && typed.expression_type(e(construct)) == Some(d.map_type())
                && typed.types().get(d.key_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                && typed.types().get(d.value_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String)),
            "birth qualified exact construction/KV identity",
            d,
        )?;
        if c.kind == Kind::Consume {
            let consume = typed
                .map_consume(e(i.birth))
                .ok_or_else(|| failure("birth actual qualified consume", i.birth))?;
            let initial = unit_root(names, i.initial.unwrap())?;
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, consume.receiver().expression())),
                "birth actual qualified consume root",
            )?;
            need(
                consume.expression() == e(i.birth)
                    && consume.source() == Some((initial, d.map_type()))
                    && consume.receiver_type() == d.map_type()
                    && consume.result_type() == ty
                    && [consume.key_type(), consume.value_type()] == [d.key_type(), d.value_type()]
                    && reference.target() == &UnitReferenceTarget::Symbol(initial)
                    && owned.formed_owned_locals().contains(&initial)
                    && !owned
                        .drops()
                        .iter()
                        .any(|d| d.target() == UnitDropTarget::Named(initial)),
                "birth qualified consume current root and no duplicate old Drop",
                consume,
            )?;
        } else {
            need(
                d.map_type() == ty,
                "birth qualified construction output type",
                d,
            )?;
        }
        if let Some(put) = i.put {
            let p = typed
                .map_put(e(put))
                .ok_or_else(|| failure("birth qualified actual put", put))?;
            let initial = unit_root(names, i.initial.unwrap())?;
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, p.receiver().expression())),
                "birth qualified actual put root",
            )?;
            need(
                p.expression() == e(put)
                    && p.key_type() == d.key_type()
                    && p.value_type() == d.value_type()
                    && typed.expression_type(p.receiver()) == Some(d.map_type())
                    && reference.target() == &UnitReferenceTarget::Symbol(initial),
                "birth qualified put current initial root",
                p,
            )?;
        }
    }
    let target = if let Some(marker) = i.root {
        let root = unit_root(names, marker)?;
        need(
            owned.formed_owned_locals().contains(&root)
                && typed.symbol_type(root) == Some(ty)
                && !owned
                    .drops()
                    .iter()
                    .any(|x| x.target() == UnitDropTarget::Temporary(e(i.birth))),
            "birth qualified actual named result formation",
            root,
        )?;
        UnitDropTarget::Named(root)
    } else {
        UnitDropTarget::Temporary(e(i.birth))
    };
    let drop = *one(
        owned.drops().iter().filter(|d| d.target() == target),
        "birth qualified unique output Drop",
    )?;
    need(
        drop.value_origin() == i.root.unwrap_or(ast::span(parsed, i.birth))
            && drop.point()
                == if let Some(site) = &i.site {
                    UnitDropPoint::LoopExit(UnitStatementId::new(unit, site.statement))
                } else {
                    UnitDropPoint::CallReturn(e(i.size.unwrap()))
                },
        "birth qualified exact output cleanup boundary",
        drop,
    )?;
    need(
        typed.map_iterations().len() == usize::from(i.site.is_some())
            && owned.map_iterations().len() == usize::from(i.site.is_some()),
        "birth qualified exact provider/control set",
        owned.map_iterations(),
    )?;
    if let Some(site) = &i.site {
        let statement = UnitStatementId::new(unit, site.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("birth qualified actual provider", site))?;
        let d = p.descriptor();
        need(
            typed.map_iteration(statement) == Some(d)
                && d.source() == e(site.source)
                && d.source_type() == ty,
            "birth qualified same original For source/type",
            d,
        )?;
        if let UnitDropTarget::Named(root) = target {
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, site.source)),
                "birth qualified actual For Name",
            )?;
            need(
                reference.target() == &UnitReferenceTarget::Symbol(root)
                    && p.source_access() == UnitIterationSourceAccess::Owned
                    && matches!(p.source(),Some(UnitLoanTarget::Place(path))if path.is_root()&&path.root()==root),
                "birth qualified For uses actual original formed root",
                p,
            )?;
        } else {
            need(
                p.source_access() == UnitIterationSourceAccess::Temporary
                    && p.source() == Some(&UnitLoanTarget::Temporary(e(i.birth))),
                "birth qualified direct original temporary",
                p,
            )?;
        }
        if c.shape == Shape::Direct {
            super::super::query_temporary_cleanup::unit(
                UnitStatementId::new(unit, site.body),
                ast::span(parsed, i.birth),
                statement,
                e(i.birth),
                owned,
            )?;
        }
        let read = typed
            .call(e(site.read))
            .ok_or_else(|| failure("birth qualified actual reader", site.read))?;
        for n in 0..2 {
            let binding = unit_root(names, site.bindings[n])?;
            let l = &p.loans()[n];
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, site.arguments[n])),
                "birth qualified component reference",
            )?;
            need(
                d.bindings()[n] == binding
                    && reference.target() == &UnitReferenceTarget::Symbol(binding)
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && l.statement() == statement
                    && l.source() == e(site.source)
                    && l.binding() == binding
                    && Some(l.origin()) == p.source()
                    && l.kind() == LoanKind::Shared
                    && read.arguments()[n].argument_index() == n
                    && read.arguments()[n].parameter_index() == n
                    && read.arguments()[n].mode() == ParameterMode::Borrow,
                "birth qualified ordered K/V Borrow lineage",
                l,
            )?;
        }
        for (kind, point, mut actions) in [
            (
                UnitIterationExitKind::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, site.body)),
                vec![
                    UnitIterationCleanupAction::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    UnitIterationCleanupAction::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ],
            ),
            (
                UnitIterationExitKind::Exhaustion,
                UnitDropPoint::LoopExit(statement),
                vec![
                    UnitIterationCleanupAction::FinishProvider(statement),
                    UnitIterationCleanupAction::EndSource(statement),
                ],
            ),
        ] {
            actions.extend(
                owned
                    .drops()
                    .iter()
                    .filter(|d| d.point() == point)
                    .copied()
                    .map(UnitIterationCleanupAction::Drop),
            );
            let exit = one(
                p.exits().iter().filter(|e| e.kind() == kind),
                "birth qualified complete exit",
            )?;
            need(
                p.exits().len() == 2
                    && exit.point() == point
                    && exit.actions() == actions
                    && owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "birth qualified full ordered exit cleanup",
                exit,
            )?;
        }
    } else {
        let size = typed
            .map_size(e(i.size.unwrap()))
            .ok_or_else(|| failure("birth qualified actual control size", i.size))?;
        need(
            size.map_type() == ty && typed.expression_type(size.receiver()) == Some(ty),
            "birth qualified no-For actual result size",
            size,
        )?;
    }
    Ok(proof)
}

pub(in crate::ssa) mod syntax {
//! Exact source/AST/symbol/scope queries, not a second reachability analysis.

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    name_resolution::{NameResolution, Scope, ScopeKind, SymbolId, SymbolKind},
    parser::{
        Expression, FunctionBody, FunctionForm, Item, LiteralKind, NameMarker, Statement,
        VariableKind,
    },
};

use crate::ssa::nullable_map_consume_lowering_tests::resource::formed_locals::{
    crate::ssa::nullable_map_consume_lowering_tests::resource::formed_locals::{ParsedFile, SourceMap, Span, Trace},
    sources::{Case, Initializer, Local, Owner},
};

#[derive(Debug)]
pub(in crate::ssa) struct LocalSyntax {
    pub statement: StatementId,
    pub item: ItemId,
    pub name: Span,
    pub initializer: ExpressionId,
    pub type_ref: Option<TypeRefId>,
    pub symbol: SymbolId,
    pub expected: &'static Local,
}

#[derive(Debug)]
struct FunctionSyntax {
    item: ItemId,
    span: Span,
    name: Span,
    body: StatementId,
    parameters: usize,
}

fn functions(parsed: &ParsedFile) -> Option<Vec<FunctionSyntax>> {
    parsed
        .roots()
        .iter()
        .map(|&item| {
            let node = parsed.ast().items().get(item).ok()?;
            let Item::Function {
                name: NameMarker::Present(name),
                parameters,
                type_parameters,
                extension_receiver: None,
                form:
                    FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    },
                ..
            } = node.payload()
            else {
                return None;
            };
            type_parameters.is_empty().then_some(FunctionSyntax {
                item,
                span: node.span(),
                name: *name,
                body: *body,
                parameters: parameters.len(),
            })
        })
        .collect()
}

fn nearest_callable(names: &NameResolution, symbol: SymbolId) -> Option<Scope> {
    let mut id = names.symbols().get(symbol.index())?.scope();
    for _ in 0..names.scopes().len() {
        let scope = *names.scopes().get(id.index())?;
        match scope.kind() {
            ScopeKind::Function | ScopeKind::Lambda => return Some(scope),
            ScopeKind::File | ScopeKind::Classifier | ScopeKind::Companion => return None,
            _ => id = scope.parent()?,
        }
    }
    None
}

fn elements(parsed: &ParsedFile, statement: StatementId) -> Option<&[StatementId]> {
    match parsed.ast().statements().get(statement).ok()?.payload() {
        Statement::Block { elements }
        | Statement::ControlBody { elements }
        | Statement::LambdaBody { elements } => Some(elements),
        _ => None,
    }
}

fn expression(parsed: &ParsedFile, statement: StatementId) -> Option<ExpressionId> {
    match parsed.ast().statements().get(statement).ok()?.payload() {
        Statement::Expression { expression } => Some(*expression),
        _ => None,
    }
}

fn is_return(parsed: &ParsedFile, statement: StatementId) -> bool {
    expression(parsed, statement)
        .and_then(|id| parsed.ast().expressions().get(id).ok())
        .is_some_and(|node| matches!(node.payload(), Expression::Return { value: None, .. }))
}

fn control_shape(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    functions: &[FunctionSyntax],
    locals: &[LocalSyntax],
) -> Option<bool> {
    let main = functions
        .iter()
        .find(|f| sources.slice(f.name).ok() == Some("main"))?;
    let body = elements(parsed, main.body)?;
    let local = |index: usize| locals.get(index).map(|value| value.statement);
    Some(match case.id {
        "P1" => {
            body.len() == 2
                && Some(body[1]) == local(0)
                && expression(parsed, body[0])
                    .and_then(|id| parsed.ast().expressions().get(id).ok())
                    .and_then(|node| sources.slice(node.span()).ok())
                    == Some("println(\"prefix\")")
        }
        "P2" => body
            .iter()
            .copied()
            .eq(locals.iter().map(|value| value.statement)),
        "P3" | "K3" => {
            let owner = if case.id == "P3" {
                functions
                    .iter()
                    .find(|f| sources.slice(f.name).ok() == Some("run"))?
            } else {
                main
            };
            let statements = elements(parsed, owner.body)?;
            let [statement] = statements else {
                return Some(false);
            };
            let node = parsed
                .ast()
                .expressions()
                .get(expression(parsed, *statement)?)
                .ok()?;
            let Expression::If {
                condition,
                then_branch,
                else_branch: None,
                ..
            } = node.payload()
            else {
                return Some(false);
            };
            let condition = parsed.ast().expressions().get(*condition).ok()?.payload();
            let valid_condition = if case.id == "P3" {
                matches!(condition, Expression::Name)
            } else {
                matches!(condition, Expression::Literal(LiteralKind::Boolean(false)))
            };
            valid_condition && elements(parsed, *then_branch)? == [locals.first()?.statement]
        }
        "K1" => body.len() == 2 && is_return(parsed, body[0]) && Some(body[1]) == local(0),
        "K2" => body == [locals.first()?.statement],
        "K4" => {
            let outer = locals.first()?;
            let node = parsed.ast().expressions().get(outer.initializer).ok()?;
            let Expression::Lambda {
                body: lambda_body, ..
            } = node.payload()
            else {
                return Some(false);
            };
            body == [outer.statement]
                && elements(parsed, *lambda_body)? == [locals.get(1)?.statement]
        }
        "K5" => {
            if body.len() != 2 {
                return Some(false);
            }
            let nested = elements(parsed, body[1])?;
            Some(body[0]) == local(0)
                && nested.len() == 2
                && is_return(parsed, nested[0])
                && Some(nested[1]) == local(1)
        }
        "K6" => {
            let run = functions
                .iter()
                .find(|f| sources.slice(f.name).ok() == Some("run"))?;
            let statements = elements(parsed, run.body)?;
            if statements.len() != 2 {
                return Some(false);
            }
            let node = parsed.ast().statements().get(statements[1]).ok()?;
            let Statement::While {
                condition,
                body: loop_body,
                ..
            } = node.payload()
            else {
                return Some(false);
            };
            let inner = elements(parsed, *loop_body)?;
            Some(statements[0]) == local(0)
                && inner.len() == 2
                && Some(inner[0]) == local(1)
                && matches!(
                    parsed.ast().expressions().get(*condition).ok()?.payload(),
                    Expression::Name
                )
                && expression(parsed, inner[1])
                    .and_then(|id| parsed.ast().expressions().get(id).ok())
                    .is_some_and(|node| matches!(node.payload(), Expression::Break { .. }))
        }
        _ => false,
    })
}

pub(in crate::ssa) fn inspect(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    unit: bool,
    trace: &mut Trace,
) -> Vec<LocalSyntax> {
    let expected_text = if unit { case.provider } else { case.single };
    let actual_text = sources
        .span(parsed.source_id(), 0, expected_text.len())
        .ok()
        .and_then(|span| sources.slice(span).ok());
    trace.check(
        "formation exact source identity",
        names.source_id() == parsed.source_id()
            && actual_text == Some(expected_text)
            && sources.source_name(parsed.source_id()).ok()
                == Some(if unit { "p/provider.ko" } else { case.path }),
        (parsed.source_id(), names.source_id(), actual_text),
    );
    let functions = functions(parsed);
    let expected_functions: &[(&str, usize)] = if matches!(case.id, "P3" | "K6") {
        &[("run", 1), ("main", 0)]
    } else {
        &[("main", 0)]
    };
    trace.check(
        "exact ordinary Function Item identities and zero-argument main",
        functions.as_ref().is_some_and(|values| {
            values.len() == expected_functions.len()
                && values
                    .iter()
                    .zip(expected_functions)
                    .all(|(actual, &(name, parameters))| {
                        sources.slice(actual.name).ok() == Some(name)
                            && actual.parameters == parameters
                            && parsed
                                .ast()
                                .items()
                                .get(actual.item)
                                .ok()
                                .is_some_and(|node| node.span() == actual.span)
                            && names
                                .symbols()
                                .iter()
                                .filter(|symbol| {
                                    symbol.span() == actual.name
                                        && symbol.kind() == SymbolKind::Function
                                        && !symbol.is_synthetic()
                                })
                                .count()
                                == 1
                    })
        }),
        &functions,
    );
    let Some(functions) = functions else {
        return Vec::new();
    };
    let mut declarations = Vec::new();
    for (statement, node) in parsed.ast().statements().iter() {
        let Statement::LocalVariable { declaration } = node.payload() else {
            continue;
        };
        let Ok(item) = parsed.ast().items().get(*declaration) else {
            trace.check("local declaration item exists", false, declaration);
            continue;
        };
        let Item::Variable {
            kind: VariableKind::Val,
            name: NameMarker::Present(name),
            initializer,
            type_ref,
            ..
        } = item.payload()
        else {
            trace.check("exact ordinary named val local", false, item);
            continue;
        };
        declarations.push((statement, *declaration, *name, *initializer, *type_ref));
    }
    declarations.sort_by_key(|value| value.2.start());
    trace.check(
        "exact number of local declarations",
        declarations.len() == case.locals.len(),
        &declarations,
    );
    let mut locals = Vec::new();
    for ((statement, item, name, initializer, type_ref), expected) in
        declarations.into_iter().zip(case.locals)
    {
        let symbols: Vec<_> = names
            .symbols()
            .iter()
            .filter(|symbol| {
                symbol.span() == name
                    && symbol.kind() == SymbolKind::Variable
                    && !symbol.is_synthetic()
            })
            .collect();
        trace.check(
            "declaration NameMarker selects one exact variable symbol",
            symbols.len() == 1 && sources.slice(name).ok() == Some(expected.name),
            (&symbols, name, expected),
        );
        let [symbol] = symbols.as_slice() else {
            continue;
        };
        let owner = nearest_callable(names, symbol.id());
        let owner_matches = match expected.owner {
            Owner::Function(name) => functions
                .iter()
                .find(|f| sources.slice(f.name).ok() == Some(name))
                .is_some_and(|function| {
                    owner.is_some_and(|scope| {
                        scope.kind() == ScopeKind::Function && scope.span() == Some(function.span)
                    })
                }),
            Owner::Lambda => {
                parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter(|(_, node)| matches!(node.payload(), Expression::Lambda { .. }))
                    .count()
                    == 1
                    && parsed.ast().expressions().iter().any(|(_, node)| {
                        matches!(node.payload(), Expression::Lambda { .. })
                            && owner.is_some_and(|scope| {
                                scope.kind() == ScopeKind::Lambda
                                    && scope.span() == Some(node.span())
                            })
                    })
            }
        };
        trace.check(
            "nearest callable scope matches exact Function Item or Lambda expression",
            owner_matches,
            (symbol, owner, &functions),
        );
        let init = parsed.ast().expressions().get(initializer).ok();
        let init_matches = init.is_some_and(|node| match expected.initializer {
            Initializer::EmptyMap => {
                matches!(node.payload(), Expression::Call { callee, arguments, .. }
                if arguments.is_empty() && parsed.ast().expressions().get(*callee).ok()
                    .is_some_and(|callee| matches!(callee.payload(), Expression::Name)))
            }
            Initializer::Null => matches!(node.payload(), Expression::Literal(LiteralKind::Null)),
            Initializer::Return => matches!(node.payload(), Expression::Return { value: None, .. }),
            Initializer::Name(_) => matches!(node.payload(), Expression::Name),
            Initializer::Lambda => matches!(node.payload(), Expression::Lambda { .. }),
        });
        trace.check(
            "exact local initializer syntax",
            init_matches,
            (expected, init),
        );
        locals.push(LocalSyntax {
            statement,
            item,
            name,
            initializer,
            type_ref,
            symbol: symbol.id(),
            expected,
        });
    }
    trace.check(
        "exact control/source arrangement",
        control_shape(case, sources, parsed, &functions, &locals) == Some(true),
        &locals,
    );
    if case.id == "K4" {
        let action = locals.first().map(|local| local.symbol);
        trace.check("uninvoked action has no name references",
            action.is_some() && !names.references().iter().any(|reference|
                matches!(reference.target(), lang_frontend::name_resolution::ReferenceTarget::Symbol(symbol)
                    if Some(*symbol) == action)), names.references());
    }
    if case.id == "K5" {
        trace.check("shadowed source identities and lexical scopes are distinct",
            matches!(locals.as_slice(), [outer, inner] if outer.symbol != inner.symbol
                && outer.name != inner.name
                && names.symbols().get(outer.symbol.index()).zip(names.symbols().get(inner.symbol.index()))
                    .is_some_and(|(outer, inner)| outer.scope() != inner.scope())), &locals);
    }
    locals
}
}
