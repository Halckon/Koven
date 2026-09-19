//! compilation-unit lowering 的 owner 状态转移与 drop fact 消费。

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{DeclarationId, UnitSymbolId},
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::Expression,
    source::Span,
    type_checking::{
        Copyability, ExpressionCategory, NominalKind, ParameterMode, UnitExpressionId, UnitTypeId,
        UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, span_key};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, Operation, Origin, ValueId},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn require_conditional_receiver_drop_obligation(
        &self,
        owner: DeclarationId,
        receiver_type: UnitTypeId,
        concrete: UnitTypeId,
        origin: Span,
    ) -> Result<Span, LoweringError> {
        let Some(UnitTypeKind::StaticSelf(interface)) = self.typed.types().get(receiver_type)
        else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, origin));
        };
        let Some(UnitTypeKind::Nominal { declaration, .. }) = self.typed.types().get(*interface)
        else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, origin));
        };
        let interface_kind = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .map(|nominal| nominal.kind());
        let resolved = super::resolve_concrete_type(
            self.typed,
            receiver_type,
            self.substitutions,
            self.static_self,
            origin,
        )?;
        if *declaration != owner
            || interface_kind != Some(NominalKind::Interface)
            || resolved != concrete
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, origin));
        }
        self.owned
            .conditional_receiver_drops()
            .iter()
            .find(|fact| {
                fact.owner() == owner
                    && fact.receiver_type() == receiver_type
                    && fact.value_origin() == origin
            })
            .map(|fact| fact.value_origin())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, origin))
    }

    pub(super) fn transfer_owned_expression(
        &mut self,
        expression: ExpressionId,
        value: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let expression = UnitExpressionId::new(self.source_unit, expression);
        let ty = self
            .typed
            .expression_type(expression)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.copyability(ty) != Copyability::MoveOnly {
            return Ok(());
        }
        // 零 payload enum case 保留 member-access 的 Place 类别，但 construction fact 仍表示
        // 每次求值新建一个 root owner；透明 group 也必须回到该 construction identity 转移。
        if let Some(origin) = self.construction_origin(expression.expression(), span)? {
            return self.take_owned_temporary_origin(origin, value, span);
        }
        match self.typed.expression_category(expression) {
            Some(ExpressionCategory::Temporary) => self.take_owned_temporary(value, span),
            Some(ExpressionCategory::Place) => {
                if self.is_this_expression(expression.expression(), span)? {
                    return self.take_owned_receiver(value, span);
                }
                let symbol = self.direct_place_symbol(expression.expression(), span)?;
                self.take_owned_binding(symbol, value, span)
            }
            None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }

    pub(super) fn take_owned_receiver(
        &mut self,
        value: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        match self.current_receiver {
            Some(receiver)
                if receiver.mode == ParameterMode::Value
                    && receiver.entity == EntityId::Value(value) =>
            {
                self.consumed_receiver = Some(receiver.into());
                self.current_receiver = None;
                Ok(())
            }
            Some(_) | None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }

    pub(super) fn is_this_expression(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::This => Ok(true),
            Expression::Group { expression } => self.is_this_expression(*expression, span),
            _ => Ok(false),
        }
    }

    pub(super) fn take_owned_binding(
        &mut self,
        symbol: UnitSymbolId,
        value: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        match self.bindings.remove(&symbol) {
            Some(LoweredValue::Value(bound)) if bound == value => {
                self.closure_bindings.remove(&symbol);
                Ok(())
            }
            Some(LoweredValue::Unit | LoweredValue::Diverged | LoweredValue::Value(_)) | None => {
                Err(lowering_error(LoweringErrorKind::MissingFact, span))
            }
        }
    }

    pub(super) fn take_owned_temporary(
        &mut self,
        value: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let before = self.temporaries.len();
        self.temporaries.retain(|_, temporary| *temporary != value);
        if self.temporaries.len() == before {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn take_owned_temporary_origin(
        &mut self,
        origin: UnitExpressionId,
        value: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        match self.temporaries.get(&origin) {
            Some(temporary) if *temporary == value => {
                // Group 会以相同 ValueId 登记透明 alias；canonical origin 验证通过后一起清除。
                self.temporaries.retain(|_, temporary| *temporary != value);
                Ok(())
            }
            Some(_) | None => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }

    fn construction_origin(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<UnitExpressionId>, LoweringError> {
        let origin = UnitExpressionId::new(self.source_unit, expression);
        if self.typed.construction(origin).is_some() {
            return Ok(Some(origin));
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Group { expression } => self.construction_origin(*expression, node.span()),
            _ => Ok(None),
        }
    }

    pub(super) fn emit_drops(&mut self, point: UnitDropPoint) -> Result<(), LoweringError> {
        let conditional = self
            .owned
            .conditional_receiver_drops()
            .iter()
            .copied()
            .filter(|fact| fact.point() == point)
            .collect::<Vec<_>>();
        if conditional.len() > 1 {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                conditional[0].value_origin(),
            ));
        }
        let facts = self
            .owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.point() == point)
            .collect::<Vec<_>>();
        if !conditional.is_empty()
            && facts
                .iter()
                .any(|fact| matches!(fact.target(), UnitDropTarget::This(_)))
        {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                conditional[0].value_origin(),
            ));
        }
        if conditional
            .first()
            .is_some_and(|fact| fact.preceding_drops() > facts.len())
        {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                conditional[0].value_origin(),
            ));
        }
        let conditional_drop = conditional
            .first()
            .copied()
            .map(|fact| {
                self.conditional_receiver_drop_owner(fact)
                    .map(|owner| owner.map(|owner| (fact, owner)))
            })
            .transpose()?
            .flatten();
        self.validate_closure_drop_facts(&facts)?;
        // 条件 receiver 按 frontend 发布的位置与无条件 drop 交错，Copyable 仍跳过。
        for index in 0..=facts.len() {
            if let Some((fact, owner)) = conditional_drop
                && fact.preceding_drops() == index
            {
                self.consumed_receiver = self.current_receiver.map(Into::into);
                self.current_receiver = None;
                self.function
                    .append_instruction(
                        self.block,
                        Operation::Drop { owner },
                        Vec::new(),
                        Origin::Source(fact.value_origin()),
                    )
                    .map_err(|_| {
                        lowering_error(LoweringErrorKind::InvalidModel, fact.value_origin())
                    })?;
            }
            let Some(fact) = facts.get(index) else { break };
            let owner = match fact.target() {
                UnitDropTarget::This(owner) => match self.current_receiver.take() {
                    Some(receiver)
                        if receiver.owner == owner
                            && matches!(receiver.entity, crate::ssa::model::EntityId::Value(_)) =>
                    {
                        self.consumed_receiver = Some(receiver.into());
                        let crate::ssa::model::EntityId::Value(value) = receiver.entity else {
                            unreachable!("receiver entity shape was checked")
                        };
                        value
                    }
                    Some(_) | None => {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            fact.value_origin(),
                        ));
                    }
                },
                UnitDropTarget::Named(symbol) => match self.bindings.remove(&symbol) {
                    Some(LoweredValue::Value(value)) => {
                        self.closure_bindings.remove(&symbol);
                        value
                    }
                    Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            fact.value_origin(),
                        ));
                    }
                },
                UnitDropTarget::Temporary(expression) => {
                    let value = self.temporaries.get(&expression).copied().ok_or_else(|| {
                        lowering_error(LoweringErrorKind::MissingFact, fact.value_origin())
                    })?;
                    // Drop 与 transfer 一样结束整个 owner，透明 Group alias 不再跨 CFG 携带。
                    self.take_owned_temporary_origin(expression, value, fact.value_origin())?;
                    value
                }
                UnitDropTarget::Captured { .. } => continue,
                UnitDropTarget::ReplacedElement(_) | UnitDropTarget::ReplacedField { .. } => {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        fact.value_origin(),
                    ));
                }
            };
            self.function
                .append_instruction(
                    self.block,
                    Operation::Drop { owner },
                    Vec::new(),
                    Origin::Source(fact.value_origin()),
                )
                .map_err(|_| {
                    lowering_error(LoweringErrorKind::InvalidModel, fact.value_origin())
                })?;
        }
        Ok(())
    }

    fn conditional_receiver_drop_owner(
        &self,
        fact: lang_frontend::ownership_checking::UnitConditionalReceiverDropFact,
    ) -> Result<Option<ValueId>, LoweringError> {
        let Some(UnitTypeKind::StaticSelf(interface)) =
            self.typed.types().get(fact.receiver_type())
        else {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                fact.value_origin(),
            ));
        };
        let Some(UnitTypeKind::Nominal { declaration, .. }) = self.typed.types().get(*interface)
        else {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                fact.value_origin(),
            ));
        };
        let interface_kind = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .map(|nominal| nominal.kind());
        if interface_kind != Some(NominalKind::Interface) {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                fact.value_origin(),
            ));
        }
        let concrete = super::resolve_concrete_type(
            self.typed,
            fact.receiver_type(),
            self.substitutions,
            self.static_self,
            fact.value_origin(),
        )?;
        if fact.owner() != *declaration {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                fact.value_origin(),
            ));
        }
        let valid_identity = |owner, mode, template_ty, ty, origin| {
            owner == fact.owner()
                && mode == ParameterMode::Value
                && template_ty == fact.receiver_type()
                && ty == concrete
                && origin == fact.value_origin()
        };
        let Some(receiver) = self.current_receiver else {
            let consumed = self.consumed_receiver.ok_or_else(|| {
                lowering_error(LoweringErrorKind::MissingFact, fact.value_origin())
            })?;
            if self.typed.copyability(concrete) != Copyability::MoveOnly
                || !valid_identity(
                    consumed.owner,
                    consumed.mode,
                    consumed.template_ty,
                    consumed.ty,
                    consumed.origin,
                )
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    fact.value_origin(),
                ));
            }
            return Ok(None);
        };
        if !valid_identity(
            receiver.owner,
            receiver.mode,
            receiver.template_ty,
            receiver.ty,
            receiver.origin,
        ) {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                fact.value_origin(),
            ));
        }
        let EntityId::Value(owner) = receiver.entity else {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                fact.value_origin(),
            ));
        };
        match self.typed.copyability(concrete) {
            Copyability::Copyable => Ok(None),
            Copyability::MoveOnly => Ok(Some(owner)),
            Copyability::Unknown | Copyability::Error => Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                fact.value_origin(),
            )),
        }
    }

    /// LoopExit facts 基于 loop-entry state 发布；若所有实际出口已一致消费 owner，缺失 binding
    /// 表示该粗粒度 fact 无需生成 drop，而不是 lowering 事实缺失。
    pub(super) fn emit_loop_exit_drops(
        &mut self,
        point: UnitDropPoint,
        span: Span,
    ) -> Result<(), LoweringError> {
        let facts = self
            .owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.point() == point)
            .collect::<Vec<_>>();
        let live_closures = facts
            .iter()
            .filter_map(|fact| match fact.target() {
                UnitDropTarget::Named(symbol) => self.closure_bindings.get(&symbol).copied(),
                UnitDropTarget::This(_)
                | UnitDropTarget::Temporary(_)
                | UnitDropTarget::Captured { .. }
                | UnitDropTarget::ReplacedElement(_)
                | UnitDropTarget::ReplacedField { .. } => None,
            })
            .collect::<Vec<_>>();
        let live_closure_facts = facts
            .iter()
            .copied()
            .filter(|fact| match fact.target() {
                UnitDropTarget::Named(symbol) => self.closure_bindings.contains_key(&symbol),
                UnitDropTarget::Captured { closure, .. } => live_closures.contains(&closure),
                UnitDropTarget::This(_)
                | UnitDropTarget::Temporary(_)
                | UnitDropTarget::ReplacedElement(_)
                | UnitDropTarget::ReplacedField { .. } => false,
            })
            .collect::<Vec<_>>();
        self.validate_closure_drop_facts(&live_closure_facts)?;
        for fact in facts {
            let symbol = match fact.target() {
                UnitDropTarget::Named(symbol) => symbol,
                UnitDropTarget::Captured { .. } => continue,
                UnitDropTarget::This(_)
                | UnitDropTarget::Temporary(_)
                | UnitDropTarget::ReplacedElement(_)
                | UnitDropTarget::ReplacedField { .. } => {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            };
            let Some(binding) = self.bindings.remove(&symbol) else {
                self.closure_bindings.remove(&symbol);
                continue;
            };
            self.closure_bindings.remove(&symbol);
            let LoweredValue::Value(owner) = binding else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            self.function
                .append_instruction(
                    self.block,
                    Operation::Drop { owner },
                    Vec::new(),
                    Origin::Source(fact.value_origin()),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        }
        Ok(())
    }

    pub(super) fn direct_place_symbol(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<UnitSymbolId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Name => self
                .references
                .get(&span_key(node.span()))
                .copied()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, node.span())),
            Expression::Group { expression } => self.direct_place_symbol(*expression, span),
            _ => Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                node.span(),
            )),
        }
    }
}
