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
        self.reject_nullable_when_value(expression, span)?;
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
        if self.nullable_proofs.contains_key(&symbol) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        match self.bindings.remove(&symbol) {
            Some(LoweredValue::Value(bound)) if bound == value => Ok(()),
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
        self.end_result_bindings(point)?;
        if let Some(actions) = self.owned.iteration_cleanup_at(point).map(<[_]>::to_vec) {
            return self.emit_iteration_cleanup(point, &actions);
        }
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
            self.emit_drop_fact(*fact)?;
        }
        Ok(())
    }

    pub(super) fn emit_drop_fact(
        &mut self,
        fact: lang_frontend::ownership_checking::UnitDropFact,
    ) -> Result<(), LoweringError> {
        // Drop facts describe the generic body. Specialize their obligation before
        // removing a binding, temporary, or its aliases from this concrete instance.
        let template = match fact.target() {
            UnitDropTarget::Named(symbol) => Some(self.typed.symbol_type(symbol)),
            UnitDropTarget::Temporary(expression) => Some(self.typed.expression_type(expression)),
            _ => None,
        };
        if let Some(template) = template {
            let template = template.ok_or_else(|| {
                lowering_error(LoweringErrorKind::MissingFact, fact.value_origin())
            })?;
            let concrete = super::resolve_concrete_type(
                self.typed,
                template,
                self.substitutions,
                self.static_self,
                fact.value_origin(),
            )?;
            if concrete != template && self.typed.copyability(concrete) == Copyability::Copyable {
                return Ok(());
            }
        }
        if let UnitDropTarget::Named(symbol) = fact.target() {
            self.end_nullable_proof(symbol, fact.value_origin())?;
        }
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
                Some(LoweredValue::Value(value)) => value,
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
            UnitDropTarget::Captured { .. } => return Ok(()),
            UnitDropTarget::ReplacedElement(_)
            | UnitDropTarget::ReplacedField { .. }
            | UnitDropTarget::ReplacedCapture { .. } => {
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
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, fact.value_origin()))?;
        Ok(())
    }

    pub(super) fn conditional_receiver_drop_owner(
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
        for fact in facts {
            let symbol = match fact.target() {
                UnitDropTarget::Named(symbol) => symbol,
                UnitDropTarget::Captured { .. } => continue,
                UnitDropTarget::This(_)
                | UnitDropTarget::Temporary(_)
                | UnitDropTarget::ReplacedElement(_)
                | UnitDropTarget::ReplacedField { .. }
                | UnitDropTarget::ReplacedCapture { .. } => {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            };
            self.end_nullable_proof(symbol, span)?;
            let Some(binding) = self.bindings.remove(&symbol) else {
                continue;
            };
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

#[cfg(test)]
mod nullable_if_cleanup_tests {
    //! Direct private guards; mutations never use the final verifier as an oracle.
    use crate::ssa::lower_frontend::cleanup_guard_harness::{self as harness, Fixture};
    use crate::ssa::lower_frontend::non_null_proof::ProofScope;
    use crate::ssa::lower_frontend::nullable_if_cleanup::coverage;
    use crate::ssa::lower_frontend::nullable_if_cleanup::*;
    use crate::ssa::lower_frontend::*;
    use lang_frontend::ownership_checking::DropFact;

    fn checkpoint<T>(
        fixture: &Fixture,
        fact: DropFact,
        check: impl FnOnce(&mut ExpressionLowerer<'_>, &mut GroupPlan, ValueId) -> T,
    ) -> T {
        let mut plan = candidate(
            &fixture.parsed,
            &fixture.names,
            &fixture.typed,
            &fixture.owned,
            fact,
        )
        .unwrap();
        let mut function = fixture.entry().clone();
        fixture.with_lowerer(&mut function, |lowerer| {
            harness::outer_context(lowerer, plan.root, plan.while_statement);
            let span = lowerer.expression_span(plan.second_if).unwrap();
            let (block, proof) = lowerer
                .function
                .blocks
                .iter()
                .find_map(|block| {
                    let term = block.terminator.as_ref()?;
                    if term.origin != Origin::Source(span) {
                        return None;
                    }
                    let TerminatorKind::NullableBranch {
                        owner,
                        view,
                        when_non_null,
                        when_null,
                    } = &term.kind
                    else {
                        return None;
                    };
                    let block = if plan.arm == 0 {
                        when_non_null.target
                    } else {
                        when_null.target
                    };
                    let column = harness::parameter(lowerer.function, EntityId::Value(*owner)).1;
                    let EntityId::Value(owner) = lowerer.function.block(block)?.parameters[column]
                    else {
                        panic!("owner column")
                    };
                    let EntityType::Loan { target: inner, .. } =
                        lowerer.function.entity(EntityId::Loan(*view))?.ty
                    else {
                        panic!("view type")
                    };
                    Some((
                        block,
                        (plan.arm == 0).then(|| {
                            NonNullProof::introduced(
                                *view,
                                owner,
                                inner,
                                ProofScope::If(plan.second_if),
                            )
                        }),
                    ))
                })
                .unwrap();
            let owner = harness::before_cleanup(
                lowerer,
                block,
                plan.root,
                plan.members[0].value_origin(),
                proof,
            );
            assert_eq!(
                lowerer
                    .checked_nullable_if_cleanup_owner(&plan, &plan.members)
                    .unwrap(),
                owner,
                "unmodified checkpoint first passes the exact consumer"
            );
            check(lowerer, &mut plan, owner)
        })
    }

    #[test]
    fn nullable_if_cleanup_rejects_coverage_consumer_and_owner_corruption() {
        let original = crate::ssa::while_flow_tests::proof_cases::retested_after_branch_source();
        for text in [
            original.clone(),
            original.replace("if (current != null)", "if (((current) != null))"),
        ] {
            let fixture = Fixture::analyze(&text);
            // These guards inspect source/facts/SSA and the explicit local state only;
            // no generic instance, callable token, closure, or receiver path is exercised.
            assert_eq!(fixture.program.modules[0].functions.len(), 1);
            assert!(fixture.entry().receiver.is_none());
            assert!(fixture.typed.type_parameters().is_empty());
            assert!(
                fixture
                    .parsed
                    .ast()
                    .expressions()
                    .iter()
                    .all(|(_, node)| !matches!(node.payload(), Expression::Lambda { .. }))
            );
            assert!(
                fixture
                    .entry()
                    .instructions
                    .iter()
                    .all(|instruction| !matches!(
                        instruction.operation,
                        Operation::DirectCall { .. }
                    ))
            );
            assert_eq!(
                fixture.entry().return_types,
                [fixture.type_ids[&fixture.typed.types().builtin(BuiltinType::Int).unwrap()]]
            );
            let facts = fixture
                .owned
                .drops()
                .iter()
                .copied()
                .filter(|&fact| {
                    candidate(
                        &fixture.parsed,
                        &fixture.names,
                        &fixture.typed,
                        &fixture.owned,
                        fact,
                    )
                    .is_some_and(|plan| plan.members[0] == fact)
                })
                .collect::<Vec<_>>();
            assert_eq!(
                facts.len(),
                2,
                "both actual retest arms publish a two-member group"
            );
            for &fact in &facts {
                let plan = candidate(
                    &fixture.parsed,
                    &fixture.names,
                    &fixture.typed,
                    &fixture.owned,
                    fact,
                )
                .unwrap();
                let conditions = fixture.owned.cleanup_conditions();
                assert_eq!(
                    coverage(&fixture.parsed, conditions, plan.members),
                    Some((plan.first_if, plan.second_if, plan.arm))
                );
                assert!(coverage(&fixture.parsed, conditions, [plan.members[0]; 2]).is_none());
                assert!(
                    coverage(
                        &fixture.parsed,
                        conditions,
                        [plan.members[1], plan.members[0]]
                    )
                    .is_none()
                );
                let other = candidate(
                    &fixture.parsed,
                    &fixture.names,
                    &fixture.typed,
                    &fixture.owned,
                    *facts.iter().find(|&&other| other != fact).unwrap(),
                )
                .unwrap();
                assert_ne!(plan.arm, other.arm);
                assert!(
                    coverage(
                        &fixture.parsed,
                        conditions,
                        [plan.members[0], other.members[1]]
                    )
                    .is_none(),
                    "real opposite-arm selector branches cannot complete this cover"
                );
                assert!(
                    candidate(
                        &fixture.parsed,
                        &fixture.names,
                        &fixture.typed,
                        &fixture.owned,
                        plan.old_drop
                    )
                    .is_none()
                );
                for remaining in [
                    vec![],
                    vec![plan.members[0]],
                    vec![plan.members[0]; 2],
                    vec![plan.members[1], plan.members[0]],
                    vec![plan.members[0], other.members[1]],
                ] {
                    checkpoint(&fixture, fact, |lowerer, plan, _| {
                        let before = lowerer.function.clone();
                        harness::unsupported(
                            lowerer.checked_nullable_if_cleanup_owner(plan, &remaining),
                            plan.members[0].value_origin(),
                        );
                        assert_eq!(
                            *lowerer.function, before,
                            "preflight failure has no side effect"
                        );
                    });
                }
                for mutation in 0..7 {
                    checkpoint(&fixture, fact, |lowerer, plan, _| {
                        let span = plan.members[0].value_origin();
                        match mutation {
                            0 => plan.root = fixture.symbol("initial"),
                            1 => plan.arm = 1 - plan.arm,
                            2 => plan.replacement = plan.second_if,
                            3 => plan.first_if = plan.second_if,
                            4 | 5 => {
                                let block = lowerer.block;
                                let (join, edge) =
                                    harness::incoming(lowerer.function, block).pop().unwrap();
                                let EntityId::Value(current) = lowerer.function.block(block).unwrap().parameters.iter().copied().find(|entity| matches!(entity, EntityId::Value(value) if lowerer.bindings[&plan.root] == LoweredValue::Value(*value))).unwrap() else { unreachable!() };
                                let column =
                                    harness::parameter(lowerer.function, EntityId::Value(current))
                                        .1;
                                let seed = lowerer
                                    .function
                                    .block(lowerer.function.entry_block().unwrap())
                                    .unwrap()
                                    .parameters[plan.seed_parameter];
                                let target = harness::edges_mut(
                                    &mut lowerer.function.blocks[join.index()]
                                        .terminator
                                        .as_mut()
                                        .unwrap()
                                        .kind,
                                )
                                .into_iter()
                                .find(|target| target.target == block)
                                .unwrap();
                                if mutation == 4 {
                                    target.arguments[column] = seed;
                                } else {
                                    target.arguments.push(edge.arguments[column]);
                                }
                            }
                            6 => {
                                let (_, edge) = harness::incoming(lowerer.function, lowerer.block)
                                    .pop()
                                    .unwrap();
                                let unrelated = lowerer
                                    .function
                                    .blocks
                                    .iter()
                                    .find(|block| {
                                        block.id != lowerer.block
                                            && block.terminator.as_ref().is_some_and(|term| {
                                                matches!(term.kind, TerminatorKind::Return { .. })
                                            })
                                    })
                                    .unwrap()
                                    .id;
                                lowerer.function.blocks[unrelated.index()]
                                    .terminator
                                    .as_mut()
                                    .unwrap()
                                    .kind = TerminatorKind::Branch(edge);
                            }
                            _ => unreachable!(),
                        }
                        assert!(
                            lowerer.nullable_if_cleanup_owner(plan).is_none(),
                            "carrier mutation {mutation}"
                        );
                        harness::unsupported(
                            lowerer.checked_nullable_if_cleanup_owner(plan, &plan.members),
                            span,
                        );
                    });
                }
                checkpoint(&fixture, fact, |lowerer, plan, owner| {
                    assert_eq!(
                        lowerer
                            .emit_nullable_if_cleanup_group(&plan.members)
                            .unwrap(),
                        Some(2)
                    );
                    assert!(!lowerer.bindings.contains_key(&plan.root));
                    assert!(!lowerer.non_null_bindings.contains_key(&plan.root));
                    let block = lowerer.function.block(lowerer.block).unwrap();
                    let drop = lowerer
                        .function
                        .instruction(*block.instructions.last().unwrap())
                        .unwrap();
                    assert_eq!(drop.operation, Operation::Drop { owner });
                    assert_eq!(drop.origin, Origin::Source(plan.members[0].value_origin()));
                });
            }
        }
    }
}
