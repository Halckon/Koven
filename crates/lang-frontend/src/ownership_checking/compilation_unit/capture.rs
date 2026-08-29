//! Source-qualified closure capture 与结构化 `Transferability` 输入事实。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::ExpressionId,
    name_resolution::{
        NameResolution, ReferenceTarget, ScopeId, ScopeKind, SourceUnitId, SourceUnitInput,
        SymbolKind, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    parser::{Expression, ParsedFile},
    source::Span,
    type_checking::{
        CompilationUnitTypes, Copyability, UnitExpressionId, UnitTransferability, UnitTypeId,
    },
};

use super::super::{
    ClosureCaptureEffect, ClosureCaptureMode, OwnershipCheckingError, Transferability,
};

/// compilation-unit closure 捕获来源的稳定身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnitClosureCaptureSource {
    /// source-qualified 词法 binding。
    Symbol(UnitSymbolId),
    /// lambda 所在 source/classifier 的 receiver。
    This,
}

/// 一个 lambda 对 source-qualified 来源的捕获输入事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitClosureCaptureDescriptor {
    lambda: UnitExpressionId,
    source: UnitClosureCaptureSource,
    ty: UnitTypeId,
    mode: ClosureCaptureMode,
    effect: ClosureCaptureEffect,
    reference_span: Span,
}

impl UnitClosureCaptureDescriptor {
    const fn new(
        lambda: UnitExpressionId,
        source: UnitClosureCaptureSource,
        ty: UnitTypeId,
        mode: ClosureCaptureMode,
        effect: ClosureCaptureEffect,
        reference_span: Span,
    ) -> Self {
        Self {
            lambda,
            source,
            ty,
            mode,
            effect,
            reference_span,
        }
    }

    /// 返回拥有 environment 的 source-qualified lambda。
    #[must_use]
    pub const fn lambda(self) -> UnitExpressionId {
        self.lambda
    }

    /// 返回被捕获的 binding 或 receiver。
    #[must_use]
    pub const fn source(self) -> UnitClosureCaptureSource {
        self.source
    }

    /// 返回 capture 形成位置看到的 unit-global 类型。
    #[must_use]
    pub const fn ty(self) -> UnitTypeId {
        self.ty
    }

    /// 返回 shared/owned capture mode。
    #[must_use]
    pub const fn mode(self) -> ClosureCaptureMode {
        self.mode
    }

    /// 返回 borrow/copy/move formation effect。
    #[must_use]
    pub const fn effect(self) -> ClosureCaptureEffect {
        self.effect
    }

    /// 返回首次触发 capture 的源码引用范围。
    #[must_use]
    pub const fn reference_span(self) -> Span {
        self.reference_span
    }
}

/// 一个具体 source-qualified lambda environment 的能力事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitClosureDescriptor {
    expression: UnitExpressionId,
    move_owned: bool,
    transferability: Transferability,
}

impl UnitClosureDescriptor {
    const fn new(
        expression: UnitExpressionId,
        move_owned: bool,
        transferability: Transferability,
    ) -> Self {
        Self {
            expression,
            move_owned,
            transferability,
        }
    }

    /// 返回 lambda expression identity。
    #[must_use]
    pub const fn expression(self) -> UnitExpressionId {
        self.expression
    }

    /// 返回是否为显式 `move` lambda。
    #[must_use]
    pub const fn move_owned(self) -> bool {
        self.move_owned
    }

    /// 返回具体 capture environment 的结构化转移能力。
    #[must_use]
    pub const fn transferability(self) -> Transferability {
        self.transferability
    }
}

pub(super) struct Analysis {
    pub(super) captures: Vec<UnitClosureCaptureDescriptor>,
    pub(super) closures: Vec<UnitClosureDescriptor>,
    pub(super) transferabilities: Vec<Transferability>,
}

#[derive(Clone, Copy)]
struct Lambda {
    expression: ExpressionId,
    scope: ScopeId,
    span: Span,
    move_owned: bool,
}

#[derive(Clone, Copy)]
struct Candidate {
    source: UnitClosureCaptureSource,
    ty: UnitTypeId,
    span: Span,
}

pub(super) fn analyze(
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
) -> Result<Analysis, OwnershipCheckingError> {
    let transferabilities = (0..typed.types().len())
        .map(UnitTypeId::new)
        .map(|ty| transferability(typed.transferability(ty)))
        .collect::<Vec<_>>();
    let mut captures = Vec::new();
    let mut closures = Vec::new();
    for source in names.names().source_units() {
        let source_unit = source.source_unit();
        let parsed = input_for_source(inputs, names, source_unit)?;
        let resolution = source.resolution();
        for lambda in lambdas(parsed, resolution, source_unit)? {
            let mut candidates = symbol_candidates(lambda, source_unit, resolution, typed)?;
            candidates.extend(this_candidates(lambda, source_unit, parsed, typed)?);
            candidates.sort_by_key(|candidate| {
                (
                    candidate.span.start(),
                    candidate.span.end(),
                    candidate.source,
                )
            });
            let mut seen = BTreeSet::new();
            candidates.retain(|candidate| seen.insert(candidate.source));

            let mode = if lambda.move_owned {
                ClosureCaptureMode::Owned
            } else {
                ClosureCaptureMode::Shared
            };
            let first_capture = captures.len();
            for candidate in candidates {
                let effect = if mode == ClosureCaptureMode::Shared {
                    ClosureCaptureEffect::Borrow
                } else {
                    match typed.copyability(candidate.ty) {
                        Copyability::Copyable => ClosureCaptureEffect::Copy,
                        Copyability::MoveOnly => ClosureCaptureEffect::Move,
                        Copyability::Unknown | Copyability::Error => ClosureCaptureEffect::Unknown,
                    }
                };
                captures.push(UnitClosureCaptureDescriptor::new(
                    UnitExpressionId::new(source_unit, lambda.expression),
                    candidate.source,
                    candidate.ty,
                    mode,
                    effect,
                    candidate.span,
                ));
            }
            let lambda_captures = &captures[first_capture..];
            let mut closure_transferability = Transferability::Transferable;
            if !lambda.move_owned && !lambda_captures.is_empty() {
                closure_transferability = Transferability::NotTransferable;
            } else {
                for capture in lambda_captures {
                    let candidate = transferabilities
                        .get(capture.ty().index())
                        .copied()
                        .ok_or_else(|| invalid_capture(source_unit, lambda.expression))?;
                    closure_transferability = combine(closure_transferability, candidate);
                }
            }
            closures.push(UnitClosureDescriptor::new(
                UnitExpressionId::new(source_unit, lambda.expression),
                lambda.move_owned,
                closure_transferability,
            ));
        }
    }
    Ok(Analysis {
        captures,
        closures,
        transferabilities,
    })
}

fn lambdas(
    parsed: &ParsedFile,
    names: &NameResolution,
    source_unit: SourceUnitId,
) -> Result<Vec<Lambda>, OwnershipCheckingError> {
    let mut lambda_scopes = names
        .scopes()
        .iter()
        .filter(|scope| scope.kind() == ScopeKind::Lambda)
        .filter_map(|scope| {
            scope
                .span()
                .map(|span| ((span.start(), span.end()), scope.id()))
        })
        .collect::<BTreeMap<_, _>>();
    let mut lambdas = Vec::new();
    for (expression, node) in parsed.ast().expressions().iter() {
        let Expression::Lambda { move_span, .. } = node.payload() else {
            continue;
        };
        let Some(scope) = lambda_scopes.remove(&(node.span().start(), node.span().end())) else {
            return Err(invalid_capture(source_unit, expression));
        };
        lambdas.push(Lambda {
            expression,
            scope,
            span: node.span(),
            move_owned: move_span.is_some(),
        });
    }
    if let Some(scope) = lambda_scopes.into_values().next()
        && let Some(span) = names
            .scopes()
            .get(scope.index())
            .and_then(|scope| scope.span())
        && let Some((expression, _)) = parsed
            .ast()
            .expressions()
            .iter()
            .find(|(_, node)| node.span() == span)
    {
        return Err(invalid_capture(source_unit, expression));
    }
    lambdas.sort_by_key(|lambda| (lambda.span.start(), lambda.span.end()));
    Ok(lambdas)
}

fn symbol_candidates(
    lambda: Lambda,
    source_unit: SourceUnitId,
    names: &NameResolution,
    typed: &CompilationUnitTypes,
) -> Result<Vec<Candidate>, OwnershipCheckingError> {
    let mut candidates = Vec::new();
    for reference in names
        .references()
        .iter()
        .filter(|reference| is_descendant(reference.scope(), lambda.scope, names))
    {
        let ReferenceTarget::Symbol(symbol) = reference.target() else {
            continue;
        };
        let Some(declaration) = names.symbols().get(symbol.index()) else {
            return Err(invalid_capture(source_unit, lambda.expression));
        };
        if is_descendant(declaration.scope(), lambda.scope, names) {
            continue;
        }
        let source = match declaration.kind() {
            SymbolKind::Variable
            | SymbolKind::ValueParameter
            | SymbolKind::LambdaParameter
            | SymbolKind::ForBinding
            | SymbolKind::DestructuringBinding => {
                UnitClosureCaptureSource::Symbol(UnitSymbolId::new(source_unit, *symbol))
            }
            SymbolKind::Field => UnitClosureCaptureSource::This,
            SymbolKind::Classifier
            | SymbolKind::ObjectValue
            | SymbolKind::TypeParameter
            | SymbolKind::Constant
            | SymbolKind::Function
            | SymbolKind::EnumVariant
            | SymbolKind::EnumCaseType => continue,
        };
        let ty = match source {
            UnitClosureCaptureSource::Symbol(symbol) => typed.symbol_type(symbol),
            UnitClosureCaptureSource::This => {
                classifier_type(lambda.scope, source_unit, names, typed)
            }
        }
        .ok_or_else(|| invalid_capture(source_unit, lambda.expression))?;
        candidates.push(Candidate {
            source,
            ty,
            span: reference.span(),
        });
    }
    Ok(candidates)
}

fn this_candidates(
    lambda: Lambda,
    source_unit: SourceUnitId,
    parsed: &ParsedFile,
    typed: &CompilationUnitTypes,
) -> Result<Vec<Candidate>, OwnershipCheckingError> {
    let mut candidates = Vec::new();
    for (expression, node) in parsed.ast().expressions().iter() {
        if matches!(node.payload(), Expression::This) && contains(lambda.span, node.span()) {
            let ty = typed
                .expression_type(UnitExpressionId::new(source_unit, expression))
                .ok_or_else(|| invalid_capture(source_unit, lambda.expression))?;
            candidates.push(Candidate {
                source: UnitClosureCaptureSource::This,
                ty,
                span: node.span(),
            });
        }
    }
    Ok(candidates)
}

fn classifier_type(
    scope: ScopeId,
    source_unit: SourceUnitId,
    names: &NameResolution,
    typed: &CompilationUnitTypes,
) -> Option<UnitTypeId> {
    let classifier_scope = ancestors(scope, names)
        .find(|scope| names.scopes()[scope.index()].kind() == ScopeKind::Classifier)?;
    let classifier_span = names.scopes()[classifier_scope.index()].span()?;
    typed
        .signatures()
        .declarations()
        .iter()
        .find_map(|declaration| {
            let nominal = declaration.nominal()?;
            if nominal.symbol().source_unit() != source_unit {
                return None;
            }
            let symbol = names.symbols().get(nominal.symbol().symbol().index())?;
            contains(classifier_span, symbol.span()).then_some(nominal.ty())
        })
}

fn input_for_source<'parsed>(
    inputs: &[SourceUnitInput<'parsed>],
    names: &ValidatedCompilationUnitNames,
    source_unit: SourceUnitId,
) -> Result<&'parsed ParsedFile, OwnershipCheckingError> {
    let source_id = names
        .names()
        .index()
        .source_units()
        .get(source_unit.index())
        .map(|source| source.source_id())
        .ok_or(OwnershipCheckingError::InvalidUnitSource {
            source_unit: source_unit.index(),
        })?;
    inputs
        .iter()
        .copied()
        .find(|input| input.source_id() == source_id)
        .map(SourceUnitInput::parsed)
        .ok_or(OwnershipCheckingError::InvalidUnitSource {
            source_unit: source_unit.index(),
        })
}

fn ancestors(scope: ScopeId, names: &NameResolution) -> impl Iterator<Item = ScopeId> + '_ {
    std::iter::successors(Some(scope), |scope| {
        names
            .scopes()
            .get(scope.index())
            .and_then(|scope| scope.parent())
    })
}

fn is_descendant(scope: ScopeId, ancestor: ScopeId, names: &NameResolution) -> bool {
    ancestors(scope, names).any(|scope| scope == ancestor)
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

const fn transferability(value: UnitTransferability) -> Transferability {
    match value {
        UnitTransferability::Transferable => Transferability::Transferable,
        UnitTransferability::NotTransferable => Transferability::NotTransferable,
        UnitTransferability::Unknown => Transferability::Unknown,
        UnitTransferability::Error => Transferability::Error,
    }
}

const fn combine(left: Transferability, right: Transferability) -> Transferability {
    match (left, right) {
        (Transferability::Error, _) | (_, Transferability::Error) => Transferability::Error,
        (Transferability::Unknown, _) | (_, Transferability::Unknown) => Transferability::Unknown,
        (Transferability::NotTransferable, _) | (_, Transferability::NotTransferable) => {
            Transferability::NotTransferable
        }
        (Transferability::Transferable, Transferability::Transferable) => {
            Transferability::Transferable
        }
    }
}

const fn invalid_capture(
    source_unit: SourceUnitId,
    expression: ExpressionId,
) -> OwnershipCheckingError {
    OwnershipCheckingError::InvalidUnitClosureCapture {
        source_unit: source_unit.index(),
        expression: expression.index(),
    }
}
