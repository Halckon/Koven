//! Subset rules for `Map<K, V>` and `MutableMap<K, V>`: construction (`mapOf`, `mutableMapOf`),
//! `.size`, `.contains(key)`, `.get(key)` / `m[key]`, `.put(key, value)` / `m[key] = value`,
//! `.remove(key)`, `.consume()`, `.withValue(key, action)`, and `$map.result` comparisons.
use std::collections::BTreeSet;

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget},
    parser::{BinaryOperator, Expression},
    type_checking::{
        BuiltinType, Copyability, IntrinsicTypeConstructor, UnitCallReceiverOrigin, UnitTypeId,
        UnitTypeKind,
    },
};

use super::Checker;

impl Checker<'_, '_> {
    /// A concrete `Map<K, V>` or `MutableMap<K, V>` with scalar or `String` keys and any
    /// non-recursive supported storage value `V` (nullable `V` must be a pointer-like handle).
    pub(super) fn map_kv(&self, ty: UnitTypeId) -> Option<(UnitTypeId, UnitTypeId)> {
        self.map_kv_seen(ty, &mut BTreeSet::new())
    }

    pub(super) fn map_kv_seen(
        &self,
        ty: UnitTypeId,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> Option<(UnitTypeId, UnitTypeId)> {
        let Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Map | IntrinsicTypeConstructor::MutableMap,
            arguments,
        }) = self.kind(ty)
        else {
            return None;
        };
        let [key, value] = arguments.as_slice() else {
            return None;
        };
        if !(self.scalar(*key)
            || self.string(*key)
            || matches!(
                self.kind(*key),
                Some(UnitTypeKind::Builtin(BuiltinType::Char))
            ))
            || !active.insert(ty)
        {
            return None;
        }
        let nullable_ok = match self.kind(*value) {
            Some(UnitTypeKind::Nullable(inner)) => {
                self.class_seen(*inner, active)
                    || self.box_payload_seen(*inner, active).is_some()
                    || self.rc_payload_seen(*inner, active).is_some()
            }
            _ => true,
        };
        let ok = nullable_ok && self.storage_type_seen(*value, active);
        active.remove(&ty);
        ok.then_some((*key, *value))
    }

    /// Whether any map typed fact claims `expression`.
    pub(super) fn has_map_fact(&self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        typed.map_construction(id).is_some()
            || typed.map_size(id).is_some()
            || typed.map_contains(id).is_some()
            || typed.map_put(id).is_some()
            || typed.map_consume(id).is_some()
            || typed.map_get(id).is_some()
            || typed.map_remove(id).is_some()
            || typed.map_with_value(id).is_some()
    }

    pub(super) fn map_operation(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        if typed.map_construction(id).is_some() {
            return self.map_construction(expression);
        }
        if typed.map_size(id).is_some() {
            return self.map_size(expression);
        }
        if typed.map_contains(id).is_some() {
            return self.map_contains(expression);
        }
        if typed.map_put(id).is_some() {
            return self.map_put(expression);
        }
        if typed.map_consume(id).is_some() {
            return self.map_consume(expression);
        }
        if typed.map_get(id).is_some() {
            return self.map_get(expression);
        }
        if typed.map_remove(id).is_some() {
            return self.map_remove(expression);
        }
        if typed.map_with_value(id).is_some() {
            return self.map_with_value(expression);
        }
        false
    }

    /// Whether the assignment target is a `MutableMap` subscript (`m[key] = value`).
    pub(super) fn is_map_put_target(&self, target: ExpressionId) -> bool {
        self.inputs.typed.map_put(self.id(target)).is_some()
    }

    pub(super) fn named_map_owner(
        &self,
        expression: ExpressionId,
        map_ty: UnitTypeId,
        allow_borrow: bool,
    ) -> Option<lang_frontend::name_resolution::UnitSymbolId> {
        if let Some(symbol) = self.named_container_owner(expression, map_ty, allow_borrow) {
            return Some(symbol);
        }
        if !allow_borrow {
            return None;
        }
        let root = self.ungrouped(expression);
        let node = self.inputs.parsed.ast().expressions().get(root).ok()?;
        if !matches!(node.payload(), Expression::Name) {
            return None;
        }
        let id = self.id(root);
        let fact = self.inputs.typed.non_null_use(id)?;
        let symbol = self.reference(root)?;
        (symbol == fact.symbol()
            && self.c(fact.narrowed_type()) == map_ty
            && self.known.contains(&symbol)
            && self.view_symbols.contains(&symbol))
        .then_some(symbol)
    }

    pub(super) fn map_put_assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
    ) -> bool {
        let Ok(_) = self.inputs.parsed.ast().expressions().get(assignment) else {
            return false;
        };
        self.map_put(target)
    }

    fn map_construction(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.map_construction(id) else {
            return false;
        };
        let map_ty = self.c(descriptor.map_type());
        let Some((key_ty, value_ty)) = self.map_kv(map_ty) else {
            return false;
        };
        let ok = descriptor.expression() == id
            && self.expression_type(expression) == Some(map_ty)
            && self.c(descriptor.key_type()) == key_ty
            && self.c(descriptor.value_type()) == value_ty;
        self.migrated |= ok;
        ok
    }

    fn map_size(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.map_size(id) else {
            return false;
        };
        let map_ty = self.c(descriptor.map_type());
        let recv_expr = descriptor.receiver().expression();
        let ok = descriptor.expression() == id
            && descriptor.receiver().source_unit() == self.inputs.source_unit
            && self.map_kv(map_ty).is_some()
            && self.expression_type(expression).is_some_and(|ty| {
                matches!(self.kind(ty), Some(UnitTypeKind::Builtin(BuiltinType::Int)))
            })
            && (self.named_map_owner(recv_expr, map_ty, true).is_some()
                || (self.expression_type_of(descriptor.receiver()) == Some(map_ty)
                    && self.expression(recv_expr)));
        self.migrated |= ok;
        ok
    }

    fn map_contains(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.map_contains(id) else {
            return false;
        };
        let Some(map_ty) = self.expression_type_of(descriptor.receiver()) else {
            return false;
        };
        let Some((key_ty, _)) = self.map_kv(map_ty) else {
            return false;
        };
        let recv_expr = descriptor.receiver().expression();
        if descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.inputs.source_unit
            || descriptor.key().source_unit() != self.inputs.source_unit
            || self.c(descriptor.key_type()) != key_ty
            || !(self.named_map_owner(recv_expr, map_ty, true).is_some()
                || (self.expression_type_of(descriptor.receiver()) == Some(map_ty)
                    && self.expression(recv_expr)))
        {
            return false;
        }
        let key = descriptor.key().expression();
        let ok = self.key_or_nothing(key, key_ty)
            && (self.borrowed_string_name(key) || self.expression(key));
        self.migrated |= ok;
        ok
    }

    pub(super) fn map_put(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.map_put(id) else {
            return false;
        };
        let Some(map_ty) = self.expression_type_of(descriptor.receiver()) else {
            return false;
        };
        let Some((key_ty, value_ty)) = self.map_kv(map_ty) else {
            return false;
        };
        if descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.inputs.source_unit
            || descriptor.key().source_unit() != self.inputs.source_unit
            || descriptor.value().source_unit() != self.inputs.source_unit
            || self.c(descriptor.key_type()) != key_ty
            || self.c(descriptor.value_type()) != value_ty
            || !matches!(
                self.kind(map_ty),
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::MutableMap,
                    ..
                })
            )
            || self
                .named_container_owner(descriptor.receiver().expression(), map_ty, false)
                .is_none()
        {
            return false;
        }
        let key = descriptor.key().expression();
        let value = descriptor.value().expression();
        let key_diverges = self.expression_type(key).is_some_and(|ty| self.nothing(ty));
        let ok = self.key_or_nothing(key, key_ty)
            && self.expression(key)
            && (key_diverges
                || ((self.adaptable_element(value, value_ty)
                    || self
                        .expression_type(value)
                        .is_some_and(|ty| self.nothing(ty)))
                    && self.expression(value)));
        self.migrated |= ok;
        ok
    }

    fn key_or_nothing(&self, key: ExpressionId, key_ty: UnitTypeId) -> bool {
        self.expression_type(key)
            .is_some_and(|ty| ty == key_ty || self.nothing(ty))
    }

    fn map_consume(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.map_consume(id).copied() else {
            return false;
        };
        let Some((root, storage)) = descriptor.source() else {
            return false;
        };
        let recv_ty = self.c(descriptor.receiver_type());
        let res_ty = self.c(descriptor.result_type());
        let storage_ty = self.c(storage);
        let Some((key_ty, value_ty)) = self.map_kv(recv_ty) else {
            return false;
        };
        let receiver_expr = descriptor.receiver().expression();
        let is_nullable_storage = matches!(
            self.kind(storage_ty),
            Some(UnitTypeKind::Nullable(inner)) if *inner == recv_ty
        );
        let owner_ok = if storage == descriptor.receiver_type() {
            self.named_container_owner(receiver_expr, recv_ty, false) == Some(root)
        } else {
            is_nullable_storage
                && self.named_map_owner(receiver_expr, recv_ty, true) == Some(root)
                && self.parameter_mode(root).is_none()
        };
        let ok = descriptor.expression() == id
            && descriptor.receiver().source_unit() == self.inputs.source_unit
            && root.source_unit() == self.inputs.source_unit
            && owner_ok
            && self.expression_type(expression) == Some(res_ty)
            && self.c(descriptor.key_type()) == key_ty
            && self.c(descriptor.value_type()) == value_ty
            && self.map_kv(res_ty) == Some((key_ty, value_ty))
            && matches!(
                self.kind(recv_ty),
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::MutableMap,
                    ..
                })
            )
            && matches!(
                self.kind(res_ty),
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Map,
                    ..
                })
            )
            && self.inputs.owned.receiver_fact(id).is_some_and(|fact| {
                fact.call() == id
                    && fact.source() == UnitCallReceiverOrigin::Expression(descriptor.receiver())
                    && fact.receiver_type() == descriptor.receiver_type()
                    && fact.kind() == UnitReceiverOwnershipKind::Move
                    && matches!(
                        fact.target(),
                        UnitReceiverOwnershipTarget::Place(place)
                            if place.is_root() && place.root() == root
                    )
            });
        if ok && is_nullable_storage {
            self.view_symbols.remove(&root);
        }
        self.migrated |= ok;
        ok
    }

    fn map_get(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.map_get(id) else {
            return false;
        };
        let Some(map_ty) = self.expression_type_of(descriptor.receiver()) else {
            return false;
        };
        let Some((key_ty, value_ty)) = self.map_kv(map_ty) else {
            return false;
        };
        let res_ty = self.c(descriptor.result_type());
        if descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.inputs.source_unit
            || descriptor.key().source_unit() != self.inputs.source_unit
            || self.c(descriptor.key_type()) != key_ty
            || self.c(descriptor.value_type()) != value_ty
            || self.expression_type(expression) != Some(res_ty)
            || self.copyability_of(value_ty) != Copyability::Copyable
            || matches!(self.kind(value_ty), Some(UnitTypeKind::Nullable(_)))
            || !matches!(
                self.kind(res_ty),
                Some(UnitTypeKind::Nullable(inner)) if *inner == value_ty
            )
            || !(self
                .named_map_owner(descriptor.receiver().expression(), map_ty, true)
                .is_some()
                || (self.expression_type_of(descriptor.receiver()) == Some(map_ty)
                    && self.expression(descriptor.receiver().expression())))
        {
            return false;
        }
        self.note_nullable(res_ty);
        let key = descriptor.key().expression();
        let ok = self.key_or_nothing(key, key_ty)
            && (self.borrowed_string_name(key) || self.expression(key));
        self.migrated |= ok;
        ok
    }

    fn map_remove(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.map_remove(id) else {
            return false;
        };
        let Some(map_ty) = self.expression_type_of(descriptor.receiver()) else {
            return false;
        };
        let Some((key_ty, value_ty)) = self.map_kv(map_ty) else {
            return false;
        };
        let res_ty = self.c(descriptor.result_type());
        if descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.inputs.source_unit
            || descriptor.key().source_unit() != self.inputs.source_unit
            || self.c(descriptor.key_type()) != key_ty
            || self.c(descriptor.value_type()) != value_ty
            || self.expression_type(expression) != Some(res_ty)
            || matches!(self.kind(value_ty), Some(UnitTypeKind::Nullable(_)))
            || !(self.scalar(value_ty)
                || self.range_type(value_ty)
                || self.value_class(value_ty)
                || self.view_inner(value_ty))
            || !matches!(
                self.kind(res_ty),
                Some(UnitTypeKind::Nullable(inner)) if *inner == value_ty
            )
            || !matches!(
                self.kind(map_ty),
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::MutableMap,
                    ..
                })
            )
            || self
                .named_container_owner(descriptor.receiver().expression(), map_ty, false)
                .is_none()
        {
            return false;
        }
        self.note_nullable(res_ty);
        let key = descriptor.key().expression();
        let ok = self.key_or_nothing(key, key_ty)
            && (self.borrowed_string_name(key) || self.expression(key));
        self.migrated |= ok;
        ok
    }

    fn map_with_value(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.map_with_value(id).copied() else {
            return false;
        };
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Some(map_ty) = self.expression_type_of(descriptor.receiver()) else {
            return false;
        };
        let Some((key_ty, _)) = self.map_kv(map_ty) else {
            return false;
        };
        if descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.inputs.source_unit
            || descriptor.key().source_unit() != self.inputs.source_unit
            || descriptor.action().source_unit() != self.inputs.source_unit
            || self.span_has_control_transfer(node.span())
            || self.c(descriptor.key_type()) != key_ty
            || !self.expression_type(expression).is_some_and(|ty| {
                matches!(
                    self.kind(ty),
                    Some(UnitTypeKind::Builtin(BuiltinType::Boolean))
                )
            })
            || self
                .named_map_owner(descriptor.receiver().expression(), map_ty, true)
                .is_none()
        {
            return false;
        }
        let key = descriptor.key().expression();
        let action = descriptor.action().expression();
        let ok = self.expression_type(key) == Some(key_ty)
            && (self.borrowed_string_name(key) || self.expression(key))
            && self.runtime_initializer(action);
        self.migrated |= ok;
        ok
    }

    /// Temporary null comparison (`m.get(k) == null`, `m.remove(k) != null`) or scalar
    /// `$map.result.tX` equality (`m[k] == value`, `removed == hit`).
    pub(super) fn nullable_equality(
        &mut self,
        expression: ExpressionId,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
    ) -> bool {
        if !matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual) {
            return false;
        }
        let operand = if crate::ssa::lowering_support::is_null_literal(self.inputs.parsed, right) {
            Some(left)
        } else if crate::ssa::lowering_support::is_null_literal(self.inputs.parsed, left) {
            Some(right)
        } else {
            None
        };
        if let Some(operand) = operand {
            let root = self.ungrouped(operand);
            let point = lang_frontend::ownership_checking::UnitDropPoint::AfterBinaryOperands(
                self.id(expression),
            );
            let target =
                lang_frontend::ownership_checking::UnitDropTarget::Temporary(self.id(root));
            let Some(ty) = self.expression_type(operand) else {
                return false;
            };
            let Some(UnitTypeKind::Nullable(inner)) = self.kind(ty) else {
                return false;
            };
            let inner = *inner;
            if !(self.scalar(inner)
                || self.range_type(inner)
                || self.value_class(inner)
                || self.view_inner(inner))
            {
                return false;
            }
            if self.copyability_of(inner) == Copyability::MoveOnly {
                let Ok(root_node) = self.inputs.parsed.ast().expressions().get(root) else {
                    return false;
                };
                let mut facts = self
                    .inputs
                    .owned
                    .drops()
                    .iter()
                    .filter(|fact| fact.point() == point && fact.target() == target);
                let (Some(fact), None) = (facts.next(), facts.next()) else {
                    return false;
                };
                if fact.value_origin() != root_node.span() {
                    return false;
                }
            }
            self.note_nullable(ty);
            let ok = self.expression(operand);
            self.migrated |= ok;
            return ok;
        }
        let (Some(left_ty), Some(right_ty)) =
            (self.expression_type(left), self.expression_type(right))
        else {
            return false;
        };
        let nullable_pair = match (self.kind(left_ty), self.kind(right_ty)) {
            (Some(UnitTypeKind::Nullable(li)), Some(UnitTypeKind::Nullable(ri))) if li == ri => {
                Some((left_ty, *li))
            }
            (Some(UnitTypeKind::Nullable(li)), _) if *li == right_ty => Some((left_ty, *li)),
            (_, Some(UnitTypeKind::Nullable(ri))) if *ri == left_ty => Some((right_ty, *ri)),
            _ => None,
        };
        let Some((nullable_ty, inner)) = nullable_pair else {
            return false;
        };
        if !self.scalar(inner) || !self.has_unit_map_result(nullable_ty) {
            return false;
        }
        self.note_nullable(nullable_ty);
        let ok = self.expression(left) && self.expression(right);
        self.migrated |= ok;
        ok
    }

    fn has_unit_map_result(&self, nullable_ty: UnitTypeId) -> bool {
        self.inputs
            .typed
            .map_gets()
            .iter()
            .map(|get| get.result_type())
            .chain(
                self.inputs
                    .typed
                    .map_removes()
                    .iter()
                    .map(|remove| remove.result_type()),
            )
            .any(|res| self.c(res) == nullable_ty)
    }

    pub(super) fn is_map_remove_symbol(
        &self,
        symbol: lang_frontend::name_resolution::UnitSymbolId,
    ) -> bool {
        let Some(symbol_span) = self
            .inputs
            .names
            .names()
            .source_units()
            .get(self.inputs.source_unit.index())
            .and_then(|source| {
                source
                    .resolution()
                    .symbols()
                    .iter()
                    .find(|candidate| candidate.id() == symbol.symbol())
                    .map(|candidate| candidate.span())
            })
        else {
            return false;
        };
        self.inputs
            .parsed
            .ast()
            .items()
            .iter()
            .any(|(_, item)| match item.payload() {
                lang_frontend::parser::Item::Variable {
                    name: lang_frontend::parser::NameMarker::Present(name_span),
                    initializer,
                    ..
                } if *name_span == symbol_span => {
                    let root = self.ungrouped(*initializer);
                    self.inputs.typed.map_remove(self.id(root)).is_some()
                }
                _ => false,
            })
    }

    pub(super) fn is_direct_statement_call(&self, call: ExpressionId) -> bool {
        let nodes = self.inputs.parsed.ast().expressions();
        let Ok(call_node) = nodes.get(call) else {
            return false;
        };
        let call_span = call_node.span();
        if nodes.iter().any(|(id, node)| {
            id != call
                && node.span().start() <= call_span.start()
                && call_span.end() <= node.span().end()
                && !matches!(
                    node.payload(),
                    Expression::Group { .. } | Expression::If { .. }
                )
        }) {
            return false;
        }
        self.inputs
            .parsed
            .ast()
            .statements()
            .iter()
            .any(|(_, statement)| match statement.payload() {
                lang_frontend::parser::Statement::Expression { expression } => {
                    self.ungrouped(*expression) == call
                }
                _ => false,
            })
    }

    /// `for ((k, v) in map)` over a named or temporary `Map<K, V>` / `MutableMap<K, V>`.
    pub(super) fn map_for(
        &mut self,
        statement: lang_frontend::ast::StatementId,
        source: ExpressionId,
        body: lang_frontend::ast::StatementId,
    ) -> bool {
        use lang_frontend::{
            ownership_checking::{
                IterationSource, UnitIterationCleanupAction as Action, UnitIterationSourceAccess,
                UnitLoanTarget,
            },
            type_checking::UnitStatementId,
        };
        let statement_id = UnitStatementId::new(self.inputs.source_unit, statement);
        let (Some(plan), Some(descriptor)) = (
            self.inputs.owned.map_iteration(statement_id),
            self.inputs.typed.map_iteration(statement_id),
        ) else {
            return false;
        };
        let source_id = self.id(source);
        if plan.descriptor() != descriptor
            || descriptor.statement() != statement_id
            || descriptor.source() != source_id
        {
            return false;
        }
        let container_ty = self.c(descriptor.source_type());
        let key_ty = self.c(descriptor.key_type());
        let value_ty = self.c(descriptor.value_type());
        if self.map_kv(container_ty) != Some((key_ty, value_ty))
            || self.expression_type(source) != Some(container_ty)
        {
            return false;
        }
        match (plan.source_access(), plan.source_kind()) {
            (
                UnitIterationSourceAccess::Owned,
                IterationSource::BorrowedSource(UnitLoanTarget::Place(place)),
            ) if place.is_root() => {
                if self.named_map_owner(source, container_ty, true) != Some(place.root()) {
                    return false;
                }
            }
            (
                UnitIterationSourceAccess::Shared,
                IterationSource::BorrowedSource(UnitLoanTarget::Place(place)),
            ) if place.is_root() => {
                let Some(symbol) = self.named_map_owner(source, container_ty, true) else {
                    return false;
                };
                let origin_matches = symbol == place.root()
                    || (self.for_borrowed.contains(&symbol)
                        && self
                            .inputs
                            .owned
                            .borrow_results()
                            .bindings()
                            .iter()
                            .any(|fact| {
                                fact.binding() == symbol
                                    && fact.origin() == &UnitLoanTarget::Place(place.clone())
                            }));
                if !origin_matches {
                    return false;
                }
            }
            (UnitIterationSourceAccess::Temporary, _) => {
                if !self.expression(source) {
                    return false;
                }
            }
            _ => return false,
        }
        if !plan.exits().iter().all(|exit| {
            exit.actions().iter().all(|action| {
                matches!(
                    action,
                    Action::Drop(_)
                        | Action::DropConditionalReceiver(_)
                        | Action::EndBinding { .. }
                        | Action::FinishProvider(_)
                        | Action::EndSource(_)
                        | Action::EndCallLoan(_)
                        | Action::EndReceiverLoan(_)
                )
            })
        }) {
            return false;
        }
        for (symbol, ty) in [
            (descriptor.key_binding(), key_ty),
            (descriptor.value_binding(), value_ty),
        ] {
            if self.symbol_type(symbol) != Some(ty) || !self.storage_type(ty) {
                return false;
            }
            self.note_nullable(ty);
            self.known.insert(symbol);
            self.for_borrowed.insert(symbol);
        }
        let ok = self.loop_body(body);
        self.migrated |= ok;
        ok
    }
}
