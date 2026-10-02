//! Frontend aggregate projection and structural destructuring facts to SSA operations.

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    type_checking::{AggregateProjectionReceiver, DestructuringMode, NominalKind, TypeKind},
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, error, place, value,
};
use crate::ssa::model::{EntityType, Operation, PlaceAccess};

impl ExpressionLowerer<'_> {
    pub(super) fn lower_aggregate_projection(
        &mut self,
        expression: ExpressionId,
    ) -> Result<LoweredValue, LoweringError> {
        let projection = self
            .typed
            .aggregate_projection(expression)
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        let AggregateProjectionReceiver::Expression(receiver_expression) = projection.receiver()
        else {
            // 隐式 `this` 的 native lowering 属于 SPEC-0191；当前必须保持确定性拒绝。
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let receiver_root = self
            .field_replace_ungroup(receiver_expression, span)
            .ok()
            .and_then(|receiver| self.parsed.ast().expressions().get(receiver).ok())
            .and_then(|node| self.references.get(&super::span_key(node.span())));
        if receiver_root.is_some_and(|root| self.owned.loans().iter().any(|active| {
            self.pending_call_loans.contains_key(&(active.call().index(), active.argument().index()))
                && active.kind() == lang_frontend::ownership_checking::LoanKind::Exclusive
                && matches!(active.target(), lang_frontend::ownership_checking::LoanTarget::Place(path)
                    if !path.is_root() && path.root() == *root)
        })) {
            // Source sibling paths can be disjoint, but SSA overlap is still parent-wide.
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let receiver_type = self
            .typed
            .expression_type(receiver_expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = self.require_value(receiver_expression)?;
        let result_type = self.expression_ssa_type(expression, span)?;
        match self.typed.types().get(receiver_type) {
            Some(TypeKind::Nominal { nominal, .. }) => {
                let descriptor = self
                    .typed
                    .nominals()
                    .iter()
                    .find(|descriptor| descriptor.id() == *nominal)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let field = descriptor
                    .fields()
                    .iter()
                    .position(|field| *field == projection.field())
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                if descriptor.kind() == NominalKind::ValueClass {
                    let (_, results) = self.append(
                        Operation::AggregateProject {
                            aggregate: receiver,
                            field,
                        },
                        vec![EntityType::Value(result_type)],
                        span,
                    )?;
                    return Ok(LoweredValue::Value(value(results[0])));
                }
                if descriptor.kind() != NominalKind::Class {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let receiver_ssa = self.expression_ssa_type(receiver_expression, span)?;
                let payload = self
                    .heap_payloads
                    .get(&receiver_ssa)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                self.read_field(
                    Operation::HeapPayloadPlace { owner: receiver },
                    payload,
                    field,
                    result_type,
                    span,
                )
            }
            Some(TypeKind::EnumCase { case, root }) => {
                let root = self
                    .type_ids
                    .get(root)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let (variant, payload) = self
                    .enum_payloads
                    .get(&(root, *case))
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let case = self
                    .typed
                    .enum_cases()
                    .iter()
                    .find(|descriptor| descriptor.id() == *case)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let field = case
                    .payloads()
                    .iter()
                    .position(|(field, _)| *field == projection.field())
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                self.read_field(
                    Operation::TaggedPayloadPlace {
                        owner: receiver,
                        variant,
                    },
                    payload,
                    field,
                    result_type,
                    span,
                )
            }
            _ => Err(error(LoweringErrorKind::MissingFact, span)),
        }
    }

    fn read_field(
        &mut self,
        root: Operation,
        aggregate: crate::ssa::model::SsaTypeId,
        field: usize,
        result_type: crate::ssa::model::SsaTypeId,
        span: lang_frontend::source::Span,
    ) -> Result<LoweredValue, LoweringError> {
        let (_, root_results) = self.append(root, vec![EntityType::Place(aggregate)], span)?;
        let (_, field_results) = self.append(
            Operation::FieldPlace {
                base: place(root_results[0]),
                field,
            },
            vec![EntityType::Place(result_type)],
            span,
        )?;
        let (_, results) = self.append(
            Operation::Read {
                source: PlaceAccess::Place(place(field_results[0])),
            },
            vec![EntityType::Value(result_type)],
            span,
        )?;
        Ok(LoweredValue::Value(value(results[0])))
    }

    pub(super) fn lower_destructuring(
        &mut self,
        statement: StatementId,
        initializer: ExpressionId,
        span: lang_frontend::source::Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .destructuring(statement)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let source = self.require_value(initializer)?;
        let result_types = descriptor
            .components()
            .iter()
            .map(|component| {
                self.type_ids
                    .get(&component.ty())
                    .copied()
                    .map(EntityType::Value)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let operation = match descriptor.mode() {
            DestructuringMode::Copy => Operation::AggregateCopyExplode { aggregate: source },
            DestructuringMode::Consume => Operation::AggregateExplode { aggregate: source },
        };
        let (_, results) = self.append(operation, result_types, span)?;
        for (component, entity) in descriptor.components().iter().zip(results) {
            self.bindings
                .insert(component.symbol(), LoweredValue::Value(value(entity)));
        }
        Ok(LoweredValue::Unit)
    }
}
