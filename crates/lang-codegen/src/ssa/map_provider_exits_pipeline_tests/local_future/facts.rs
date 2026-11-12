//! Public local formation, multi-For live owner identity and actual Pure last-use facts.
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod ast {
    //! Original local declaration chains and source-order For sites; binding and form stay distinct.
    use super::*;
    use lang_frontend::parser::{
        ForBinding, FunctionBody, FunctionForm, Item, NameMarker, Statement, VariableKind,
    };
    #[derive(Debug)]
    pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Local {
        pub marker: Span,
        pub initializer: ExpressionId,
        pub kind: VariableKind,
    }
    #[derive(Debug)]
    pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Loop {
        pub statement: StatementId,
        pub source: ExpressionId,
        pub span: Span,
        pub bindings: [Span; 2],
        pub guard: Local,
        pub guard_span: Span,
        pub kind: E,
        pub point: DropPoint,
    }
    #[derive(Debug)]
    pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Scanner {
        pub name: Span,
        pub span: Span,
        pub factory: Span,
        pub birth: Local,
        pub root: Local,
        pub birth_span: Span,
        pub loops: Vec<Loop>,
    }
    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn within(outer: Span, inner: Span) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() <= inner.start()
            && inner.end() <= outer.end()
    }
    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn span(
        parsed: &ParsedFile,
        id: ExpressionId,
    ) -> Result<Span, Failure> {
        parsed
            .ast()
            .expressions()
            .get(id)
            .map(|n| n.span())
            .map_err(|e| failure("local future original expression", e))
    }
    fn declaration(parsed: &ParsedFile, id: StatementId) -> Result<Local, Failure> {
        let n = parsed
            .ast()
            .statements()
            .get(id)
            .map_err(|e| failure("local future statement", e))?;
        let Statement::LocalVariable { declaration } = n.payload() else {
            return Err(failure("local future actual local statement", n));
        };
        let n = parsed
            .ast()
            .items()
            .get(*declaration)
            .map_err(|e| failure("local future declaration", e))?;
        let Item::Variable {
            kind,
            name: NameMarker::Present(marker),
            initializer,
            ..
        } = n.payload()
        else {
            return Err(failure("local future actual variable declaration", n));
        };
        Ok(Local {
            marker: *marker,
            initializer: *initializer,
            kind: *kind,
        })
    }
    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn scanners(
        (binding, exit, continuation): (usize, usize, usize),
        sources: &SourceMap,
        parsed: &ParsedFile,
    ) -> Result<Vec<Scanner>, Failure> {
        let function = |wanted: &str| {
            one(
                parsed.roots().iter().filter_map(|id| {
                    let n = parsed.ast().items().get(*id).ok()?;
                    match n.payload() {
                        Item::Function {
                            name: NameMarker::Present(name),
                            form,
                            parameters,
                            ..
                        } if sources.slice(*name) == Ok(wanted) && parameters.is_empty() => {
                            Some((n.span(), *name, form))
                        }
                        _ => None,
                    }
                }),
                "local future original zero-argument function",
            )
        };
        [false, true]
            .into_iter()
            .map(|empty| {
                let (scanner, name, form) = function(if empty { "scanEmpty" } else { "scan" })?;
                let factory = function(if empty { "makeEmpty" } else { "make" })?.1;
                let scope = match form {
                    FunctionForm::ImplicitUnitBlock(b)
                    | FunctionForm::Explicit {
                        body: FunctionBody::Block(b),
                        ..
                    } => *b,
                    _ => return Err(failure("local future original scanner block", form)),
                };
                let Statement::Block { elements } = parsed
                    .ast()
                    .statements()
                    .get(scope)
                    .map_err(|e| failure("local future scope", e))?
                    .payload()
                else {
                    return Err(failure("local future actual Block", scope));
                };
                let first = one(
                    elements
                        .iter()
                        .copied()
                        .filter(|id| {
                            parsed
                                .ast()
                                .statements()
                                .get(*id)
                                .is_ok_and(|n| matches!(n.payload(), Statement::For { .. }))
                        })
                        .take(1),
                    "local future first actual For",
                )?;
                let locals = elements
                    .iter()
                    .copied()
                    .take_while(|id| *id != first)
                    .filter(|id| {
                        parsed
                            .ast()
                            .statements()
                            .get(*id)
                            .is_ok_and(|n| matches!(n.payload(), Statement::LocalVariable { .. }))
                    })
                    .collect::<Vec<_>>();
                need(
                    locals.len() == if binding == 2 { 2 } else { 1 },
                    "local future actual binding chain",
                    &locals,
                )?;
                let birth = declaration(parsed, locals[0])?;
                let root = declaration(
                    parsed,
                    *locals
                        .last()
                        .ok_or_else(|| failure("local future actual root declaration", &locals))?,
                )?;
                let kind = if binding == 1 {
                    VariableKind::Var
                } else {
                    VariableKind::Val
                };
                need(
                    root.kind == kind
                        && birth.kind == kind
                        && sources.slice(root.marker) == Ok("source"),
                    "local future binding selects val/var/alias independently of form",
                    (&root, &birth),
                )?;
                if binding == 2 {
                    need(
                        sources.slice(birth.marker) == Ok("original")
                            && sources.slice(span(parsed, root.initializer)?) == Ok("original")
                            && parsed
                                .ast()
                                .expressions()
                                .get(root.initializer)
                                .is_ok_and(|n| matches!(n.payload(), Expression::Name)),
                        "local future original alias Name Move chain",
                        (&root, &birth),
                    )?;
                } else {
                    need(
                        root.marker == birth.marker && root.initializer == birth.initializer,
                        "local future direct initializer",
                        (&root, &birth),
                    )?;
                }
                let node = parsed
                    .ast()
                    .expressions()
                    .get(birth.initializer)
                    .map_err(|e| failure("local future factory Call", e))?;
                need(
                    matches!(node.payload(),Expression::Call{arguments,..} if arguments.is_empty()),
                    "local future actual zero-argument Call birth",
                    node,
                )?;
                let mut loops = Vec::new();
                for &statement in elements {
                    let node = parsed
                        .ast()
                        .statements()
                        .get(statement)
                        .map_err(|e| failure("local future statement", e))?;
                    let Statement::For {
                        source,
                        body,
                        binding: ForBinding::Destructuring { names, .. },
                        ..
                    } = node.payload()
                    else {
                        continue;
                    };
                    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice()
                    else {
                        return Err(failure("local future actual K/V markers", names));
                    };
                    need(
                        parsed
                            .ast()
                            .expressions()
                            .get(*source)
                            .is_ok_and(|n| matches!(n.payload(), Expression::Name))
                            && sources.slice(span(parsed, *source)?) == Ok("source"),
                        "local future original source Name",
                        source,
                    )?;
                    let body_node = parsed
                        .ast()
                        .statements()
                        .get(*body)
                        .map_err(|e| failure("local future body", e))?;
                    let Statement::Block { elements: children } = body_node.payload() else {
                        return Err(failure("local future actual body Block", body_node));
                    };
                    let guard = declaration(
                        parsed,
                        *children
                            .first()
                            .ok_or_else(|| failure("local future Guard", children))?,
                    )?;
                    need(
                        sources.slice(guard.marker) == Ok("local")
                            && sources.slice(span(parsed, guard.initializer)?) == Ok("Guard()"),
                        "local future original Guard declaration",
                        &guard,
                    )?;
                    let transfers = parsed
                        .ast()
                        .expressions()
                        .iter()
                        .filter_map(|(id, n)| {
                            let kind = match n.payload() {
                                Expression::Continue { .. } => E::Continue(id),
                                Expression::Break { .. } => E::Break(id),
                                _ => return None,
                            };
                            within(body_node.span(), n.span()).then_some(kind)
                        })
                        .collect::<Vec<_>>();
                    let kind = match transfers.as_slice() {
                        [] => E::Fallthrough,
                        [kind] => *kind,
                        _ => return Err(failure("local future finite actual transfer", transfers)),
                    };
                    need(
                        matches!(
                            (if loops.is_empty() { exit } else { 0 }, kind),
                            (0, E::Fallthrough) | (1, E::Continue(_)) | (2, E::Break(_))
                        ),
                        "local future original first/second exit",
                        kind,
                    )?;
                    let point = match kind {
                        E::Fallthrough => DropPoint::AfterStatement(*body),
                        E::Continue(e) | E::Break(e) => DropPoint::ControlTransfer(e),
                        other => return Err(failure("local future actual exit", other)),
                    };
                    let guard_span = span(parsed, guard.initializer)?;
                    loops.push(Loop {
                        statement,
                        source: *source,
                        span: node.span(),
                        bindings: [*key, *value],
                        guard,
                        guard_span,
                        kind,
                        point,
                    });
                }
                need(
                    loops.len() == if continuation == 2 { 2 } else { 1 },
                    "local future continuation form determines real For count",
                    &loops,
                )?;
                let birth_span = span(parsed, birth.initializer)?;
                Ok(Scanner {
                    name,
                    span: scanner,
                    factory,
                    birth,
                    root,
                    birth_span,
                    loops,
                })
            })
            .collect()
    }
    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn argument(
        parsed: &ParsedFile,
        id: ExpressionId,
    ) -> Result<ExpressionId, Failure> {
        let node = parsed
            .ast()
            .expressions()
            .get(id)
            .map_err(|e| failure("local future original Borrow call", e))?;
        let Expression::Call { arguments, .. } = node.payload() else {
            return Err(failure("local future actual Borrow Call", node));
        };
        let [argument] = arguments.as_slice() else {
            return Err(failure("local future actual Borrow arity", arguments));
        };
        Ok(argument.value)
    }
}
mod loans {
    //! Actual synchronous source loans and their original CallReturn endpoints.
    use super::*;
    use lang_frontend::{
        parser::{Item, NameMarker},
        type_checking::UnitTypeId,
    };
    fn reader(sources: &SourceMap, parsed: &ParsedFile) -> Result<(Span, Span), Failure> {
        one(
            parsed.roots().iter().filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                let Item::Function {
                    name: NameMarker::Present(name),
                    parameters,
                    ..
                } = n.payload()
                else {
                    return None;
                };
                (sources.slice(*name) == Ok("readMap") && parameters.len() == 1)
                    .then(|| (*name, parameters[0].span))
            }),
            "local future original readMap declaration/parameter",
        )
    }

    pub(super) fn unit(
        input: (&Case, &SourceMap, &ParsedFile, &Scanner),
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        source: (UnitSymbolId, UnitTypeId),
        drop: &UnitDropFact,
    ) -> Result<(), Failure> {
        let (case, sources, parsed, site) = input;
        let (root, ty) = source;
        let unit = root.source_unit();
        let (read_name, parameter) = reader(sources, parsed)?;
        let read = one(
            typed
                .signatures()
                .declarations()
                .iter()
                .filter(|d| d.callable().is_some_and(|c| c.name_span() == read_name)),
            "local future qualified original readMap",
        )?;
        let calls = typed
            .calls()
            .iter()
            .filter(|c| {
                c.target() == UnitCallTarget::Declaration(read.declaration())
                    && ast::span(parsed, c.expression().expression())
                        .is_ok_and(|s| ast::within(site.span, s))
            })
            .collect::<Vec<_>>();
        let sizes = typed
            .map_sizes()
            .iter()
            .filter(|s| {
                s.expression().source_unit() == unit
                    && ast::span(parsed, s.expression().expression())
                        .is_ok_and(|span| ast::within(site.span, span))
            })
            .collect::<Vec<_>>();
        need(
            calls.len() == if case.form == 0 { 2 } else { 1 }
                && sizes.len() == usize::from(case.form != 2),
            "local future qualified actual read set",
            (&calls, &sizes),
        )?;
        let mut uses = Vec::new();
        for call in calls {
            let arg =
                UnitExpressionId::new(unit, ast::argument(parsed, call.expression().expression())?);
            let contracts = owned
                .call_argument_contracts_for(call.expression())
                .collect::<Vec<_>>();
            need(
                qualified(parsed, names, arg.expression())? == root
                    && matches!(call.arguments(),[a] if a.mode()==ParameterMode::Borrow && a.argument_index()==0 && a.parameter_index()==0 && a.parameter_type()==ty)
                    && matches!(contracts.as_slice(),[c] if c.argument()==arg && c.parameter_index()==0 && c.kind()==UnitCallArgumentOwnershipKind::SharedLoan)
                    && !owned
                        .value_deliveries()
                        .iter()
                        .any(|d| d.call() == call.expression()),
                "local future qualified real caller Borrow contract without Move",
                (call, contracts),
            )?;
            uses.push((call.expression(), arg, Some(parameter)));
        }
        for size in sizes {
            need(
                size.map_type() == ty
                    && qualified(parsed, names, size.receiver().expression())? == root,
                "local future qualified original size root/type",
                size,
            )?;
            uses.push((size.expression(), size.receiver(), None));
        }
        let last = names
            .names()
            .references()
            .iter()
            .filter(|r| {
                r.target() == &UnitReferenceTarget::Symbol(root) && ast::within(site.span, r.span())
            })
            .map(|r| r.span())
            .max_by_key(|s| (s.start(), s.end()))
            .ok_or_else(|| failure("local future qualified actual last root reference", root))?;
        let final_call = one(
            uses.iter().filter_map(|(call, arg, _)| {
                ast::span(parsed, arg.expression())
                    .is_ok_and(|s| s == last)
                    .then_some(*call)
            }),
            "local future qualified actual final read",
        )?;
        need(
            drop.point() == UnitDropPoint::CallReturn(final_call)
                && if case.form == 0 {
                    typed.map_size(final_call).is_some()
                } else {
                    typed.call(final_call).is_some()
                },
            "local future qualified Drop at true final read",
            drop,
        )?;
        let root_loans = owned
            .loans()
            .iter()
            .filter(|l| matches!(l.target(),UnitLoanTarget::Place(p) if p.is_root() && p.root()==root))
            .collect::<Vec<_>>();
        need(
            root_loans.len() == uses.len(),
            "local future qualified full real loan set",
            &root_loans,
        )?;
        for (call, arg, parameter) in uses {
            let loan = one(
                root_loans
                    .iter()
                    .copied()
                    .filter(|l| l.call() == call && l.argument() == arg),
                "local future qualified exact original call loan",
            )?;
            need(
                loan.kind() == LoanKind::Shared
                    && loan.begin_span() == ast::span(parsed, arg.expression())?
                    && loan.end_span() == ast::span(parsed, call.expression())?
                    && loan.parameter_span() == parameter,
                "local future qualified real loan end retains actual call span/parameter",
                loan,
            )?;
        }
        Ok(())
    }
}
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Report};
use ast::Scanner;
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{
        SymbolKind, UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, IterationExitKind as E, IterationSource, LoanKind,
        MapIterationComponent, OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitDropFact,
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE, UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicTypeConstructor, MapIterationKind, ParameterMode, UnitCallTarget,
        UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

fn qualified(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    id: ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    let span = ast::span(parsed, id)?;
    one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == span)
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "local future actual qualified Name symbol",
    )
}
fn marker(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    span: Span,
    kind: SymbolKind,
) -> bool {
    names.names().source_units().iter().any(|u| {
        u.source_unit() == id.source_unit()
            && u.resolution().symbols().iter().any(|s| {
                s.id() == id.symbol() && s.span() == span && s.kind() == kind && !s.is_synthetic()
            })
    })
}

pub(super) fn unit(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    report.check(
        "local future complete qualified provider set",
        typed.map_iterations().len() == if case.form == 2 { 4 } else { 2 }
            && owned.map_iterations().len() == typed.map_iterations().len(),
        (typed.map_iterations(), owned.map_iterations()),
    );
    match ast::scanners((case.binding, case.exit, case.form), sources, parsed) {
        Ok(sites) => {
            for site in &sites {
                report.query(
                    "local future original qualified formation/cleanup/loans",
                    &unit_site((case, sources, parsed), site, names, typed, owned),
                );
            }
        }
        Err(e) => report.query::<()>("local future original AST", &Err(e)),
    }
}
fn unit_site(
    input: (&Case, &SourceMap, &ParsedFile),
    site: &Scanner,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let (case, sources, parsed) = input;
    let root = qualified(parsed, names, site.loops[0].source)?;
    let unit = root.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let original = if site.root.initializer == site.birth.initializer {
        root
    } else {
        qualified(parsed, names, site.root.initializer)?
    };
    for (id, local) in [(root, &site.root), (original, &site.birth)] {
        need(
            id.source_unit() == unit
                && marker(names, id, local.marker, SymbolKind::Variable)
                && owned.formed_owned_locals().contains(&id),
            "local future qualified actual declaration/formation",
            (id, local),
        )?;
    }
    need(
        (root == original) == (case.binding != 2)
            && (root == original
                || !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Named(original)))
            && !owned.drops().iter().any(|d| {
                d.target() == UnitDropTarget::Temporary(expression(site.birth.initializer))
            }),
        "local future qualified alias original has no duplicate Drop",
        (root, original),
    )?;
    let ty = typed
        .symbol_type(root)
        .ok_or_else(|| failure("local future qualified source type", root))?;
    let constructor = if case.mutable {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        typed.symbol_type(original) == Some(ty)
            && typed.is_resource_type(ty) == Some(false)
            && typed.expression_type(expression(site.birth.initializer)) == Some(ty)
            && typed.expression_category(expression(site.birth.initializer))
                == Some(ExpressionCategory::Temporary)
            && typed.expression_type(expression(site.root.initializer)) == Some(ty)
            && typed.expression_category(expression(site.root.initializer))
                == Some(if root == original {
                    ExpressionCategory::Temporary
                } else {
                    ExpressionCategory::Place
                })
            && matches!(typed.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && matches!(arguments.as_slice(),[k,v] if k==v && typed.types().get(*k)==Some(&UnitTypeKind::Builtin(BuiltinType::String)))),
        "local future qualified exact Pure Map birth/type/category",
        ty,
    )?;
    let factory = one(
        typed.signatures().declarations().iter().filter(|d| {
            d.callable()
                .is_some_and(|c| c.name_span() == site.factory && c.parameters().is_empty())
        }),
        "local future qualified original selected factory",
    )?;
    let call = typed
        .call(expression(site.birth.initializer))
        .ok_or_else(|| failure("local future qualified actual birth Call", site))?;
    need(
        call.target() == UnitCallTarget::Declaration(factory.declaration())
            && call.arguments().is_empty()
            && call.receiver().is_none()
            && call.return_type() == ty
            && call.result_source() == CallableResultSource::Owned,
        "local future qualified actual Call formation",
        call,
    )?;
    let scanner = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .filter(|c| c.name_span() == site.name),
        "local future qualified original scanner",
    )?;
    need(
        scanner.parameters().is_empty()
            && typed.types().get(scanner.return_type())
                == Some(&UnitTypeKind::Builtin(BuiltinType::Unit)),
        "local future qualified zero-argument Unit scanner",
        scanner,
    )?;
    let drop = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(root)),
        "local future unique qualified Pure Drop",
    )?;
    need(
        drop.value_origin() == site.root.marker,
        "local future qualified original root Drop marker",
        drop,
    )?;
    for loop_site in &site.loops {
        let statement = UnitStatementId::new(unit, loop_site.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("local future actual qualified Map plan", loop_site))?;
        let d = p.descriptor();
        need(
            typed.map_iteration(statement) == Some(d)
                && d.source() == expression(loop_site.source)
                && d.source_type() == ty
                && d.kind()
                    == if case.mutable {
                        MapIterationKind::MutableMap
                    } else {
                        MapIterationKind::Map
                    }
                && qualified(parsed, names, loop_site.source)? == root
                && typed.expression_category(expression(loop_site.source))
                    == Some(ExpressionCategory::Place)
                && p.source_access() == UnitIterationSourceAccess::Owned
                && matches!(p.source_kind(),IterationSource::BorrowedSource(UnitLoanTarget::Place(path)) if path.is_root() && path.root()==root),
            "local future original qualified For descriptor/Owned lineage",
            p,
        )?;
        for (index, binding) in d.bindings().into_iter().enumerate() {
            need(
                marker(
                    names,
                    binding,
                    loop_site.bindings[index],
                    SymbolKind::ForBinding,
                ) && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                    && typed.types().get([d.key_type(), d.value_type()][index])
                        == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[index].symbol() == binding
                    && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                    && p.loans()[index].statement() == statement
                    && p.loans()[index].source() == d.source()
                    && p.loans()[index].binding() == binding
                    && p.loans()[index].component()
                        == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                    && p.loans()[index].kind() == LoanKind::Shared
                    && Some(p.loans()[index].origin()) == p.source(),
                "local future exact qualified original K/V identities",
                p.loans(),
            )?;
        }
        let point = match loop_site.point {
            DropPoint::AfterStatement(s) => {
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, s))
            }
            DropPoint::ControlTransfer(e) => UnitDropPoint::ControlTransfer(expression(e)),
            other => return Err(failure("local future qualified actual point", other)),
        };
        let kind = match loop_site.kind {
            E::Fallthrough => UE::Fallthrough,
            E::Continue(e) => UE::Continue(expression(e)),
            E::Break(e) => UE::Break(expression(e)),
            other => return Err(failure("local future actual qualified kind", other)),
        };
        let guard = *one(
            owned
                .drops()
                .iter()
                .filter(|d| d.value_origin() == loop_site.guard.marker),
            "local future original qualified Guard Drop",
        )?;
        need(
            guard.point() == point
                && matches!(guard.target(),UnitDropTarget::Named(id) if marker(names,id,loop_site.guard.marker,SymbolKind::Variable) && owned.formed_owned_locals().contains(&id)),
            "local future actual qualified Guard target/point",
            guard,
        )?;
        need(
            p.exits().len() == 2,
            "local future complete qualified exit set",
            p.exits(),
        )?;
        for (kind, point, body) in [
            (kind, point, true),
            (UE::Exhaustion, UnitDropPoint::LoopExit(statement), false),
        ] {
            let exit = one(
                p.exits()
                    .iter()
                    .filter(|e| e.kind() == kind && e.point() == point),
                "local future original qualified exit",
            )?;
            let mut expected = if body {
                vec![
                    UA::Drop(guard),
                    UA::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    UA::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ]
            } else {
                vec![]
            };
            if !body || matches!(kind, UE::Break(_)) {
                expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
            }
            need(
                exit.actions() == expected
                    && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
                "local future qualified full cleanup never drops still-needed source",
                (exit, expected),
            )?;
        }
    }
    loans::unit(
        (case, sources, parsed, site),
        names,
        typed,
        owned,
        (root, ty),
        &drop,
    )
}
