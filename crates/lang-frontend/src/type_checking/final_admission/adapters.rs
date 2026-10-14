//! 同一资格规则的单文件/source-qualified 事实适配；不重新进行语义选择。
use super::super::*;
use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::{
        NameResolution, Namespace, ReferenceTarget, SourceUnitId, UnitReferenceTarget,
        UnitSymbolId, ValidatedCompilationUnitNames,
    },
    source::Span,
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
pub(super) trait Facts {
    fn expression_type(&self, expression: ExpressionId) -> Option<usize>;
    fn type_ref_type(&self, type_ref: TypeRefId) -> Option<usize>;
    fn shape(&self, ty: usize) -> Option<Shape>;
    fn selection(&self, expression: ExpressionId) -> Option<Selection>;
    fn static_qualifier(&self, span: Span) -> bool;
    fn declaration(&self, span: Span) -> Option<(usize, usize)>;
    fn referenced_symbol(&self, span: Span) -> Option<usize>;
    fn statement_assignment(&self) -> bool {
        false
    }
}
pub(super) struct Single<'a> {
    pub typed: &'a TypedFile,
    pub names: &'a NameResolution,
}
pub(super) struct Unit<'a> {
    pub typed: &'a CompilationUnitTypes,
    pub names: &'a ValidatedCompilationUnitNames,
    pub source: SourceUnitId,
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

// 两套事实表具有相同的已发布 descriptor 合同；仅 identity 适配不同。
macro_rules! selected {
    ($typed:expr, $id:expr, $function_value:path, $receiver:path) => {{
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
            CallReceiverOrigin::Expression
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
    fn declaration(&self, span: Span) -> Option<(usize, usize)> {
        let symbol = self
            .names
            .symbols()
            .iter()
            .find(|s| s.span() == span && s.namespace() == Namespace::Value)?;
        Some((
            symbol.id().index(),
            self.typed.symbol_type(symbol.id())?.index(),
        ))
    }
    fn referenced_symbol(&self, span: Span) -> Option<usize> {
        self.names
            .references()
            .iter()
            .find_map(|r| match r.target() {
                ReferenceTarget::Symbol(symbol)
                    if r.span() == span && r.namespace() == Namespace::Value =>
                {
                    Some(symbol.index())
                }
                _ => None,
            })
    }
    fn statement_assignment(&self) -> bool {
        true
    }
}
impl Facts for Unit<'_> {
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
            UnitCallReceiverOrigin::Expression
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
    fn declaration(&self, span: Span) -> Option<(usize, usize)> {
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
            symbol.id().index(),
            self.typed
                .symbol_type(UnitSymbolId::new(self.source, symbol.id()))?
                .index(),
        ))
    }
    fn referenced_symbol(&self, span: Span) -> Option<usize> {
        self.names
            .names()
            .references()
            .iter()
            .find_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(symbol)
                    if r.span() == span && symbol.source_unit() == self.source =>
                {
                    Some(symbol.symbol().index())
                }
                UnitReferenceTarget::Declaration(declaration) if r.span() == span => self
                    .names
                    .names()
                    .declaration_symbol(*declaration)
                    .filter(|s| s.source_unit() == self.source)
                    .map(|s| s.symbol().index()),
                _ => None,
            })
    }
}
