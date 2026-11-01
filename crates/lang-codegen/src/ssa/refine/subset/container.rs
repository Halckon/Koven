//! Subset rules for sequential containers (`Array<T>`, `List<T>`, `MutableList<T>`):
//! construction (`arrayOf`, `listOf`, `mutableListOf`, `MutableList<T>()`), `.size`,
//! Copyable indexing `xs[i]`, borrowed element arguments `f(xs[i])`, element assignment
//! `xs[i] = v`, and `MutableList<T>` mutating operations.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget},
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, ContainerConstructionKind, Copyability, ExpressionCategory,
        IntrinsicTypeConstructor, ParameterMode, SequentialContainerKind, UnitTypeId, UnitTypeKind,
    },
};

use super::Checker;

impl Checker<'_, '_> {
    /// A concrete sequential container (`Array<T>`, `List<T>`, `MutableList<T>`) whose
    /// element type is a supported non-`Unit` storage type.
    pub(super) fn container_element(&self, ty: UnitTypeId) -> Option<UnitTypeId> {
        let Some(UnitTypeKind::Intrinsic {
            constructor:
                IntrinsicTypeConstructor::Array
                | IntrinsicTypeConstructor::List
                | IntrinsicTypeConstructor::MutableList,
            arguments,
        }) = self.kind(ty)
        else {
            return None;
        };
        let [element] = arguments.as_slice() else {
            return None;
        };
        self.storage_type(*element).then_some(*element)
    }

    /// Whether any sequential-container typed fact claims `expression`.
    pub(super) fn has_container_fact(&self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        typed.container_construction(id).is_some()
            || typed.container_size(id).is_some()
            || typed.element_place(id).is_some()
            || typed.container_append(id).is_some()
            || typed.container_clear(id).is_some()
            || typed.container_remove_at(id).is_some()
            || typed.container_remove_first(id).is_some()
            || typed.container_remove_last(id).is_some()
            || typed.container_insert_at(id).is_some()
    }

    pub(super) fn container_operation(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        if typed.container_construction(id).is_some() {
            return self.container_construction(expression);
        }
        if typed.container_size(id).is_some() {
            return self.container_size(expression);
        }
        if typed.element_place(id).is_some() {
            return self.container_index(expression);
        }
        if typed.container_append(id).is_some() {
            return self.container_append(expression);
        }
        if typed.container_clear(id).is_some() {
            return self.container_clear(expression);
        }
        if typed.container_remove_at(id).is_some() {
            return self.container_remove_at(expression);
        }
        if typed.container_remove_first(id).is_some() {
            return self.container_remove_first(expression);
        }
        if typed.container_remove_last(id).is_some() {
            return self.container_remove_last(expression);
        }
        if typed.container_insert_at(id).is_some() {
            return self.container_insert_at(expression);
        }
        false
    }

    fn container_construction(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.container_construction(id).cloned() else {
            return false;
        };
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Expression::Call { arguments, .. } = node.payload() else {
            return false;
        };
        let container_ty = self.c(descriptor.container_type());
        let element_ty = self.c(descriptor.element_type());
        let constructor = match descriptor.container() {
            SequentialContainerKind::Array => IntrinsicTypeConstructor::Array,
            SequentialContainerKind::List => IntrinsicTypeConstructor::List,
            SequentialContainerKind::MutableList => IntrinsicTypeConstructor::MutableList,
        };
        if descriptor.expression() != id
            || self.expression_type(expression) != Some(container_ty)
            || self.inputs.typed.expression_category(id) != Some(ExpressionCategory::Temporary)
            || self.container_element(container_ty) != Some(element_ty)
            || !matches!(
                self.kind(container_ty),
                Some(UnitTypeKind::Intrinsic { constructor: actual, arguments: args })
                    if *actual == constructor && args.as_slice() == [element_ty]
            )
            || arguments.len() != descriptor.parameter_modes().len()
            || self.span_has_control_transfer(node.span())
        {
            return false;
        }
        let ok = match descriptor.kind() {
            ContainerConstructionKind::EmptyMutableList => {
                descriptor.container() == SequentialContainerKind::MutableList
                    && arguments.is_empty()
                    && descriptor.parameter_modes().is_empty()
            }
            ContainerConstructionKind::ListForm => {
                descriptor
                    .parameter_modes()
                    .iter()
                    .all(|mode| *mode == ParameterMode::Value)
                    && arguments.iter().all(|argument| {
                        argument.named_prefix.is_none()
                            && argument.mode_marker.is_none()
                            && self.adaptable_element(argument.value, element_ty)
                            && self.expression(argument.value)
                    })
            }
            ContainerConstructionKind::RuntimeLength => {
                let [size, initializer] = arguments.as_slice() else {
                    return false;
                };
                matches!(
                    descriptor.container(),
                    SequentialContainerKind::Array | SequentialContainerKind::List
                ) && descriptor.parameter_modes() == [ParameterMode::Borrow, ParameterMode::Borrow]
                    && size.named_prefix.is_none()
                    && initializer.named_prefix.is_none()
                    && matches!(
                        size.mode_marker,
                        None | Some(lang_frontend::parser::ParameterModeMarker::Borrow(_))
                    )
                    && matches!(
                        initializer.mode_marker,
                        None | Some(lang_frontend::parser::ParameterModeMarker::Borrow(_))
                    )
                    && self.is_int_index(size.value)
                    && self.expression(size.value)
                    && self.runtime_initializer(initializer.value)
            }
        };
        self.migrated |= ok;
        ok
    }

    fn runtime_initializer(&mut self, value: ExpressionId) -> bool {
        let root = self.ungrouped(value);
        let Ok(node) = self.inputs.parsed.ast().expressions().get(root) else {
            return false;
        };
        if self.span_has_control_transfer(node.span()) {
            return false;
        }
        match node.payload() {
            Expression::Name => self.reference(root).is_some_and(|symbol| {
                self.known.contains(&symbol)
                    && (self.view_parameter(symbol) || self.callables.contains(&symbol))
            }),
            Expression::Lambda { .. } => {
                self.inputs.owned.callable_origin(self.id(root)).is_some()
                    && self.lambda_formation(root)
            }
            Expression::Call { .. } => self.callable_value(root) && self.expression(value),
            _ => false,
        }
    }

    fn container_size(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.container_size(id) else {
            return false;
        };
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let container_ty = self.c(descriptor.container_type());
        let element_ty = self.c(descriptor.element_type());
        let result_ty = self.c(descriptor.result_type());
        let constructor = match descriptor.container() {
            SequentialContainerKind::Array => IntrinsicTypeConstructor::Array,
            SequentialContainerKind::List => IntrinsicTypeConstructor::List,
            SequentialContainerKind::MutableList => IntrinsicTypeConstructor::MutableList,
        };
        if descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.inputs.source_unit
            || descriptor.span() != node.span()
            || self.expression_type(expression) != Some(result_ty)
            || !matches!(
                self.kind(result_ty),
                Some(UnitTypeKind::Builtin(BuiltinType::Int))
            )
            || self.container_element(container_ty) != Some(element_ty)
            || self.expression_type_of(descriptor.receiver()) != Some(container_ty)
            || !matches!(
                self.kind(container_ty),
                Some(UnitTypeKind::Intrinsic { constructor: actual, arguments: args })
                    if *actual == constructor && args.as_slice() == [element_ty]
            )
        {
            return false;
        }
        let receiver = descriptor.receiver().expression();
        let ok = self
            .named_container_owner(receiver, container_ty, true)
            .is_some()
            && self
                .inputs
                .owned
                .loans()
                .iter()
                .any(|loan| loan.call() == id && loan.kind() == LoanKind::Shared);
        self.migrated |= ok;
        ok
    }

    fn container_index(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.element_place(id) else {
            return false;
        };
        let Some(container_ty) = self.expression_type_of(descriptor.receiver()) else {
            return false;
        };
        let element_ty = self.c(descriptor.element_type());
        if descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.inputs.source_unit
            || descriptor.index().source_unit() != self.inputs.source_unit
            || self.container_element(container_ty) != Some(element_ty)
            || self.expression_type(expression) != Some(element_ty)
            || self.copyability_of(element_ty) != Copyability::Copyable
            || self
                .named_container_owner(descriptor.receiver().expression(), container_ty, true)
                .is_none()
        {
            return false;
        }
        let index = descriptor.index().expression();
        let ok = self.is_int_index(index) && self.expression(index);
        self.migrated |= ok;
        ok
    }

    /// `f(xs[i])` where `f` borrows a MoveOnly element from a known local or Borrow container.
    pub(super) fn borrowed_container_element(
        &mut self,
        call: ExpressionId,
        value: ExpressionId,
        parameter: UnitTypeId,
    ) -> bool {
        let root = self.ungrouped(value);
        let id = self.id(root);
        let Some(descriptor) = self.inputs.typed.element_place(id) else {
            return false;
        };
        let Some(container_ty) = self.expression_type_of(descriptor.receiver()) else {
            return false;
        };
        let element_ty = self.c(descriptor.element_type());
        if descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.inputs.source_unit
            || descriptor.index().source_unit() != self.inputs.source_unit
            || element_ty != parameter
            || self.container_element(container_ty) != Some(element_ty)
            || self.expression_type_of(id) != Some(element_ty)
        {
            return false;
        }
        let Some(symbol) =
            self.named_container_owner(descriptor.receiver().expression(), container_ty, true)
        else {
            return false;
        };
        let loan_ok = self.inputs.owned.loans().iter().any(|loan| {
            loan.call() == self.id(call)
                && (loan.argument() == self.id(value) || loan.argument() == id)
                && loan.kind() == LoanKind::Shared
                && matches!(
                    loan.target(),
                    UnitLoanTarget::Place(place)
                        if place.root() == symbol
                            && place.fields().is_empty()
                            && place.element().is_some()
                )
        });
        let index = descriptor.index().expression();
        let ok = loan_ok && self.is_int_index(index) && self.expression(index);
        self.migrated |= ok;
        ok
    }

    /// Whether the assignment target is a container element (`xs[i] = v`).
    pub(super) fn is_element_target(&self, target: ExpressionId) -> bool {
        let root = self.ungrouped(target);
        self.inputs.typed.element_place(self.id(root)).is_some()
    }

    /// `xs[i] = v` on an owned local mutable container (`Array<T>` or `MutableList<T>`).
    pub(super) fn element_assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
        value: ExpressionId,
    ) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(assignment) else {
            return false;
        };
        if self.span_has_control_transfer(node.span()) {
            return false;
        }
        let assignment_id = self.id(assignment);
        let root_target = self.ungrouped(target);
        let target_id = self.id(root_target);
        let Some(place_desc) = self.inputs.typed.element_place(target_id) else {
            return false;
        };
        let Some(container_ty) = self.expression_type_of(place_desc.receiver()) else {
            return false;
        };
        let element_ty = self.c(place_desc.element_type());
        if !place_desc.is_mutable()
            || place_desc.receiver().source_unit() != self.inputs.source_unit
            || place_desc.index().source_unit() != self.inputs.source_unit
            || self.container_element(container_ty) != Some(element_ty)
            || self
                .named_container_owner(place_desc.receiver().expression(), container_ty, false)
                .is_none()
        {
            return false;
        }
        let facts: Vec<_> = self
            .inputs
            .owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == UnitDropPoint::AfterReplacement(assignment_id))
            .collect();
        let drop_ok = if self.copyability_of(element_ty) == Copyability::MoveOnly {
            matches!(
                facts.as_slice(),
                [fact] if fact.target() == UnitDropTarget::ReplacedElement(assignment_id)
            )
        } else {
            facts.is_empty()
        };
        let index = place_desc.index().expression();
        if !drop_ok
            || !self.is_int_index(index)
            || !self.expression(index)
            || !self.adaptable_element(value, element_ty)
            || !self.expression(value)
        {
            return false;
        }
        self.element_replacements.insert(assignment_id);
        self.migrated = true;
        true
    }

    fn container_append(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.container_append(id) else {
            return false;
        };
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let container_ty = self.c(descriptor.container_type());
        let element_ty = self.c(descriptor.element_type());
        if descriptor.expression() != id
            || descriptor.element().source_unit() != self.inputs.source_unit
            || self.span_has_control_transfer(node.span())
            || !self.mutable_list_owner(descriptor.receiver(), container_ty, element_ty)
        {
            return false;
        }
        let element = descriptor.element().expression();
        let ok = self.adaptable_element(element, element_ty) && self.expression(element);
        self.migrated |= ok;
        ok
    }

    fn container_clear(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.container_clear(id) else {
            return false;
        };
        let container_ty = self.c(descriptor.container_type());
        let element_ty = self.c(descriptor.element_type());
        let ok = descriptor.expression() == id
            && self.mutable_list_owner(descriptor.receiver(), container_ty, element_ty);
        self.migrated |= ok;
        ok
    }

    fn container_remove_at(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.container_remove_at(id) else {
            return false;
        };
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let container_ty = self.c(descriptor.container_type());
        let element_ty = self.c(descriptor.element_type());
        if descriptor.expression() != id
            || descriptor.index().source_unit() != self.inputs.source_unit
            || self.expression_type(expression) != Some(element_ty)
            || self.span_has_control_transfer(node.span())
            || !self.mutable_list_owner(descriptor.receiver(), container_ty, element_ty)
        {
            return false;
        }
        let index = descriptor.index().expression();
        let ok = self.is_int_index(index) && self.expression(index);
        self.migrated |= ok;
        ok
    }

    fn container_remove_first(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.container_remove_first(id) else {
            return false;
        };
        let container_ty = self.c(descriptor.container_type());
        let element_ty = self.c(descriptor.element_type());
        let ok = descriptor.expression() == id
            && self.expression_type(expression) == Some(element_ty)
            && self.mutable_list_owner(descriptor.receiver(), container_ty, element_ty);
        self.migrated |= ok;
        ok
    }

    fn container_remove_last(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.container_remove_last(id) else {
            return false;
        };
        let container_ty = self.c(descriptor.container_type());
        let element_ty = self.c(descriptor.element_type());
        let ok = descriptor.expression() == id
            && self.expression_type(expression) == Some(element_ty)
            && self.mutable_list_owner(descriptor.receiver(), container_ty, element_ty);
        self.migrated |= ok;
        ok
    }

    fn container_insert_at(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.container_insert_at(id) else {
            return false;
        };
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let container_ty = self.c(descriptor.container_type());
        let element_ty = self.c(descriptor.element_type());
        if descriptor.expression() != id
            || descriptor.index().source_unit() != self.inputs.source_unit
            || descriptor.element().source_unit() != self.inputs.source_unit
            || self.span_has_control_transfer(node.span())
            || !self.mutable_list_owner(descriptor.receiver(), container_ty, element_ty)
        {
            return false;
        }
        let index = descriptor.index().expression();
        let element = descriptor.element().expression();
        let ok = self.is_int_index(index)
            && self.expression(index)
            && self.adaptable_element(element, element_ty)
            && self.expression(element);
        self.migrated |= ok;
        ok
    }

    fn mutable_list_owner(
        &self,
        receiver: lang_frontend::type_checking::UnitExpressionId,
        container_ty: UnitTypeId,
        element_ty: UnitTypeId,
    ) -> bool {
        receiver.source_unit() == self.inputs.source_unit
            && self.container_element(container_ty) == Some(element_ty)
            && matches!(
                self.kind(container_ty),
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::MutableList,
                    arguments,
                }) if arguments.as_slice() == [element_ty]
            )
            && self
                .named_container_owner(receiver.expression(), container_ty, false)
                .is_some()
    }

    fn named_container_owner(
        &self,
        expression: ExpressionId,
        container_ty: UnitTypeId,
        allow_borrow: bool,
    ) -> Option<UnitSymbolId> {
        let root = self.ungrouped(expression);
        let node = self.inputs.parsed.ast().expressions().get(root).ok()?;
        let Expression::Name = node.payload() else {
            return None;
        };
        let id = self.id(root);
        if self.inputs.typed.non_null_use(id).is_some()
            || self
                .inputs
                .constant_owned
                .is_some_and(|owned| owned.materialization_at(id).is_some())
        {
            return None;
        }
        let symbol = self.reference(root)?;
        if !self.known.contains(&symbol)
            || self.callables.contains(&symbol)
            || self.symbol_type(symbol) != Some(container_ty)
            || self.expression_type_of(id) != Some(container_ty)
        {
            return None;
        }
        let mode_ok = match self.parameter_mode(symbol) {
            None | Some(ParameterMode::Value) => true,
            Some(ParameterMode::Borrow) => allow_borrow,
            Some(ParameterMode::Inout) => false,
        };
        mode_ok.then_some(symbol)
    }

    fn is_int_index(&self, expression: ExpressionId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        !self.span_has_control_transfer(node.span())
            && self.expression_type(expression).is_some_and(|ty| {
                matches!(self.kind(ty), Some(UnitTypeKind::Builtin(BuiltinType::Int)))
            })
    }

    fn adaptable_element(&self, value: ExpressionId, expected: UnitTypeId) -> bool {
        self.expression_type(value).is_some_and(|actual| {
            actual == expected
                || matches!(
                    self.kind(expected),
                    Some(UnitTypeKind::Nullable(inner)) if *inner == actual
                )
        })
    }

    /// `for (x in xs)`, `for (_ in xs)`, and `for ((a, b) in xs)` over a named sequential container.
    pub(super) fn sequential_for(
        &mut self,
        statement: lang_frontend::ast::StatementId,
        source: ExpressionId,
        body: lang_frontend::ast::StatementId,
    ) -> bool {
        use lang_frontend::{
            ownership_checking::{UnitIterationCleanupAction as Action, UnitIterationSourceAccess},
            type_checking::{IterationProvider, UnitSequentialIterationBinding, UnitStatementId},
        };
        let statement_id = UnitStatementId::new(self.inputs.source_unit, statement);
        if self.inputs.typed.map_iteration(statement_id).is_some() || self.scope_has_while_loop() {
            return false;
        }
        let Some(plan) = self.inputs.owned.iteration(statement_id) else {
            return false;
        };
        let descriptor = plan.descriptor();
        let source_id = self.id(source);
        let Some(UnitLoanTarget::Place(place)) = plan.source() else {
            return false;
        };
        if self.inputs.typed.sequential_iteration(statement_id) != Some(descriptor)
            || !matches!(descriptor.provider(), IterationProvider::Sequential(_))
            || descriptor.statement() != statement_id
            || descriptor.source() != source_id
            || !place.is_root()
            || !matches!(
                plan.source_access(),
                UnitIterationSourceAccess::Owned | UnitIterationSourceAccess::Shared
            )
        {
            return false;
        }
        let container_ty = self.c(descriptor.source_type());
        let element_ty = self.c(descriptor.element_type());
        if self.container_element(container_ty) != Some(element_ty)
            || self.expression_type(source) != Some(container_ty)
            || self.named_container_owner(source, container_ty, true) != Some(place.root())
            || !plan.exits().iter().all(|exit| {
                exit.actions().iter().all(|action| {
                    matches!(
                        action,
                        Action::Drop(_)
                            | Action::EndBinding { .. }
                            | Action::EndElement(_)
                            | Action::FinishProvider(_)
                            | Action::EndSource(_)
                    )
                })
            })
        {
            return false;
        }
        match descriptor.binding() {
            UnitSequentialIterationBinding::Discard => {}
            UnitSequentialIterationBinding::Name(symbol) => {
                if self.symbol_type(*symbol) != Some(element_ty) || !self.storage_type(element_ty) {
                    return false;
                }
                self.note_nullable(element_ty);
                self.known.insert(*symbol);
                self.for_borrowed.insert(*symbol);
            }
            UnitSequentialIterationBinding::Destructure(parts) => {
                if !(self.value_class(element_ty) || self.move_value_class(element_ty)) {
                    return false;
                }
                for part in parts {
                    let part_ty = self.c(part.ty());
                    if !self.storage_type(part_ty) {
                        return false;
                    }
                    self.note_nullable(part_ty);
                    if let Some(symbol) = part.symbol() {
                        if self.symbol_type(symbol) != Some(part_ty) {
                            return false;
                        }
                        self.known.insert(symbol);
                        self.for_borrowed.insert(symbol);
                    }
                }
            }
        }
        let ok = self.loop_body(body);
        self.migrated |= ok;
        ok
    }

    fn scope_has_while_loop(&self) -> bool {
        let Some(scope) = self.inputs.scope_span() else {
            return true;
        };
        self.inputs
            .parsed
            .ast()
            .statements()
            .iter()
            .any(|(_, node)| {
                let span = node.span();
                scope.source_id() == span.source_id()
                    && scope.start() <= span.start()
                    && span.end() <= scope.end()
                    && matches!(
                        node.payload(),
                        lang_frontend::parser::Statement::While { .. }
                    )
            })
    }

    fn span_has_control_transfer(&self, span: Span) -> bool {
        self.inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, candidate)| {
                span.source_id() == candidate.span().source_id()
                    && span.start() <= candidate.span().start()
                    && candidate.span().end() <= span.end()
                    && matches!(
                        candidate.payload(),
                        Expression::Return { .. }
                            | Expression::Break { .. }
                            | Expression::Continue { .. }
                    )
            })
            .any(|(_, candidate)| {
                !self
                    .inputs
                    .parsed
                    .ast()
                    .expressions()
                    .iter()
                    .any(|(_, boundary)| {
                        let bspan = boundary.span();
                        matches!(boundary.payload(), Expression::Lambda { .. })
                            && bspan != candidate.span()
                            && span.source_id() == bspan.source_id()
                            && span.start() <= bspan.start()
                            && bspan.end() <= span.end()
                            && bspan.start() <= candidate.span().start()
                            && candidate.span().end() <= bspan.end()
                    })
            })
    }
}
