//! Owned nullable subject 的一次求值、证明 edge 与显式 extraction。
use super::{ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, error, value};
use crate::ssa::model::{
    Edge, EntityId, EntityType, LoanKind, Operation, Origin, TerminatorKind, ValueId,
};
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{DropPoint, NullableWhenExtractionKind},
    parser::{Expression, LiteralKind, WhenCondition, WhenEntry},
    source::Span,
    type_checking::{NullableWhenSubjectCategory, WhenDomain},
};

impl ExpressionLowerer<'_> {
    pub(super) fn lower_nullable_when(
        &mut self,
        expression: ExpressionId,
        entries: &[WhenEntry],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let plan = self
            .typed
            .nullable_when(expression)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let ownership = self
            .owned
            .nullable_when(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if !plan.native_eligible()
            || !matches!(
                plan.category(),
                NullableWhenSubjectCategory::OwnedRoot | NullableWhenSubjectCategory::Temporary
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if ownership.subject() != plan.subject()
            || ownership.category() != plan.category()
            || entries.len() != plan.entries().len()
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        // Defer subject ASAP cleanup until the discriminator has read its owner.
        // Group aliases share the same single evaluation and owner.
        let mut subject_expressions = vec![plan.subject()];
        let mut evaluated = plan.subject();
        while let Ok(node) = self.parsed.ast().expressions().get(evaluated) {
            let Expression::Group { expression } = node.payload() else {
                break;
            };
            evaluated = *expression;
            subject_expressions.push(evaluated);
        }
        let owner = match self.lower_expression(evaluated)? {
            LoweredValue::Value(owner) => owner,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        if plan.category() == NullableWhenSubjectCategory::Temporary {
            self.temporaries.insert(plan.subject().index(), owner);
        }
        let inner = match self.typed.types().get(plan.subject_type()) {
            Some(lang_frontend::type_checking::TypeKind::Nullable(inner)) => self
                .type_ids
                .get(&self.resolve_type(*inner, span)?)
                .copied(),
            _ => None,
        }
        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let baseline = self.bindings.clone();
        let temporaries = self.temporaries.clone();
        let carried = self.linear_binding_slots(&baseline, span)?;
        let subject_slot = carried
            .slots
            .iter()
            .position(|slot| slot.source == EntityId::Value(owner))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let types = carried.slots.iter().map(|slot| slot.ty).collect::<Vec<_>>();
        let null = self
            .function
            .add_block(types.clone(), Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let mut proven_types = types;
        proven_types.push(EntityType::Loan {
            kind: LoanKind::Shared,
            target: inner,
        });
        let non_null = self
            .function
            .add_block(proven_types, Origin::Source(span))
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Loan(view) =
            self.function.block(non_null).expect("new block").parameters[carried.slots.len()]
        else {
            unreachable!("requested loan");
        };
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::NullableBranch {
                    owner,
                    when_null: Edge {
                        target: null,
                        arguments: carried.slots.iter().map(|slot| slot.source).collect(),
                    },
                    when_non_null: Edge {
                        target: non_null,
                        arguments: carried.slots.iter().map(|slot| slot.source).collect(),
                    },
                    view,
                },
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let mut exits = Vec::new();
        for (block, proven) in [(null, false), (non_null, true)] {
            self.block = block;
            self.temporaries.clone_from(&temporaries);
            self.bindings = self.rebind_linear_bindings(&baseline, block, &carried, span)?;
            let branch_owner =
                value(self.function.block(block).expect("new block").parameters[subject_slot]);
            if let Some(symbol) = plan.stable_symbol() {
                self.bindings
                    .insert(symbol, LoweredValue::Value(branch_owner));
                if proven {
                    self.non_null_bindings.insert(symbol, view);
                }
            } else {
                self.temporaries
                    .insert(plan.subject().index(), branch_owner);
                if proven {
                    self.append(Operation::BorrowEnd { loan: view }, Vec::new(), span)?;
                }
            }
            for subject in subject_expressions.iter().rev() {
                self.emit_drops(DropPoint::AfterExpression(*subject))?;
            }
            let mut reached = false;
            for (index, (entry, typed_entry)) in entries.iter().zip(plan.entries()).enumerate() {
                let mut matched = entry.else_span.is_some();
                if !matched {
                    for (alternative, (condition, typed_alt)) in entry
                        .conditions
                        .iter()
                        .zip(typed_entry.alternatives())
                        .enumerate()
                    {
                        // Evaluate expression conditions even if their matching domain is empty.
                        // A literal null has no effects and needs no temporary nullable owner.
                        if let WhenCondition::Expression(candidate) = condition
                            && !matches!(
                                self.parsed
                                    .ast()
                                    .expressions()
                                    .get(*candidate)
                                    .map(|node| node.payload()),
                                Ok(Expression::Literal(LiteralKind::Null))
                            )
                        {
                            let result = self.lower(*candidate)?;
                            if matches!(result, LoweredValue::Diverged) {
                                reached = true;
                                break;
                            }
                        }
                        let mut accepts = domain_contains(typed_alt.match_domain(), proven);
                        let mut misses = domain_contains(typed_alt.fallthrough_domain(), proven);
                        if let WhenCondition::TypeTest {
                            type_ref, negated, ..
                        } = condition
                        {
                            let target = self.typed.type_ref_type(*type_ref).ok_or_else(|| {
                                error(LoweringErrorKind::MissingFact, typed_alt.span())
                            })?;
                            if self.type_ids.get(&self.resolve_type(target, span)?) == Some(&inner)
                            {
                                accepts = proven != *negated;
                                misses = !accepts;
                            }
                        }
                        if accepts && misses {
                            // Open-domain equality/type tests need a checked comparison recipe.
                            return Err(error(
                                LoweringErrorKind::UnsupportedNode,
                                typed_alt.span(),
                            ));
                        }
                        if accepts {
                            self.emit_drops(DropPoint::WhenAlternativeMatch {
                                control: expression,
                                entry: index,
                                alternative,
                            })?;
                            matched = true;
                            break;
                        }
                    }
                }
                if reached {
                    break;
                }
                if !matched {
                    continue;
                }
                // Only the common entry domain may expose the source binding as an inner view.
                if proven
                    && !typed_entry.body_domain().is_non_null()
                    && let Some(symbol) = plan.stable_symbol()
                    && let Some(view) = self.non_null_bindings.remove(&symbol)
                {
                    self.append(Operation::BorrowEnd { loan: view }, Vec::new(), span)?;
                }
                let result = self.lower_control_body(entry.body)?;
                if !matches!(result, LoweredValue::Diverged) {
                    self.emit_drops(DropPoint::BranchExit {
                        control: expression,
                        branch: index,
                    })?;
                    if proven
                        && let Some(symbol) = plan.stable_symbol()
                        && let Some(view) = self.non_null_bindings.remove(&symbol)
                    {
                        self.append(Operation::BorrowEnd { loan: view }, Vec::new(), span)?;
                    }

                    exits.push(self.branch_exit(result));
                }
                reached = true;
                break;
            }
            if !reached {
                self.function
                    .set_terminator(self.block, TerminatorKind::Abort, Origin::Source(span))
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
            }
        }
        self.temporaries = temporaries;
        // Temporary subject obligations are discharged on each branch by frontend drop facts.
        self.temporaries.remove(&plan.subject().index());
        if self.expression_is_unit(expression, span)? {
            for exit in &mut exits {
                exit.result = LoweredValue::Unit;
            }
        }
        self.merge_exits(exits, &baseline, span)
    }

    pub(super) fn lower_nullable_extraction(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Option<ValueId>, LoweringError> {
        let Some((plan, fact)) = self.owned.nullable_whens().iter().find_map(|plan| {
            plan.extractions()
                .iter()
                .find(|fact| fact.expression() == expression)
                .map(|fact| (plan, fact))
        }) else {
            return Ok(None);
        };
        let typed = self.typed.nullable_when(plan.expression()).ok_or_else(|| {
            error(
                LoweringErrorKind::MissingFact,
                self.parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .expect("checked expression")
                    .span(),
            )
        })?;
        let span = typed.span();
        let symbol = typed
            .stable_symbol()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let proof = self
            .non_null_bindings
            .get(&symbol)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if fact.kind() != NullableWhenExtractionKind::Consume {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let inner = self.expression_ssa_type(expression, span)?;
        let (_, result) = self.append(
            Operation::NullableTake { owner, proof },
            vec![EntityType::Value(inner)],
            span,
        )?;
        self.bindings.remove(&symbol);
        self.non_null_bindings.remove(&symbol);
        Ok(Some(value(result[0])))
    }
}

fn domain_contains(domain: &WhenDomain, non_null: bool) -> bool {
    match domain {
        WhenDomain::Nullable => true,
        WhenDomain::NonNull => non_null,
        WhenDomain::Null => !non_null,
        WhenDomain::Empty => false,
        WhenDomain::Finite(_) => false,
    }
}
