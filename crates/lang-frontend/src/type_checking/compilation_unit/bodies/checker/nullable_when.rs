//! Pack checked When domains and bind real Name/Group uses to their active edge.

use crate::{
    ast::ExpressionId,
    name_resolution::{SourceUnitId, UnitSymbolId},
    parser::{Expression, LiteralKind, WhenCondition, WhenEntry},
    source::Span,
    type_checking::{
        CompilationUnitTypeError, UnitNullableWhenAlternativeDescriptor,
        UnitNullableWhenDescriptor, UnitNullableWhenDomain, UnitNullableWhenEntryDescriptor,
        UnitNullableWhenUseWitness, UnitStatementId, UnitTypeId,
    },
};

use super::{BodyChecker, UnitExpressionId, flow::FlowKey, when::WhenFlowDomain};

#[derive(Clone)]
pub(super) struct ActiveWhenFrame {
    control: UnitExpressionId,
    subject: UnitExpressionId,
    entry_index: usize,
    body: UnitStatementId,
    symbol: UnitSymbolId,
    generation: u64,
    nullable_type: UnitTypeId,
    inner_type: UnitTypeId,
    callable_depth: usize,
}

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn begin_nullable_when(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
        subject: Option<ExpressionId>,
        subject_type: Option<UnitTypeId>,
        inner_type: Option<UnitTypeId>,
        origin: Option<(FlowKey, u64)>,
        entries: &[WhenEntry],
    ) -> Result<Option<UnitNullableWhenDescriptor>, CompilationUnitTypeError> {
        let (Some(subject), Some(nullable_type), Some(inner_type)) =
            (subject, subject_type, inner_type)
        else {
            return Ok(None);
        };
        let [null_entry, else_entry] = entries else {
            return Ok(None);
        };
        let [WhenCondition::Expression(condition)] = null_entry.conditions.as_slice() else {
            return Ok(None);
        };
        if null_entry.else_span.is_some()
            || else_entry.else_span.is_none()
            || !else_entry.conditions.is_empty()
            || !matches!(
                self.file(source)
                    .ast()
                    .expressions()
                    .get(*condition)
                    .map(|node| node.payload()),
                Ok(Expression::Literal(LiteralKind::Null))
            )
        {
            return Ok(None);
        }
        let reference = self.nullable_when_reference(source, subject);
        let stable = match (origin, reference) {
            (Some((FlowKey::Symbol(symbol), version)), Some(reference)) => {
                Some((symbol, version, reference))
            }
            _ => None,
        };
        Ok(Some(UnitNullableWhenDescriptor {
            expression: UnitExpressionId::new(source, expression),
            subject: UnitExpressionId::new(source, subject),
            nullable_type,
            inner_type,
            source_category: self.non_null_assertion_source_category(source, subject)?,
            stable_symbol: stable.map(|(symbol, _, _)| symbol),
            subject_reference: stable.map(|(_, _, reference)| reference),
            source_generation: stable.map(|(_, version, _)| version),
            entries: Vec::new(),
            falls_through: false,
        }))
    }

    pub(super) fn push_nullable_when_frame(
        &mut self,
        descriptor: Option<&UnitNullableWhenDescriptor>,
        entry_index: usize,
        entry: &WhenEntry,
        body_domain: &WhenFlowDomain,
        source_valid: bool,
    ) -> usize {
        let base = self.active_nullable_whens.len();
        let Some(descriptor) = descriptor else {
            return base;
        };
        let (Some(symbol), Some(generation)) =
            (descriptor.stable_symbol(), descriptor.source_generation())
        else {
            return base;
        };
        if !source_valid
            || body_domain.nullability() != UnitNullableWhenDomain::NonNull
            || generation == u64::MAX
            || self.flow_version(FlowKey::Symbol(symbol)) != generation
            || self.flow_facts.get(&FlowKey::Symbol(symbol)) != Some(&descriptor.inner_type())
        {
            return base;
        }
        self.active_nullable_whens.push(ActiveWhenFrame {
            control: descriptor.expression(),
            subject: descriptor.subject(),
            entry_index,
            body: UnitStatementId::new(descriptor.expression().source_unit(), entry.body),
            symbol,
            generation,
            nullable_type: descriptor.nullable_type(),
            inner_type: descriptor.inner_type(),
            callable_depth: self.callable_loop_bases.len(),
        });
        base
    }

    pub(super) fn record_nullable_when_use(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        symbol: UnitSymbolId,
        nullable_type: UnitTypeId,
        inner_type: UnitTypeId,
    ) {
        let Some(reference) = self.nullable_when_reference(source, expression) else {
            return;
        };
        let generation = self.flow_version(FlowKey::Symbol(symbol));
        let Some(frame) = self.active_nullable_whens.iter().rev().find(|frame| {
            frame.symbol == symbol
                && frame.generation == generation
                && generation != u64::MAX
                && frame.nullable_type == nullable_type
                && frame.inner_type == inner_type
                && frame.callable_depth == self.callable_loop_bases.len()
        }) else {
            return;
        };
        self.parts
            .nullable
            .when_uses
            .push(UnitNullableWhenUseWitness {
                expression: UnitExpressionId::new(source, expression),
                reference,
                control: frame.control,
                subject: frame.subject,
                entry_index: frame.entry_index,
                body: frame.body,
                symbol,
                source_generation: generation,
                nullable_type,
                inner_type,
            });
    }

    fn nullable_when_reference(
        &self,
        source: SourceUnitId,
        mut expression: ExpressionId,
    ) -> Option<UnitExpressionId> {
        loop {
            match self
                .file(source)
                .ast()
                .expressions()
                .get(expression)
                .ok()?
                .payload()
            {
                Expression::Name => return Some(UnitExpressionId::new(source, expression)),
                Expression::Group { expression: inner } => expression = *inner,
                _ => return None,
            }
        }
    }

    pub(super) fn finish_nullable_when_facts(&mut self) {
        self.parts
            .nullable
            .whens
            .sort_by_key(|fact| fact.expression());
        self.parts
            .nullable
            .when_uses
            .sort_by_key(|fact| fact.expression());
    }
}

pub(super) fn begin_entry(
    descriptor: Option<&UnitNullableWhenDescriptor>,
    index: usize,
    entry: &WhenEntry,
    input: &WhenFlowDomain,
) -> Option<UnitNullableWhenEntryDescriptor> {
    let descriptor = descriptor?;
    Some(UnitNullableWhenEntryDescriptor {
        index,
        body: UnitStatementId::new(descriptor.expression().source_unit(), entry.body),
        span: entry.span,
        is_else: entry.else_span.is_some(),
        input_domain: input.nullability(),
        body_domain: UnitNullableWhenDomain::Empty,
        remaining_domain: UnitNullableWhenDomain::Empty,
        alternatives: Vec::new(),
        falls_through: false,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record_alternative(
    entry: &mut Option<UnitNullableWhenEntryDescriptor>,
    source: SourceUnitId,
    condition: &WhenCondition,
    span: Span,
    input: &WhenFlowDomain,
    matched: &WhenFlowDomain,
    remaining: &WhenFlowDomain,
) {
    if let (Some(entry), WhenCondition::Expression(expression)) = (entry, condition) {
        entry
            .alternatives
            .push(UnitNullableWhenAlternativeDescriptor {
                expression: UnitExpressionId::new(source, *expression),
                span,
                input_domain: input.nullability(),
                matched_domain: matched.nullability(),
                remaining_domain: remaining.nullability(),
            });
    }
}

pub(super) fn finish_entry(
    descriptor: &mut Option<UnitNullableWhenDescriptor>,
    entry: Option<UnitNullableWhenEntryDescriptor>,
    body: &WhenFlowDomain,
    remaining: &WhenFlowDomain,
    falls_through: bool,
) {
    if let (Some(descriptor), Some(mut entry)) = (descriptor, entry) {
        entry.body_domain = body.nullability();
        entry.remaining_domain = remaining.nullability();
        entry.falls_through = falls_through;
        descriptor.entries.push(entry);
    }
}
