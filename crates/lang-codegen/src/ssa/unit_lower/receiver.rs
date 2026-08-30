//! Instance receiver entry binding 与 `this` 的基础 lowering。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget},
    source::Span,
    type_checking::{
        Copyability, ParameterMode, UnitCallDescriptor, UnitCallReceiverOrigin, UnitExpressionId,
    },
};

use super::{
    LoweredValue, UnitExpressionLowerer, lowering_error, require_value, resolve_concrete_type,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, LoanId, LoanKind, Operation, Origin, PlaceAccess},
};

pub(super) struct LoweredReceiver {
    pub(super) entity: EntityId,
    pub(super) created_loan: Option<(LoanId, Span)>,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_call_receiver(
        &mut self,
        call: UnitExpressionId,
        descriptor: &UnitCallDescriptor,
        span: Span,
    ) -> Result<Option<LoweredReceiver>, LoweringError> {
        let Some(receiver) = descriptor.receiver() else {
            if self.owned.ownership().receiver_fact(call).is_some() {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            return Ok(None);
        };
        let fact = self
            .owned
            .ownership()
            .receiver_fact(call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if fact.source() != receiver.origin()
            || fact.receiver_type() != receiver.ty()
            || !receiver_kind_matches(receiver.mode(), fact.kind())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let concrete = resolve_concrete_type(
            self.typed,
            receiver.ty(),
            self.substitutions,
            fact.begin_span(),
        )?;
        let target = self
            .type_ids
            .get(&concrete)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, fact.begin_span()))?;

        if matches!(fact.target(), UnitReceiverOwnershipTarget::This(_)) {
            return self.lower_existing_this_receiver(fact, target);
        }
        let UnitCallReceiverOrigin::Expression(expression) = receiver.origin() else {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                fact.begin_span(),
            ));
        };
        if expression.source_unit() != self.source_unit {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                fact.begin_span(),
            ));
        }
        if fact.kind() == UnitReceiverOwnershipKind::SharedLoan
            && let UnitReceiverOwnershipTarget::Place(place) = fact.target()
            && place.is_root()
            && self.direct_place_symbol(expression.expression(), fact.begin_span())? == place.root()
            && let Some(loan) = self.borrow_bindings.get(&place.root()).copied()
        {
            if self
                .function
                .entity(EntityId::Loan(loan))
                .map(|entity| entity.ty)
                != Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                })
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    fact.begin_span(),
                ));
            }
            return Ok(Some(LoweredReceiver {
                entity: EntityId::Loan(loan),
                created_loan: None,
            }));
        }
        let value = match self.lower(expression.expression())? {
            LoweredValue::Value(value) => value,
            LoweredValue::Unit | LoweredValue::Diverged => {
                return Err(lowering_error(
                    LoweringErrorKind::InvalidModel,
                    fact.begin_span(),
                ));
            }
        };
        match fact.kind() {
            UnitReceiverOwnershipKind::SharedLoan => {
                self.validate_receiver_loan_target(fact.target(), expression, fact.begin_span())?;
                self.begin_receiver_loan(
                    value,
                    target,
                    LoanKind::Shared,
                    fact.begin_span(),
                    fact.end_span(),
                )
            }
            UnitReceiverOwnershipKind::ExclusiveLoan => {
                self.validate_receiver_loan_target(fact.target(), expression, fact.begin_span())?;
                self.begin_receiver_loan(
                    value,
                    target,
                    LoanKind::Exclusive,
                    fact.begin_span(),
                    fact.end_span(),
                )
            }
            UnitReceiverOwnershipKind::Copy => Ok(Some(LoweredReceiver {
                entity: EntityId::Value(value),
                created_loan: None,
            })),
            UnitReceiverOwnershipKind::Move => {
                let UnitReceiverOwnershipTarget::Place(place) = fact.target() else {
                    return Err(lowering_error(
                        LoweringErrorKind::MissingFact,
                        fact.begin_span(),
                    ));
                };
                if !place.is_root() {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.begin_span(),
                    ));
                }
                self.take_owned_binding(place.root(), value, fact.begin_span())?;
                Ok(Some(LoweredReceiver {
                    entity: EntityId::Value(value),
                    created_loan: None,
                }))
            }
            UnitReceiverOwnershipKind::Temporary => {
                let UnitReceiverOwnershipTarget::Temporary(target_expression) = fact.target()
                else {
                    return Err(lowering_error(
                        LoweringErrorKind::MissingFact,
                        fact.begin_span(),
                    ));
                };
                if *target_expression != expression {
                    return Err(lowering_error(
                        LoweringErrorKind::MissingFact,
                        fact.begin_span(),
                    ));
                }
                if self.typed.types().copyability(concrete) == Copyability::MoveOnly {
                    self.take_owned_temporary(value, fact.begin_span())?;
                }
                Ok(Some(LoweredReceiver {
                    entity: EntityId::Value(value),
                    created_loan: None,
                }))
            }
        }
    }

    fn lower_existing_this_receiver(
        &mut self,
        fact: &lang_frontend::ownership_checking::UnitReceiverOwnershipFact,
        target: crate::ssa::model::SsaTypeId,
    ) -> Result<Option<LoweredReceiver>, LoweringError> {
        let receiver = self
            .current_receiver
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, fact.begin_span()))?;
        let UnitReceiverOwnershipTarget::This(owner) = fact.target() else {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                fact.begin_span(),
            ));
        };
        if *owner != receiver.owner || self.type_ids.get(&receiver.ty).copied() != Some(target) {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                fact.begin_span(),
            ));
        }
        match (fact.kind(), receiver.entity) {
            (UnitReceiverOwnershipKind::SharedLoan, EntityId::Value(value))
                if receiver.mode == ParameterMode::Value =>
            {
                self.begin_receiver_loan(
                    value,
                    target,
                    LoanKind::Shared,
                    fact.begin_span(),
                    fact.end_span(),
                )
            }
            (UnitReceiverOwnershipKind::SharedLoan, EntityId::Loan(loan)) => {
                match self
                    .function
                    .entity(EntityId::Loan(loan))
                    .map(|entity| entity.ty)
                {
                    Some(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: actual,
                    }) if actual == target => Ok(Some(LoweredReceiver {
                        entity: EntityId::Loan(loan),
                        created_loan: None,
                    })),
                    Some(EntityType::Loan {
                        kind: LoanKind::Exclusive,
                        target: actual,
                    }) if actual == target && receiver.mode == ParameterMode::Inout => {
                        self.begin_shared_reborrow(loan, target, fact.begin_span(), fact.end_span())
                    }
                    _ => Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.begin_span(),
                    )),
                }
            }
            (UnitReceiverOwnershipKind::ExclusiveLoan, EntityId::Loan(loan))
                if receiver.mode == ParameterMode::Inout
                    && self
                        .function
                        .entity(EntityId::Loan(loan))
                        .map(|entity| entity.ty)
                        == Some(EntityType::Loan {
                            kind: LoanKind::Exclusive,
                            target,
                        }) =>
            {
                Ok(Some(LoweredReceiver {
                    entity: EntityId::Loan(loan),
                    created_loan: None,
                }))
            }
            (UnitReceiverOwnershipKind::Copy, EntityId::Value(value))
                if receiver.mode == ParameterMode::Value =>
            {
                Ok(Some(LoweredReceiver {
                    entity: EntityId::Value(value),
                    created_loan: None,
                }))
            }
            (UnitReceiverOwnershipKind::Move, EntityId::Value(value))
                if receiver.mode == ParameterMode::Value =>
            {
                self.current_receiver = None;
                Ok(Some(LoweredReceiver {
                    entity: EntityId::Value(value),
                    created_loan: None,
                }))
            }
            _ => Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                fact.begin_span(),
            )),
        }
    }

    fn begin_shared_reborrow(
        &mut self,
        source: LoanId,
        target: crate::ssa::model::SsaTypeId,
        begin_span: Span,
        end_span: Span,
    ) -> Result<Option<LoweredReceiver>, LoweringError> {
        let (_, loans) = self
            .function
            .append_instruction(
                self.block,
                Operation::SharedReborrow { source },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                Origin::Source(begin_span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, end_span))?;
        let EntityId::Loan(loan) = loans[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, end_span));
        };
        Ok(Some(LoweredReceiver {
            entity: EntityId::Loan(loan),
            created_loan: Some((loan, end_span)),
        }))
    }

    fn begin_receiver_loan(
        &mut self,
        value: crate::ssa::model::ValueId,
        target: crate::ssa::model::SsaTypeId,
        kind: LoanKind,
        begin_span: Span,
        end_span: Span,
    ) -> Result<Option<LoweredReceiver>, LoweringError> {
        let (_, places) = self
            .function
            .append_instruction(
                self.block,
                Operation::RootPlace { owner: value },
                vec![EntityType::Place(target)],
                Origin::Source(begin_span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, end_span))?;
        let EntityId::Place(place) = places[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, end_span));
        };
        let (_, loans) = self
            .function
            .append_instruction(
                self.block,
                Operation::BorrowBegin { place, kind },
                vec![EntityType::Loan { kind, target }],
                Origin::Source(begin_span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, end_span))?;
        let EntityId::Loan(loan) = loans[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, end_span));
        };
        Ok(Some(LoweredReceiver {
            entity: EntityId::Loan(loan),
            created_loan: Some((loan, end_span)),
        }))
    }

    fn validate_receiver_loan_target(
        &self,
        target: &UnitReceiverOwnershipTarget,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        match target {
            UnitReceiverOwnershipTarget::Place(place) if place.is_root() => {
                let symbol = self.direct_place_symbol(expression.expression(), span)?;
                if symbol == place.root() {
                    Ok(())
                } else {
                    Err(lowering_error(LoweringErrorKind::MissingFact, span))
                }
            }
            UnitReceiverOwnershipTarget::Temporary(target) if *target == expression => Ok(()),
            UnitReceiverOwnershipTarget::Place(_)
            | UnitReceiverOwnershipTarget::Temporary(_)
            | UnitReceiverOwnershipTarget::This(_) => {
                Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
            }
        }
    }

    pub(super) fn lower_this(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let receiver = self
            .current_receiver
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let expression = UnitExpressionId::new(self.source_unit, expression);
        let ty = self
            .typed
            .types()
            .expression_type(expression)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let ty = resolve_concrete_type(self.typed, ty, self.substitutions, span)?;
        if ty != receiver.ty {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        match receiver.entity {
            EntityId::Value(value) => Ok(LoweredValue::Value(value)),
            EntityId::Loan(loan) => {
                if self.typed.types().copyability(ty) != Copyability::Copyable {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                let ssa = self
                    .type_ids
                    .get(&ty)
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let (_, results) = self
                    .function
                    .append_instruction(
                        self.block,
                        Operation::Read {
                            source: PlaceAccess::Loan(loan),
                        },
                        vec![EntityType::Value(ssa)],
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                Ok(LoweredValue::Value(require_value(results[0], span)?))
            }
            EntityId::Place(_) => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
    }
}

const fn receiver_kind_matches(mode: ParameterMode, kind: UnitReceiverOwnershipKind) -> bool {
    matches!(
        (mode, kind),
        (ParameterMode::Borrow, UnitReceiverOwnershipKind::SharedLoan)
            | (
                ParameterMode::Inout,
                UnitReceiverOwnershipKind::ExclusiveLoan
            )
            | (
                ParameterMode::Value,
                UnitReceiverOwnershipKind::Copy
                    | UnitReceiverOwnershipKind::Move
                    | UnitReceiverOwnershipKind::Temporary
            )
    )
}
