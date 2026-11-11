//! Deterministic precheck for the migrated subset (SPEC-0350 §3). It reads typed and
//! ownership facts plus AST construct kinds; it never runs lowering. Anything outside
//! the subset keeps the existing per-family path.
mod callable;
mod container;
mod control;
mod field;
mod heap;
mod inout;
use control::LoopRoute;
pub(super) use control::accepts;
mod map;
pub(super) mod named_route;
mod range_borrow;
mod rc;
mod receiver;
mod scalar_text;
mod tagged;
mod value;
mod when;

use std::collections::BTreeSet;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    parser::{
        AssignmentOperator, BinaryOperator, Expression, Item, LiteralKind, PrefixOperator,
        Statement,
    },
    type_checking::{
        BuiltinType, Copyability, NominalKind, ParameterMode, UnitCallTarget, UnitExpressionId,
        UnitNominalSignature, UnitTypeId, UnitTypeKind,
    },
};

use super::{RefineBody, RefineInputs};
use crate::ssa::{
    lowering_support::string_literal, unit_plan::resolve_nominal_runtime_field_types,
};

struct Checker<'i, 'a> {
    inputs: &'i RefineInputs<'a>,
    /// Parameters and locals declared so far, with whether they are `var`.
    known: BTreeSet<UnitSymbolId>,
    /// Local bindings holding a concrete callable owner (function pointer or closure).
    callables: BTreeSet<UnitSymbolId>,
    /// Expressions lowered in branch form, whose own drop points are never emitted.
    branch_form: BTreeSet<UnitExpressionId>,
    /// Temporary field parents whose pending loan and cleanup lower together.
    temporary_field_parents: BTreeSet<UnitExpressionId>,
    /// Pilot routing gate: the function touches a nullable value.
    nullable: bool,
    /// Enclosing `while`/`loop` statements; `break`/`continue` need one.
    loops: usize,
    /// Routing gate: the function uses a construct whose family moved to this layer.
    migrated: bool,
    /// A validated local destructuring in this callable requires refinement.
    has_local_destructuring: bool,
    /// A temporary sequential source can refine only in an already migrated scope.
    has_temporary_sequential_for: bool,
    loop_route: LoopRoute,
    loop_route_retry: bool,
    /// Field replacements in the subset; their `BeforeReplacement` drops are consumed by
    /// the replacement itself.
    replacements: BTreeSet<UnitExpressionId>,
    /// Container element replacements (`xs[i] = v`) whose `AfterReplacement` drops are
    /// consumed by `ContainerReplace`.
    element_replacements: BTreeSet<UnitExpressionId>,
    /// Element or component bindings introduced by a sequential `for` loop (`Borrow` loans).
    for_borrowed: BTreeSet<UnitSymbolId>,
    /// Local `borrow val` bindings holding a `RangeView` descriptor (`NewRangeDescriptor`).
    range_view_bindings: BTreeSet<UnitSymbolId>,
    /// MoveOnly nullable bindings whose non-null proof view is active in the current branch.
    view_symbols: BTreeSet<UnitSymbolId>,
}

fn accepts_with_loop_route(inputs: &RefineInputs<'_>, loop_route: LoopRoute) -> (bool, bool) {
    let mut known: BTreeSet<_> = inputs.parameters.iter().copied().collect();
    if let RefineBody::Lambda(thunk) = &inputs.body {
        known.extend(thunk.captures.iter().map(|capture| capture.symbol));
    }
    let mut checker = Checker {
        inputs,
        known,
        callables: BTreeSet::new(),
        branch_form: BTreeSet::new(),
        temporary_field_parents: BTreeSet::new(),
        nullable: false,
        loops: 0,
        migrated: false,
        has_local_destructuring: false,
        has_temporary_sequential_for: false,
        loop_route,
        loop_route_retry: false,
        replacements: BTreeSet::new(),
        element_replacements: BTreeSet::new(),
        for_borrowed: BTreeSet::new(),
        range_view_bindings: BTreeSet::new(),
        view_symbols: BTreeSet::new(),
    };
    let mut signature_types: Vec<_> = inputs
        .parameters
        .iter()
        .filter_map(|symbol| checker.symbol_type(*symbol))
        .chain([inputs.return_type])
        .chain(inputs.receiver.map(|receiver| receiver.ty))
        .collect();
    if let RefineBody::Lambda(thunk) = &inputs.body {
        signature_types.extend(thunk.captures.iter().map(|capture| capture.ty));
    }
    for ty in signature_types {
        checker.note_nullable(ty);
        checker.migrated |= checker.container_element(ty).is_some()
            || checker.range_view_element(ty).is_some()
            || checker.range_type(ty)
            || checker.box_payload(ty).is_some()
            || checker.enum_type(ty)
            || checker.box_enum_type(ty)
            || checker.map_kv(ty).is_some()
            || checker.rc_payload(ty).is_some()
            || checker.class_has_deinit(ty)
            || checker.move_value_class(ty);
    }
    if inputs.function_item.is_some_and(|item| {
        matches!(
            inputs
                .parsed
                .ast()
                .items()
                .get(item)
                .map(|node| node.payload()),
            Ok(Item::Deinit { .. })
        )
    }) {
        checker.migrated = true;
    }
    // Copyable Borrow parameters are read by copy, except String when subjects read the
    // loan; Inout parameters are Copyable types or plain classes, read through and replaced
    // under the caller's exclusive loan.
    let signature = inputs.parameters.len() == inputs.modes.len()
        && inputs
            .parameters
            .iter()
            .zip(&inputs.modes)
            .all(|(symbol, mode)| {
                checker.symbol_type(*symbol).is_some_and(|ty| {
                    // ADR-0034: a Borrow function parameter is a callable view.
                    (*mode == ParameterMode::Borrow && checker.view_function(ty))
                        || checker.storage_type(ty)
                            && match mode {
                                ParameterMode::Value => true,
                                // A MoveOnly Borrow parameter is only lent on (a plain class,
                                // container, or boxed owner through its loan), never read by copy.
                                ParameterMode::Borrow => {
                                    checker.copyability_of(ty) == Copyability::Copyable
                                        || checker.class(ty)
                                        || checker.value_class(ty)
                                        || checker.move_value_class(ty)
                                        || checker.string(ty)
                                        || checker.container_element(ty).is_some()
                                        || checker.range_view_element(ty).is_some()
                                        || checker.box_payload(ty).is_some()
                                        || checker.box_enum_type(ty)
                                        || checker.map_kv(ty).is_some()
                                        || checker.rc_payload(ty).is_some()
                                        || checker.nullable_storage(ty)
                                }
                                ParameterMode::Inout => {
                                    checker.copyability_of(ty) == Copyability::Copyable
                                        || checker.class(ty)
                                }
                            }
                })
            })
        && (checker.unit(inputs.return_type)
            || checker.nothing(inputs.return_type)
            || checker.storage_type(inputs.return_type)
            // A borrow-result function forwards a loan of a nullable owner unchanged.
            || (inputs.borrow_return.is_some() && checker.nullable_storage(inputs.return_type))
            || checker.callable_return())
        && inputs.receiver.is_none_or(|receiver| match receiver.mode {
            ParameterMode::Value | ParameterMode::Borrow => {
                checker.enum_type(receiver.ty)
                    || checker.box_enum_type(receiver.ty)
                    || checker.class(receiver.ty)
                    || checker.value_class(receiver.ty)
                    || checker.move_value_class(receiver.ty)
                    || checker.object_type(receiver.ty)
            }
            ParameterMode::Inout => {
                checker.class(receiver.ty)
                    || checker.value_class(receiver.ty)
                    || checker.move_value_class(receiver.ty)
                    || checker.enum_type(receiver.ty)
                    || checker.box_enum_type(receiver.ty)
            }
        });
    // An Inout parameter or receiver, a MoveOnly Value receiver, or a StaticSelf/object
    // specialization belongs to the migrated subset.
    checker.migrated |= checker.nothing(inputs.return_type)
        || inputs.modes.contains(&ParameterMode::Inout)
        || inputs.static_self.is_some()
        || inputs.receiver.is_some_and(|receiver| {
            receiver.mode == ParameterMode::Inout
                || checker.object_type(receiver.ty)
                || (receiver.mode == ParameterMode::Value
                    && checker.copyability_of(receiver.ty) == Copyability::MoveOnly)
        });
    let body_ok = match &inputs.body {
        RefineBody::Expression(expression) => {
            if inputs.borrow_return.is_some() {
                checker.borrow_return_body(*expression, *expression)
            } else if inputs.carrier_return.is_some() {
                checker.carrier_return_body(*expression, *expression)
            } else {
                checker.expression(*expression)
            }
        }
        RefineBody::Block(statement) => checker.statement(*statement),
        RefineBody::Lambda(thunk) => checker.thunk_body(thunk),
    };
    // Check the temporary-source route after the body so statement order does not matter.
    let accepted = signature
        && body_ok
        && (loop_route == LoopRoute::Existing
            || checker.loop_route == LoopRoute::Verified && checker.has_local_destructuring)
        && (!checker.has_temporary_sequential_for
            || checker.has_local_destructuring
            || checker.scope_requires_refine())
        && checker.drops_supported()
        && (checker.nullable
            || checker.migrated
            || checker.bounded_receiver_route()
            || checker.bounded_named_call().is_some()
            || checker.bounded_temporary_call().is_some());
    (accepted, signature && checker.loop_route_retry)
}

/// Reuse the exact existing bounded shape without consulting observation products.
pub(super) fn bounded_receiver_call(inputs: &RefineInputs<'_>) -> Option<UnitExpressionId> {
    if !query_checker(inputs).bounded_receiver_route() {
        return None;
    }
    let RefineBody::Block(body) = inputs.body else {
        return None;
    };
    let Statement::Block { elements } = inputs.parsed.ast().statements().get(body).ok()?.payload()
    else {
        return None;
    };
    let [statement] = elements.as_slice() else {
        return None;
    };
    let Statement::Expression { expression } = inputs
        .parsed
        .ast()
        .statements()
        .get(*statement)
        .ok()?
        .payload()
    else {
        return None;
    };
    Some(UnitExpressionId::new(inputs.source_unit, *expression))
}

fn query_checker<'i, 'a>(inputs: &'i RefineInputs<'a>) -> Checker<'i, 'a> {
    Checker {
        inputs,
        known: BTreeSet::new(),
        callables: BTreeSet::new(),
        branch_form: BTreeSet::new(),
        temporary_field_parents: BTreeSet::new(),
        nullable: false,
        loops: 0,
        migrated: false,
        has_local_destructuring: false,
        has_temporary_sequential_for: false,
        loop_route: LoopRoute::Existing,
        loop_route_retry: false,
        replacements: BTreeSet::new(),
        element_replacements: BTreeSet::new(),
        for_borrowed: BTreeSet::new(),
        range_view_bindings: BTreeSet::new(),
        view_symbols: BTreeSet::new(),
    }
}

/// Every storage type an accepted body uses, plus nullable inners and Boolean flags.
pub(super) fn type_demands(inputs: &RefineInputs<'_>) -> Vec<UnitTypeId> {
    let checker = query_checker(inputs);
    let Some(item) = inputs.scope_span() else {
        return Vec::new();
    };
    let typed = inputs.typed;
    let mut demands: Vec<UnitTypeId> = inputs
        .parameters
        .iter()
        .filter_map(|symbol| checker.symbol_type(*symbol))
        .chain([inputs.return_type])
        .chain(inputs.receiver.map(|receiver| receiver.ty))
        .collect();
    if let RefineBody::Lambda(thunk) = &inputs.body {
        demands.extend(thunk.captures.iter().map(|capture| capture.ty));
    }
    for (expression, node) in inputs.parsed.ast().expressions().iter() {
        let span = node.span();
        if item.start() <= span.start() && span.end() <= item.end() {
            let id = UnitExpressionId::new(inputs.source_unit, expression);
            checker.demand_prefix_nullable_arguments(id, node.payload(), &mut demands);
            if let Some(ty) = inputs.typed.expression_type(id) {
                let ty = match checker.kind(ty) {
                    Some(UnitTypeKind::EnumCase { root, .. }) => *root,
                    _ => ty,
                };
                if !matches!(checker.kind(ty), Some(UnitTypeKind::Nullable(inner)) if checker.nothing(*inner))
                {
                    demands.extend(inputs.concrete(ty));
                }
            }
            if let Some(site) = inputs.source_plan.call_site(inputs.source_token, id) {
                for route in site.delegation() {
                    demands.push(route.outer_receiver());
                    demands.push(route.delegate_receiver());
                }
            }
            if let Some(prim) = inputs.typed.ownership_primitive(id) {
                demands.push(checker.c(prim.value_type()));
            }
        }
    }
    demands.extend(typed.body_symbol_types().keys().filter_map(|symbol| {
        (symbol.source_unit() == inputs.source_unit)
            .then(|| checker.symbol_type(*symbol))
            .flatten()
    }));
    demands.extend(typed.types().builtin(BuiltinType::Boolean));
    // Case tests compare the `Int` discriminant; numeric range `for` loops and container
    // operations use `Int` lengths/indices.
    if demands.iter().any(|ty| {
        checker.enum_type(*ty)
            || checker.box_enum_type(*ty)
            || checker.container_element(*ty).is_some()
            || checker.range_view_element(*ty).is_some()
            || checker.map_kv(*ty).is_some()
            || matches!(
                checker.kind(*ty),
                Some(UnitTypeKind::Builtin(BuiltinType::IntRange))
            )
    }) {
        demands.extend(typed.types().builtin(BuiltinType::Int));
    }
    if demands.iter().any(|ty| checker.map_kv(*ty).is_some()) {
        demands.extend(typed.types().builtin(BuiltinType::ULong));
    }
    if demands.iter().any(|ty| {
        matches!(
            checker.kind(*ty),
            Some(UnitTypeKind::Builtin(BuiltinType::LongRange))
        )
    }) {
        demands.extend(typed.types().builtin(BuiltinType::Long));
    }
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    for ty in demands {
        checker.push_demand(ty, &mut seen, &mut ordered);
    }
    ordered
}

const fn integer(builtin: BuiltinType) -> bool {
    matches!(
        builtin,
        BuiltinType::Byte
            | BuiltinType::Short
            | BuiltinType::Int
            | BuiltinType::Long
            | BuiltinType::UByte
            | BuiltinType::UShort
            | BuiltinType::UInt
            | BuiltinType::ULong
    )
}

impl Checker<'_, '_> {
    fn kind(&self, ty: UnitTypeId) -> Option<&UnitTypeKind> {
        self.inputs.typed.types().get(ty)
    }

    fn unit(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.kind(ty),
            Some(UnitTypeKind::Builtin(BuiltinType::Unit))
        )
    }

    pub(super) fn nothing(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.kind(ty),
            Some(UnitTypeKind::Builtin(BuiltinType::Nothing))
        )
    }

    fn scalar(&self, ty: UnitTypeId) -> bool {
        matches!(self.kind(ty), Some(UnitTypeKind::Builtin(b)) if *b == BuiltinType::Boolean || integer(*b))
    }

    fn string(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.kind(ty),
            Some(UnitTypeKind::Builtin(BuiltinType::String))
        )
    }

    pub(super) fn integer_type(&self, ty: UnitTypeId) -> bool {
        matches!(self.kind(ty), Some(UnitTypeKind::Builtin(b)) if integer(*b))
    }

    /// A concrete non-recursive `class` whose fields are supported storage types.
    fn class(&self, ty: UnitTypeId) -> bool {
        self.class_seen(ty, &mut BTreeSet::new())
    }

    /// Concrete field types of a `class`, in declaration order.
    fn class_fields(&self, ty: UnitTypeId) -> Option<Vec<UnitTypeId>> {
        let (nominal, arguments) = self.class_signature(ty)?;
        resolve_nominal_runtime_field_types(self.inputs.typed, ty, nominal, &arguments).ok()
    }

    fn class_signature(&self, ty: UnitTypeId) -> Option<(&UnitNominalSignature, Vec<UnitTypeId>)> {
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.kind(ty)
        else {
            return None;
        };
        let nominal = self
            .inputs
            .typed
            .signatures()
            .declaration(*declaration)?
            .nominal()?;
        (nominal.kind() == NominalKind::Class).then(|| (nominal, arguments.clone()))
    }

    /// MoveOnly nullable inners are read through proof views.
    fn view_inner(&self, ty: UnitTypeId) -> bool {
        self.view_inner_seen(ty, &mut BTreeSet::new())
    }

    /// Types a slot may hold: scalars, String, plain classes and their nullables.
    fn storage_type(&self, ty: UnitTypeId) -> bool {
        self.storage_type_seen(ty, &mut BTreeSet::new())
    }

    /// Records whether the function handles a nullable value at all.
    fn note_nullable(&mut self, ty: UnitTypeId) {
        if matches!(self.kind(ty), Some(UnitTypeKind::Nullable(_))) {
            self.nullable = true;
        }
    }

    fn expression_type(&self, expression: ExpressionId) -> Option<UnitTypeId> {
        self.expression_type_of(self.id(expression))
    }

    /// Typed facts carry template types; a generic instance reads them concretely.
    fn expression_type_of(&self, id: UnitExpressionId) -> Option<UnitTypeId> {
        self.inputs
            .typed
            .expression_type(id)
            .and_then(|ty| self.inputs.concrete(ty))
    }

    fn symbol_type(&self, symbol: UnitSymbolId) -> Option<UnitTypeId> {
        self.inputs
            .typed
            .symbol_type(symbol)
            .and_then(|ty| self.inputs.concrete(ty))
    }

    /// The concrete identity of a type carried by a typed descriptor; unresolvable types
    /// stay as they are and match no concrete rule.
    fn c(&self, ty: UnitTypeId) -> UnitTypeId {
        self.inputs.concrete(ty).unwrap_or(ty)
    }

    fn copyability_of(&self, ty: UnitTypeId) -> Copyability {
        self.inputs.typed.copyability(self.c(ty))
    }

    fn id(&self, expression: ExpressionId) -> UnitExpressionId {
        UnitExpressionId::new(self.inputs.source_unit, expression)
    }

    /// No specialized typed fact may claim the expression.
    fn plain(&self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        typed.construction(id).is_none()
            && typed.aggregate_projection(id).is_none()
            && typed.non_null_assertion(id).is_none()
            && typed.ownership_primitive(id).is_none()
            && self
                .inputs
                .constant_owned
                .is_none_or(|owned| owned.materialization_at(id).is_none())
            && self.unspecialized(expression)
    }

    /// No typed fact outside the class/assertion families claims the expression.
    fn unspecialized(&self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        typed.integer_operation(id).is_none()
            && typed.integer_range_operation(id).is_none()
            && typed.string_operation(id).is_none()
            && typed.rc_operation(id).is_none()
            && typed.range_size(id).is_none()
            && typed.nullable_when(id).is_none()
            && !self.has_container_fact(expression)
    }

    fn reference(&self, expression: ExpressionId) -> Option<UnitSymbolId> {
        let span = self
            .inputs
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .ok()?
            .span();
        self.inputs
            .references
            .get(&(span.start(), span.end()))
            .copied()
    }

    fn expression(&mut self, expression: ExpressionId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Some(ty) = self.expression_type(expression) else {
            return false;
        };
        if let Expression::When {
            subject: Some(subject),
            ..
        } = *node.payload()
            && self.inputs.typed.expression_falls_through(self.id(subject)) == Some(false)
        {
            self.migrated = true;
            return self.expression(subject);
        }
        let id = self.id(expression);
        if matches!(node.payload(), Expression::Call { .. })
            && let Some(receiver) =
                crate::ssa::lowering_support::non_completing_call_receiver(self.inputs.typed, id)
        {
            self.migrated = true;
            return receiver.source_unit() == self.inputs.source_unit
                && self.expression(receiver.expression());
        }
        // Jumps and abort calls are typed `Nothing`. An if or group can carry that
        // divergence; the inner expression still has to pass its own admission.
        let returns = (matches!(
            node.payload(),
            Expression::Return { .. }
                | Expression::Break { .. }
                | Expression::Continue { .. }
                | Expression::If { .. }
                | Expression::Group { .. }
        ) || matches!(node.payload(), Expression::When { .. })
            && self.inputs.typed.expression_falls_through(id) == Some(false)
            || self.inputs.typed.call(id).is_some_and(|desc| {
                desc.aborts()
                    || self.named_nothing_call(desc)
                    || desc.target() == UnitCallTarget::FunctionValue
            }))
            && matches!(
                self.kind(ty),
                Some(UnitTypeKind::Builtin(BuiltinType::Nothing))
            );
        if let Expression::Lambda { .. } = node.payload() {
            return self.returned_lambda(expression);
        }
        if self.inputs.typed.constants().is_some_and(|facts| {
            facts
                .uses()
                .binary_search_by_key(&id, |use_| use_.expression())
                .is_ok()
        }) {
            return self.storage_type(ty) && self.constant_materialization(expression);
        }
        if self.inputs.typed.ownership_primitive(id).is_some() {
            return (self.unit(ty) || self.storage_type(ty) || self.nothing(ty))
                && self.ownership_primitive(expression);
        }
        if self.has_container_fact(expression) {
            return (self.unit(ty) || self.storage_type(ty))
                && self.container_operation(expression);
        }
        // `Rc`, integer, integer-range, and String operations carry their own typed facts.
        if self.inputs.typed.rc_operation(id).is_some() {
            return self.storage_type(ty) && self.rc_operation(expression);
        }
        if self.inputs.typed.integer_operation(id).is_some() {
            return self.storage_type(ty) && self.integer_operation(expression);
        }
        if self.inputs.typed.integer_range_operation(id).is_some() {
            return self.storage_type(ty) && self.integer_range_operation(expression);
        }
        if self.inputs.typed.string_operation(id).is_some() {
            return self.storage_type(ty) && self.string_operation(expression);
        }
        if self.inputs.typed.range_size(id).is_some() {
            return self.storage_type(ty) && self.range_size(expression);
        }
        if !self.unspecialized(expression)
            || !(returns
                || self.unit(ty)
                || self.storage_type(ty)
                || self.callable_value(expression))
        {
            return false;
        }
        self.note_nullable(ty);
        if let Some(descriptor) = self.inputs.typed.construction(id) {
            match descriptor.target() {
                lang_frontend::type_checking::UnitConstructionTarget::EnumCase(_) => {
                    return self.enum_construction(expression);
                }
                lang_frontend::type_checking::UnitConstructionTarget::IntrinsicBox => {
                    return self.box_construction(expression);
                }
                lang_frontend::type_checking::UnitConstructionTarget::IntrinsicRc => {
                    return self.rc_construction(expression);
                }
                _ if self.value_class(self.c(descriptor.result_type()))
                    || self.move_value_class(self.c(descriptor.result_type())) =>
                {
                    return self.value_construction(expression);
                }
                _ => {}
            }
            return self.construction(expression);
        }
        if self.inputs.typed.aggregate_projection(id).is_some() {
            return self.projection(expression)
                || self.enum_projection(expression)
                || self.value_projection(expression);
        }
        if self.inputs.typed.non_null_assertion(id).is_some() {
            return self.non_null_assertion(expression);
        }
        match node.payload().clone() {
            Expression::Group { expression } => self.expression(expression),
            Expression::Literal(LiteralKind::Integer(_)) => self.integer_type(ty),
            Expression::Literal(LiteralKind::Boolean(_) | LiteralKind::Null) => true,
            Expression::String { .. } => self
                .inputs
                .sources
                .source_text(self.inputs.parsed.source_id())
                .ok()
                .and_then(|text| {
                    string_literal::decode_plain(self.inputs.parsed, text, expression).ok()
                })
                .flatten()
                .is_some(),
            Expression::Name => self.name(expression),
            Expression::This => self.this_expression(expression),
            Expression::TypeTest {
                expression: operand,
                type_ref,
                ..
            } => self.type_test_expression(operand, type_ref),
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => self.expression(operand),
            Expression::Prefix {
                operator: operator @ (PrefixOperator::Plus | PrefixOperator::Minus),
                operand,
                ..
            } => {
                self.integer_type(ty)
                    && self.expression_type(operand) == Some(ty)
                    && self.prefix_integer(operator, operand)
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => self.binary(expression, left, operator, right),
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let saved = self.view_symbols.clone();
                let cond_ok = self.condition(condition, true);
                let (reaches_then, reaches_else) = self.condition_outcomes(condition);
                let then_ok = cond_ok && (!reaches_then || self.statement(then_branch));
                let then_views = self.view_symbols.clone();
                self.view_symbols = saved.clone();
                self.note_else_view(condition);
                let else_ok =
                    !reaches_else || else_branch.is_none_or(|branch| self.statement(branch));
                let else_views = self.view_symbols.clone();
                let then_continues = reaches_then && self.statement_may_continue(then_branch);
                let else_continues = reaches_else
                    && else_branch.is_none_or(|branch| self.statement_may_continue(branch));
                self.view_symbols = saved
                    .into_iter()
                    .filter(|symbol| {
                        (!then_continues || then_views.contains(symbol))
                            && (!else_continues || else_views.contains(symbol))
                    })
                    .collect();
                then_ok
                    && else_ok
                    && (self.unit(ty) || else_branch.is_some())
                    && (!returns || else_branch.is_some())
            }
            Expression::When {
                subject: None,
                entries,
                ..
            } => self.subjectless_when(ty, &entries),
            Expression::When {
                subject: Some(subject),
                entries,
                ..
            } => self.subject_when(expression, ty, subject, &entries),
            Expression::Call { arguments, .. } => {
                let values: Vec<_> = arguments.iter().map(|argument| argument.value).collect();
                let function_value = self
                    .inputs
                    .typed
                    .call(self.id(expression))
                    .is_some_and(|descriptor| descriptor.target() == UnitCallTarget::FunctionValue);
                self.call(expression, &values)
                    && (!function_value || self.function_callee(expression, &values))
                    && self.arguments(expression, &values)
            }
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } if self.is_field_target(target) => self.field_assignment(expression, target, value),
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } if self.is_element_target(target) => {
                self.element_assignment(expression, target, value)
            }
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                ..
            } if self.is_map_put_target(target) => self.map_put_assignment(expression, target),
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                value,
                ..
            } => self.assignment(expression, target, value),
            Expression::Assignment {
                target,
                operator:
                    operator @ (AssignmentOperator::AddAssign
                    | AssignmentOperator::SubtractAssign
                    | AssignmentOperator::MultiplyAssign
                    | AssignmentOperator::DivideAssign
                    | AssignmentOperator::RemainderAssign),
                value,
                ..
            } => {
                if self.is_element_target(target) {
                    self.element_compound_assignment(expression, target, operator, value)
                } else {
                    self.compound_assignment(target, operator, value)
                }
            }
            Expression::Return { value, .. } => {
                if self.inputs.borrow_return.is_some() {
                    value.is_some_and(|value| self.borrow_return_body(expression, value))
                } else if self.inputs.carrier_return.is_some() {
                    value.is_some_and(|value| self.carrier_return_body(expression, value))
                } else {
                    value.is_none_or(|value| {
                        self.expression(value) || self.returned_nullable_owner(value)
                    })
                }
            }
            Expression::Break { .. } | Expression::Continue { .. } => self.loops > 0,
            _ => false,
        }
    }

    fn returned_nullable_owner(&self, expression: ExpressionId) -> bool {
        let root = self.ungrouped(expression);
        let Some(fact) = self.inputs.typed.non_null_use(self.id(root)) else {
            return false;
        };
        let symbol = fact.symbol();
        let narrowed = self.c(fact.narrowed_type());
        self.reference(root) == Some(symbol)
            && self.known.contains(&symbol)
            && !self.callables.contains(&symbol)
            && !matches!(
                self.parameter_mode(symbol),
                Some(ParameterMode::Borrow | ParameterMode::Inout)
            )
            && self.view_symbols.contains(&symbol)
            && self.copyability_of(narrowed) == Copyability::MoveOnly
            && (self.c(fact.declared_type()) == self.inputs.return_type
                || (narrowed == self.inputs.return_type
                    && self.string(narrowed)
                    && !self.is_map_remove_symbol(symbol)))
    }

    fn name(&mut self, expression: ExpressionId) -> bool {
        if self.object_name(expression) || self.known_function(expression) {
            self.migrated = true;
            return true;
        }
        let Some(symbol) = self.reference(expression) else {
            return false;
        };
        if !self.known.contains(&symbol) {
            return false;
        };
        if self.callables.contains(&symbol) {
            return self
                .inputs
                .typed
                .non_null_use(self.id(expression))
                .is_none();
        }
        // A loan parameter is read by copy, so only a Copyable one is a value here.
        if matches!(
            self.parameter_mode(symbol),
            Some(ParameterMode::Borrow | ParameterMode::Inout)
        ) && self
            .symbol_type(symbol)
            .is_none_or(|ty| self.copyability_of(ty) != Copyability::Copyable)
        {
            return false;
        }
        match self.inputs.typed.non_null_use(self.id(expression)) {
            // Only Copyable inners are narrowed by value in the migrated subset.
            Some(fact) => {
                fact.symbol() == symbol
                    && self.copyability_of(self.c(fact.narrowed_type())) == Copyability::Copyable
            }
            None => true,
        }
    }

    fn binary(
        &mut self,
        expression: ExpressionId,
        left: ExpressionId,
        operator: BinaryOperator,
        right: ExpressionId,
    ) -> bool {
        if matches!(
            operator,
            BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
        ) || self
            .inputs
            .typed
            .null_comparison(self.id(expression))
            .is_some()
        {
            return self.condition(expression, false);
        }
        if self.nullable_equality(expression, left, operator, right) {
            return true;
        }
        let Some(operand) = self.expression_type(left) else {
            return false;
        };
        if self.nothing(operand)
            && matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
        {
            // The builder propagates the left exit before asking for the right value.
            return self.expression(left);
        }
        let supported = match operator {
            BinaryOperator::Add if self.string(operand) => true,
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder
            | BinaryOperator::BitwiseAnd
            | BinaryOperator::BitwiseOr
            | BinaryOperator::BitwiseXor
            | BinaryOperator::Shl
            | BinaryOperator::Shr
            | BinaryOperator::Ushr
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => self.integer_type(operand),
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                self.scalar(operand) || self.string(operand)
            }
            _ => false,
        };
        let string = self.string(operand);
        supported && self.operand(left, string) && self.operand(right, string)
    }

    /// A String operand may be a Borrow String binding or a MoveOnly non-null name.
    fn operand(&mut self, expression: ExpressionId, string: bool) -> bool {
        if string && self.borrowed_string_name(expression) {
            return true;
        }
        if string
            && let Some(fact) = self.inputs.typed.non_null_use(self.id(expression))
            && self.copyability_of(self.c(fact.narrowed_type())) == Copyability::MoveOnly
        {
            let root = self.ungrouped(expression);
            return self.reference(root) == Some(fact.symbol())
                && self.known.contains(&fact.symbol())
                && self.view_symbols.contains(&fact.symbol());
        }
        self.expression(expression)
    }

    fn statement(&mut self, statement: StatementId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().statements().get(statement) else {
            return false;
        };
        match node.payload().clone() {
            Statement::Block { elements }
            | Statement::ControlBody { elements }
            | Statement::LambdaBody { elements } => {
                let mut iter = elements.into_iter();
                while let Some(element) = iter.next() {
                    if !self.statement(element) {
                        return false;
                    }
                    if !self.statement_may_continue(element) {
                        if iter.any(|stmt| self.stmt_has_runtime_container(stmt)) {
                            self.migrated = true;
                        }
                        break;
                    }
                }
                true
            }
            Statement::Expression { expression } => self.expression(expression),
            Statement::LocalVariable { declaration } => self.local_variable(declaration),
            Statement::While {
                condition, body, ..
            } => self.check_while(condition, body, node.span()),
            Statement::Loop { body, .. } => {
                self.strip_reassigned_loop_views(node.span(), false);
                self.loop_body(body)
            }
            Statement::For { source, body, .. } => {
                self.strip_reassigned_loop_views(node.span(), false);
                self.numeric_for(statement, source, body)
                    || self.sequential_for(statement, source, body, node.span())
                    || self.range_view_for(statement, source, body)
                    || self.map_for(statement, source, body)
            }
            Statement::LocalDestructuring { initializer, .. } => {
                self.local_destructuring(statement, initializer)
            }
            _ => false,
        }
    }

    fn loop_body(&mut self, body: StatementId) -> bool {
        self.loops += 1;
        let supported = self.statement(body);
        self.loops -= 1;
        supported
    }
}
