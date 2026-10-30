//! 同一资格规则的单文件/source-qualified 事实适配；不重新进行语义选择。
use super::super::*;
use crate::{
    ast::{ExpressionId, ItemId, TypeRefId},
    diagnostic::{DiagnosticCode, Severity},
    name_resolution::{
        NameResolution, Namespace, ReferenceTarget, SourceUnitId, SymbolId, SymbolKind,
        UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    parser::{Expression, Item, NameMarker, ParsedFile},
    source::{SourceMap, Span},
};

pub(super) enum Shape {
    Complete,
    Builtin(BuiltinType),
    Nullable(usize),
    Components(Vec<usize>),
    Deferred(DeferredReason),
    Error,
    UnselectedInteger,
}
#[derive(Default)]
pub(super) struct Selection {
    pub static_callee: bool,
    pub qualifier: bool,
    pub types: Vec<usize>,
    pub receivers: Vec<ExpressionId>,
}
/// A selected write operation, not a value-producing Map query.
pub(super) struct MapWriteTarget {
    pub receiver: ExpressionId,
    pub key: ExpressionId,
    pub value: ExpressionId,
    pub key_type: usize,
    pub value_type: usize,
    pub coherent: bool,
}
pub(super) trait Facts {
    type Symbol: Copy + Ord;
    fn expression_type(&self, expression: ExpressionId) -> Option<usize>;
    fn type_ref_type(&self, type_ref: TypeRefId) -> Option<usize>;
    fn shape(&self, ty: usize) -> Option<Shape>;
    fn selection(&self, expression: ExpressionId) -> Option<Selection>;
    fn map_write_target(&self, target: ExpressionId) -> Option<MapWriteTarget>;
    fn valid_map_consume(
        &self,
        _sources: &SourceMap,
        _parsed: &ParsedFile,
        _expression: ExpressionId,
    ) -> bool {
        true
    }

    fn static_qualifier(&self, span: Span) -> bool;
    fn static_candidate(&self, span: Span) -> bool;
    fn ownership_primitive_candidate(&self, span: Span) -> bool;
    fn rejected_function_overload(&self, callee_span: Span, no_match_code: DiagnosticCode) -> bool;
    fn rejected_bare_enum_case(
        &self,
        expression: ExpressionId,
        name_span: Span,
        inference_code: DiagnosticCode,
    ) -> bool;
    fn declaration(&self, span: Span) -> Option<(Self::Symbol, usize)>;
    fn referenced_symbol(&self, span: Span) -> Option<Self::Symbol>;
    fn statement_assignment(&self) -> bool {
        false
    }
}
pub(super) struct Single<'a> {
    pub typed: &'a TypedFile,
    pub names: &'a NameResolution,
    pub environment: &'a TypeEnvironment,
}
pub(super) struct Unit<'a> {
    pub typed: &'a CompilationUnitTypes,
    pub names: &'a ValidatedCompilationUnitNames,
    pub environment: &'a TypeEnvironment,
    pub source: SourceUnitId,
}

fn static_symbol(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function | SymbolKind::Classifier | SymbolKind::EnumVariant
    )
}
fn ownership_primitive_external(
    environment: &TypeEnvironment,
    symbol: crate::name_resolution::ExternalSymbolId,
) -> bool {
    matches!(
        environment.binding(symbol),
        Some(ExternalTypeBinding::IntrinsicCallable(
            IntrinsicCallable::Replace | IntrinsicCallable::Swap
        ))
    )
}

fn static_external(
    environment: &TypeEnvironment,
    symbol: crate::name_resolution::ExternalSymbolId,
) -> bool {
    matches!(
        environment.binding(symbol),
        Some(
            ExternalTypeBinding::Function(_)
                | ExternalTypeBinding::IntrinsicCallable(_)
                | ExternalTypeBinding::Intrinsic(_)
        )
    )
}

trait ParameterTypeIndex {
    fn type_index(&self) -> usize;
}
impl ParameterTypeIndex for FunctionParameterType {
    fn type_index(&self) -> usize {
        self.ty.index()
    }
}
impl ParameterTypeIndex for UnitFunctionParameterType {
    fn type_index(&self) -> usize {
        self.ty().index()
    }
}

trait LocalExpression {
    fn local(self) -> ExpressionId;
}
impl LocalExpression for ExpressionId {
    fn local(self) -> ExpressionId {
        self
    }
}
impl LocalExpression for UnitExpressionId {
    fn local(self) -> ExpressionId {
        self.expression()
    }
}

// 消费描述符不替代真实 AST/reference；这里只验证已选事实，不重新执行 receiver。
fn consume_receiver_matches(
    sources: &SourceMap,
    parsed: &ParsedFile,
    call: ExpressionId,
    receiver: ExpressionId,
) -> bool {
    let Ok(node) = parsed.ast().expressions().get(call) else {
        return false;
    };
    let Expression::Call {
        callee,
        type_arguments,
        arguments,
        ..
    } = node.payload()
    else {
        return false;
    };
    if !type_arguments.is_empty() || !arguments.is_empty() {
        return false;
    }
    matches!(parsed.ast().expressions().get(*callee).map(|node| node.payload()),
        Ok(Expression::Member { receiver: actual, name_span, safe: false, .. })
            if *actual == receiver && sources.slice(*name_span) == Ok("consume"))
}

fn direct_name_span(parsed: &ParsedFile, mut expression: ExpressionId) -> Option<Span> {
    loop {
        let node = parsed.ast().expressions().get(expression).ok()?;
        match node.payload() {
            Expression::Group { expression: inner } => expression = *inner,
            Expression::Name => return Some(node.span()),
            _ => return None,
        }
    }
}

macro_rules! valid_consume {
    ($facts:expr, $sources:expr, $parsed:expr, $id:expr, $operation:expr, $kind:ident) => {{
        let facts = $facts;
        let typed = facts.typed;
        let id = $id;
        let operation = $operation;
        let expected_root = direct_name_span($parsed, operation.receiver().local())
            .and_then(|span| facts.referenced_symbol(span));
        let source_valid = match operation.source() {
            Some((root, storage)) => Some(root) == expected_root
                && typed.symbol_type(root) == Some(storage)
                && (storage == operation.receiver_type()
                    || matches!(typed.types().get(storage), Some($kind::Nullable(inner)) if *inner == operation.receiver_type())),
            None => expected_root.is_none(),
        };
        source_valid
            && consume_receiver_matches($sources, $parsed, id.local(), operation.receiver().local())
            && typed.map_consumes().iter().filter(|fact| fact.expression() == id).count() == 1
            && typed.expression_type(operation.receiver()) == Some(operation.receiver_type())
            && typed.expression_type(id) == Some(operation.result_type())
            && matches!(typed.types().get(operation.receiver_type()),
                Some($kind::Intrinsic { constructor: IntrinsicTypeConstructor::MutableMap, arguments })
                    if arguments.as_slice() == [operation.key_type(), operation.value_type()])
            && matches!(typed.types().get(operation.result_type()),
                Some($kind::Intrinsic { constructor: IntrinsicTypeConstructor::Map, arguments })
                    if arguments.as_slice() == [operation.key_type(), operation.value_type()])
    }};
}

// 两套事实表具有相同的已发布 descriptor 合同；仅 identity 适配不同。
macro_rules! selected {
    ($typed:expr, $id:expr, $function_value:path, $receiver:path, $enum_case:pat) => {{
        let typed = $typed;
        let id = $id;
        let mut result = Selection::default();
        let mut selected = false;
        if let Some(call) = typed.call(id) {
            selected = true;
            result.static_callee = !matches!(call.target(), $function_value);
            result.types.push(call.return_type().index());
            result
                .types
                .extend(call.instance().type_arguments().iter().map(|ty| ty.index()));
            result.types.extend(
                call.arguments()
                    .iter()
                    .map(|arg| arg.parameter_type().index()),
            );
            if let Some(receiver) = call.receiver() {
                result.types.push(receiver.ty().index());
                if let $receiver(expression) = receiver.origin() {
                    result.receivers.push(expression.local());
                }
            }
        }
        if let Some(construction) = typed.construction(id) {
            selected = true;
            result.static_callee = true;
            result.qualifier = matches!(construction.target(), $enum_case);
            result.types.push(construction.result_type().index());
            result.types.extend(
                construction
                    .instance()
                    .type_arguments()
                    .iter()
                    .map(|ty| ty.index()),
            );
            result.types.extend(
                construction
                    .arguments()
                    .iter()
                    .map(|arg| arg.parameter_type().index()),
            );
        }
        if let Some(construction) = typed.container_construction(id) {
            selected = true;
            result.static_callee = true;
            result.types.extend([
                construction.container_type().index(),
                construction.element_type().index(),
            ]);
        }
        if let Some(construction) = typed.map_construction(id) {
            selected = true;
            result.static_callee = true;
            result.types.extend([
                construction.map_type().index(),
                construction.key_type().index(),
                construction.value_type().index(),
            ]);
        }
        if let Some(operation) = typed.string_operation(id) {
            selected = true;
            result.static_callee = true;
            result.receivers.push(operation.receiver().local());
            result.types.extend([
                operation.receiver_type().index(),
                operation.result_type().index(),
            ]);
        }
        if let Some(operation) = typed.integer_range_operation(id) {
            selected = true;
            result.types.extend([
                operation.left_type().index(),
                operation.right_type().index(),
                operation.result_type().index(),
            ]);
        }
        if let Some(operation) = typed.integer_operation(id) {
            selected = true;
            result.static_callee = true;
            result.receivers.push(operation.receiver().local());
            result.types.extend([
                operation.receiver_type().index(),
                operation.result_type().index(),
            ]);
        }
        if let Some(operation) = typed.rc_operation(id) {
            selected = true;
            result.static_callee = true;
            result.receivers.push(operation.receiver().local());
            result.types.push(operation.payload_type().index());
        }
        if let Some(operation) = typed.map_consume(id) {
            selected = true;
            result.static_callee = true;
            result.receivers.push(operation.receiver().local());
            result.types.extend([
                operation.receiver_type().index(),
                operation.result_type().index(),
                operation.key_type().index(),
                operation.value_type().index(),
            ]);
            if let Some((_, storage)) = operation.source() {
                result.types.push(storage.index());
            }
        }
        if typed.ownership_primitive(id).is_some()
            || typed.container_append(id).is_some()
            || typed.container_clear(id).is_some()
            || typed.container_remove_at(id).is_some()
            || typed.container_remove_last(id).is_some()
            || typed.container_remove_first(id).is_some()
            || typed.container_insert_at(id).is_some()
            || typed.map_contains(id).is_some()
            || typed.map_get(id).is_some()
            || typed.map_put(id).is_some()
            || typed.map_remove(id).is_some()
            || typed.map_require_value(id).is_some()
            || typed.map_with_value(id).is_some()
        {
            selected = true;
            result.static_callee = true;
        }
        if typed.aggregate_projection(id).is_some()
            || typed.container_size(id).is_some()
            || typed.map_size(id).is_some()
            || typed.range_size(id).is_some()
            || typed.element_place(id).is_some()
            || typed.non_null_assertion(id).is_some()
        {
            selected = true;
        }
        selected.then_some(result)
    }};
}
macro_rules! shape {
    ($kind:expr, $kind_name:ident) => {
        match $kind? {
            $kind_name::Builtin(builtin) => Shape::Builtin(*builtin),
            $kind_name::Nullable(inner) => Shape::Nullable(inner.index()),
            $kind_name::StaticSelf(inner) | $kind_name::EnumCase { root: inner, .. } => {
                Shape::Components(vec![inner.index()])
            }
            $kind_name::Function {
                parameters,
                return_type,
                ..
            } => Shape::Components(
                parameters
                    .iter()
                    .map(|p| p.type_index())
                    .chain([return_type.index()])
                    .collect(),
            ),
            $kind_name::Nominal { arguments, .. } | $kind_name::Intrinsic { arguments, .. } => {
                Shape::Components(arguments.iter().map(|ty| ty.index()).collect())
            }
            $kind_name::TypeParameter(_) | $kind_name::Capability(_) => Shape::Complete,
            $kind_name::IntegerLiteral(_) => Shape::UnselectedInteger,
            $kind_name::Deferred(reason) => Shape::Deferred(*reason),
            $kind_name::Error => Shape::Error,
        }
    };
}
impl Facts for Single<'_> {
    fn map_write_target(&self, target: ExpressionId) -> Option<MapWriteTarget> {
        let put = self.typed.map_put(target)?;
        let receiver = self
            .typed
            .expression_type(put.receiver())
            .and_then(|ty| self.typed.types().get(ty));
        Some(MapWriteTarget {
            receiver: put.receiver(),
            key: put.key(),
            value: put.value(),
            key_type: put.key_type().index(),
            value_type: put.value_type().index(),
            coherent: put.expression() == target
                && matches!(receiver,
                    Some(TypeKind::Intrinsic {
                        constructor: IntrinsicTypeConstructor::MutableMap, arguments
                    }) if arguments.as_slice() == [put.key_type(), put.value_type()]),
        })
    }
    type Symbol = SymbolId;
    fn valid_map_consume(
        &self,
        sources: &SourceMap,
        parsed: &ParsedFile,
        expression: ExpressionId,
    ) -> bool {
        let Some(operation) = self.typed.map_consume(expression) else {
            return true;
        };
        valid_consume!(self, sources, parsed, expression, operation, TypeKind)
    }

    fn expression_type(&self, expression: ExpressionId) -> Option<usize> {
        self.typed.expression_type(expression).map(TypeId::index)
    }
    fn type_ref_type(&self, type_ref: TypeRefId) -> Option<usize> {
        self.typed.type_ref_type(type_ref).map(TypeId::index)
    }
    fn shape(&self, ty: usize) -> Option<Shape> {
        Some(shape!(self.typed.types().get(TypeId::new(ty)), TypeKind))
    }
    fn selection(&self, expression: ExpressionId) -> Option<Selection> {
        let mut selection = selected!(
            self.typed,
            expression,
            CallableTarget::FunctionValue,
            CallReceiverOrigin::Expression,
            ConstructionTarget::EnumCase(_)
        );
        if let Some(constant) = self
            .typed
            .constants()
            .and_then(|facts| facts.use_at(expression))
        {
            let selection = selection.get_or_insert_with(Selection::default);
            selection.qualifier = true;
            selection.types.push(constant.ty().index());
        }
        selection
    }
    fn rejected_function_overload(&self, callee_span: Span, no_match_code: DiagnosticCode) -> bool {
        self.typed.diagnostics().iter().any(|diagnostic| {
            diagnostic.severity() == Severity::Error
                && diagnostic.code() == no_match_code
                && diagnostic.primary_span() == callee_span
        }) && self.names.references().iter().any(|reference| {
            reference.span() == callee_span
                && reference.namespace() == Namespace::Value
                && matches!(reference.target(), ReferenceTarget::OverloadSet(symbols)
                if symbols.len() > 1 && symbols.iter().all(|symbol| {
                    self.names.symbols().get(symbol.index())
                        .is_some_and(|symbol| symbol.kind() == SymbolKind::Function)
                }))
        })
    }
    fn rejected_bare_enum_case(
        &self,
        expression: ExpressionId,
        name_span: Span,
        inference_code: DiagnosticCode,
    ) -> bool {
        if self.typed.construction(expression).is_some()
            || !self.typed.diagnostics().iter().any(|diagnostic| {
                diagnostic.severity() == Severity::Error
                    && diagnostic.code() == inference_code
                    && diagnostic.primary_span() == name_span
            })
        {
            return false;
        }
        let Some(symbol) = self.referenced_symbol(name_span) else {
            return false;
        };
        self.typed.enum_cases().iter().any(|case| {
            case.value_symbol() == symbol
                && case.payloads().is_empty()
                && self.typed.expression_type(expression) == Some(case.root_type())
                && matches!(self.typed.types().get(case.root_type()),
                    Some(TypeKind::Nominal { nominal, arguments })
                        if *nominal == case.root() && !arguments.is_empty())
        })
    }
    fn static_qualifier(&self, span: Span) -> bool {
        self.names.references().iter().any(|r| {
            r.span() == span
                && r.namespace() == Namespace::Type
                && matches!(
                    r.target(),
                    ReferenceTarget::Symbol(_) | ReferenceTarget::External(_)
                )
        })
    }
    fn static_candidate(&self, span: Span) -> bool {
        self.names
            .references()
            .iter()
            .filter(|reference| reference.span() == span)
            .any(|reference| match reference.target() {
                ReferenceTarget::Symbol(symbol) => self
                    .names
                    .symbols()
                    .get(symbol.index())
                    .is_some_and(|symbol| static_symbol(symbol.kind())),
                ReferenceTarget::OverloadSet(symbols) => {
                    !symbols.is_empty()
                        && symbols.iter().all(|symbol| {
                            self.names
                                .symbols()
                                .get(symbol.index())
                                .is_some_and(|symbol| static_symbol(symbol.kind()))
                        })
                }
                ReferenceTarget::External(symbol) => static_external(self.environment, *symbol),
                ReferenceTarget::ExternalOverloadSet(symbols) => {
                    !symbols.is_empty()
                        && symbols
                            .iter()
                            .all(|symbol| static_external(self.environment, *symbol))
                }
                _ => false,
            })
    }
    fn ownership_primitive_candidate(&self, span: Span) -> bool {
        self.names.references().iter().any(|reference| {
            reference.span() == span
                && reference.namespace() == Namespace::Value
                && match reference.target() {
                    ReferenceTarget::External(symbol) => {
                        ownership_primitive_external(self.environment, *symbol)
                    }
                    ReferenceTarget::ExternalOverloadSet(symbols) => symbols
                        .iter()
                        .any(|symbol| ownership_primitive_external(self.environment, *symbol)),
                    _ => false,
                }
        })
    }
    fn declaration(&self, span: Span) -> Option<(Self::Symbol, usize)> {
        let symbol = self
            .names
            .symbols()
            .iter()
            .find(|s| s.span() == span && s.namespace() == Namespace::Value)?;
        Some((symbol.id(), self.typed.symbol_type(symbol.id())?.index()))
    }
    fn referenced_symbol(&self, span: Span) -> Option<Self::Symbol> {
        self.names
            .references()
            .iter()
            .find_map(|r| match r.target() {
                ReferenceTarget::Symbol(symbol)
                    if r.span() == span && r.namespace() == Namespace::Value =>
                {
                    Some(*symbol)
                }
                _ => None,
            })
    }
    fn statement_assignment(&self) -> bool {
        true
    }
}
impl Facts for Unit<'_> {
    fn map_write_target(&self, target: ExpressionId) -> Option<MapWriteTarget> {
        let id = UnitExpressionId::new(self.source, target);
        let put = self.typed.map_put(id)?;
        let receiver = self
            .typed
            .expression_type(put.receiver())
            .and_then(|ty| self.typed.types().get(ty));
        Some(MapWriteTarget {
            receiver: put.receiver().expression(),
            key: put.key().expression(),
            value: put.value().expression(),
            key_type: put.key_type().index(),
            value_type: put.value_type().index(),
            coherent: put.expression() == id
                && [put.receiver(), put.key(), put.value()]
                    .iter()
                    .all(|expression| expression.source_unit() == self.source)
                && matches!(receiver,
                    Some(UnitTypeKind::Intrinsic {
                        constructor: IntrinsicTypeConstructor::MutableMap, arguments
                    }) if arguments.as_slice() == [put.key_type(), put.value_type()]),
        })
    }
    type Symbol = UnitSymbolId;
    fn valid_map_consume(
        &self,
        sources: &SourceMap,
        parsed: &ParsedFile,
        expression: ExpressionId,
    ) -> bool {
        let id = UnitExpressionId::new(self.source, expression);
        let Some(operation) = self.typed.map_consume(id) else {
            return true;
        };
        operation.receiver().source_unit() == self.source
            && valid_consume!(self, sources, parsed, id, operation, UnitTypeKind)
    }

    fn expression_type(&self, expression: ExpressionId) -> Option<usize> {
        self.typed
            .expression_type(UnitExpressionId::new(self.source, expression))
            .map(UnitTypeId::index)
    }
    fn type_ref_type(&self, type_ref: TypeRefId) -> Option<usize> {
        self.typed
            .type_ref_type(UnitTypeRefId::new(self.source, type_ref))
            .map(UnitTypeId::index)
    }
    fn shape(&self, ty: usize) -> Option<Shape> {
        Some(shape!(
            self.typed.types().get(UnitTypeId::new(ty)),
            UnitTypeKind
        ))
    }
    fn selection(&self, expression: ExpressionId) -> Option<Selection> {
        let id = UnitExpressionId::new(self.source, expression);
        let mut selection = selected!(
            self.typed,
            id,
            UnitCallTarget::FunctionValue,
            UnitCallReceiverOrigin::Expression,
            UnitConstructionTarget::EnumCase(_)
        );
        if let Some(constant) = self
            .typed
            .constants()
            .and_then(|facts| facts.uses().iter().find(|fact| fact.expression() == id))
        {
            let selection = selection.get_or_insert_with(Selection::default);
            selection.qualifier = true;
            selection.types.push(constant.ty().index());
        }
        selection
    }
    fn rejected_function_overload(&self, callee_span: Span, no_match_code: DiagnosticCode) -> bool {
        if !self.typed.diagnostics().iter().any(|diagnostic| {
            diagnostic.severity() == Severity::Error
                && diagnostic.code() == no_match_code
                && diagnostic.primary_span() == callee_span
        }) {
            return false;
        }
        self.names.names().references().iter().any(|reference| {
            reference.source_unit() == self.source
                && reference.span() == callee_span
                && reference.namespace() == Some(Namespace::Value)
                && match reference.target() {
                    UnitReferenceTarget::OverloadSet(declarations) => {
                        declarations.len() > 1
                            && declarations.iter().all(|id| {
                                self.names
                                    .names()
                                    .index()
                                    .declarations()
                                    .iter()
                                    .find(|declaration| declaration.id() == *id)
                                    .is_some_and(|declaration| {
                                        declaration.kind() == SymbolKind::Function
                                    })
                            })
                    }
                    UnitReferenceTarget::Symbols(symbols) => {
                        symbols.len() > 1
                            && symbols.iter().all(|id| {
                                self.names
                                    .names()
                                    .source_units()
                                    .iter()
                                    .find(|source| source.source_unit() == id.source_unit())
                                    .and_then(|source| {
                                        source.resolution().symbols().get(id.symbol().index())
                                    })
                                    .is_some_and(|symbol| symbol.kind() == SymbolKind::Function)
                            })
                    }
                    _ => false,
                }
        })
    }
    fn rejected_bare_enum_case(
        &self,
        expression: ExpressionId,
        name_span: Span,
        inference_code: DiagnosticCode,
    ) -> bool {
        let id = UnitExpressionId::new(self.source, expression);
        if self.typed.construction(id).is_some()
            || self
                .typed
                .expression_type(id)
                .and_then(|ty| self.typed.types().get(ty))
                != Some(&UnitTypeKind::Error)
            || !self.typed.diagnostics().iter().any(|diagnostic| {
                diagnostic.severity() == Severity::Error
                    && diagnostic.code() == inference_code
                    && diagnostic.primary_span() == name_span
            })
        {
            return false;
        }
        let Some(symbol) = self
            .names
            .names()
            .references()
            .iter()
            .find_map(|reference| {
                if reference.source_unit() != self.source
                    || reference.span() != name_span
                    || reference.namespace() != Some(Namespace::Value)
                {
                    return None;
                }
                match reference.target() {
                    UnitReferenceTarget::Symbol(symbol) => Some(*symbol),
                    _ => None,
                }
            })
        else {
            return false;
        };
        self.typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|declaration| declaration.nominal())
            .flat_map(|nominal| nominal.enum_cases())
            .any(|case| case.value_symbol() == symbol && case.payloads().is_empty())
    }
    fn static_qualifier(&self, span: Span) -> bool {
        self.names.names().references().iter().any(|r| {
            r.span() == span
                && r.source_unit() == self.source
                && (matches!(r.target(), UnitReferenceTarget::Package(_))
                    || (r.namespace() == Some(Namespace::Type)
                        && matches!(
                            r.target(),
                            UnitReferenceTarget::Declaration(_)
                                | UnitReferenceTarget::Symbol(_)
                                | UnitReferenceTarget::External(_)
                        )))
        })
    }
    fn static_candidate(&self, span: Span) -> bool {
        let declaration = |id| {
            self.names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|declaration| declaration.id() == id)
                .is_some_and(|declaration| static_symbol(declaration.kind()))
        };
        let symbol = |id: UnitSymbolId| {
            self.names
                .names()
                .source_units()
                .iter()
                .find(|source| source.source_unit() == id.source_unit())
                .and_then(|source| source.resolution().symbols().get(id.symbol().index()))
                .is_some_and(|symbol| static_symbol(symbol.kind()))
        };
        self.names
            .names()
            .references()
            .iter()
            .filter(|reference| reference.span() == span && reference.source_unit() == self.source)
            .any(|reference| match reference.target() {
                UnitReferenceTarget::Declaration(id) => declaration(*id),
                UnitReferenceTarget::OverloadSet(ids) => {
                    !ids.is_empty() && ids.iter().all(|id| declaration(*id))
                }
                UnitReferenceTarget::Symbol(id) => symbol(*id),
                UnitReferenceTarget::Symbols(ids) => {
                    !ids.is_empty() && ids.iter().all(|id| symbol(*id))
                }
                UnitReferenceTarget::External(id) => static_external(self.environment, *id),
                UnitReferenceTarget::ExternalOverloadSet(ids) => {
                    !ids.is_empty() && ids.iter().all(|id| static_external(self.environment, *id))
                }
                _ => false,
            })
    }
    fn ownership_primitive_candidate(&self, span: Span) -> bool {
        self.names.names().references().iter().any(|reference| {
            reference.source_unit() == self.source
                && reference.span() == span
                && reference.namespace() == Some(Namespace::Value)
                && match reference.target() {
                    UnitReferenceTarget::External(symbol) => {
                        ownership_primitive_external(self.environment, *symbol)
                    }
                    UnitReferenceTarget::ExternalOverloadSet(symbols) => symbols
                        .iter()
                        .any(|symbol| ownership_primitive_external(self.environment, *symbol)),
                    _ => false,
                }
        })
    }
    fn declaration(&self, span: Span) -> Option<(Self::Symbol, usize)> {
        let names = self
            .names
            .names()
            .source_units()
            .iter()
            .find(|source| source.source_unit() == self.source)?
            .resolution();
        let symbol = names
            .symbols()
            .iter()
            .find(|s| s.span() == span && s.namespace() == Namespace::Value)?;
        Some((
            UnitSymbolId::new(self.source, symbol.id()),
            self.typed
                .symbol_type(UnitSymbolId::new(self.source, symbol.id()))?
                .index(),
        ))
    }
    fn referenced_symbol(&self, span: Span) -> Option<Self::Symbol> {
        self.names
            .names()
            .references()
            .iter()
            .find_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(symbol) if r.span() == span => Some(*symbol),
                UnitReferenceTarget::Declaration(declaration) if r.span() == span => {
                    self.names.names().declaration_symbol(*declaration)
                }
                _ => None,
            })
    }
}

/// 仅声明 initializer 之间的真实名称引用建立依赖；不推导或改变任何 raw 类型。
/// 先核定义根因，再核其读取点，避免 source-key 顺序成为诊断因果顺序。
pub(super) fn initializer_order(
    files: &[(&ParsedFile, Unit<'_>)],
) -> Result<Vec<(usize, ItemId)>, super::FinalTypeAdmissionInternalError> {
    use super::FinalTypeAdmissionInternalError as Internal;
    use std::collections::{BTreeMap, BTreeSet};
    let mut definitions = BTreeMap::new();
    for (file_index, (parsed, facts)) in files.iter().enumerate() {
        let mut pending = parsed.roots().iter().rev().copied().collect::<Vec<_>>();
        while let Some(item) = pending.pop() {
            let node = parsed
                .ast()
                .items()
                .get(item)
                .map_err(TypeCheckingError::from)?;
            match node.payload() {
                Item::Modified { declaration, .. } => pending.push(*declaration),
                Item::Classifier(classifier) => {
                    if let Some(body) = &classifier.body {
                        pending.extend(body.members.iter().rev().copied());
                    }
                }
                Item::Companion(companion) => {
                    pending.extend(companion.body.members.iter().rev().copied())
                }
                Item::Variable {
                    name: NameMarker::Present(span),
                    initializer,
                    ..
                }
                | Item::Constant {
                    name: NameMarker::Present(span),
                    initializer,
                    ..
                } => {
                    let (symbol, _) =
                        facts
                            .declaration(*span)
                            .ok_or(Internal::MissingRequiredFact {
                                span: *span,
                                kind: "initializer declaration identity",
                            })?;
                    let initializer_span = parsed
                        .ast()
                        .expressions()
                        .get(*initializer)
                        .map_err(TypeCheckingError::from)?
                        .span();
                    definitions.insert(symbol, (file_index, item, initializer_span));
                }
                // 函数/析构的局部声明仍在其实际 body 上下文中按原 AST 遍历。
                _ => {}
            }
        }
    }
    let mut dependencies = BTreeMap::<UnitSymbolId, BTreeSet<UnitSymbolId>>::new();
    let mut dependents = BTreeMap::<UnitSymbolId, BTreeSet<UnitSymbolId>>::new();
    for (&symbol, &(file_index, _, span)) in &definitions {
        let facts = &files[file_index].1;
        let edges = facts
            .names
            .names()
            .references()
            .iter()
            .filter(|reference| {
                reference.source_unit() == facts.source
                    && reference.namespace() == Some(Namespace::Value)
                    && reference.span().start() >= span.start()
                    && reference.span().end() <= span.end()
            })
            .filter_map(|reference| facts.referenced_symbol(reference.span()))
            .filter(|target| definitions.contains_key(target))
            .collect::<BTreeSet<_>>();
        for &target in &edges {
            dependents.entry(target).or_default().insert(symbol);
        }
        dependencies.insert(symbol, edges);
    }
    let mut ready = dependencies
        .iter()
        .filter_map(|(&symbol, edges)| edges.is_empty().then_some(symbol))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(definitions.len());
    while let Some(symbol) = ready.pop_first() {
        if dependencies.remove(&symbol).is_none() {
            continue;
        }
        let &(file, item, _) = &definitions[&symbol];
        ordered.push((file, item));
        for dependent in dependents.get(&symbol).into_iter().flatten() {
            if let Some(edges) = dependencies.get_mut(dependent) {
                edges.remove(&symbol);
                if edges.is_empty() {
                    ready.insert(*dependent);
                }
            }
        }
    }
    // 循环不会获得新语义或伪造的类型；仍检查每个原 initializer 的真实 Deferred。
    // 稳定次序至少保留原始未完成根因，独立错误也继续检查。
    for symbol in dependencies.keys() {
        let &(file, item, _) = &definitions[symbol];
        ordered.push((file, item));
    }
    Ok(ordered)
}
