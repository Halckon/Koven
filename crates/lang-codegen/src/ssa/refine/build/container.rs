//! Sequential containers (`Array<T>`, `List<T>`, `MutableList<T>`) in the refined CFG:
//! construction, `.size`, indexing `xs[i]`, borrowed element arguments `f(xs[i])`,
//! element assignment `xs[i] = v`, and `MutableList<T>` mutating operations.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::UnitDropPoint,
    parser::Expression,
    source::Span,
    type_checking::{ContainerConstructionKind, UnitTypeId},
};

use super::{Builder, Lowered, callable::LentArgument};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::ROp,
};

impl Builder<'_, '_> {
    pub(super) fn container_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<Lowered>, LoweringError> {
        let id = self.unit(expression);
        let typed = self.inputs.typed;
        if typed.container_construction(id).is_some() {
            return self.container_construction(expression, span).map(Some);
        }
        if typed.container_size(id).is_some() {
            return self.container_size(expression, span).map(Some);
        }
        if typed.element_place(id).is_some() {
            return self.container_index(expression, span).map(Some);
        }
        if typed.container_append(id).is_some() {
            return self.container_append(expression, span).map(Some);
        }
        if typed.container_clear(id).is_some() {
            return self.container_clear(expression, span).map(Some);
        }
        if typed.container_remove_at(id).is_some() {
            return self.container_remove_at(expression, span).map(Some);
        }
        if typed.container_remove_first(id).is_some() {
            return self.container_remove_first(expression, span).map(Some);
        }
        if typed.container_remove_last(id).is_some() {
            return self.container_remove_last(expression, span).map(Some);
        }
        if typed.container_insert_at(id).is_some() {
            return self.container_insert_at(expression, span).map(Some);
        }
        Ok(None)
    }

    fn container_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (Expression::Call { arguments, .. }, _) = self.node(expression)? else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let container_ty = self.c(descriptor.container_type());
        let element_ty = self.c(descriptor.element_type());
        let container = self.ssa_type(container_ty, span)?;
        let mut elements = Vec::with_capacity(arguments.len());
        match descriptor.kind() {
            ContainerConstructionKind::EmptyMutableList => {
                if !arguments.is_empty() {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
            ContainerConstructionKind::ListForm => {
                for argument in arguments {
                    let Some(value) = self.required(argument.value)? else {
                        return Ok(Lowered::Diverged);
                    };
                    let value = self.adapt(value, argument.value, element_ty)?;
                    self.deliver(argument.value);
                    elements.push(value);
                }
            }
            ContainerConstructionKind::RuntimeLength => {
                return self
                    .runtime_container_construction(expression, &arguments, container, span);
            }
        }
        let slot = self.value(
            ROp::ContainerConstruct {
                container,
                elements,
            },
            EntityType::Value(container),
            span,
        )?;
        Ok(Lowered::Value(slot))
    }

    fn runtime_container_construction(
        &mut self,
        expression: ExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        container: crate::ssa::model::SsaTypeId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        use crate::ssa::{
            model::{ComparisonOperator, ScalarConstant},
            refine::body::{RAbort, RTerm},
        };
        let [size, initializer] = arguments else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let id = self.unit(expression);
        let int_ty = self.ssa_type(self.expression_type(size.value)?, size.span)?;
        let (size_loan, created_size, size_read) =
            self.runtime_size_loan(size.value, int_ty, size.span)?;
        let Some(size_loan) = size_loan else {
            return Ok(Lowered::Diverged);
        };
        let length = self.value(
            ROp::ReadLoan { loan: size_loan },
            EntityType::Value(int_ty),
            size.span,
        )?;
        let bool_unit = self
            .inputs
            .typed
            .types()
            .builtin(lang_frontend::type_checking::BuiltinType::Boolean)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let bool_ty = self.ssa_type(bool_unit, span)?;
        let zero = self.value(
            ROp::Constant(ScalarConstant::Integer(0)),
            EntityType::Value(int_ty),
            span,
        )?;
        let negative = self.value(
            ROp::Compare {
                operator: ComparisonOperator::LessThan,
                left: length,
                right: zero,
            },
            EntityType::Value(bool_ty),
            span,
        )?;
        let abort = self.body.block();
        let success = self.body.block();
        self.terminate(
            RTerm::Branch {
                condition: negative,
                when_true: abort,
                when_false: success,
            },
            span,
        )?;
        self.block = Some(abort);
        self.terminate(RTerm::Abort(RAbort::NegativeSize), span)?;
        self.block = Some(success);
        let (init_loan, created_init, init_read) =
            self.runtime_initializer_loan(initializer.value, initializer.span)?;
        let Some(init_loan) = init_loan else {
            return Ok(Lowered::Diverged);
        };
        let result = self.value(
            ROp::ContainerGenerateBorrowed {
                container,
                length,
                initializer: init_loan,
            },
            EntityType::Value(container),
            span,
        )?;
        for loan in [created_init, created_size].into_iter().flatten() {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        for arg in [size_read, init_read].into_iter().flatten() {
            self.drops(UnitDropPoint::AfterExpression(self.unit(arg)))?;
        }
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(result))
    }

    fn runtime_size_loan(
        &mut self,
        expression: ExpressionId,
        int_ty: crate::ssa::model::SsaTypeId,
        span: Span,
    ) -> Result<
        (
            Option<crate::ssa::refine::body::Slot>,
            Option<crate::ssa::refine::body::Slot>,
            Option<ExpressionId>,
        ),
        LoweringError,
    > {
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let id = self.unit(root);
        if let (Expression::Name, root_span) = self.node(root)?
            && self.inputs.typed.non_null_use(id).is_none()
            && !self
                .inputs
                .constant_owned
                .is_some_and(|owned| owned.materialization_at(id).is_some())
            && let Ok(symbol) = self.reference(root_span)
            && let Some(&loan) = self.borrowed.get(&symbol)
        {
            return Ok((Some(loan), None, Some(expression)));
        }
        let Some(owner) = self.required(expression)? else {
            return Ok((None, None, None));
        };
        let place = self.value(ROp::RootPlace { owner }, EntityType::Place(int_ty), span)?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: int_ty,
            },
            span,
        )?;
        Ok((Some(loan), Some(loan), None))
    }

    fn runtime_initializer_loan(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<
        (
            Option<crate::ssa::refine::body::Slot>,
            Option<crate::ssa::refine::body::Slot>,
            Option<ExpressionId>,
        ),
        LoweringError,
    > {
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let (payload, root_span) = self.node(root)?;
        match payload {
            Expression::Name => {
                let symbol = self.reference(root_span)?;
                if let Some(&loan) = self.borrowed.get(&symbol) {
                    return Ok((Some(loan), None, Some(expression)));
                }
                let owner = *self
                    .bindings
                    .get(&symbol)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, root_span))?;
                let EntityType::Value(callable) = self.body.slots[owner.0] else {
                    return Err(error(LoweringErrorKind::InvalidModel, root_span));
                };
                let place = self.value(
                    ROp::RootPlace { owner },
                    EntityType::Place(callable),
                    root_span,
                )?;
                let loan = self.value(
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: callable,
                    },
                    root_span,
                )?;
                Ok((Some(loan), Some(loan), Some(expression)))
            }
            Expression::Lambda { .. } => {
                let owner = self.form_callable(root, root_span)?;
                let callable = self
                    .inputs
                    .callables
                    .get(&self.unit(root))
                    .map(|callable| callable.callable)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, root_span))?;
                let place = self.value(
                    ROp::RootPlace { owner },
                    EntityType::Place(callable),
                    root_span,
                )?;
                let loan = self.value(
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: callable,
                    },
                    root_span,
                )?;
                Ok((Some(loan), Some(loan), None))
            }
            Expression::Call { .. } => {
                let Some(owner) = self.required(expression)? else {
                    return Ok((None, None, None));
                };
                let EntityType::Value(callable) = self.body.slots[owner.0] else {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                };
                let place =
                    self.value(ROp::RootPlace { owner }, EntityType::Place(callable), span)?;
                let loan = self.value(
                    ROp::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    },
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: callable,
                    },
                    span,
                )?;
                Ok((Some(loan), Some(loan), None))
            }
            _ => Err(error(LoweringErrorKind::UnsupportedNode, root_span)),
        }
    }

    fn container_size(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_size(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = descriptor.receiver().expression();
        let receiver_span = self.span(receiver);
        let symbol = self.named_container_symbol(receiver, span)?;
        let container_ty = self.c(descriptor.container_type());
        let target = self.ssa_type(container_ty, receiver_span)?;
        let (loan, created) = if let Some(&loan) = self.borrowed.get(&symbol) {
            (loan, None)
        } else {
            let owner = *self
                .bindings
                .get(&symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, receiver_span))?;
            let place = self.value(
                ROp::RootPlace { owner },
                EntityType::Place(target),
                receiver_span,
            )?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
                receiver_span,
            )?;
            (loan, Some(loan))
        };
        let result_ty = self.ssa_type(self.c(descriptor.result_type()), span)?;
        let result = self.value(
            ROp::ContainerLength { owner: loan },
            EntityType::Value(result_ty),
            span,
        )?;
        if let Some(loan) = created {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        self.drops(UnitDropPoint::CallReturn(id))?;
        self.drops(UnitDropPoint::AfterExpression(descriptor.receiver()))?;
        Ok(Lowered::Value(result))
    }

    fn container_index(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .element_place(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = descriptor.receiver().expression();
        let symbol = self.named_container_symbol(receiver, span)?;
        let owner = self
            .borrowed
            .get(&symbol)
            .or_else(|| self.bindings.get(&symbol))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(index) = self.required(descriptor.index().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let element = self.ssa_type(self.c(descriptor.element_type()), span)?;
        let place = self.value(
            ROp::ContainerElementPlace { owner, index },
            EntityType::Place(element),
            span,
        )?;
        let value = self.value(ROp::ReadPlace { place }, EntityType::Value(element), span)?;
        Ok(Lowered::Value(value))
    }

    /// Borrows `xs[i]` as a shared loan for a `Borrow` call argument.
    pub(super) fn borrowed_container_element_loan(
        &mut self,
        expression: ExpressionId,
        parameter: UnitTypeId,
        span: Span,
    ) -> Result<Option<LentArgument>, LoweringError> {
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let id = self.unit(root);
        let Some(descriptor) = self.inputs.typed.element_place(id) else {
            return Ok(None);
        };
        if self.c(descriptor.element_type()) != parameter {
            return Ok(None);
        }
        let receiver = descriptor.receiver().expression();
        let symbol = self.named_container_symbol(receiver, span)?;
        let owner = self
            .borrowed
            .get(&symbol)
            .or_else(|| self.bindings.get(&symbol))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let index = self
            .required(descriptor.index().expression())?
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element = self.ssa_type(parameter, span)?;
        let arg_span = self.span(expression);
        let place = self.value(
            ROp::ContainerElementPlace { owner, index },
            EntityType::Place(element),
            arg_span,
        )?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: element,
            },
            arg_span,
        )?;
        Ok(Some((loan, vec![loan], Some(expression))))
    }

    pub(super) fn is_element_target(&self, target: ExpressionId) -> bool {
        let mut root = target;
        while let Ok((Expression::Group { expression: inner }, _)) = self.node(root) {
            root = inner;
        }
        self.inputs.typed.element_place(self.unit(root)).is_some()
    }

    pub(super) fn element_assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
        value: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let mut root_target = target;
        while let (Expression::Group { expression: inner }, _) = self.node(root_target)? {
            root_target = inner;
        }
        let descriptor = self
            .inputs
            .typed
            .element_place(self.unit(root_target))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let Some(index) = self.required(descriptor.index().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let Some(replacement) = self.required(value)? else {
            return Ok(Lowered::Diverged);
        };
        let element_ty = self.c(descriptor.element_type());
        let replacement = self.adapt(replacement, value, element_ty)?;
        self.deliver(value);
        let owner = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.push(
            ROp::ContainerReplace {
                owner,
                index,
                value: replacement,
            },
            Vec::new(),
            span,
        )?;
        let replaced = self.collect_drops(UnitDropPoint::AfterReplacement(self.unit(assignment)));
        self.finish_drops(&replaced);
        Ok(Lowered::Unit)
    }

    fn container_append(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_append(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let element_expr = descriptor.element().expression();
        let Some(element) = self.required(element_expr)? else {
            return Ok(Lowered::Diverged);
        };
        let element = self.adapt(element, element_expr, self.c(descriptor.element_type()))?;
        self.deliver(element_expr);
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.push(
            ROp::ContainerAppend {
                owner: binding,
                element,
            },
            vec![binding],
            span,
        )?;
        self.drops(UnitDropPoint::AfterExpression(descriptor.element()))?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Unit)
    }

    fn container_clear(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_clear(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.push(ROp::ContainerClear { owner: binding }, vec![binding], span)?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Unit)
    }

    fn container_remove_at(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_remove_at(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let Some(index) = self.required(descriptor.index().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element_ty = self.ssa_type(self.c(descriptor.element_type()), span)?;
        let removed = self.body.slot(EntityType::Value(element_ty));
        self.push(
            ROp::ContainerRemoveAt {
                owner: binding,
                index,
            },
            vec![removed, binding],
            span,
        )?;
        self.drops(UnitDropPoint::AfterExpression(descriptor.index()))?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(removed))
    }

    fn container_remove_first(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_remove_first(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element_ty = self.ssa_type(self.c(descriptor.element_type()), span)?;
        let removed = self.body.slot(EntityType::Value(element_ty));
        self.push(
            ROp::ContainerRemoveFirst { owner: binding },
            vec![removed, binding],
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(removed))
    }

    fn container_remove_last(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_remove_last(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element_ty = self.ssa_type(self.c(descriptor.element_type()), span)?;
        let removed = self.body.slot(EntityType::Value(element_ty));
        self.push(
            ROp::ContainerRemoveLast { owner: binding },
            vec![removed, binding],
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(removed))
    }

    fn container_insert_at(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_insert_at(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let Some(index) = self.required(descriptor.index().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let element_expr = descriptor.element().expression();
        let Some(element) = self.required(element_expr)? else {
            return Ok(Lowered::Diverged);
        };
        let element = self.adapt(element, element_expr, self.c(descriptor.element_type()))?;
        self.deliver(element_expr);
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.push(
            ROp::ContainerInsertAt {
                owner: binding,
                index,
                element,
            },
            vec![binding],
            span,
        )?;
        self.drops(UnitDropPoint::AfterExpression(descriptor.index()))?;
        self.drops(UnitDropPoint::AfterExpression(descriptor.element()))?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Unit)
    }

    pub(super) fn named_container_symbol(
        &self,
        mut expression: ExpressionId,
        span: Span,
    ) -> Result<UnitSymbolId, LoweringError> {
        loop {
            let (payload, node_span) = self.node(expression)?;
            match payload {
                Expression::Name => return self.reference(node_span),
                Expression::Group { expression: inner } => expression = inner,
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
    }
}
