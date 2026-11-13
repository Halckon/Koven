//! Exact source/AST/symbol/scope queries, not a second reachability analysis.

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId, TypeRefId},
    name_resolution::{NameResolution, Scope, ScopeKind, SymbolId, SymbolKind},
    parser::{
        Expression, FunctionBody, FunctionForm, Item, LiteralKind, NameMarker, Statement,
        VariableKind,
    },
};

use super::{
    super::{ParsedFile, SourceMap, Span, Trace},
    sources::{Case, Initializer, Local, Owner},
};

#[derive(Debug)]
pub(super) struct LocalSyntax {
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

pub(super) fn inspect(
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
