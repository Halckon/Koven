//! Retain a tag value when the published subject read retires its named enum owner.
use super::*;

pub(super) struct OwnedWhenSubject {
    root: UnitSymbolId,
    operands: Vec<ExpressionId>,
    snapshot_tag: bool,
}

#[derive(Clone, Copy)]
pub(super) enum WhenSubjectValue {
    Boolean(ValueId),
    TaggedOwner { owner: ValueId, tagged: SsaTypeId },
    TaggedDiscriminant { tag: ValueId, tagged: SsaTypeId },
}

impl WhenSubjectValue {
    pub(super) fn tagged_type(self) -> Option<SsaTypeId> {
        match self {
            Self::Boolean(_) => None,
            Self::TaggedOwner { tagged, .. } | Self::TaggedDiscriminant { tagged, .. } => {
                Some(tagged)
            }
        }
    }
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn owned_when_subject(
        &self,
        subject: ExpressionId,
        span: Span,
    ) -> Result<OwnedWhenSubject, LoweringError> {
        let root = self
            .direct_name_symbol(subject, span)?
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        if !matches!(self.bindings.get(&root), Some(LoweredValue::Value(_))) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let operands = self.enum_operand_chain(subject, span)?;
        if !operands.last().is_some_and(|operand| {
            self.parsed
                .ast()
                .expressions()
                .get(*operand)
                .is_ok_and(|node| matches!(node.payload(), Expression::Name))
        }) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let snapshot_tag = operands.iter().any(|&operand| {
            let point =
                UnitDropPoint::AfterExpression(UnitExpressionId::new(self.source_unit, operand));
            self.owned
                .drops()
                .iter()
                .any(|fact| fact.point() == point && fact.target() == UnitDropTarget::Named(root))
        });
        Ok(OwnedWhenSubject {
            root,
            operands,
            snapshot_tag,
        })
    }

    pub(super) fn lower_when_subject(
        &mut self,
        subject: Option<(ExpressionId, Option<SsaTypeId>)>,
        owned: Option<OwnedWhenSubject>,
        entries: &[WhenEntry],
        span: Span,
    ) -> Result<(Option<WhenSubjectValue>, Option<UnitSymbolId>), LoweringError> {
        let Some((expression, tagged)) = subject else {
            return Ok((None, None));
        };
        if let Some(plan) = owned.as_ref().filter(|plan| plan.snapshot_tag) {
            let tagged =
                tagged.ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            // Only static case tests can use a tag snapshot; no value equality or
            // effectful condition is silently changed into tag comparison.
            if let Some(condition) = entries
                .iter()
                .flat_map(|entry| &entry.conditions)
                .find(|condition| !matches!(condition, WhenCondition::TypeTest { .. }))
            {
                let rejection_span = match condition {
                    WhenCondition::Expression(expression) => self
                        .parsed
                        .ast()
                        .expressions()
                        .get(*expression)
                        .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
                        .span(),
                    _ => span,
                };
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    rejection_span,
                ));
            }
            let operand = *plan
                .operands
                .last()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let LoweredValue::Value(owner) = self.lower_expression(operand)? else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            if self
                .function
                .entity(EntityId::Value(owner))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(tagged))
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let tag = self.lower_enum_discriminant(owner, span)?;
            for &operand in plan.operands.iter().rev() {
                self.emit_drops(UnitDropPoint::AfterExpression(UnitExpressionId::new(
                    self.source_unit,
                    operand,
                )))?;
            }
            return Ok((
                Some(WhenSubjectValue::TaggedDiscriminant { tag, tagged }),
                None,
            ));
        }
        let value = self.require_expression_value(expression)?;
        let subject = match tagged {
            Some(tagged) => WhenSubjectValue::TaggedOwner {
                owner: value,
                tagged,
            },
            None => WhenSubjectValue::Boolean(value),
        };
        Ok((Some(subject), owned.map(|plan| plan.root)))
    }
}
