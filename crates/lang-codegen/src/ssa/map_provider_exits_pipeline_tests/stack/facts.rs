//! Original AST anchors plus normalized snapshots from the same public FE products.
mod representation {
    //! Original component and Guard nominal identities from the observed typed products.
    use super::*;
    use lang_frontend::{
        name_resolution::{SymbolKind, ValidatedCompilationUnitNames},
        ownership_checking::CompilationUnitOwnership,
        type_checking::{
            BuiltinType, CompilationUnitTypes, NominalKind, ParameterMode, UnitConstructionTarget,
            UnitExpressionId, UnitTypeId, UnitTypeKind,
        },
    };

    pub(super) fn unit(
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        ty: UnitTypeId,
    ) -> Result<Representation, Failure> {
        let (inner, nullable) = match typed.types().get(ty) {
            Some(UnitTypeKind::Nullable(inner)) => (*inner, true),
            _ => (ty, false),
        };
        if typed.types().get(inner) == Some(&UnitTypeKind::Builtin(BuiltinType::String)) {
            need(!nullable, "stack qualified String is nonnullable", ty)?;
            return Ok(Representation {
                nullable: false,
                deinit: None,
                strings: 0,
            });
        }
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = typed.types().get(inner)
        else {
            return Err(failure("stack qualified actual component type", ty));
        };
        let descriptor = one(
            typed
                .signatures()
                .declarations()
                .iter()
                .filter(|d| d.declaration() == *declaration)
                .filter_map(|d| d.nominal()),
            "stack qualified component nominal",
        )?;
        let declared = one(
            names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.id() == *declaration),
            "stack original qualified Resource declaration",
        )?;
        let class = parsed.ast().items().get(declared.root()).unwrap();
        let deinit = descriptor
            .deinit()
            .ok_or_else(|| failure("stack qualified Resource deinit", descriptor))?;
        let original = parsed.ast().items().get(deinit.item().item()).unwrap();
        need(
            arguments.is_empty()
                && descriptor.kind() == NominalKind::Class
                && descriptor.type_parameters().is_empty()
                && descriptor.interfaces().is_empty()
                && deinit.owner() == *declaration
                && deinit.receiver_type() == inner
                && deinit.receiver_mode() == ParameterMode::Borrow
                && deinit.item().source_unit() == declared.source_unit()
                && inside(original.span(), class.span())
                && matches!(original.payload(),Item::Deinit{body,..} if *body==deinit.body().statement())
                && descriptor.fields().iter().all(|f| {
                    typed.types().get(f.ty()) == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                }),
            "stack qualified exact Resource/String field/original deinit",
            (descriptor, deinit, class),
        )?;
        Ok(Representation {
            nullable,
            deinit: Some(original.span()),
            strings: descriptor.fields().len(),
        })
    }
    fn locals(
        parsed: &ParsedFile,
        sites: &[AstSite],
    ) -> Vec<(Span, lang_frontend::ast::ExpressionId)> {
        parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, n)| {
                let Item::Variable {
                    name: NameMarker::Present(marker),
                    initializer,
                    ..
                } = n.payload()
                else {
                    return None;
                };
                sites
                    .iter()
                    .any(|s| inside(n.span(), s.body_span))
                    .then_some((*marker, *initializer))
            })
            .collect()
    }
    pub(super) fn unit_guards(
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        sites: &[AstSite],
    ) -> Result<Vec<GuardInput>, Failure> {
        let source = one(
            names.names().source_units().iter(),
            "stack Guard original qualified source",
        )?;
        let unit = source.source_unit();
        let mut result = Vec::new();
        for (marker, initializer) in locals(parsed, sites) {
            let local = one(
                owned.formed_owned_locals().iter().copied().filter(|s| {
                    s.source_unit() == unit
                        && source.resolution().symbols().iter().any(|n| {
                            n.id() == s.symbol()
                                && n.span() == marker
                                && n.kind() == SymbolKind::Variable
                        })
                }),
                "stack qualified original formed Guard",
            )?;
            let construction = typed
                .construction(UnitExpressionId::new(unit, initializer))
                .ok_or_else(|| failure("stack qualified Guard construction", initializer))?;
            let repr = self::unit(parsed, names, typed, construction.result_type())?;
            need(
                repr.deinit.is_some()
                    && !repr.nullable
                    && repr.strings == 0
                    && construction.arguments().is_empty()
                    && construction.instance().type_arguments().is_empty()
                    && matches!(typed.types().get(construction.result_type()),Some(UnitTypeKind::Nominal{declaration,arguments}) if construction.target()==UnitConstructionTarget::Nominal(*declaration)&&arguments.is_empty())
                    && typed.symbol_type(local) == Some(construction.result_type())
                    && typed.expression_type(UnitExpressionId::new(unit, initializer))
                        == Some(construction.result_type()),
                "stack qualified Guard exact formation and nominal",
                (construction, &repr, local),
            )?;
            result.push(GuardInput {
                marker,
                initializer: parsed.ast().expressions().get(initializer).unwrap().span(),
                deinit: repr.deinit.unwrap(),
            });
        }
        Ok(result)
    }
}
mod source_drops {
    //! Exact global source Drop obligations, including lexical Returns outside For plans.
    use super::*;
    use lang_frontend::{
        name_resolution::ValidatedCompilationUnitNames,
        ownership_checking::{CompilationUnitOwnership, UnitDropPoint, UnitDropTarget},
    };

    pub(super) fn unit(
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        owned: &CompilationUnitOwnership,
        sites: &[Site],
    ) -> Result<Vec<SourceDrop>, Failure> {
        let source = one(
            names.names().source_units().iter(),
            "stack global Drop original qualified source",
        )?;
        let mut result = Vec::new();
        for site in sites {
            let (root, target) = match site.source {
                Source::Parameter {
                    marker,
                    borrow: false,
                    ..
                } => {
                    let symbol = one(
                        owned.bindings().iter().filter_map(|binding| {
                            let id = binding.symbol();
                            (id.source_unit() == source.source_unit()
                                && source
                                    .resolution()
                                    .symbols()
                                    .iter()
                                    .any(|s| s.id() == id.symbol() && s.span() == marker))
                            .then_some(id)
                        }),
                        "stack global Drop exact qualified source root",
                    )?;
                    (marker, UnitDropTarget::Named(symbol))
                }
                Source::Call { span, .. } => (
                    span,
                    UnitDropTarget::Temporary(lang_frontend::type_checking::UnitExpressionId::new(
                        source.source_unit(),
                        site.ast.source,
                    )),
                ),
                Source::Parameter { borrow: true, .. } => continue,
            };
            let drops = owned
                .drops()
                .iter()
                .filter(|d| d.target() == target)
                .collect::<Vec<_>>();
            need(
                !drops.is_empty(),
                "stack qualified source global Drop obligations",
                target,
            )?;
            for drop in drops {
                need(
                    drop.value_origin() == root,
                    "stack qualified global Drop exact source birth",
                    drop,
                )?;
                let point = match drop.point() {
                    UnitDropPoint::LoopExit(id) => {
                        need(
                            id.source_unit() == source.source_unit(),
                            "stack global Drop original unit",
                            drop,
                        )?;
                        loop_point(parsed, id.statement(), sites)?
                    }
                    UnitDropPoint::ControlTransfer(id) => {
                        need(
                            id.source_unit() == source.source_unit(),
                            "stack global Return Drop original unit",
                            drop,
                        )?;
                        transfer_point(parsed, id.expression(), site.ast.function)?
                    }
                    UnitDropPoint::AfterStatement(id) => {
                        need(
                            id.source_unit() == source.source_unit(),
                            "stack global scope Drop original unit",
                            drop,
                        )?;
                        scope_point(parsed, id.statement(), site.ast.function)?
                    }
                    _ => {
                        return Err(failure(
                            "stack qualified global source Drop original point",
                            drop,
                        ));
                    }
                };
                // The public UnitDropFact deliberately has no owner or condition
                // ID. Preserve its real qualified target/point; SSA independently
                // proves the current owner on every incoming path.
                save(
                    &mut result,
                    SourceDrop {
                        root,
                        function: site.ast.function,
                        point,
                    },
                );
            }
        }
        Ok(result)
    }
    fn save(drops: &mut Vec<SourceDrop>, drop: SourceDrop) {
        if !drops.contains(&drop) {
            drops.push(drop);
        }
    }
    fn loop_point(
        parsed: &ParsedFile,
        statement: StatementId,
        sites: &[Site],
    ) -> Result<SourceDropPoint, Failure> {
        let site = one(
            sites.iter().filter(|s| s.ast.statement == statement),
            "stack global Drop actual provider statement",
        )?;
        need(
            parsed.ast().statements().get(statement).unwrap().span() == site.ast.span,
            "stack global Drop original LoopExit",
            site,
        )?;
        Ok(SourceDropPoint::LoopExit(site.ast.span))
    }
    fn transfer_point(
        parsed: &ParsedFile,
        expression: ExpressionId,
        function: Span,
    ) -> Result<SourceDropPoint, Failure> {
        let node = parsed.ast().expressions().get(expression).unwrap();
        need(
            inside(node.span(), function),
            "stack global transfer Drop stays in source callable",
            node,
        )?;
        match node.payload() {
            Expression::Return { value, .. } => Ok(SourceDropPoint::Return {
                span: node.span(),
                operand: value.map(|v| parsed.ast().expressions().get(v).unwrap().span()),
            }),
            Expression::Break { .. } => Ok(SourceDropPoint::Break(node.span())),
            _ => Err(failure("stack actual global finite transfer AST", node)),
        }
    }
    fn scope_point(
        parsed: &ParsedFile,
        statement: StatementId,
        function: Span,
    ) -> Result<SourceDropPoint, Failure> {
        let span = parsed.ast().statements().get(statement).unwrap().span();
        let callable = one(
            parsed.roots().iter().filter_map(|id| {
                let node = parsed.ast().items().get(*id).ok()?;
                match node.payload() {
                    Item::Function {
                        form:
                            lang_frontend::parser::FunctionForm::ImplicitUnitBlock(body)
                            | lang_frontend::parser::FunctionForm::Explicit {
                                body: lang_frontend::parser::FunctionBody::Block(body),
                                ..
                            },
                        ..
                    } if node.span() == function => Some(*body),
                    _ => None,
                }
            }),
            "stack original implicit Return source scope",
        )?;
        need(
            callable == statement,
            "stack global AfterStatement Drop is exact callable body",
            (statement, callable),
        )?;
        Ok(SourceDropPoint::ScopeEnd(span))
    }
}
mod unit;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
};
pub(super) use unit::collect as unit;

#[derive(Clone, Debug)]
pub(super) struct AstSite {
    pub statement: StatementId,
    pub source: ExpressionId,
    pub body: StatementId,
    pub span: Span,
    pub body_span: Span,
    pub function: Span,
    pub names: [Span; 2],
}
#[derive(Clone, Copy, Debug)]
pub(super) enum Source {
    Parameter {
        slot: usize,
        marker: Span,
        borrow: bool,
    },
    Call {
        span: Span,
        callee: Span,
        arity: usize,
    },
}
#[derive(Clone, Debug)]
pub(super) struct Site {
    pub ast: AstSite,
    pub source: Source,
    pub mutable: bool,
    pub key_type: String,
    pub value_type: String,
    pub owner_markers: Vec<Span>,
    pub representation: Representation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Value(usize),
    Key(usize),
    Finish(usize),
    End(usize),
    Drop(Span),
}
#[derive(Clone, Debug)]
pub(super) struct Cleanup {
    pub point: Span,
    pub kind: u8, // 0 fallthrough, 1 exhaustion, 2 Return, 3 Break
    pub actions: Vec<Action>,
}
#[derive(Clone, Debug)]
pub(super) struct Call {
    pub span: Span,
    pub callee: Span,
    pub arguments: Vec<(Span, Option<Span>, bool)>,
    pub loans: Vec<Option<CallLoan>>,
}
#[derive(Clone, Debug)]
pub(super) struct CallLoan {
    pub begin: Span,
    pub end: Span,
}
#[derive(Clone, Debug)]
pub(super) struct Representation {
    pub nullable: bool,
    pub deinit: Option<Span>,
    pub strings: usize,
}
#[derive(Clone, Debug)]
pub(super) struct GuardInput {
    pub marker: Span,
    pub initializer: Span,
    pub deinit: Span,
}
#[derive(Clone, Debug)]
pub(super) struct NameInput {
    pub span: Span,
    pub marker: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SourceDropPoint {
    LoopExit(Span),
    Return { span: Span, operand: Option<Span> },
    Break(Span),
    ScopeEnd(Span),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceDrop {
    pub root: Span,
    pub function: Span,
    pub point: SourceDropPoint,
}
#[derive(Clone, Debug)]
pub(super) struct Inputs {
    pub sites: Vec<Site>,
    pub cleanups: Vec<Cleanup>,
    pub calls: Vec<Call>,
    pub guards: Vec<GuardInput>,
    pub names: Vec<NameInput>,
    pub source_drops: Vec<SourceDrop>,
}

pub(super) fn inside(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && inner.start() >= outer.start()
        && inner.end() <= outer.end()
}

pub(super) fn ast(parsed: &ParsedFile) -> Result<Vec<AstSite>, Failure> {
    let mut sites = Vec::new();
    for (statement, node) in parsed.ast().statements().iter() {
        let Statement::For {
            source,
            body,
            binding,
            ..
        } = node.payload()
        else {
            continue;
        };
        let function = one(
            parsed.roots().iter().filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                (matches!(n.payload(), Item::Function { .. }) && inside(node.span(), n.span()))
                    .then_some(n.span())
            }),
            "stack For exact enclosing callable",
        )?;
        let ForBinding::Destructuring { names, .. } = binding else {
            return Err(failure("stack original K/V binding pair", binding));
        };
        let names = names
            .iter()
            .map(|name| match name {
                NameMarker::Present(s) => Ok(*s),
                _ => Err(failure("stack original present binding", name)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let names = names
            .try_into()
            .map_err(|v| failure("stack exact K/V arity", v))?;
        sites.push(AstSite {
            statement,
            source: *source,
            body: *body,
            span: node.span(),
            body_span: parsed.ast().statements().get(*body).unwrap().span(),
            function,
            names,
        });
    }
    sites.sort_by_key(|s| s.span.start());
    need(!sites.is_empty(), "stack complete actual For set", &sites)?;
    Ok(sites)
}

pub(super) fn parameter(
    parsed: &ParsedFile,
    function: Span,
    marker: Span,
) -> Result<usize, Failure> {
    let parameters = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            match n.payload() {
                Item::Function { parameters, .. } if n.span() == function => Some(parameters),
                _ => None,
            }
        }),
        "stack original callable parameter list",
    )?;
    one(
        parameters
            .iter()
            .enumerate()
            .filter_map(|(i, p)| matches!(p.name,NameMarker::Present(s) if s==marker).then_some(i)),
        "stack exact source parameter slot",
    )
}

pub(super) fn callable(parsed: &ParsedFile, marker: Span) -> Result<Span, Failure> {
    one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            matches!(n.payload(),Item::Function{name:NameMarker::Present(s),..} if *s==marker)
                .then_some(n.span())
        }),
        "stack original selected callable declaration",
    )
}

pub(super) fn layer(sites: &[AstSite], statement: StatementId) -> Result<usize, Failure> {
    one(
        sites
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (s.statement == statement).then_some(i)),
        "stack action actual provider statement",
    )
}

pub(super) fn exits(parsed: &ParsedFile, sites: &[AstSite], index: usize) -> Vec<(u8, Span)> {
    let site = &sites[index];
    let mut expected = vec![(0, site.body_span), (1, site.span)];
    for (_, node) in parsed.ast().expressions().iter() {
        if !inside(node.span(), site.body_span) {
            continue;
        }
        let kind = match node.payload() {
            Expression::Return { .. } => 2,
            Expression::Break { .. }
                if !sites.iter().any(|s| {
                    s.statement != site.statement
                        && inside(s.span, site.body_span)
                        && inside(node.span(), s.body_span)
                }) =>
            {
                3
            }
            _ => continue,
        };
        expected.push((kind, node.span()));
    }
    expected
}

pub(super) fn save(
    cleanups: &mut Vec<Cleanup>,
    point: Span,
    kind: u8,
    actions: Vec<Action>,
) -> Result<(), Failure> {
    if let Some(prior) = cleanups.iter().find(|c| c.point == point && c.kind == kind) {
        need(
            prior.actions == actions,
            "stack same-point shared Return actions identical",
            (prior, actions),
        )?;
    } else {
        cleanups.push(Cleanup {
            point,
            kind,
            actions,
        });
    }
    Ok(())
}

pub(super) fn validate_order(actions: &[Action]) -> Result<(), Failure> {
    for (index, action) in actions.iter().enumerate() {
        match action {
            Action::Value(site) => need(
                actions.get(index + 1) == Some(&Action::Key(*site)),
                "stack FE exact VEnd/KEnd adjacency",
                actions,
            )?,
            Action::Finish(site) => need(
                actions.get(index + 1) == Some(&Action::End(*site)),
                "stack FE exact Finish/EndSource adjacency",
                actions,
            )?,
            _ => {}
        }
    }
    Ok(())
}
