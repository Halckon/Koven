//! Deterministic precheck for the migrated subset (SPEC-0350 §3). It reads typed and
//! ownership facts plus AST construct kinds; it never runs lowering. Anything outside
//! the subset keeps the existing per-family path.
mod callable;
mod container;
mod drops;
mod field;
mod heap;
mod inout;
mod rc;
mod receiver;
mod scalar_text;
mod tagged;
mod value;
mod when;

use std::collections::BTreeSet;

use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{SymbolKind, UnitSymbolId},
    parser::{
        AssignmentOperator, BinaryOperator, Expression, Item, LiteralKind, NameMarker,
        PrefixOperator, Statement,
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
    /// Pilot routing gate: the function touches a nullable value.
    nullable: bool,
    /// Enclosing `while`/`loop` statements; `break`/`continue` need one.
    loops: usize,
    /// Routing gate: the function uses a construct whose family moved to this layer.
    migrated: bool,
    /// Field replacements in the subset; their `BeforeReplacement` drops are consumed by
    /// the replacement itself.
    replacements: BTreeSet<UnitExpressionId>,
    /// Container element replacements (`xs[i] = v`) whose `AfterReplacement` drops are
    /// consumed by `ContainerReplace`.
    element_replacements: BTreeSet<UnitExpressionId>,
    /// Element or component bindings introduced by a sequential `for` loop (`Borrow` loans).
    for_borrowed: BTreeSet<UnitSymbolId>,
    /// MoveOnly nullable bindings whose non-null proof view is active in the current branch.
    view_symbols: BTreeSet<UnitSymbolId>,
}

pub(super) fn accepts(inputs: &RefineInputs<'_>) -> bool {
    let mut known: BTreeSet<_> = inputs.parameters.iter().copied().collect();
    if let RefineBody::Lambda(thunk) = &inputs.body {
        known.extend(thunk.captures.iter().map(|capture| capture.symbol));
    }
    let mut checker = Checker {
        inputs,
        known,
        callables: BTreeSet::new(),
        branch_form: BTreeSet::new(),
        nullable: false,
        loops: 0,
        migrated: false,
        replacements: BTreeSet::new(),
        element_replacements: BTreeSet::new(),
        for_borrowed: BTreeSet::new(),
        view_symbols: BTreeSet::new(),
    };
    let mut signature_types: Vec<_> = inputs
        .parameters
        .iter()
        .filter_map(|symbol| checker.symbol_type(*symbol))
        .chain([inputs.return_type])
        .collect();
    if let RefineBody::Lambda(thunk) = &inputs.body {
        signature_types.extend(thunk.captures.iter().map(|capture| capture.ty));
    }
    for ty in signature_types {
        checker.note_nullable(ty);
        checker.migrated |= checker.container_element(ty).is_some()
            || checker.box_payload(ty).is_some()
            || checker.box_enum_type(ty)
            || checker.map_kv(ty).is_some()
            || checker.rc_payload(ty).is_some();
    }
    // Copyable Borrow parameters are read by copy, except String when subjects read the
    // loan; Inout parameters are scalars or plain classes, read through and replaced under
    // the caller's exclusive loan.
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
                                    || checker.box_payload(ty).is_some()
                                    || checker.box_enum_type(ty)
                                    || checker.map_kv(ty).is_some()
                                    || (matches!(inputs.body, RefineBody::Lambda(_))
                                        && matches!(
                                            checker.kind(ty),
                                            Some(UnitTypeKind::Nullable(inner)) if checker.storage_type(*inner)
                                        ))
                            }
                            ParameterMode::Inout => checker.scalar(ty) || checker.class(ty),
                        }
                })
            })
        && (checker.unit(inputs.return_type)
            || checker.storage_type(inputs.return_type)
            || checker.callable_return())
        // Receivers: Copyable enums by value or shared loan (SPEC-0356), plain classes by
        // shared or exclusive loan (SPEC-0358).
        && inputs.receiver.is_none_or(|receiver| match receiver.mode {
            ParameterMode::Value => {
                checker.enum_type(receiver.ty) || checker.value_class(receiver.ty)
            }
            ParameterMode::Borrow => {
                checker.enum_type(receiver.ty)
                    || checker.class(receiver.ty)
                    || checker.value_class(receiver.ty)
            }
            ParameterMode::Inout => checker.class(receiver.ty),
        });
    // An Inout parameter or receiver belongs to the migrated Inout family.
    checker.migrated |= inputs.modes.contains(&ParameterMode::Inout)
        || inputs
            .receiver
            .is_some_and(|receiver| receiver.mode == ParameterMode::Inout);
    let body_ok = match &inputs.body {
        RefineBody::Expression(expression) => checker.expression(*expression),
        RefineBody::Block(statement) => checker.statement(*statement),
        RefineBody::Lambda(thunk) => checker.thunk_body(thunk),
    };
    signature && body_ok && checker.drops_supported() && (checker.nullable || checker.migrated)
}

/// Every storage type an accepted body uses, plus nullable inners and Boolean flags.
pub(super) fn type_demands(inputs: &RefineInputs<'_>) -> Vec<UnitTypeId> {
    let checker = Checker {
        inputs,
        known: BTreeSet::new(),
        callables: BTreeSet::new(),
        branch_form: BTreeSet::new(),
        nullable: false,
        loops: 0,
        migrated: false,
        replacements: BTreeSet::new(),
        element_replacements: BTreeSet::new(),
        for_borrowed: BTreeSet::new(),
        view_symbols: BTreeSet::new(),
    };
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
            if let Some(ty) = inputs.typed.expression_type(id) {
                let ty = match checker.kind(ty) {
                    Some(UnitTypeKind::EnumCase { root, .. }) => *root,
                    _ => ty,
                };
                demands.extend(inputs.concrete(ty));
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
            || checker.map_kv(*ty).is_some()
            || matches!(
                checker.kind(*ty),
                Some(UnitTypeKind::Builtin(BuiltinType::IntRange))
            )
    }) {
        demands.extend(typed.types().builtin(BuiltinType::Int));
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

    fn scalar(&self, ty: UnitTypeId) -> bool {
        matches!(self.kind(ty), Some(UnitTypeKind::Builtin(b)) if *b == BuiltinType::Boolean || integer(*b))
    }

    fn string(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.kind(ty),
            Some(UnitTypeKind::Builtin(BuiltinType::String))
        )
    }

    fn integer_type(&self, ty: UnitTypeId) -> bool {
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
            && self.inputs.constant_owned.is_none_or(|owned| {
                owned.short_circuit_at(id).is_none_or(|plan| {
                    plan.rhs()
                        == lang_frontend::ownership_checking::UnitShortCircuitRhs::Conditional
                })
            })
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
        let id = self.id(expression);
        // Jumps and abort calls are typed `Nothing`; a `return` operand carries the value.
        // A `Nothing`-typed `if` diverges on every branch.
        let returns = (matches!(
            node.payload(),
            Expression::Return { .. }
                | Expression::Break { .. }
                | Expression::Continue { .. }
                | Expression::If { .. }
        ) || self.inputs.typed.call(id).is_some_and(|desc| desc.aborts()))
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
            return (self.unit(ty) || self.storage_type(ty))
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
                let then_ok = cond_ok && self.statement(then_branch);
                self.view_symbols = saved;
                then_ok
                    && else_branch.is_none_or(|branch| self.statement(branch))
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
            } => self.subject_when(ty, subject, &entries),
            Expression::Call { arguments, .. } => {
                let values: Vec<_> = arguments.iter().map(|argument| argument.value).collect();
                let function_value = self
                    .inputs
                    .typed
                    .call(self.id(expression))
                    .is_some_and(|descriptor| descriptor.target() == UnitCallTarget::FunctionValue);
                self.call(expression)
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
                value,
                ..
            } => self.assignment(expression, target, value),
            Expression::Return { value, .. } => value.is_none_or(|value| self.expression(value)),
            Expression::Break { .. } | Expression::Continue { .. } => self.loops > 0,
            _ => false,
        }
    }

    fn name(&self, expression: ExpressionId) -> bool {
        let Some(symbol) = self.reference(expression) else {
            return false;
        };
        if !self.known.contains(&symbol) {
            return false;
        }
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
        let Some(operand) = self.expression_type(left) else {
            return false;
        };
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
            return self.reference(expression) == Some(fact.symbol())
                && self.known.contains(&fact.symbol())
                && self.view_symbols.contains(&fact.symbol());
        }
        self.expression(expression)
    }

    /// Mirrors the builder: `!`, groups, `&&`, `||` and null comparisons are branch form
    /// (their own drop points are never emitted); other conditions are plain values.
    /// `views`: a MoveOnly null comparison may open a proof view only directly in an
    /// `if`/`while` condition (through groups and `&&`), on its true edge.
    fn condition(&mut self, expression: ExpressionId, views: bool) -> bool {
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
            self.note_nullable(self.c(fact.nullable_type()));
            let inner_ok = match self.kind(self.c(fact.nullable_type())) {
                Some(UnitTypeKind::Nullable(inner)) if self.scalar(*inner) => true,
                Some(UnitTypeKind::Nullable(inner)) if self.view_inner(*inner) => {
                    if views && fact.non_null_when_true() {
                        self.view_symbols.insert(fact.symbol());
                        true
                    } else {
                        !views
                    }
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
                    && self.condition(right, views);
                if !views {
                    self.view_symbols = saved;
                }
                ok
            }
            _ => self.expression(expression),
        }
    }

    fn is_extension_call(
        &self,
        descriptor: &lang_frontend::type_checking::UnitCallDescriptor,
    ) -> bool {
        match descriptor.target() {
            UnitCallTarget::Declaration(id) => self
                .inputs
                .typed
                .signatures()
                .declaration(id)
                .and_then(|signature| signature.callable())
                .is_some_and(|callable| {
                    callable.range_extension().is_some() || callable.text_extension().is_some()
                }),
            _ => false,
        }
    }

    fn call(&self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.call(id) else {
            return false;
        };
        let is_extension = self.is_extension_call(descriptor);
        let simple = (descriptor.receiver().is_none()
            || is_extension
            || self.receiver_call(expression))
            && descriptor.range_construction().is_none()
            && descriptor.borrow_return().is_none()
            && descriptor
                .arguments()
                .iter()
                .all(|argument| match argument.mode() {
                    ParameterMode::Value => self.storage_type(self.c(argument.parameter_type())),
                    ParameterMode::Borrow => {
                        self.storage_type(self.c(argument.parameter_type()))
                            || self.view_function(self.c(argument.parameter_type()))
                    }
                    ParameterMode::Inout => {
                        self.scalar(self.c(argument.parameter_type()))
                            || self.class(self.c(argument.parameter_type()))
                    }
                });
        // A call through a function value is further restricted to view parameters.
        let target = descriptor.prints_line()
            || descriptor.aborts()
            || descriptor.text_primitive().is_some()
            || descriptor.target() == UnitCallTarget::FunctionValue
            || (matches!(
                descriptor.target(),
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            ) && self
                .inputs
                .source_plan
                .call_site(self.inputs.source_token, id)
                .is_some_and(|site| {
                    site.delegation().is_empty()
                        && self.inputs.function_ids.contains_key(site.key())
                }));
        simple && target
    }

    /// Arguments are plain expressions, except that a Borrow argument may be a MoveOnly
    /// non-null name read through its proof view.
    fn arguments(&mut self, call: ExpressionId, values: &[ExpressionId]) -> bool {
        let Some(descriptor) = self.inputs.typed.call(self.id(call)) else {
            return false;
        };
        let is_extension = self.is_extension_call(descriptor);
        let mut modes: Vec<_> = descriptor
            .arguments()
            .iter()
            .map(|argument| {
                (
                    Some(argument.argument_index()),
                    argument.mode(),
                    self.c(argument.parameter_type()),
                )
            })
            .collect();
        let mut extension_rec = None;
        if is_extension
            && let Some(receiver) = descriptor.receiver()
            && let lang_frontend::type_checking::UnitCallReceiverOrigin::Expression(expression) =
                receiver.origin()
            && expression.source_unit() == self.inputs.source_unit
        {
            extension_rec = Some(expression.expression());
            modes.insert(0, (None, receiver.mode(), self.c(receiver.ty())));
        }
        modes.into_iter().all(|(index, mode, parameter)| {
            let Some(value) = index.map_or(extension_rec, |i| values.get(i).copied()) else {
                return false;
            };
            if mode == ParameterMode::Inout {
                return self.inout_argument(call, value);
            }
            if mode == ParameterMode::Borrow && self.view_function(parameter) {
                return self.callable_argument(value);
            }
            if mode == ParameterMode::Borrow
                && (self.borrowed_move_only_name(value, parameter)
                    || self.borrowed_string_field(call, value, parameter)
                    || self.borrowed_rc_payload(call, value, parameter)
                    || self.borrowed_container_element(call, value, parameter))
            {
                return true;
            }
            // A borrowed MoveOnly value is wrapped for a nullable parameter only when it is
            // a temporary whose drop obligation can move to the wrapper.
            if mode == ParameterMode::Borrow
                && self.expression_type(value).is_some_and(|actual| {
                    actual != parameter && self.copyability_of(actual) != Copyability::Copyable
                })
                && self.inputs.typed.expression_category(self.id(value))
                    != Some(lang_frontend::type_checking::ExpressionCategory::Temporary)
                && self
                    .inputs
                    .constant_owned
                    .is_none_or(|owned| owned.materialization_at(self.id(value)).is_none())
            {
                return false;
            }
            if mode == ParameterMode::Borrow
                && let Some(fact) = self.inputs.typed.non_null_use(self.id(value))
                && self.copyability_of(self.c(fact.narrowed_type())) == Copyability::MoveOnly
            {
                return self.rc_payload(self.c(fact.narrowed_type())).is_none()
                    && self.reference(value) == Some(fact.symbol())
                    && self.known.contains(&fact.symbol())
                    && self.view_symbols.contains(&fact.symbol());
            }
            self.expression(value)
        })
    }

    fn statement(&mut self, statement: StatementId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().statements().get(statement) else {
            return false;
        };
        match node.payload().clone() {
            Statement::Block { elements }
            | Statement::ControlBody { elements }
            | Statement::LambdaBody { elements } => {
                elements.into_iter().all(|element| self.statement(element))
            }
            Statement::Expression { expression } => self.expression(expression),
            Statement::LocalVariable { declaration } => self.local_variable(declaration),
            Statement::While {
                condition, body, ..
            } => {
                let saved = self.view_symbols.clone();
                let ok = self.condition(condition, true) && self.loop_body(body);
                self.view_symbols = saved;
                ok
            }
            Statement::Loop { body, .. } => self.loop_body(body),
            Statement::For { source, body, .. } => {
                self.numeric_for(statement, source, body)
                    || self.sequential_for(statement, source, body)
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

    fn local_variable(&mut self, declaration: ItemId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().items().get(declaration) else {
            return false;
        };
        let Item::Variable {
            name: NameMarker::Present(name_span),
            initializer,
            ..
        } = node.payload().clone()
        else {
            return false;
        };
        let initializer_id = self.id(initializer);
        if self
            .inputs
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .any(|fact| fact.initializer() == initializer_id)
            || self
                .inputs
                .sources
                .slice(name_span)
                .is_ok_and(|name| name == "_")
        {
            return false;
        }
        let symbol = self
            .inputs
            .names
            .names()
            .source_units()
            .get(self.inputs.source_unit.index())
            .and_then(|source| {
                source.resolution().symbols().iter().find(|symbol| {
                    symbol.span() == name_span && symbol.kind() == SymbolKind::Variable
                })
            })
            .and_then(|local| {
                self.inputs
                    .typed
                    .body_symbol_types()
                    .keys()
                    .copied()
                    .find(|symbol| {
                        symbol.source_unit() == self.inputs.source_unit
                            && symbol.symbol() == local.id()
                    })
            });
        let Some(symbol) = symbol else {
            return false;
        };
        let Some(declared) = self.symbol_type(symbol) else {
            return false;
        };
        self.note_nullable(declared);
        if self.view_or_function(declared) {
            return self.callable_local(symbol, initializer);
        }
        let supported =
            (self.unit(declared) || self.storage_type(declared)) && self.expression(initializer);
        self.known.insert(symbol);
        supported
    }
}
