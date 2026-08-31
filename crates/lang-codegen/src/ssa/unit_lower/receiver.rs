//! Instance receiver entry binding 与 `this` 的基础 lowering。

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{Namespace, SymbolKind, UnitReferenceTarget, UnitSymbolId},
    ownership_checking::{UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget},
    parser::Expression,
    source::Span,
    type_checking::{
        Copyability, NominalKind, ParameterMode, UnitCallDescriptor, UnitCallReceiverOrigin,
        UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

use super::{
    LoweredValue, UnitExpressionLowerer, lowering_error, require_value, resolve_concrete_type,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        EntityId, EntityType, LoanId, LoanKind, Operation, Origin, PlaceAccess, PlaceId, SsaTypeId,
    },
    unit_plan::UnitDelegatedCallRoute,
};

pub(super) struct LoweredReceiver {
    pub(super) entity: EntityId,
    pub(super) created_loans: Vec<(LoanId, Span)>,
    pub(super) writeback: Option<ReceiverWriteback>,
}

pub(super) struct ReceiverWriteback {
    pub(super) symbol: UnitSymbolId,
    pub(super) place: PlaceId,
    pub(super) target: SsaTypeId,
    pub(super) original: crate::ssa::model::ValueId,
    pub(super) span: Span,
    pub(super) kind: ReceiverWritebackKind,
}

#[derive(Clone, Copy)]
pub(super) enum ReceiverWritebackKind {
    Copyable,
    MoveOnly,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_delegated_call_receiver(
        &mut self,
        call: UnitExpressionId,
        descriptor: &UnitCallDescriptor,
        receiver: Option<LoweredReceiver>,
        routes: &[UnitDelegatedCallRoute],
        span: Span,
    ) -> Result<Option<LoweredReceiver>, LoweringError> {
        let Some(mut receiver) = receiver else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(receiver_descriptor) = descriptor.receiver() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let fact = self
            .owned
            .ownership()
            .receiver_fact(call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete = resolve_concrete_type(
            self.typed,
            receiver_descriptor.ty(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        let Some(first) = routes.first() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if concrete != first.outer_receiver()
            || receiver_descriptor.mode() != ParameterMode::Borrow
            || fact.kind() != UnitReceiverOwnershipKind::SharedLoan
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        for route in routes {
            let outer_target = self
                .type_ids
                .get(&route.outer_receiver())
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let delegate_target = self
                .type_ids
                .get(&route.delegate_receiver())
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let field = self
                .field_indices
                .get(&(route.outer_receiver(), route.field()))
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let EntityId::Loan(base) = receiver.entity else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            if self
                .function
                .entity(EntityId::Loan(base))
                .map(|entity| entity.ty)
                != Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: outer_target,
                })
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let (_, results) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::SharedHeapFieldLoan { base, field },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: delegate_target,
                    }],
                    Origin::Source(fact.begin_span()),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let EntityId::Loan(delegate) = results[0] else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            receiver.entity = EntityId::Loan(delegate);
            receiver.created_loans.push((delegate, fact.end_span()));
        }
        Ok(Some(receiver))
    }

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
            self.static_self,
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
                created_loans: Vec::new(),
                writeback: None,
            }));
        }
        if fact.kind() == UnitReceiverOwnershipKind::ExclusiveLoan
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
                    kind: LoanKind::Exclusive,
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
                created_loans: Vec::new(),
                writeback: None,
            }));
        }
        let object_value = if fact.kind() == UnitReceiverOwnershipKind::SharedLoan
            && matches!(
                fact.target(),
                UnitReceiverOwnershipTarget::Temporary(target) if *target == expression
            ) {
            self.lower_stateless_object_receiver(
                expression.expression(),
                concrete,
                target,
                fact.begin_span(),
            )?
        } else {
            None
        };
        let lowered = match object_value {
            Some(value) => LoweredValue::Value(value),
            None => self.lower(expression.expression())?,
        };
        let value = match lowered {
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
                    None,
                )
            }
            UnitReceiverOwnershipKind::ExclusiveLoan => {
                self.validate_receiver_loan_target(fact.target(), expression, fact.begin_span())?;
                let writeback = self.inline_receiver_writeback(
                    concrete,
                    fact.target(),
                    expression.expression(),
                    value,
                    fact.begin_span(),
                )?;
                self.begin_receiver_loan(
                    value,
                    target,
                    LoanKind::Exclusive,
                    fact.begin_span(),
                    fact.end_span(),
                    writeback,
                )
            }
            UnitReceiverOwnershipKind::Copy => Ok(Some(LoweredReceiver {
                entity: EntityId::Value(value),
                created_loans: Vec::new(),
                writeback: None,
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
                    created_loans: Vec::new(),
                    writeback: None,
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
                    created_loans: Vec::new(),
                    writeback: None,
                }))
            }
        }
    }

    fn lower_stateless_object_receiver(
        &mut self,
        expression: ExpressionId,
        concrete: UnitTypeId,
        target: crate::ssa::model::SsaTypeId,
        span: Span,
    ) -> Result<Option<crate::ssa::model::ValueId>, LoweringError> {
        let Some(declaration) = self.stateless_object_declaration(expression)? else {
            return Ok(None);
        };
        let Some(UnitTypeKind::Nominal {
            declaration: type_declaration,
            arguments,
        }) = self.typed.types().types().get(concrete)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let declarations = self.names.names().index().declarations();
        let value_declaration = declarations.get(declaration.index());
        let type_declaration_record = declarations.get(type_declaration.index());
        // `object` 在值/类型命名空间各有 declaration；共同 AST root 才是同一 singleton identity。
        let is_object = arguments.is_empty()
            && matches!(value_declaration, Some(declaration) if declaration.kind() == SymbolKind::ObjectValue)
            && matches!(
                (value_declaration, type_declaration_record),
                (Some(value), Some(ty))
                    if value.source_unit() == ty.source_unit() && value.root() == ty.root()
            )
            && self
                .typed
                .types()
                .signatures()
                .declaration(*type_declaration)
                .and_then(|signature| signature.nominal())
                .is_some_and(|nominal| nominal.kind() == NominalKind::Object);
        if !is_object {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::AggregateConstruct {
                    aggregate: target,
                    fields: Vec::new(),
                },
                vec![EntityType::Value(target)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(Some(require_value(results[0], span)?))
    }

    fn stateless_object_declaration(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<lang_frontend::name_resolution::DeclarationId>, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        match node.payload() {
            Expression::Group { expression } => self.stateless_object_declaration(*expression),
            Expression::Name => Ok(self
                .names
                .names()
                .references()
                .iter()
                .find(|reference| {
                    reference.source_unit() == self.source_unit
                        && reference.namespace() == Some(Namespace::Value)
                        && reference.span() == node.span()
                })
                .and_then(|reference| match reference.target() {
                    UnitReferenceTarget::Declaration(declaration) => Some(*declaration),
                    _ => None,
                })),
            _ => Ok(None),
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
                    None,
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
                        created_loans: Vec::new(),
                        writeback: None,
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
                    created_loans: Vec::new(),
                    writeback: None,
                }))
            }
            (UnitReceiverOwnershipKind::Copy, EntityId::Value(value))
                if receiver.mode == ParameterMode::Value =>
            {
                Ok(Some(LoweredReceiver {
                    entity: EntityId::Value(value),
                    created_loans: Vec::new(),
                    writeback: None,
                }))
            }
            (UnitReceiverOwnershipKind::Move, EntityId::Value(value))
                if receiver.mode == ParameterMode::Value =>
            {
                self.current_receiver = None;
                Ok(Some(LoweredReceiver {
                    entity: EntityId::Value(value),
                    created_loans: Vec::new(),
                    writeback: None,
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
            created_loans: vec![(loan, end_span)],
            writeback: None,
        }))
    }

    fn begin_receiver_loan(
        &mut self,
        value: crate::ssa::model::ValueId,
        target: crate::ssa::model::SsaTypeId,
        kind: LoanKind,
        begin_span: Span,
        end_span: Span,
        writeback: Option<(UnitSymbolId, ReceiverWritebackKind)>,
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
            created_loans: vec![(loan, end_span)],
            writeback: writeback.map(|(symbol, kind)| ReceiverWriteback {
                symbol,
                place,
                target,
                original: value,
                span: end_span,
                kind,
            }),
        }))
    }

    fn inline_receiver_writeback(
        &self,
        concrete: UnitTypeId,
        target: &UnitReceiverOwnershipTarget,
        expression: ExpressionId,
        value: crate::ssa::model::ValueId,
        span: Span,
    ) -> Result<Option<(UnitSymbolId, ReceiverWritebackKind)>, LoweringError> {
        let Some(UnitTypeKind::Nominal { declaration, .. }) =
            self.typed.types().types().get(concrete)
        else {
            return Ok(None);
        };
        let kind = self
            .typed
            .types()
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .map(|nominal| nominal.kind())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(kind, NominalKind::ValueClass | NominalKind::EnumClass) {
            return Ok(None);
        }
        let kind = match self.typed.types().copyability(concrete) {
            Copyability::Copyable => ReceiverWritebackKind::Copyable,
            Copyability::MoveOnly => ReceiverWritebackKind::MoveOnly,
            Copyability::Unknown | Copyability::Error => {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        };
        let UnitReceiverOwnershipTarget::Place(place) = target else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if !place.is_root() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let symbol = self.direct_place_symbol(expression, span)?;
        if symbol != place.root()
            || self.bindings.get(&symbol).copied() != Some(LoweredValue::Value(value))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(Some((symbol, kind)))
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
        let ty = resolve_concrete_type(self.typed, ty, self.substitutions, self.static_self, span)?;
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
