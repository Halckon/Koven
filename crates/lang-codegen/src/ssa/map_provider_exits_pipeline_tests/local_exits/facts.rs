//! Public local formation, alias provenance and iteration cleanup identities.
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod ast {
    //! Shared original AST anchors for local formation and control-transfer queries.
    use super::{Failure, failure, need, one};
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        ownership_checking::{DropPoint, IterationExitKind as E},
        parser::{
            Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile,
            Statement, VariableKind,
        },
        source::{SourceMap, Span},
        type_checking::MapIterationKind,
    };
    #[derive(Debug)]
    pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Local {
        pub(in crate::ssa::map_provider_exits_pipeline_tests) marker: Span,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) initializer: ExpressionId,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) kind: VariableKind,
    }
    #[derive(Debug)]
    pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Site {
        pub(in crate::ssa::map_provider_exits_pipeline_tests) scan: Span,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) name: Span,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) factory: Span,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) result: Span,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) map_kind: MapIterationKind,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) entry: Span,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) root: Local,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) birth: Local,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) birth_span: Span,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) statement: StatementId,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) source: ExpressionId,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) span: Span,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) bindings: [Span; 2],
        pub(in crate::ssa::map_provider_exits_pipeline_tests) guard: Local,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) guard_span: Span,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) kind: E,
        pub(in crate::ssa::map_provider_exits_pipeline_tests) point: DropPoint,
    }
    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn within(
        outer: Span,
        inner: Span,
    ) -> bool {
        outer.source_id() == inner.source_id()
            && outer.start() <= inner.start()
            && inner.end() <= outer.end()
    }
    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn expression_span(
        parsed: &ParsedFile,
        id: ExpressionId,
    ) -> Result<Span, Failure> {
        parsed
            .ast()
            .expressions()
            .get(id)
            .map(|n| n.span())
            .map_err(|e| failure("G2 original expression", e))
    }
    fn function<'a>(
        sources: &SourceMap,
        parsed: &'a ParsedFile,
        wanted: &str,
    ) -> Result<(Span, Span, &'a FunctionForm), Failure> {
        one(
            parsed.roots().iter().filter_map(|id| {
                let node = parsed.ast().items().get(*id).ok()?;
                match node.payload() {
                    Item::Function {
                        name: NameMarker::Present(name),
                        form,
                        parameters,
                        ..
                    } if sources.slice(*name) == Ok(wanted) && parameters.is_empty() => {
                        Some((node.span(), *name, form))
                    }
                    _ => None,
                }
            }),
            "G2 actual zero-parameter function root",
        )
    }
    fn local(parsed: &ParsedFile, statement: StatementId) -> Result<Local, Failure> {
        let node = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|e| failure("G2 local statement", e))?;
        let Statement::LocalVariable { declaration } = node.payload() else {
            return Err(failure("G2 actual local statement", node));
        };
        let item = parsed
            .ast()
            .items()
            .get(*declaration)
            .map_err(|e| failure("G2 local declaration", e))?;
        let Item::Variable {
            kind,
            name: NameMarker::Present(marker),
            initializer,
            ..
        } = item.payload()
        else {
            return Err(failure("G2 original variable declaration", item));
        };
        Ok(Local {
            marker: *marker,
            initializer: *initializer,
            kind: *kind,
        })
    }
    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn sites(
        (mutable, formation, exit): (bool, usize, usize),
        sources: &SourceMap,
        parsed: &ParsedFile,
    ) -> Result<Vec<Site>, Failure> {
        let entry = function(sources, parsed, "entry")?.0;
        let result = one(
            parsed.roots().iter().filter_map(|id| {
                let node = parsed.ast().items().get(*id).ok()?;
                match node.payload() {
                    Item::Function {
                        name: NameMarker::Present(name),
                        parameters,
                        ..
                    } if sources.slice(*name) == Ok("result") && parameters.len() == 2 => {
                        Some(*name)
                    }
                    _ => None,
                }
            }),
            "G2 original result function root",
        )?;
        let map_kind = if mutable {
            MapIterationKind::MutableMap
        } else {
            MapIterationKind::Map
        };
        [false, true]
            .into_iter()
            .map(|empty| {
                let (scan, name, form) =
                    function(sources, parsed, if empty { "scanEmpty" } else { "scan" })?;
                let factory =
                    function(sources, parsed, if empty { "makeEmpty" } else { "make" })?.1;
                let scope = match form {
                    FunctionForm::ImplicitUnitBlock(b)
                    | FunctionForm::Explicit {
                        body: FunctionBody::Block(b),
                        ..
                    } => *b,
                    _ => return Err(failure("G2 original scanner block", form)),
                };
                let Statement::Block { elements } = parsed
                    .ast()
                    .statements()
                    .get(scope)
                    .map_err(|e| failure("G2 original scanner scope", e))?
                    .payload()
                else {
                    return Err(failure("G2 actual scanner Block", scope));
                };
                let statement = one(
                    elements.iter().copied().filter(|id| {
                        parsed
                            .ast()
                            .statements()
                            .get(*id)
                            .is_ok_and(|n| matches!(n.payload(), Statement::For { .. }))
                    }),
                    "G2 one actual scanner For",
                )?;
                let node = parsed
                    .ast()
                    .statements()
                    .get(statement)
                    .map_err(|e| failure("G2 actual For", e))?;
                let Statement::For {
                    source,
                    body,
                    binding: ForBinding::Destructuring { names, .. },
                    ..
                } = node.payload()
                else {
                    return Err(failure("G2 original destructured For", node));
                };
                let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice()
                else {
                    return Err(failure("G2 actual K/V markers", names));
                };
                let declarations =
                    elements
                        .iter()
                        .copied()
                        .take_while(|id| *id != statement)
                        .filter(|id| {
                            parsed.ast().statements().get(*id).is_ok_and(|n| {
                                matches!(n.payload(), Statement::LocalVariable { .. })
                            })
                        })
                        .collect::<Vec<_>>();
                need(
                    declarations.len() == if formation == 2 { 2 } else { 1 },
                    "G2 actual local formation chain",
                    &declarations,
                )?;
                let root = local(
                    parsed,
                    *declarations
                        .last()
                        .ok_or_else(|| failure("G2 source declaration", &declarations))?,
                )?;
                let birth = local(parsed, declarations[0])?;
                let wanted = if formation == 1 {
                    VariableKind::Var
                } else {
                    VariableKind::Val
                };
                need(
                    root.kind == wanted
                        && birth.kind == wanted
                        && sources.slice(root.marker) == Ok("source")
                        && sources.slice(expression_span(parsed, *source)?) == Ok("source")
                        && parsed
                            .ast()
                            .expressions()
                            .get(*source)
                            .is_ok_and(|n| matches!(n.payload(), Expression::Name)),
                    "G2 original val/var root and Name source",
                    (&root, &birth, source),
                )?;
                if formation == 2 {
                    need(
                        sources.slice(birth.marker) == Ok("original")
                            && sources.slice(expression_span(parsed, root.initializer)?)
                                == Ok("original")
                            && parsed
                                .ast()
                                .expressions()
                                .get(root.initializer)
                                .is_ok_and(|n| matches!(n.payload(), Expression::Name)),
                        "G2 actual alias Name Move chain",
                        (&root, &birth),
                    )?;
                } else {
                    need(
                        root.initializer == birth.initializer,
                        "G2 direct actual initializer",
                        (&root, &birth),
                    )?;
                }
                let Expression::Call { arguments, .. } = parsed
                    .ast()
                    .expressions()
                    .get(birth.initializer)
                    .map_err(|e| failure("G2 original factory call", e))?
                    .payload()
                else {
                    return Err(failure("G2 selected original factory Call", &birth));
                };
                need(arguments.is_empty(), "G2 actual factory arity", arguments)?;
                let formed = one(
                    elements.iter().copied().filter_map(|id| {
                        let n = parsed.ast().statements().get(id).ok()?;
                        let Statement::Expression { expression } = n.payload() else {
                            return None;
                        };
                        let span = expression_span(parsed, *expression).ok()?;
                        (birth.marker.end() < span.start()
                            && span.end() < node.span().start()
                            && sources.slice(span) == Ok("println(\"formed\")"))
                        .then_some(span)
                    }),
                    "G2 real formation-to-For marker",
                )?;
                need(
                    formed.start() > expression_span(parsed, root.initializer)?.end(),
                    "G2 source initialized before actual marker",
                    formed,
                )?;
                let body_node = parsed
                    .ast()
                    .statements()
                    .get(*body)
                    .map_err(|e| failure("G2 original body", e))?;
                let Statement::Block {
                    elements: body_elements,
                } = body_node.payload()
                else {
                    return Err(failure("G2 actual body block", body_node));
                };
                let guard = local(
                    parsed,
                    *body_elements
                        .first()
                        .ok_or_else(|| failure("G2 original Guard local", body_elements))?,
                )?;
                need(
                    guard.kind == VariableKind::Val
                        && sources.slice(guard.marker) == Ok("local")
                        && sources.slice(expression_span(parsed, guard.initializer)?)
                            == Ok("Guard()"),
                    "G2 original Guard declaration",
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
                            Expression::Return { .. } => E::Return(id),
                            _ => return None,
                        };
                        within(body_node.span(), n.span()).then_some(kind)
                    })
                    .collect::<Vec<_>>();
                let kind = match transfers.as_slice() {
                    [] => E::Fallthrough,
                    [kind] => *kind,
                    _ => return Err(failure("G2 actual unique transfer", transfers)),
                };
                need(
                    matches!(
                        (exit, kind),
                        (0, E::Fallthrough)
                            | (1, E::Continue(_))
                            | (2, E::Break(_))
                            | (3, E::Return(_))
                    ),
                    "G2 metadata matches actual exit",
                    kind,
                )?;
                let point = match kind {
                    E::Fallthrough => DropPoint::AfterStatement(*body),
                    E::Continue(e) | E::Break(e) | E::Return(e) => DropPoint::ControlTransfer(e),
                    _ => return Err(failure("G2 actual body exit", kind)),
                };
                let birth_span = expression_span(parsed, birth.initializer)?;
                let guard_span = expression_span(parsed, guard.initializer)?;
                Ok(Site {
                    scan,
                    name,
                    factory,
                    result,
                    map_kind,
                    entry,
                    root,
                    birth,
                    birth_span,
                    statement,
                    source: *source,
                    span: node.span(),
                    bindings: [*key, *value],
                    guard,
                    guard_span,
                    kind,
                    point,
                })
            })
            .collect()
    }

    /// The original Return operand and its two Name arguments, without any lowering inference.
    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn returned(
        site: &Site,
        parsed: &ParsedFile,
    ) -> Result<Option<(ExpressionId, [ExpressionId; 2])>, Failure> {
        let E::Return(id) = site.kind else {
            return Ok(None);
        };
        let node = parsed
            .ast()
            .expressions()
            .get(id)
            .map_err(|e| failure("G2 original Return", e))?;
        let Expression::Return {
            value: Some(value), ..
        } = node.payload()
        else {
            return Err(failure("G2 actual Return operand", node));
        };
        let call = parsed
            .ast()
            .expressions()
            .get(*value)
            .map_err(|e| failure("G2 original result Call", e))?;
        let Expression::Call { arguments, .. } = call.payload() else {
            return Err(failure("G2 actual result Call syntax", call));
        };
        let [key, val] = arguments.as_slice() else {
            return Err(failure("G2 actual result call arity", arguments));
        };
        Ok(Some((*value, [key.value, val.value])))
    }
}
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Report};
use ast::{Site, expression_span, returned, sites, within};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        SymbolKind, UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, IterationExitKind as E, IterationSource, LoanKind,
        MapIterationComponent, OwnershipBindingKind, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction as UA, UnitIterationExitKind as UE, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ExpressionCategory,
        IntrinsicTypeConstructor, MapIterationKind, ParameterMode, UnitCallTarget,
        UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

fn unit_reference(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    id: ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    let span = expression_span(parsed, id)?;
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
        "G2 qualified original Name symbol",
    )
}
fn unit_marker(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    marker: Span,
    kind: SymbolKind,
) -> bool {
    names.names().source_units().iter().any(|u| {
        u.source_unit() == id.source_unit()
            && u.resolution().symbols().iter().any(|s| {
                s.id() == id.symbol() && s.span() == marker && s.kind() == kind && !s.is_synthetic()
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
        "G2 exact two original qualified Map plans",
        typed.map_iterations().len() == 2 && owned.map_iterations().len() == 2,
        (typed.map_iterations(), owned.map_iterations()),
    );
    match sites((case.mutable, case.form, case.exit), sources, parsed) {
        Ok(sites) => {
            for site in &sites {
                report.query(
                    "G2 original unit local formation and exits",
                    &unit_site(site, parsed, names, typed, owned),
                );
            }
        }
        Err(e) => report.query::<()>("G2 original local AST", &Err(e)),
    }
}
fn unit_site(
    site: &Site,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let root = unit_reference(parsed, names, site.source)?;
    let original = if site.root.initializer == site.birth.initializer {
        root
    } else {
        unit_reference(parsed, names, site.root.initializer)?
    };
    let unit = root.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    for (id, local) in [(root, &site.root), (original, &site.birth)] {
        need(
            unit_marker(names, id, local.marker, SymbolKind::Variable)
                && id.source_unit() == unit
                && owned.formed_owned_locals().contains(&id),
            "G2 actual qualified formation history",
            (id, local),
        )?;
    }
    need(
        (root == original) == (site.root.marker == site.birth.marker)
            && (root == original
                || !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Named(original)))
            && !owned.drops().iter().any(|d| {
                d.target() == UnitDropTarget::Temporary(expression(site.birth.initializer))
            }),
        "G2 unit original Move source has no Drop",
        (root, original),
    )?;
    let statement = UnitStatementId::new(unit, site.statement);
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("G2 original qualified local Map plan", statement))?;
    let d = p.descriptor();
    let constructor = if site.map_kind == MapIterationKind::MutableMap {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        typed.map_iteration(statement) == Some(d)
            && d.kind() == site.map_kind
            && d.source() == expression(site.source)
            && typed.sequential_iteration(statement).is_none()
            && owned.iteration(statement).is_none()
            && typed.expression_type(expression(site.source)) == Some(d.source_type())
            && typed.expression_category(expression(site.source))
                == Some(ExpressionCategory::Place)
            && typed.symbol_type(root) == Some(d.source_type())
            && typed.symbol_type(original) == Some(d.source_type())
            && typed.expression_type(expression(site.root.initializer)) == Some(d.source_type())
            && typed.expression_category(expression(site.root.initializer))
                == Some(if root == original {
                    ExpressionCategory::Temporary
                } else {
                    ExpressionCategory::Place
                })
            && typed.expression_type(expression(site.birth.initializer)) == Some(d.source_type())
            && typed.expression_category(expression(site.birth.initializer))
                == Some(ExpressionCategory::Temporary)
            && typed.is_resource_type(d.source_type()) == Some(false)
            && matches!(typed.types().get(d.source_type()),Some(UnitTypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && arguments.as_slice()==[d.key_type(),d.value_type()])
            && [d.key_type(), d.value_type()]
                .into_iter()
                .all(|t| typed.types().get(t) == Some(&UnitTypeKind::Builtin(BuiltinType::String)))
            && p.source_access() == UnitIterationSourceAccess::Owned
            && matches!(p.source_kind(),IterationSource::BorrowedSource(UnitLoanTarget::Place(path)) if path.is_root() && path.root()==root),
        "G2 original qualified local type/category/source capability",
        p,
    )?;
    let factory = one(
        typed.signatures().declarations().iter().filter(|d| {
            d.callable()
                .is_some_and(|c| c.name_span() == site.factory && c.parameters().is_empty())
        }),
        "G2 qualified selected factory",
    )?;
    let call = typed
        .call(expression(site.birth.initializer))
        .ok_or_else(|| failure("G2 qualified actual factory Call", site))?;
    need(
        call.target() == UnitCallTarget::Declaration(factory.declaration())
            && call.arguments().is_empty()
            && call.receiver().is_none()
            && call.return_type() == d.source_type()
            && call.result_source() == CallableResultSource::Owned,
        "G2 selected qualified Call owner birth",
        call,
    )?;
    for (index, binding) in d.bindings().into_iter().enumerate() {
        need(
            unit_marker(names, binding, site.bindings[index], SymbolKind::ForBinding)
                && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && p.bindings()[index].symbol() == binding
                && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                && p.loans()[index].binding() == binding
                && p.loans()[index].statement() == statement
                && p.loans()[index].source() == d.source()
                && p.loans()[index].component()
                    == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                && p.loans()[index].kind() == LoanKind::Shared
                && Some(p.loans()[index].origin()) == p.source(),
            "G2 exact qualified original K/V lineage",
            p.loans(),
        )?;
    }
    let scanner = one(
        typed.signatures().declarations().iter().filter(|d| {
            d.callable()
                .is_some_and(|c| c.name_span() == site.name && c.parameters().is_empty())
        }),
        "G2 actual qualified scanner",
    )?;
    let signature = scanner
        .callable()
        .ok_or_else(|| failure("G2 scanner callable", scanner))?;
    need(
        typed.types().get(signature.return_type())
            == Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
        "G2 qualified scanner Int result",
        signature,
    )?;
    let caller = one(
        typed
            .calls()
            .iter()
            .filter(|c| c.target() == UnitCallTarget::Declaration(scanner.declaration())),
        "G2 actual qualified entry caller",
    )?;
    need(
        caller.arguments().is_empty()
            && caller.return_type() == signature.return_type()
            && caller.expression().source_unit() == unit
            && within(
                site.entry,
                expression_span(parsed, caller.expression().expression())?,
            ),
        "G2 entry actual qualified scanner call",
        caller,
    )?;
    if let Some((value, arguments)) = returned(site, parsed)? {
        let operand = typed
            .call(expression(value))
            .ok_or_else(|| failure("G2 actual qualified result Call", value))?;
        need(
            typed.expression_type(expression(value)) == Some(signature.return_type())
                && operand.return_type() == signature.return_type()
                && matches!(operand.target(),UnitCallTarget::Declaration(id) if typed.signatures().declarations().iter().any(|d|d.declaration()==id && d.callable().is_some_and(|c|c.name_span()==site.result)))
                && matches!(operand.arguments(),[k,v] if k.mode()==ParameterMode::Borrow && v.mode()==ParameterMode::Borrow && k.parameter_type()==d.key_type() && v.parameter_type()==d.value_type())
                && unit_reference(parsed, names, arguments[0])? == d.key_binding()
                && unit_reference(parsed, names, arguments[1])? == d.value_binding(),
            "G2 qualified result Call uses actual K/V roots",
            operand,
        )?;
    }
    let point = match site.point {
        DropPoint::AfterStatement(s) => {
            UnitDropPoint::AfterStatement(UnitStatementId::new(unit, s))
        }
        DropPoint::ControlTransfer(e) => UnitDropPoint::ControlTransfer(expression(e)),
        other => return Err(failure("G2 actual qualified point", other)),
    };
    let kind = match site.kind {
        E::Fallthrough => UE::Fallthrough,
        E::Continue(e) => UE::Continue(expression(e)),
        E::Break(e) => UE::Break(expression(e)),
        E::Return(e) => UE::Return(expression(e)),
        other => return Err(failure("G2 actual qualified kind", other)),
    };
    let source = owned
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    let terminal = matches!(kind, UE::Break(_) | UE::Return(_));
    need(
        source.len() == if terminal { 2 } else { 1 }
            && source.iter().all(|d| {
                d.value_origin() == site.root.marker
                    && (d.point() == UnitDropPoint::LoopExit(statement)
                        || terminal && d.point() == point)
            })
            && source
                .iter()
                .filter(|d| d.point() == UnitDropPoint::LoopExit(statement))
                .count()
                == 1
            && (!terminal || source.iter().filter(|d| d.point() == point).count() == 1),
        "G2 original qualified source Drop set",
        &source,
    )?;
    let guard = one(
        owned
            .formed_owned_locals()
            .iter()
            .copied()
            .filter(|id| unit_marker(names, *id, site.guard.marker, SymbolKind::Variable)),
        "G2 actual qualified Guard formation",
    )?;
    need(
        typed
            .expression_type(expression(site.guard.initializer))
            .is_some_and(|t| typed.is_resource_type(t) == Some(true)),
        "G2 qualified Guard Resource type",
        guard,
    )?;
    let guard = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(guard)),
        "G2 actual qualified Guard Drop",
    )?;
    need(
        guard.point() == point && guard.value_origin() == site.guard.marker,
        "G2 original qualified Guard point",
        guard,
    )?;
    need(
        p.exits().len() == 2,
        "G2 unit complete actual exits",
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
            "G2 exact qualified exit",
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
        if !body || terminal {
            expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
        }
        expected.extend(
            source
                .iter()
                .filter(|d| d.point() == point)
                .map(|d| UA::Drop(**d)),
        );
        need(
            exit.actions() == expected
                && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
            "G2 original unit complete ordered cleanup",
            (exit, expected),
        )?;
    }
    Ok(())
}
