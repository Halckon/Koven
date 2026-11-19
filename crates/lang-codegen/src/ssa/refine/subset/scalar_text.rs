//! Subset rules for integer intrinsics (`inv`, checked conversions), prefix `+`/`-`,
//! `IntRange`/`LongRange` value operations, and `String.clone()` (SPEC-0371).
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{
        AssignmentOperator, Expression, Item, LiteralKind, PrefixOperator, Statement, VariableKind,
    },
    type_checking::{
        BuiltinType, IntegerOperationKind, ParameterMode, StringOperationKind, UnitExpressionId,
        UnitTypeId, UnitTypeKind,
    },
};

use super::{Checker, RefineBody};

impl Checker<'_, '_> {
    /// SPEC-0454 only supplies a trigger; the enclosing signature/body/drop checks still apply.
    pub(super) fn linear_scalar_body(&self) -> bool {
        self.inputs.function_item.is_some()
            && self.inputs.receiver.is_none()
            && self.inputs.static_self.is_none()
            && self.inputs.borrow_return.is_none()
            && self.inputs.carrier_return.is_none()
            && self
                .inputs
                .parameters
                .iter()
                .zip(&self.inputs.modes)
                .all(|(symbol, mode)| {
                    matches!(mode, ParameterMode::Value | ParameterMode::Borrow)
                        && self.symbol_type(*symbol).is_some_and(|ty| self.scalar(ty))
                })
            && (self.scalar(self.inputs.return_type) || self.unit(self.inputs.return_type))
            && match self.inputs.body {
                RefineBody::Expression(expression) => self.linear_scalar_expression(expression),
                RefineBody::Block(statement) => self.linear_scalar_statement(statement),
                RefineBody::Lambda(_) => false,
            }
    }

    /// Restrict shape, not semantics: typed facts and the normal Checker validate each node.
    fn linear_scalar_expression(&self, expression: ExpressionId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let ty = self.expression_type(expression);
        let scalar = ty.is_some_and(|ty| self.scalar(ty));
        let value_or_exit =
            ty.is_some_and(|ty| self.scalar(ty) || self.unit(ty) || self.nothing(ty));
        match node.payload() {
            Expression::Literal(_) | Expression::Name => scalar,
            Expression::Group { expression } => self.linear_scalar_expression(*expression),
            Expression::Prefix { operand, .. } => scalar && self.linear_scalar_expression(*operand),
            Expression::Binary { left, right, .. } => {
                scalar
                    && self.linear_scalar_expression(*left)
                    && self.linear_scalar_expression(*right)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                value_or_exit
                    && self.linear_scalar_expression(*condition)
                    && self.linear_scalar_statement(*then_branch)
                    && else_branch.is_none_or(|branch| self.linear_scalar_statement(branch))
            }
            Expression::Call { arguments, .. } => {
                value_or_exit
                    && arguments
                        .iter()
                        .all(|argument| self.linear_scalar_expression(argument.value))
            }
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } => self.linear_scalar_expression(*target) && self.linear_scalar_expression(*value),
            Expression::Return { value, .. } => {
                value.is_none_or(|value| self.linear_scalar_expression(value))
            }
            Expression::Break { .. } | Expression::Continue { .. } => true,
            _ => false,
        }
    }

    fn linear_scalar_statement(&self, statement: StatementId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().statements().get(statement) else {
            return false;
        };
        match node.payload() {
            Statement::Block { elements } | Statement::ControlBody { elements } => elements
                .iter()
                .all(|element| self.linear_scalar_statement(*element)),
            Statement::While {
                condition, body, ..
            } => self.linear_scalar_expression(*condition) && self.linear_scalar_statement(*body),
            Statement::Expression { expression } => self.linear_scalar_expression(*expression),
            Statement::LocalVariable { declaration } => self
                .inputs
                .parsed
                .ast()
                .items()
                .get(*declaration)
                .is_ok_and(|item| {
                    matches!(item.payload(),
                    Item::Variable { kind: VariableKind::Val | VariableKind::Var, initializer, .. }
                        if self.linear_scalar_expression(*initializer))
                }),
            _ => false,
        }
    }

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
        if let Some(receiver) = self.inputs.non_completing_projection_receiver(id) {
            return self.expression(receiver);
        }
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
        let receiver_ty = self.c(descriptor.receiver_type());
        if self.borrowed_string_name(receiver)
            || self.borrowed_rc_payload(expression, receiver, receiver_ty)
            || self.borrowed_container_element(expression, receiver, receiver_ty)
        {
            self.migrated = true;
            return true;
        }
        if let Some(ok) = self.string_proof_operand(receiver) {
            self.migrated |= ok;
            return ok;
        }
        if self.shared_field_read_type(receiver, None) == Some(receiver_ty) {
            self.migrated = true;
            return true;
        }
        let ok = self.expression(receiver);
        self.migrated |= ok;
        ok
    }

    pub(super) fn operand(
        &mut self,
        binary: ExpressionId,
        expression: ExpressionId,
        string: bool,
    ) -> bool {
        if string {
            if self.borrowed_string_name(expression) {
                return true;
            }
            if let Some(ok) = self.string_proof_operand(expression) {
                return ok;
            }
            if self
                .inputs
                .has_shared_field_operand_loan(binary, expression)
            {
                let projection = self
                    .inputs
                    .typed
                    .aggregate_projection(self.id(self.ungrouped(expression)));
                let admitted = if let Some(ok) =
                    projection.and_then(|projection| self.borrowed_enum_payload(projection))
                {
                    ok && self
                        .expression_type(expression)
                        .is_some_and(|ty| self.string(ty))
                } else {
                    self.shared_field_read_type(expression, None)
                        .is_some_and(|ty| self.string(ty))
                };
                self.migrated |= admitted;
                return admitted;
            }
        }
        self.expression(expression)
    }

    pub(super) fn string_equality_operand(
        &mut self,
        binary: ExpressionId,
        expression: ExpressionId,
    ) -> bool {
        let Some(ty) = self.expression_type(expression) else {
            return false;
        };
        if !self.inputs.is_nullable_string(ty) {
            return self.operand(binary, expression, true);
        }
        self.note_nullable(ty);
        let root = self.ungrouped(expression);
        if let Some(symbol) = self.direct_place_symbol(root)
            && self.symbol_type(symbol) == Some(ty)
            && self.borrowed_nullable_presence(symbol, ty) == Some(true)
        {
            return self.known.contains(&symbol);
        }
        self.expression(expression)
    }

    /// The caller has selected a String operation that reads through a shared proof.
    pub(super) fn string_proof_operand(&mut self, expression: ExpressionId) -> Option<bool> {
        let root = self.ungrouped(expression);
        let id = self.id(root);
        let fact = self.inputs.typed.non_null_use(id)?;
        let ok = self.string(self.c(fact.narrowed_type()))
            && self.reference(root) == Some(fact.symbol())
            && self.known.contains(&fact.symbol())
            && self.view_symbols.contains(&fact.symbol());
        if ok && self.inputs.typed.nullable_when_use(id).is_some() {
            self.string_when_reads.insert(id);
        }
        Some(ok)
    }

    pub(super) fn shared_field_read_type(
        &mut self,
        expression: ExpressionId,
        temporary_parent: Option<UnitExpressionId>,
    ) -> Option<UnitTypeId> {
        self.field_read_type(expression, temporary_parent, true)
    }

    pub(super) fn stable_index_field_type(
        &mut self,
        expression: ExpressionId,
    ) -> Option<UnitTypeId> {
        self.field_read_type(expression, None, false)
    }

    fn field_read_type(
        &mut self,
        expression: ExpressionId,
        temporary_parent: Option<UnitExpressionId>,
        allow_rc_parent: bool,
    ) -> Option<UnitTypeId> {
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
                    let parent =
                        self.field_read_type(receiver_expr, temporary_parent, allow_rc_parent)?;
                    if !(self.class(parent)
                        || self.value_class(parent)
                        || self.move_value_class(parent))
                    {
                        return None;
                    }
                    parent
                } else if allow_rc_parent && let Some(parent) = self.rc_value_owner(receiver_expr) {
                    parent
                } else if temporary_parent == Some(self.id(self.ungrouped(receiver_expr))) {
                    let parent = self.expression_type(receiver_expr)?;
                    if self
                        .inputs
                        .typed
                        .expression_falls_through(self.id(receiver_expr))
                        != Some(true)
                        || !self.expression(receiver_expr)
                    {
                        return None;
                    }
                    parent
                } else {
                    let root = self.ungrouped(receiver_expr);
                    if !self.plain(receiver_expr)
                        || !self.plain(root)
                        || !matches!(
                            self.inputs
                                .parsed
                                .ast()
                                .expressions()
                                .get(root)
                                .ok()?
                                .payload(),
                            Expression::Name | Expression::This
                        )
                    {
                        return None;
                    }
                    self.owner_type(Owner::Expression(root), ParameterMode::Borrow)?
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
        name_resolution::{ScopeKind, UnitSymbolId},
        ownership_checking::UnitShortCircuitRhs,
        parser::{BinaryOperator, Expression, Item, PrefixOperator, Statement},
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
            let Some([_, when_true]) = self.condition_proofs(expression, views) else {
                return false;
            };
            if views {
                self.view_symbols.extend(when_true);
            }
            true
        }

        pub(in crate::ssa::refine::subset) fn if_condition_proofs(
            &mut self,
            condition: ExpressionId,
        ) -> Option<[BTreeSet<UnitSymbolId>; 2]> {
            let mut proofs = self.condition_proofs(condition, true)?;
            if let Some((symbol, edge)) = self.inputs.negated_rc_null_test(condition)
                && matches!(
                    self.parameter_mode(symbol),
                    None | Some(ParameterMode::Value)
                )
            {
                proofs[usize::from(edge)].insert(symbol);
            }
            Some(proofs)
        }

        pub(in crate::ssa::refine::subset) fn condition_proofs(
            &mut self,
            expression: ExpressionId,
            views: bool,
        ) -> Option<[BTreeSet<UnitSymbolId>; 2]> {
            let node = self
                .inputs
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .ok()?;
            let id = self.id(expression);
            let boolean = self.expression_type(expression).is_some_and(|ty| {
                matches!(
                    self.kind(ty),
                    Some(UnitTypeKind::Builtin(BuiltinType::Boolean))
                )
            });
            let mut result = [BTreeSet::new(), BTreeSet::new()];
            if let Some(fact) = self.inputs.typed.null_comparison(id) {
                self.branch_form.insert(id);
                if !boolean || !self.known.contains(&fact.symbol()) {
                    return None;
                }
                let nullable = self.c(fact.nullable_type());
                self.note_nullable(nullable);
                if let Some(supported) = self.borrowed_nullable_presence(fact.symbol(), nullable) {
                    if matches!(self.kind(nullable), Some(UnitTypeKind::Nullable(inner)) if self.string(*inner) || self.rc_payload(*inner).is_some())
                    {
                        result[usize::from(fact.non_null_when_true())].insert(fact.symbol());
                    }
                    return supported.then_some(result);
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
                return inner_ok.then_some(result);
            }
            match node.payload().clone() {
                Expression::Group { expression: inner } => {
                    self.branch_form.insert(id);
                    (boolean && self.plain(expression)).then_some(())?;
                    self.condition_proofs(inner, views)
                }
                Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand: inner,
                    ..
                } => {
                    self.branch_form.insert(id);
                    (boolean && self.plain(expression)).then_some(())?;
                    let [when_false, when_true] = self.condition_proofs(inner, false)?;
                    Some([when_true, when_false])
                }
                Expression::Binary {
                    left,
                    operator: operator @ (BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr),
                    right,
                    ..
                } => {
                    self.branch_form.insert(id);
                    (boolean && self.plain(expression)).then_some(())?;
                    let and = operator == BinaryOperator::LogicalAnd;
                    let views = views && and;
                    let saved = self.view_symbols.clone();
                    let left_proofs = self.condition_proofs(left, views)?;
                    let (left_true, left_false) = self.condition_outcomes(left);
                    let bypass_possible = if and { left_false } else { left_true };
                    let bypass = usize::from(!and);
                    let gate = usize::from(and);
                    if self.logical_reaches_rhs(id, left, operator) {
                        let added = left_proofs[gate]
                            .difference(&self.view_symbols)
                            .copied()
                            .collect::<Vec<_>>();
                        self.view_symbols.extend(added.iter().copied());
                        let mut right_proofs = self.condition_proofs(right, views)?;
                        let (right_true, right_false) = self.condition_outcomes(right);
                        for (outcome, reachable) in
                            [right_false, right_true].into_iter().enumerate()
                        {
                            if reachable {
                                right_proofs[outcome].extend(left_proofs[gate].iter().copied());
                                if outcome == bypass && bypass_possible {
                                    right_proofs[outcome]
                                        .retain(|symbol| left_proofs[bypass].contains(symbol));
                                }
                            } else if outcome == bypass && bypass_possible {
                                right_proofs[outcome].clone_from(&left_proofs[bypass]);
                            }
                        }
                        result = right_proofs;
                        for symbol in added {
                            self.view_symbols.remove(&symbol);
                        }
                    } else {
                        result[bypass].clone_from(&left_proofs[bypass]);
                    }
                    if !views {
                        self.view_symbols = saved;
                    }
                    Some(result)
                }
                _ => self.expression(expression).then_some(result),
            }
        }

        pub(in crate::ssa::refine::subset) fn note_else_view(&mut self, condition: ExpressionId) {
            let root = self.ungrouped(condition);
            let id = self.id(root);
            if let Some(fact) = self.inputs.typed.null_comparison(id)
                && !fact.non_null_when_true()
                && self.known.contains(&fact.symbol())
                && let Some(UnitTypeKind::Nullable(inner)) = self.kind(self.c(fact.nullable_type()))
                && (self.parameter_mode(fact.symbol()) != Some(ParameterMode::Borrow)
                    || self.string(*inner)
                    || self.rc_payload(*inner).is_some())
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

        pub(in crate::ssa::refine::subset) fn note_continuing_if_views(
            &mut self,
            span: Span,
            [(then_continues, then_views), (else_continues, else_views)]: [(bool, &BTreeSet<UnitSymbolId>);
                2],
        ) {
            let candidates = if then_continues {
                then_views
            } else {
                else_views
            };
            for &symbol in candidates {
                if (then_continues || else_continues)
                    && (!else_continues || else_views.contains(&symbol))
                    && (match self.parameter_mode(symbol) {
                        Some(ParameterMode::Borrow | ParameterMode::Inout) => {
                            self.inputs.borrowed_narrowing_used_after(symbol, span)
                        }
                        None | Some(ParameterMode::Value) if then_continues != else_continues => {
                            self.inputs.rc_narrowing_used_after(symbol, span)
                        }
                        _ => false,
                    })
                {
                    self.view_symbols.insert(symbol);
                }
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
            if self.inputs.constant_owned.is_none()
                && self.inputs.typed.ordinary_rhs_bypassed(control)
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
                        Item::Variable { initializer, .. } => {
                            self.expression_may_continue(*initializer)
                        }
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
