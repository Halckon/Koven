//! Borrowed String, MoveOnly, and field-projection call-argument loan lowering.
use super::*;

impl Builder<'_, '_> {
    /// A Borrow String parameter or capture read through its shared loan in `==`, `!=`, `+`.
    pub(in crate::ssa::refine::build) fn borrowed_string_loan(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<Slot>, LoweringError> {
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let (Expression::Name | Expression::This, span) = self.node(root)? else {
            return Ok(None);
        };
        let id = self.unit(root);
        if self.inputs.typed.non_null_use(id).is_some()
            || self
                .inputs
                .constant_owned
                .is_some_and(|owned| owned.materialization_at(id).is_some())
        {
            return Ok(None);
        }
        let Some(&symbol) = self.inputs.references.get(&(span.start(), span.end())) else {
            return Ok(None);
        };
        let is_string = matches!(
            self.symbol_type(symbol)
                .and_then(|ty| self.inputs.typed.types().get(ty)),
            Some(UnitTypeKind::Builtin(BuiltinType::String))
        );
        Ok(is_string
            .then(|| self.borrowed.get(&symbol).copied())
            .flatten())
    }

    /// A MoveOnly Borrow parameter/capture or Map iteration component lent directly to a Borrow call parameter.
    pub(in crate::ssa::refine::build) fn borrowed_move_only_loan(
        &self,
        expression: ExpressionId,
        parameter: UnitTypeId,
    ) -> Result<Option<Slot>, LoweringError> {
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let (payload, span) = self.node(root)?;
        let id = self.unit(root);
        if self.inputs.typed.non_null_use(id).is_some()
            || self
                .inputs
                .constant_owned
                .is_some_and(|owned| owned.materialization_at(id).is_some())
        {
            return Ok(None);
        }
        match payload {
            Expression::Name => {
                let Some(&symbol) = self.inputs.references.get(&(span.start(), span.end())) else {
                    return Ok(None);
                };
                let is_map_for_component = self.loops.iter().any(|frame| {
                    frame.map_for.is_some_and(|map_for| {
                        map_for.key_binding == symbol || map_for.value_binding == symbol
                    })
                });
                if !self.move_only(parameter) && !is_map_for_component {
                    return Ok(None);
                }
                Ok((self.symbol_type(symbol) == Some(parameter))
                    .then(|| self.borrowed.get(&symbol).copied())
                    .flatten()
                    // An exclusive parameter uses the ordinary shared reborrow path.
                    .filter(|slot| {
                        matches!(
                            self.body.slots[slot.0],
                            EntityType::Loan {
                                kind: LoanKind::Shared,
                                ..
                            }
                        )
                    }))
            }
            Expression::This => {
                if !self.move_only(parameter) {
                    return Ok(None);
                }
                Ok(self
                    .inputs
                    .receiver
                    .filter(|receiver| {
                        receiver.mode == lang_frontend::type_checking::ParameterMode::Borrow
                            && receiver.ty == parameter
                    })
                    .and(self.receiver)
                    .or_else(|| {
                        let symbol = self
                            .inputs
                            .references
                            .get(&(span.start(), span.end()))
                            .copied()?;
                        (self.symbol_type(symbol) == Some(parameter))
                            .then(|| self.borrowed.get(&symbol).copied())
                            .flatten()
                    }))
            }
            _ => Ok(None),
        }
    }

    /// `owner.field` of a MoveOnly type lent to a Borrow call parameter via `SharedHeapFieldLoan`.
    pub(in crate::ssa::refine::build) fn borrowed_string_field_loan(
        &mut self,
        expression: ExpressionId,
        parameter: UnitTypeId,
        span: Span,
    ) -> Result<Option<ProjectionLoan>, LoweringError> {
        let is_deinit = self.inputs.function_item.is_some_and(|item| {
            matches!(
                self.inputs
                    .parsed
                    .ast()
                    .items()
                    .get(item)
                    .map(|node| node.payload()),
                Ok(lang_frontend::parser::Item::Deinit { .. })
            )
        });
        if !self.move_only(parameter) && !is_deinit {
            return Ok(None);
        }
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let id = self.unit(root);
        let Some(projection) = self.inputs.typed.aggregate_projection(id) else {
            return Ok(None);
        };
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.c(projection.ty()) != parameter
        {
            return Ok(None);
        }
        if let lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(receiver) =
            projection.receiver()
            && let Some(lang_frontend::type_checking::UnitTypeKind::EnumCase { case, root }) = self
                .inputs
                .typed
                .expression_type(receiver)
                .and_then(|ty| self.inputs.typed.types().get(ty))
                .cloned()
        {
            if receiver.source_unit() != self.inputs.source_unit {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let root_ty = self.c(root);
            let receiver_expr = receiver.expression();
            let receiver_root = self.ungrouped_expr(receiver_expr)?;
            let (Expression::Name, receiver_span) = self.node(receiver_root)? else {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            };
            let symbol = self.reference(receiver_span)?;
            let owner = *self
                .bindings
                .get(&symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, receiver_span))?;
            let (_, variant, payload_agg) = self.variant(root_ty, case, span)?;
            let field = self
                .types
                .field_indices
                .get(&(root_ty, projection.field()))
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let target = self.ssa_type(parameter, span)?;
            let arg_span = self.span(expression);
            let base = self.value(
                ROp::TaggedPayloadPlace { owner, variant },
                EntityType::Place(payload_agg),
                arg_span,
            )?;
            let place = self.value(
                ROp::FieldPlace { base, field },
                EntityType::Place(target),
                arg_span,
            )?;
            let field_loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
                arg_span,
            )?;
            return Ok(Some((field_loan, vec![field_loan], vec![receiver_expr])));
        }
        Ok(Some(
            self.field_projection_loan(root, self.span(expression))?,
        ))
    }

    /// Resolves a single- or multi-hop class field projection `r.f1.f2...` into a leaf
    /// `SharedHeapFieldLoan`, all created loans in creation order, and the receiver
    /// expressions encountered from root to immediate parent.
    pub(in crate::ssa::refine::build) fn field_projection_loan(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<ProjectionLoan, LoweringError> {
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let id = self.unit(root);
        let projection = self
            .inputs
            .typed
            .aggregate_projection(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if let Some(result) = self.shared_current_heap_field(projection, span)? {
            return Ok(result);
        }
        let (base_loan, mut created, drops, owner_type) = match projection.receiver() {
            lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(receiver)
                if receiver.source_unit() == self.inputs.source_unit =>
            {
                let mut ungrouped = receiver.expression();
                while let (Expression::Group { expression: inner }, _) = self.node(ungrouped)? {
                    ungrouped = inner;
                }
                if self
                    .inputs
                    .typed
                    .aggregate_projection(self.unit(ungrouped))
                    .is_some()
                {
                    let (parent_loan, created, mut drops) =
                        self.field_projection_loan(receiver.expression(), span)?;
                    let owner_type = self.expression_type(ungrouped)?;
                    drops.push(receiver.expression());
                    (parent_loan, created, drops, owner_type)
                } else if self
                    .inputs
                    .typed
                    .rc_operation(self.unit(ungrouped))
                    .is_some_and(|rc| {
                        rc.kind() == lang_frontend::type_checking::RcOperationKind::Value
                    })
                {
                    let (parent, created, mut drops) = self.rc_payload_loan(ungrouped, span)?;
                    let owner_type = self.expression_type(ungrouped)?;
                    drops.push(receiver.expression());
                    (parent, created, drops, owner_type)
                } else if let (Expression::Name, receiver_span) = self.node(ungrouped)?
                    && let Ok(symbol) = self.reference(receiver_span)
                    && !self.borrowed.contains_key(&symbol)
                    && self.narrowed_view(ungrouped)?.is_none()
                {
                    let owner_type = match self.inputs.typed.non_null_use(self.unit(ungrouped)) {
                        Some(fact) => self.c(fact.narrowed_type()),
                        None => self.expression_type(ungrouped)?,
                    };
                    if self.is_class(owner_type) {
                        let owner = *self
                            .bindings
                            .get(&symbol)
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, receiver_span))?;
                        let owner_ssa = self.ssa_type(owner_type, span)?;
                        let payload_ssa = *self
                            .types
                            .heap_payloads
                            .get(&owner_ssa)
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        let base = self.value(
                            ROp::HeapPayloadPlace { owner },
                            EntityType::Place(payload_ssa),
                            receiver_span,
                        )?;
                        let field = self
                            .types
                            .field_indices
                            .get(&(owner_type, projection.field()))
                            .copied()
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        let target = self.ssa_type(self.c(projection.ty()), span)?;
                        let place = self.value(
                            ROp::FieldPlace { base, field },
                            EntityType::Place(target),
                            span,
                        )?;
                        let field_loan = self.value(
                            ROp::BorrowBegin {
                                place,
                                kind: LoanKind::Shared,
                            },
                            EntityType::Loan {
                                kind: LoanKind::Shared,
                                target,
                            },
                            span,
                        )?;
                        return Ok((field_loan, vec![field_loan], vec![ungrouped]));
                    }
                    let (owner, owner_type, receiver_expr) =
                        self.projection_owner(projection.receiver(), span)?;
                    let owner_loan = self.owner_loan(owner, LoanKind::Shared, span)?;
                    let owner_loan = self.shared_operand(owner_loan, span)?;
                    let mut created = Vec::new();
                    if owner_loan.scoped {
                        created.push(owner_loan.loan);
                    }
                    let drops = receiver_expr.into_iter().collect();
                    (owner_loan.loan, created, drops, owner_type)
                } else {
                    let (owner, owner_type, receiver_expr) =
                        self.projection_owner(projection.receiver(), span)?;
                    let owner_loan = self.owner_loan(owner, LoanKind::Shared, span)?;
                    let owner_loan = self.shared_operand(owner_loan, span)?;
                    let mut created = Vec::new();
                    if owner_loan.scoped {
                        created.push(owner_loan.loan);
                    }
                    let drops = receiver_expr.into_iter().collect();
                    (owner_loan.loan, created, drops, owner_type)
                }
            }
            _ => {
                let (owner, owner_type, receiver_expr) =
                    self.projection_owner(projection.receiver(), span)?;
                let owner_loan = self.owner_loan(owner, LoanKind::Shared, span)?;
                let owner_loan = self.shared_operand(owner_loan, span)?;
                let mut created = Vec::new();
                if owner_loan.scoped {
                    created.push(owner_loan.loan);
                }
                let drops = receiver_expr.into_iter().collect();
                (owner_loan.loan, created, drops, owner_type)
            }
        };
        let field = self
            .types
            .field_indices
            .get(&(owner_type, projection.field()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let target = self.ssa_type(self.c(projection.ty()), span)?;
        let op = if self.is_value_class(owner_type) {
            ROp::SharedFieldLoan {
                base: base_loan,
                field,
            }
        } else {
            ROp::SharedHeapFieldLoan {
                base: base_loan,
                field,
            }
        };
        let field_loan = self.value(
            op,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            },
            span,
        )?;
        created.push(field_loan);
        Ok((field_loan, created, drops))
    }

    pub(in crate::ssa::refine::build) fn is_class(&self, ty: UnitTypeId) -> bool {
        let Some(UnitTypeKind::Nominal { declaration, .. }) = self.inputs.typed.types().get(ty)
        else {
            return false;
        };
        self.inputs
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .is_some_and(|nominal| nominal.kind() == NominalKind::Class)
    }
}
