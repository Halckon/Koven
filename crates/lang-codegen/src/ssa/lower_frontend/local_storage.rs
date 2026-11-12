//! Concrete local storage demanded by ownership formation and construction facts.

use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{NameResolution, ScopeId, ScopeKind, SymbolId, SymbolKind},
    ownership_checking::OwnershipCheckedFile,
    parser::{Expression, Item, ParsedFile, Statement, StringPart, WhenCondition},
    source::Span,
    type_checking::{IntrinsicTypeConstructor, TypeId, TypeKind, TypedFile},
};

use super::{
    LoweringError, LoweringErrorKind, error, instances::resolve_concrete_type,
    nominal::NominalTypeMapper, orchestrate::FunctionPlanBody, present_name,
};
use crate::ssa::model::Module;

pub(super) struct Planner<'a> {
    pub(super) module: &'a mut Module,
    pub(super) mapper: &'a mut NominalTypeMapper,
    pub(super) parsed: &'a ParsedFile,
    pub(super) names: &'a NameResolution,
    pub(super) typed: &'a TypedFile,
    pub(super) owned: &'a OwnershipCheckedFile,
    pub(super) substitutions: &'a BTreeMap<SymbolId, TypeId>,
}

impl Planner<'_> {
    /// Demand storage from successful ownership initialization facts for this callable.
    /// Formation is historical: a later move or exit does not remove the type demand.
    pub(super) fn intern_formed_nullable_maps(
        &mut self,
        function: ItemId,
    ) -> Result<(), LoweringError> {
        let (item, function_span) = super::orchestrate::unwrap_modified(self.parsed, function)?;
        if !matches!(item, Item::Function { .. }) {
            return Err(error(LoweringErrorKind::MissingFact, function_span));
        }
        for &id in self.owned.formed_owned_locals() {
            let symbol = self
                .names
                .symbols()
                .get(id.index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, function_span))?;
            let span = symbol.span();
            if symbol.id() != id
                || symbol.kind() != SymbolKind::Variable
                || symbol.is_synthetic()
                || symbol.name() == "_"
                || span.source_id() != self.parsed.source_id()
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            if !formed_in_function(self.names, symbol.scope(), function_span)? {
                continue;
            }
            let declared = self
                .typed
                .symbol_type(id)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let Some(TypeKind::Nullable(inner)) = self.typed.types().get(declared) else {
                continue;
            };
            // This slice plans only Map payloads; other nullable representations keep
            // their existing demand paths and are not resolved speculatively.
            if !matches!(
                self.typed.types().get(*inner),
                Some(TypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Map
                        | IntrinsicTypeConstructor::MutableMap,
                    ..
                })
            ) {
                continue;
            }
            let declared = resolve_concrete_type(self.typed, declared, self.substitutions, span)?;
            // Keep the complete Resource/deinit/K/V checks before canonical cache lookup.
            self.mapper
                .intern(self.module, self.names, self.typed, declared, span)?;
        }
        Ok(())
    }

    pub(super) fn intern_body(&mut self, body: FunctionPlanBody) -> Result<(), LoweringError> {
        match body {
            FunctionPlanBody::Expression(expression) => self.expression(expression),
            FunctionPlanBody::Block(block) => self.statement(block),
        }
    }

    fn statement(&mut self, statement: StatementId) -> Result<(), LoweringError> {
        let parsed = self.parsed;
        let node = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        match node.payload() {
            Statement::Block { elements } | Statement::ControlBody { elements } => {
                for &element in elements {
                    self.statement(element)?;
                }
            }
            Statement::LocalVariable { declaration } => {
                self.local_variable(*declaration, node.span())?;
            }
            Statement::LocalDestructuring { initializer, .. }
            | Statement::Expression {
                expression: initializer,
            } => self.expression(*initializer)?,
            Statement::While {
                condition, body, ..
            } => {
                self.expression(*condition)?;
                self.statement(*body)?;
            }
            Statement::For { source, body, .. } => {
                self.expression(*source)?;
                self.statement(*body)?;
            }
            Statement::Loop { body, .. } => self.statement(*body)?,
            // Each callable owns its body and substitutions; never borrow the enclosing plan.
            Statement::LambdaBody { .. } => {}
            Statement::Error => return Err(error(LoweringErrorKind::MissingFact, node.span())),
        }
        Ok(())
    }

    fn expression(&mut self, expression: ExpressionId) -> Result<(), LoweringError> {
        let parsed = self.parsed;
        let node = parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        match node.payload() {
            Expression::Group { expression }
            | Expression::Cast { expression, .. }
            | Expression::TypeTest { expression, .. }
            | Expression::Prefix {
                operand: expression,
                ..
            }
            | Expression::NonNullAssert {
                operand: expression,
                ..
            }
            | Expression::Propagate {
                value: expression, ..
            }
            | Expression::Member {
                receiver: expression,
                ..
            } => self.expression(*expression)?,
            Expression::Binary { left, right, .. }
            | Expression::Assignment {
                target: left,
                value: right,
                ..
            }
            | Expression::Index {
                receiver: left,
                index: right,
            } => {
                self.expression(*left)?;
                self.expression(*right)?;
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                self.expression(*callee)?;
                for argument in arguments {
                    self.expression(argument.value)?;
                }
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.expression(*condition)?;
                self.statement(*then_branch)?;
                if let Some(branch) = else_branch {
                    self.statement(*branch)?;
                }
            }
            Expression::When {
                subject, entries, ..
            } => {
                if let Some(subject) = subject {
                    self.expression(*subject)?;
                }
                for entry in entries {
                    for condition in &entry.conditions {
                        match condition {
                            WhenCondition::Expression(expression)
                            | WhenCondition::Contains { expression, .. } => {
                                self.expression(*expression)?;
                            }
                            WhenCondition::TypeTest { .. } => {}
                        }
                    }
                    self.statement(entry.body)?;
                }
            }
            Expression::Return { value, .. }
            | Expression::CallableReference {
                receiver: value, ..
            } => {
                if let Some(value) = value {
                    self.expression(*value)?;
                }
            }
            Expression::String { parts } => {
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        self.expression(*expression)?;
                    }
                }
            }
            Expression::Lambda { .. }
            | Expression::Name
            | Expression::This
            | Expression::Literal(_)
            | Expression::Break { .. }
            | Expression::Continue { .. }
            | Expression::SuperMember { .. } => {}
            Expression::Error => return Err(error(LoweringErrorKind::MissingFact, node.span())),
        }
        Ok(())
    }

    fn local_variable(&mut self, declaration: ItemId, span: Span) -> Result<(), LoweringError> {
        let (item, _) = super::orchestrate::unwrap_modified(self.parsed, declaration)?;
        let Item::Variable {
            name, initializer, ..
        } = item
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        // Nested control bodies can contain their own locals even when this initializer
        // is not itself a construction. A lambda remains a separate callable boundary.
        self.expression(initializer)?;
        let mut construction = initializer;
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(construction)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => construction = *expression,
                _ => break,
            }
        }
        let Some(descriptor) = self.typed.construction(construction) else {
            return Ok(());
        };
        // Typed facts also describe dead code; only Phase 3 establishes runtime demand.
        let Some(plan) = self.owned.construction_plan(construction) else {
            return Ok(());
        };
        if descriptor.expression() != construction
            || plan.construction() != construction
            || plan.target() != descriptor.target()
            || plan.root_obligation().is_some_and(|root| {
                root.construction() != construction
                    || root.result_type() != descriptor.result_type()
            })
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if plan.terminating_operand().is_some() {
            return Ok(());
        }
        let name = present_name(name).ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self
            .names
            .symbols()
            .iter()
            .find(|symbol| symbol.span() == name && symbol.kind() == SymbolKind::Variable)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, name))?;
        if symbol.name() == "_" {
            return Ok(());
        }
        let declared = self
            .typed
            .symbol_type(symbol.id())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, name))?;
        let declared = resolve_concrete_type(self.typed, declared, self.substitutions, span)?;
        let actual = resolve_concrete_type(
            self.typed,
            descriptor.result_type(),
            self.substitutions,
            span,
        )?;
        if matches!(self.typed.types().get(declared), Some(TypeKind::Nullable(inner)) if *inner == actual)
        {
            // Keep the canonical mapper's inline-nullable and resource-wrapper guards.
            self.mapper
                .intern(self.module, self.names, self.typed, declared, span)?;
        }
        Ok(())
    }
}

// Match the nearest callable, not span containment: lambda and deinit scopes must
// never contribute local demand to an enclosing ordinary Function plan.
fn formed_in_function(
    names: &NameResolution,
    mut id: ScopeId,
    function_span: Span,
) -> Result<bool, LoweringError> {
    for _ in 0..names.scopes().len() {
        let scope = names
            .scopes()
            .get(id.index())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, function_span))?;
        match scope.kind() {
            ScopeKind::Function => return Ok(scope.span() == Some(function_span)),
            ScopeKind::Lambda | ScopeKind::File | ScopeKind::Classifier | ScopeKind::Companion => {
                return Ok(false);
            }
            _ => {
                id = scope
                    .parent()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, function_span))?;
            }
        }
    }
    Err(error(LoweringErrorKind::MissingFact, function_span))
}

pub(in crate::ssa::lower_frontend) mod integer {
    //! Local facts and submission adapt into the shared integer, checked, and range algorithms.
    use super::super::{
        BlockId, BuiltinType, CheckedArithmeticOperator, EntityId, EntityType, ExpressionLowerer,
        Function, LoweredValue, LoweringError, LoweringErrorKind, Operation, Span, SsaTypeId,
        SymbolId, ValueId, builtin_type, error, value,
    };
    use crate::ssa::expression_core::checked::{self, CarryColumn, CheckedCarry, CheckedLowering};
    use lang_frontend::{
        ast::ExpressionId,
        type_checking::{IntegerOperationKind, TypeId, TypeKind},
    };
    use std::collections::BTreeMap;

    pub(in crate::ssa) struct CheckedRoles {
        baseline: BTreeMap<SymbolId, LoweredValue>,
        carried: super::super::control::LinearBindings,
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa::lower_frontend) fn checked(
            &mut self,
            operator: CheckedArithmeticOperator,
            left: ValueId,
            right: ValueId,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            checked::lower_checked_arithmetic(self, operator, left, right, ty, span)
        }

        pub(in crate::ssa::lower_frontend) fn checked_operation(
            &mut self,
            operation: Operation,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            checked::lower_checked_operation(self, operation, ty, span)
        }

        pub(in crate::ssa::lower_frontend) fn lower_integer_range(
            &mut self,
            expression: ExpressionId,
        ) -> Result<LoweredValue, LoweringError> {
            let span = self.expression_span(expression)?;
            let id = expression;
            let descriptor = self
                .typed
                .integer_range_operation(id)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            if descriptor.expression() != id
                || self.typed.expression_type(descriptor.left()) != Some(descriptor.left_type())
                || self.typed.expression_type(descriptor.right()) != Some(descriptor.right_type())
                || self.typed.expression_type(id) != Some(descriptor.result_type())
            {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let types = [
                descriptor.left_type(),
                descriptor.right_type(),
                descriptor.result_type(),
            ]
            .map(|ty| match self.typed.types().get(ty) {
                Some(TypeKind::Builtin(builtin)) => Some(*builtin),
                _ => None,
            });
            let left_id = descriptor.left();
            let right_id = descriptor.right();
            if !crate::ssa::integer::matches_range_fact(
                self.parsed,
                expression,
                left_id,
                right_id,
                descriptor.kind(),
                types,
            ) {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            let left = match self.lower(left_id)? {
                LoweredValue::Value(value) => value,
                LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            let right = match self.lower(right_id)? {
                LoweredValue::Value(value) => value,
                LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            let ty = self.expression_ssa_type(expression, span)?;
            let (operation, negate) =
                crate::ssa::integer::range_operation(descriptor.kind(), left, right);
            let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
            let EntityId::Value(operand) = results[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            if !negate {
                return Ok(LoweredValue::Value(operand));
            }
            let (_, results) = self.append(
                Operation::BooleanNot { operand },
                vec![EntityType::Value(ty)],
                span,
            )?;
            let EntityId::Value(result) = results[0] else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            Ok(LoweredValue::Value(result))
        }

        pub(in crate::ssa::lower_frontend) fn lower_integer_operation(
            &mut self,
            expression: ExpressionId,
        ) -> Result<LoweredValue, LoweringError> {
            let span = self.expression_span(expression)?;
            lower_integer(self, expression, span)
        }
    }

    impl CheckedLowering for ExpressionLowerer<'_> {
        type Roles = CheckedRoles;

        fn boolean_type(&self) -> Option<SsaTypeId> {
            self.type_ids.iter().find_map(|(frontend, ssa)| {
                (builtin_type(self.typed, *frontend) == Some(BuiltinType::Boolean)).then_some(*ssa)
            })
        }

        fn ir_cursor(&mut self) -> (&mut Function, &mut BlockId) {
            (&mut *self.function, &mut self.block)
        }

        fn capture_roles(&self, span: Span) -> Result<CheckedCarry<Self::Roles>, LoweringError> {
            let baseline = self.bindings.clone();
            let carried = self.linear_binding_slots(&baseline, span)?;
            self.carry_checked_map(&carried, span)?;
            let columns = carried
                .slots
                .iter()
                .map(|slot| CarryColumn {
                    source: slot.source,
                    ty: slot.ty,
                })
                .collect();
            Ok(CheckedCarry {
                columns,
                roles: CheckedRoles { baseline, carried },
            })
        }

        fn restore_roles(
            &mut self,
            roles: Self::Roles,
            from: BlockId,
            success: BlockId,
            span: Span,
        ) -> Result<(), LoweringError> {
            self.bindings =
                self.rebind_linear_bindings(&roles.baseline, success, &roles.carried, span)?;
            self.resume_checked_map(from, &roles.carried, span)
        }
    }

    impl IntegerLowering for ExpressionLowerer<'_> {
        type Expr = ExpressionId;
        type Type = TypeId;

        fn fact(&self, expression: ExpressionId) -> Option<IntegerFact<ExpressionId, TypeId>> {
            self.typed
                .integer_operation(expression)
                .map(|descriptor| IntegerFact {
                    expression: descriptor.expression(),
                    receiver: descriptor.receiver(),
                    receiver_type: descriptor.receiver_type(),
                    result_type: descriptor.result_type(),
                    kind: descriptor.kind(),
                })
        }

        fn receiver_belongs_here(&self, _receiver: ExpressionId) -> bool {
            true
        }

        fn expression_type(&self, expression: ExpressionId) -> Option<TypeId> {
            self.typed.expression_type(expression)
        }

        fn builtin_type(&self, ty: TypeId) -> Option<BuiltinType> {
            match self.typed.types().get(ty) {
                Some(TypeKind::Builtin(builtin)) => Some(*builtin),
                _ => None,
            }
        }

        fn receiver_matches_ast(&self, expression: ExpressionId, receiver: ExpressionId) -> bool {
            crate::ssa::integer::intrinsic_receiver_matches(self.parsed, expression, receiver)
        }

        fn lower_operand(&mut self, receiver: ExpressionId) -> Result<LoweredValue, LoweringError> {
            self.lower(receiver)
        }

        fn result_ssa_type(
            &self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<SsaTypeId, LoweringError> {
            self.expression_ssa_type(expression, span)
        }

        fn append_scalar(
            &mut self,
            operation: Operation,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
            Ok(LoweredValue::Value(value(results[0])))
        }

        fn append_checked(
            &mut self,
            operation: Operation,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            self.checked_operation(operation, ty, span)
        }
    }

    #[derive(Clone, Copy)]
    pub(in crate::ssa::lower_frontend) struct IntegerFact<E, T> {
        pub(in crate::ssa::lower_frontend) expression: E,
        pub(in crate::ssa::lower_frontend) receiver: E,
        pub(in crate::ssa::lower_frontend) receiver_type: T,
        pub(in crate::ssa::lower_frontend) result_type: T,
        pub(in crate::ssa::lower_frontend) kind: IntegerOperationKind,
    }

    /// Only the fact queries and submission boundaries required by integer intrinsics.
    pub(in crate::ssa::lower_frontend) trait IntegerLowering {
        type Expr: Copy + Eq;
        type Type: Copy + Eq;

        fn fact(&self, expression: Self::Expr) -> Option<IntegerFact<Self::Expr, Self::Type>>;
        fn receiver_belongs_here(&self, receiver: Self::Expr) -> bool;
        fn expression_type(&self, expression: Self::Expr) -> Option<Self::Type>;
        fn builtin_type(&self, ty: Self::Type) -> Option<BuiltinType>;
        fn receiver_matches_ast(&self, expression: Self::Expr, receiver: Self::Expr) -> bool;
        fn lower_operand(&mut self, receiver: Self::Expr) -> Result<LoweredValue, LoweringError>;
        fn result_ssa_type(
            &self,
            expression: Self::Expr,
            span: Span,
        ) -> Result<SsaTypeId, LoweringError>;
        fn append_scalar(
            &mut self,
            operation: Operation,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError>;
        fn append_checked(
            &mut self,
            operation: Operation,
            ty: SsaTypeId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError>;
    }

    pub(in crate::ssa::lower_frontend) fn lower_integer<H: IntegerLowering>(
        host: &mut H,
        expression: H::Expr,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = host
            .fact(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.expression != expression
            || !host.receiver_belongs_here(descriptor.receiver)
            || host.expression_type(descriptor.receiver) != Some(descriptor.receiver_type)
            || host.expression_type(expression) != Some(descriptor.result_type)
            || !host.receiver_matches_ast(expression, descriptor.receiver)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let integer_types = [descriptor.receiver_type, descriptor.result_type]
            .into_iter()
            .all(|ty| {
                matches!(
                    host.builtin_type(ty),
                    Some(
                        BuiltinType::Byte
                            | BuiltinType::Short
                            | BuiltinType::Int
                            | BuiltinType::Long
                            | BuiltinType::UByte
                            | BuiltinType::UShort
                            | BuiltinType::UInt
                            | BuiltinType::ULong
                    )
                )
            });
        if !integer_types {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        match descriptor.kind {
            IntegerOperationKind::Invert if descriptor.receiver_type != descriptor.result_type => {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            IntegerOperationKind::Invert | IntegerOperationKind::CheckedConvert => {}
        }
        let operand = match host.lower_operand(descriptor.receiver)? {
            LoweredValue::Value(value) => value,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        // Keep this after receiver evaluation: adapters retain their original type errors.
        let ty = host.result_ssa_type(expression, span)?;
        if descriptor.kind == IntegerOperationKind::CheckedConvert {
            return host.append_checked(Operation::CheckedIntegerConvert { operand }, ty, span);
        }
        host.append_scalar(Operation::IntegerNot { operand }, ty, span)
    }
}
