//! Subset rules for integer intrinsics (`inv`, checked conversions), prefix `+`/`-`,
//! `IntRange`/`LongRange` value operations, and `String.clone()` (SPEC-0371).
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, LiteralKind, PrefixOperator},
    type_checking::{
        BuiltinType, Copyability, IntegerOperationKind, ParameterMode, StringOperationKind,
        UnitTypeId, UnitTypeKind,
    },
};

use super::Checker;

impl Checker<'_, '_> {
    pub(super) fn range_type(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.kind(ty),
            Some(UnitTypeKind::Builtin(
                BuiltinType::IntRange | BuiltinType::LongRange
            ))
        )
    }

    pub(super) fn prefix_integer(
        &mut self,
        operator: PrefixOperator,
        operand: ExpressionId,
    ) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(operand) else {
            return false;
        };
        if operator == PrefixOperator::Minus
            && let Expression::Literal(LiteralKind::Integer(kind)) = node.payload()
        {
            return self
                .inputs
                .sources
                .slice(node.span())
                .ok()
                .and_then(|text| {
                    lang_frontend::type_checking::integer_literal_magnitude(text, *kind)
                })
                .is_some_and(|magnitude| {
                    magnitude == (1u128 << 127)
                        || i128::try_from(magnitude)
                            .ok()
                            .and_then(i128::checked_neg)
                            .is_some()
                });
        }
        self.expression(operand)
    }

    pub(super) fn integer_operation(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.integer_operation(id) else {
            return false;
        };
        let receiver_ty = self.c(descriptor.receiver_type());
        let result_ty = self.c(descriptor.result_type());
        let kind_ok = match descriptor.kind() {
            IntegerOperationKind::Invert => receiver_ty == result_ty,
            IntegerOperationKind::CheckedConvert => true,
        };
        let ok = descriptor.expression() == id
            && descriptor.receiver().source_unit() == self.inputs.source_unit
            && self.integer_type(receiver_ty)
            && self.integer_type(result_ty)
            && kind_ok
            && self.expression_type_of(descriptor.receiver()) == Some(receiver_ty)
            && self.expression_type(expression) == Some(result_ty)
            && crate::ssa::integer::intrinsic_receiver_matches(
                self.inputs.parsed,
                expression,
                descriptor.receiver().expression(),
            )
            && self.expression(descriptor.receiver().expression());
        self.migrated |= ok;
        ok
    }

    pub(super) fn integer_range_operation(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.integer_range_operation(id).copied() else {
            return false;
        };
        let left_ty = self.c(descriptor.left_type());
        let right_ty = self.c(descriptor.right_type());
        let result_ty = self.c(descriptor.result_type());
        let types = [left_ty, right_ty, result_ty].map(|ty| match self.kind(ty) {
            Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        });
        let ok = descriptor.expression() == id
            && descriptor.left().source_unit() == self.inputs.source_unit
            && descriptor.right().source_unit() == self.inputs.source_unit
            && self.expression_type_of(descriptor.left()) == Some(left_ty)
            && self.expression_type_of(descriptor.right()) == Some(right_ty)
            && self.expression_type(expression) == Some(result_ty)
            && crate::ssa::integer::matches_range_fact(
                self.inputs.parsed,
                expression,
                descriptor.left().expression(),
                descriptor.right().expression(),
                descriptor.kind(),
                types,
            )
            && self.expression(descriptor.left().expression())
            && self.expression(descriptor.right().expression());
        self.migrated |= ok;
        ok
    }

    pub(super) fn string_operation(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.string_operation(id) else {
            return false;
        };
        let Some(effect) = self.inputs.owned.string_effect(id) else {
            return false;
        };
        if descriptor.kind() != StringOperationKind::Clone
            || descriptor.receiver_mode() != ParameterMode::Borrow
            || descriptor.result_mode() != ParameterMode::Value
            || effect.receiver() != descriptor.receiver()
            || effect.receiver_type() != descriptor.receiver_type()
            || effect.result_type() != descriptor.result_type()
            || effect.kind() != descriptor.kind()
            || descriptor.receiver().source_unit() != self.inputs.source_unit
            || !self.string(self.c(descriptor.receiver_type()))
            || !self.string(self.c(descriptor.result_type()))
        {
            return false;
        }
        let receiver = descriptor.receiver().expression();
        let receiver_id = self.id(receiver);
        let receiver_ty = self.c(descriptor.receiver_type());
        if self.borrowed_string_name(receiver)
            || self.borrowed_rc_payload(expression, receiver, receiver_ty)
            || self.borrowed_container_element(expression, receiver, receiver_ty)
        {
            self.migrated = true;
            return true;
        }
        if let Some(fact) = self.inputs.typed.non_null_use(receiver_id)
            && self.copyability_of(self.c(fact.narrowed_type())) == Copyability::MoveOnly
        {
            let root = self.ungrouped(receiver);
            let ok = self.reference(root) == Some(fact.symbol())
                && self.known.contains(&fact.symbol())
                && self.view_symbols.contains(&fact.symbol());
            self.migrated |= ok;
            return ok;
        }
        if self.clone_field_receiver(receiver) == Some(receiver_ty) {
            self.migrated = true;
            return true;
        }
        let ok = self.expression(receiver);
        self.migrated |= ok;
        ok
    }

    fn clone_field_receiver(&self, expression: ExpressionId) -> Option<UnitTypeId> {
        use super::receiver::Owner;
        use lang_frontend::type_checking::{
            UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
        };
        let root = self.ungrouped(expression);
        let id = self.id(root);
        let typed = self.inputs.typed;
        let projection = typed.aggregate_projection(id)?;
        let field_ty = self.c(projection.ty());
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.expression_type_of(id) != Some(field_ty)
            || !self.storage_type(field_ty)
            || !self.unspecialized(root)
        {
            return None;
        }
        let parent_ty = match projection.receiver() {
            UnitAggregateProjectionReceiver::This(declaration) => {
                self.owner_type(Owner::This(declaration), ParameterMode::Borrow)?
            }
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return None;
                }
                let receiver_expr = receiver.expression();
                if typed
                    .aggregate_projection(self.id(self.ungrouped(receiver_expr)))
                    .is_some()
                {
                    let parent = self.clone_field_receiver(receiver_expr)?;
                    if !(self.class(parent)
                        || self.value_class(parent)
                        || self.move_value_class(parent))
                    {
                        return None;
                    }
                    parent
                } else {
                    if !self.plain(receiver_expr) {
                        return None;
                    }
                    let ungrouped = self.ungrouped(receiver_expr);
                    let owner_ty =
                        self.owner_type(Owner::Expression(ungrouped), ParameterMode::Borrow)?;
                    let allowed_source = match self
                        .inputs
                        .parsed
                        .ast()
                        .expressions()
                        .get(ungrouped)
                        .map(|node| node.payload())
                    {
                        Ok(Expression::This) => true,
                        Ok(Expression::Name) => self.reference(ungrouped).is_some_and(|symbol| {
                            matches!(
                                self.parameter_mode(symbol),
                                Some(ParameterMode::Borrow | ParameterMode::Inout)
                            ) || typed.non_null_use(self.id(ungrouped)).is_some()
                                || (self.parameter_mode(symbol).is_none()
                                    && (self
                                        .class_signature(owner_ty)
                                        .is_some_and(|(_, args)| args.is_empty())
                                        || self
                                            .value_signature(owner_ty)
                                            .is_some_and(|(_, args)| args.is_empty())))
                        }),
                        _ => false,
                    };
                    if !allowed_source {
                        return None;
                    }
                    owner_ty
                }
            }
        };
        let is_field = self.class_field(parent_ty, projection.field())
            || ((self.value_class(parent_ty) || self.move_value_class(parent_ty))
                && self.value_signature(parent_ty).is_some_and(|(nominal, _)| {
                    nominal
                        .fields()
                        .iter()
                        .any(|candidate| candidate.symbol() == projection.field())
                }));
        is_field.then_some(field_ty)
    }

    pub(super) fn numeric_for(
        &mut self,
        statement: lang_frontend::ast::StatementId,
        source: ExpressionId,
        body: lang_frontend::ast::StatementId,
    ) -> bool {
        let statement_id =
            lang_frontend::type_checking::UnitStatementId::new(self.inputs.source_unit, statement);
        if self.inputs.typed.map_iteration(statement_id).is_some() {
            return false;
        }
        let Some(plan) = self.inputs.owned.iteration(statement_id) else {
            return false;
        };
        let descriptor = plan.descriptor();
        let source_id = self.id(source);
        if self.inputs.typed.sequential_iteration(statement_id) != Some(descriptor)
            || !matches!(
                descriptor.provider(),
                lang_frontend::type_checking::IterationProvider::NumericRange(
                    BuiltinType::Int | BuiltinType::Long
                )
            )
            || descriptor.statement() != statement_id
            || descriptor.source() != source_id
            || plan.source_access()
                != lang_frontend::ownership_checking::UnitIterationSourceAccess::Owned
            || plan.source_kind()
                != &lang_frontend::ownership_checking::IterationSource::OwnedSnapshot(source_id)
        {
            return false;
        }
        let element_ty = self.c(descriptor.element_type());
        let source_ty = self.c(descriptor.source_type());
        if !self.integer_type(element_ty)
            || !self.range_type(source_ty)
            || self.expression_type(source) != Some(source_ty)
            || !self.expression(source)
        {
            return false;
        }
        match descriptor.binding() {
            lang_frontend::type_checking::UnitSequentialIterationBinding::Discard => {}
            lang_frontend::type_checking::UnitSequentialIterationBinding::Name(symbol) => {
                if self.symbol_type(*symbol) != Some(element_ty) {
                    return false;
                }
                self.known.insert(*symbol);
            }
            lang_frontend::type_checking::UnitSequentialIterationBinding::Destructure(_) => {
                return false;
            }
        }
        let ok = self.loop_body(body);
        self.migrated |= ok;
        ok
    }

    pub(super) fn constant_materialization(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(facts) = self.inputs.typed.constants() else {
            return false;
        };
        let Ok(index) = facts
            .uses()
            .binary_search_by_key(&id, |use_| use_.expression())
        else {
            return false;
        };
        let descriptor = &facts.uses()[index];
        if let Some(owned) = self.inputs.constant_owned {
            let Some(plan) = owned.materialization_at(id) else {
                return false;
            };
            if plan.descriptor() != descriptor {
                return false;
            }
        }
        let Some(ty) = self.expression_type(expression) else {
            return false;
        };
        let supported = match descriptor.value() {
            lang_frontend::type_checking::ConstValue::Boolean(_) => matches!(
                self.kind(ty),
                Some(UnitTypeKind::Builtin(BuiltinType::Boolean))
            ),
            lang_frontend::type_checking::ConstValue::Integer { .. } => self.integer_type(ty),
            lang_frontend::type_checking::ConstValue::Char(_) => matches!(
                self.kind(ty),
                Some(UnitTypeKind::Builtin(BuiltinType::Char))
            ),
            lang_frontend::type_checking::ConstValue::String(_) => self.string(ty),
        };
        if supported {
            self.migrated = true;
        }
        supported
    }
}

pub(in crate::ssa::refine) mod control {
    //! Conservative condition outcomes, short-circuit structure, and loop-destructuring routing.
    use std::collections::{BTreeMap, BTreeSet};

    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        name_resolution::ScopeKind,
        ownership_checking::UnitShortCircuitRhs,
        parser::{BinaryOperator, Expression, Item, PrefixOperator, Statement, VariableKind},
        source::Span,
        type_checking::{
            BuiltinType, ParameterMode, UnitExpressionId, UnitStatementId, UnitTypeKind,
        },
    };

    use super::super::{Checker, RefineInputs, accepts_with_loop_route};

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(in crate::ssa::refine::subset) enum LoopRoute {
        Existing,
        Candidate,
        Verified,
    }

    pub(in crate::ssa::refine) fn accepts(inputs: &RefineInputs<'_>) -> bool {
        let (accepted, retry) = accepts_with_loop_route(inputs, LoopRoute::Existing);
        accepted
            || retry
                && has_destructuring_candidate(inputs)
                && accepts_with_loop_route(inputs, LoopRoute::Candidate).0
    }

    // Span membership only avoids a retry on unrelated files. The retry itself must
    // see the descriptor through this callable's normal reachable-body traversal.
    fn has_destructuring_candidate(inputs: &RefineInputs<'_>) -> bool {
        let Some(scope) = inputs.scope_span() else {
            return false;
        };
        inputs.typed.destructurings().iter().any(|descriptor| {
            let statement = descriptor.statement();
            descriptor.is_reachable()
                && statement.source_unit() == inputs.source_unit
                && inputs
                    .parsed
                    .ast()
                    .statements()
                    .get(statement.statement())
                    .is_ok_and(|node| {
                        let span = node.span();
                        span.source_id() == scope.source_id()
                            && scope.start() <= span.start()
                            && span.end() <= scope.end()
                    })
        })
    }

    fn index_while_scopes(inputs: &RefineInputs<'_>) -> Option<BTreeMap<(usize, usize), bool>> {
        let names = inputs
            .names
            .names()
            .source_units()
            .get(inputs.source_unit.index())?;
        let mut while_spans: BTreeSet<_> = inputs
            .parsed
            .ast()
            .statements()
            .iter()
            .filter_map(|(_, node)| {
                let span = node.span();
                matches!(node.payload(), Statement::While { .. })
                    .then_some((span.start(), span.end()))
            })
            .collect();
        let scopes = names.resolution().scopes();
        let mut callable_by_scope = Vec::with_capacity(scopes.len());
        let mut has_while = BTreeMap::new();
        for scope in scopes {
            let key = scope.span().map(|span| (span.start(), span.end()));
            let callable = if matches!(scope.kind(), ScopeKind::Function | ScopeKind::Lambda) {
                let key = key?;
                has_while.insert(key, false);
                Some(key)
            } else if let Some(parent) = scope.parent() {
                // Name resolution assigns the parent before entering a child.
                *callable_by_scope.get(parent.index())?
            } else {
                None
            };
            callable_by_scope.push(callable);
            if scope.kind() == ScopeKind::Loop && key.is_some_and(|key| while_spans.remove(&key)) {
                *has_while.get_mut(&callable?)? = true;
            }
        }
        // Preserve the conservative route if the AST and lexical scope facts disagree.
        while_spans.is_empty().then_some(has_while)
    }

    impl Checker<'_, '_> {
        pub(in crate::ssa::refine::subset) fn check_while(
            &mut self,
            condition: ExpressionId,
            body: StatementId,
            span: Span,
        ) -> bool {
            let invalidated = self.strip_reassigned_loop_views(span, true);
            let saved = self.view_symbols.clone();
            let ok = self.condition(condition, true)
                && (!self.condition_outcomes(condition).0 || self.loop_body(body));
            self.view_symbols = saved;
            if ok && invalidated {
                self.loop_route = LoopRoute::Verified;
            }
            ok
        }

        pub(in crate::ssa::refine::subset) fn scope_has_while_loop(&self) -> bool {
            let Some(owner) = self.inputs.scope_span() else {
                return true;
            };
            self.inputs
                .while_scopes
                .0
                .get_or_init(|| index_while_scopes(self.inputs))
                .as_ref()
                .and_then(|scopes| scopes.get(&(owner.start(), owner.end())))
                .copied()
                .unwrap_or(true)
        }

        /// Mirrors the builder: `!`, groups, `&&`, `||` and null comparisons are branch form
        /// (their own drop points are never emitted); other conditions are plain values.
        /// `views`: a MoveOnly null comparison may open a proof view only directly in an
        /// `if`/`while` condition (through groups and `&&`), on its true edge.
        pub(in crate::ssa::refine::subset) fn condition(
            &mut self,
            expression: ExpressionId,
            views: bool,
        ) -> bool {
            let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
                return false;
            };
            let id = self.id(expression);
            let boolean = self.expression_type(expression).is_some_and(|ty| {
                matches!(
                    self.kind(ty),
                    Some(UnitTypeKind::Builtin(BuiltinType::Boolean))
                )
            });
            if let Some(fact) = self.inputs.typed.null_comparison(id) {
                self.branch_form.insert(id);
                let nullable = self.c(fact.nullable_type());
                self.note_nullable(nullable);
                if let Some(supported) = self.borrowed_nullable_presence(fact.symbol(), nullable) {
                    return boolean && supported && self.known.contains(&fact.symbol());
                }
                let inner_ok = match self.kind(nullable) {
                    Some(UnitTypeKind::Nullable(inner))
                        if self.scalar(*inner)
                            || self.range_type(*inner)
                            || self.value_class(*inner) =>
                    {
                        true
                    }
                    Some(UnitTypeKind::Nullable(inner)) if self.view_inner(*inner) => {
                        if views && fact.non_null_when_true() {
                            self.view_symbols.insert(fact.symbol());
                        }
                        true
                    }
                    _ => false,
                };
                return boolean && inner_ok && self.known.contains(&fact.symbol());
            }
            match node.payload().clone() {
                Expression::Group { expression: inner } => {
                    self.branch_form.insert(id);
                    boolean && self.plain(expression) && self.condition(inner, views)
                }
                Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand: inner,
                    ..
                } => {
                    self.branch_form.insert(id);
                    boolean && self.plain(expression) && self.condition(inner, false)
                }
                Expression::Binary {
                    left,
                    operator: operator @ (BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr),
                    right,
                    ..
                } => {
                    self.branch_form.insert(id);
                    let views = views && operator == BinaryOperator::LogicalAnd;
                    let saved = self.view_symbols.clone();
                    let ok = boolean
                        && self.plain(expression)
                        && self.condition(left, views)
                        && (!self.logical_reaches_rhs(id, left, operator)
                            || self.condition(right, views));
                    if !views {
                        self.view_symbols = saved;
                    }
                    ok
                }
                _ => self.expression(expression),
            }
        }

        pub(in crate::ssa::refine::subset) fn note_else_view(&mut self, condition: ExpressionId) {
            let root = self.ungrouped(condition);
            let id = self.id(root);
            if let Some(fact) = self.inputs.typed.null_comparison(id)
                && !fact.non_null_when_true()
                && self.parameter_mode(fact.symbol()) != Some(ParameterMode::Borrow)
                && self.known.contains(&fact.symbol())
                && let Some(UnitTypeKind::Nullable(inner)) = self.kind(self.c(fact.nullable_type()))
                && self.view_inner(*inner)
                && (self.scope_requires_refine()
                    || self.map_kv(*inner).is_some()
                    || self.is_map_remove_symbol(fact.symbol())
                    || self
                        .inputs
                        .typed
                        .non_null_uses()
                        .iter()
                        .any(|use_fact| use_fact.symbol() == fact.symbol())
                    || self.inputs.owned.loans().iter().any(|loan| {
                        matches!(
                            loan.target(),
                            lang_frontend::ownership_checking::UnitLoanTarget::Place(place)
                                if place.is_root() && place.root() == fact.symbol()
                        )
                    }))
            {
                self.view_symbols.insert(fact.symbol());
            }
        }

        fn expression_may_continue(&self, expression: ExpressionId) -> bool {
            self.inputs
                .typed
                .expression_falls_through(self.id(expression))
                != Some(false)
        }

        /// A Boolean leaf may take either normal edge unless the frontend proved it exits.
        pub(in crate::ssa::refine::subset) fn condition_outcomes(
            &self,
            expression: ExpressionId,
        ) -> (bool, bool) {
            let expression = self.id(expression);
            if self.inputs.constant_owned.is_some() {
                self.inputs.typed.structural_boolean_outcomes(expression)
            } else {
                self.inputs.typed.ordinary_boolean_outcomes(expression)
            }
        }

        pub(in crate::ssa::refine::subset) fn logical_reaches_rhs(
            &self,
            control: UnitExpressionId,
            left: ExpressionId,
            operator: BinaryOperator,
        ) -> bool {
            if self
                .inputs
                .constant_owned
                .and_then(|owned| owned.short_circuit_at(control))
                .is_some_and(|plan| plan.rhs() == UnitShortCircuitRhs::Never)
            {
                return false;
            }
            let (yes, no) = self.condition_outcomes(left);
            if operator == BinaryOperator::LogicalAnd {
                yes
            } else {
                no
            }
        }

        pub(in crate::ssa::refine::subset) fn statement_may_continue(
            &self,
            statement: StatementId,
        ) -> bool {
            let Ok(node) = self.inputs.parsed.ast().statements().get(statement) else {
                return true;
            };
            match node.payload() {
                Statement::Block { elements }
                | Statement::ControlBody { elements }
                | Statement::LambdaBody { elements } => elements
                    .iter()
                    .all(|&element| self.statement_may_continue(element)),
                Statement::Loop { .. } | Statement::While { .. } => {
                    self.inputs.typed.loop_falls_through(UnitStatementId::new(
                        self.inputs.source_unit,
                        statement,
                    )) != Some(false)
                }
                Statement::LocalVariable { declaration } => {
                    let Ok(item) = self.inputs.parsed.ast().items().get(*declaration) else {
                        return true;
                    };
                    match item.payload() {
                        Item::Variable {
                            kind: VariableKind::BorrowVal(_),
                            initializer,
                            ..
                        } => self.expression_may_continue(*initializer),
                        _ => true,
                    }
                }
                Statement::Expression { expression } => {
                    if !self.expression_may_continue(*expression)
                        || self
                            .expression_type(*expression)
                            .is_some_and(|ty| self.nothing(ty))
                    {
                        return false;
                    }
                    if let Ok(node) = self.inputs.parsed.ast().expressions().get(*expression)
                        && let Expression::If {
                            condition,
                            then_branch,
                            else_branch,
                            ..
                        } = node.payload()
                    {
                        let (yes, no) = self.condition_outcomes(*condition);
                        return (yes && self.statement_may_continue(*then_branch))
                            || (no
                                && else_branch
                                    .is_none_or(|branch| self.statement_may_continue(branch)));
                    }
                    true
                }
                _ => true,
            }
        }
    }
}
