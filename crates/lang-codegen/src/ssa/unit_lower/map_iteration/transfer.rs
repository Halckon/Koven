//! A later owner disposition is proved by exact, source-qualified Value delivery facts.
use super::*;
use lang_frontend::ownership_checking::{UnitValueDeliveryFact, UnitValueDeliveryKind};

#[derive(Clone, Copy)]
pub(in super::super) struct QualifiedTransfer {
    pub(in super::super) statement: UnitStatementId,
    pub(in super::super) source: SourceIdentity,
    pub(in super::super) handoff: Option<handoff::Proof>,
}

impl QualifiedTransfer {
    pub(in super::super) fn deliveries<'a>(
        self,
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &'a CompilationUnitOwnership,
    ) -> Result<Vec<&'a UnitValueDeliveryFact>, LoweringError> {
        let plan = owned.map_iteration(self.statement).ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let span = parsed
            .ast()
            .statements()
            .get(self.statement.statement())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let root = match self.source {
            SourceIdentity::Parameter { root, .. } | SourceIdentity::Formed { root, .. } => root,
            _ => return Err(bad()),
        };
        let formation = if matches!(self.source, SourceIdentity::Formed { .. }) {
            Some(formed::formation(
                parsed,
                names,
                typed,
                owned,
                root,
                plan.descriptor().source_type(),
                span,
            )?)
        } else {
            None
        };
        let replacement = formation
            .as_ref()
            .and_then(|f| f.replacement)
            .map(|id| {
                parsed
                    .ast()
                    .expressions()
                    .get(id)
                    .map(|n| n.span())
                    .map_err(|_| bad())
            })
            .transpose()?;
        let mut result = Vec::new();
        for fact in owned.value_deliveries() {
            if fact.kind() != UnitValueDeliveryKind::Move
                || fact
                    .place()
                    .is_none_or(|p| !p.is_root() || p.root() != root)
                || fact.call().source_unit() != self.statement.source_unit()
            {
                continue;
            }
            let call_node = parsed
                .ast()
                .expressions()
                .get(fact.call().expression())
                .map_err(|_| bad())?;
            if call_node.span().start() < span.end()
                || replacement.is_some_and(|at| call_node.span().end() >= at.start())
                || !same_callable(parsed, span, call_node.span())
            {
                continue;
            }
            let Expression::Call { arguments, .. } = call_node.payload() else {
                return Err(bad());
            };
            let call = typed.call(fact.call()).ok_or_else(bad)?;
            let mapped = call
                .arguments()
                .iter()
                .filter(|a| {
                    arguments.get(a.argument_index()).is_some_and(|a| {
                        UnitExpressionId::new(root.source_unit(), a.value) == fact.argument()
                    })
                })
                .collect::<Vec<_>>();
            let [argument] = mapped.as_slice() else {
                return Err(bad());
            };
            let canonical = crate::ssa::map_source::transparent_expression(
                parsed,
                fact.argument().expression(),
                |e| {
                    let e = UnitExpressionId::new(root.source_unit(), e);
                    typed.expression_type(e).zip(typed.expression_category(e))
                },
            )?;
            let node = parsed
                .ast()
                .expressions()
                .get(canonical)
                .map_err(|_| bad())?;
            let references = names
                .names()
                .references()
                .iter()
                .filter(|r| r.source_unit() == root.source_unit() && r.span() == node.span())
                .collect::<Vec<_>>();
            if node.payload() != &Expression::Name
                || !matches!(references.as_slice(), [r] if r.target() == &UnitReferenceTarget::Symbol(root))
                || argument.mode() != ParameterMode::Value
                || argument.parameter_type() != plan.descriptor().source_type()
                || typed.expression_type(fact.argument()) != Some(argument.parameter_type())
                || typed.expression_category(fact.argument()) != Some(ExpressionCategory::Place)
                || typed.copyability(argument.parameter_type()) != Copyability::MoveOnly
                || call.expression() != fact.call()
            {
                return Err(bad());
            }
            result.push(fact);
        }
        Ok(result)
    }
}

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    plan: &UnitMapIterationOwnershipPlan,
    source: SourceIdentity,
) -> Result<Option<QualifiedTransfer>, LoweringError> {
    if plan.source_access() != UnitIterationSourceAccess::Owned {
        return Ok(None);
    }
    let handle = QualifiedTransfer {
        statement: plan.descriptor().statement(),
        source,
        handoff: None,
    };
    if !handle.deliveries(parsed, names, typed, owned)?.is_empty() {
        return Ok(Some(handle));
    }
    let span = parsed
        .ast()
        .statements()
        .get(plan.descriptor().statement().statement())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?
        .span();
    Ok(
        handoff::qualify((parsed, names, typed, owned), plan, source, span)?.map(|proof| {
            QualifiedTransfer {
                handoff: Some(proof),
                ..handle
            }
        }),
    )
}

fn same_callable(parsed: &ParsedFile, left: Span, right: Span) -> bool {
    let enclosing = |at| {
        parsed
            .ast()
            .items()
            .iter()
            .filter(|(_, n)| {
                matches!(n.payload(), Item::Function { .. } | Item::Deinit { .. })
                    && contains(n.span(), at)
            })
            .min_by_key(|(_, n)| n.span().end() - n.span().start())
            .map(|(id, _)| id)
    };
    enclosing(left).is_some()
        && enclosing(left) == enclosing(right)
        && !parsed.ast().expressions().iter().any(|(_, n)| {
            matches!(n.payload(), Expression::Lambda { .. }) && contains(n.span(), right)
        })
}
