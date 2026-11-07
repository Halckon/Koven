//! SPEC-0197 compilation-unit body 类型检查 driver。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, ItemId, StatementId},
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::{
        CompilationUnitNames, DeclarationId, Namespace, SourceUnitId, SourceUnitInput, SymbolKind,
        UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames, ordered_unit_diagnostics,
    },
    parser::{
        Expression, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile, Statement,
        VariableKind,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitSignatures, DeferredReason, ExpressionUse, ExternalTypeBinding,
        NominalKind, ParameterMode, TypeCheckingError, TypeEnvironment,
        UnitAggregateProjectionDescriptor, UnitAggregateProjectionKind,
        UnitAggregateProjectionReceiver, UnitCallableSignature, UnitFunctionParameterType,
        UnitRuntimeFieldLayoutDescriptor, UnitRuntimeFieldLayoutField, UnitTypeId, UnitTypeKind,
        collect_expression_uses,
    },
};

mod assignment;
mod bindings;
mod call_writeback;
mod callable_body;
mod calls;
mod closure_writeback;
mod constant_dependencies;
mod constant_evaluation;
mod constant_facts;
mod constants;
mod construction;
mod container;
mod container_operations;
mod control;
mod control_join;
pub(super) mod copyability;
mod destructuring;
mod dispatch_owner_types;
mod expression;
mod expression_facts;
mod flow;
mod generic_body_types;
mod index;
mod integer;
mod integer_range;
mod iteration;
mod lambda;
mod literals;
mod map;
mod members;
mod missing_classifier;
mod missing_name;
mod nullable;
mod nullable_when;
mod operators;
mod ownership_primitives;
mod pending_constraints;
mod pending_types;
mod postfix;
mod range_carrier;
mod range_construction;
mod rc;
mod receiver_recovery;
mod string;
mod top_level;
mod trial;
mod type_graph;
mod type_refs;
mod when;
mod while_flow;

use super::super::signatures::collect_signatures_with_files;
use expression_facts::ExpressionExpected;
use flow::{FlowKey, FlowVersions};

use super::{
    CompilationUnitTypeError, CompilationUnitTypeParts, CompilationUnitTypes, UnitExpressionId,
    UnitStatementId,
};

#[derive(Clone, Copy)]
pub(super) struct ExpressionCheck {
    pub(super) ty: UnitTypeId,
    pub(super) falls_through: bool,
}

/// 收集 unit-wide signatures，并在同一个类型空间检查当前已接通的顶层 callable body。
///
/// Signature/body 用户错误始终进入 recovery product；只有输入身份或尚未接通的合法 body
/// 节点才返回内部错误。最终是否可交给 ownership 由 [`CompilationUnitTypes::validate`] 决定。
pub fn check_compilation_unit_types(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
) -> Result<CompilationUnitTypes, CompilationUnitTypeError> {
    check_compilation_unit_types_recovering(sources, inputs, names.names(), environment)
}

/// 同一类型 producer 的 raw 门；上游错误随 signature 资格保留，不制造 validated names。
pub(crate) fn check_compilation_unit_types_recovering(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &CompilationUnitNames,
    environment: &TypeEnvironment,
) -> Result<CompilationUnitTypes, CompilationUnitTypeError> {
    let prepared = collect_signatures_with_files(sources, inputs, names, environment)?;
    BodyChecker::new(
        sources,
        prepared.files,
        names,
        environment,
        prepared.signatures,
    )
    .run()
}

pub(super) struct BodyChecker<'a> {
    sources: &'a SourceMap,
    names: &'a CompilationUnitNames,
    environment: &'a TypeEnvironment,
    files: Vec<&'a ParsedFile>,
    expression_uses: Vec<Vec<ExpressionUse>>,
    signatures: CompilationUnitSignatures,
    references: BTreeMap<(SourceUnitId, usize, usize, u8), UnitReferenceTarget>,
    symbols_by_span: BTreeMap<(SourceUnitId, usize, usize, u8), UnitSymbolId>,
    stable_flow_symbols: BTreeSet<UnitSymbolId>,
    flow_facts: BTreeMap<FlowKey, UnitTypeId>,
    flow_versions: FlowVersions,
    pending_immutable_bindings: BTreeSet<UnitSymbolId>,
    replacement_values: BTreeMap<UnitExpressionId, bool>,
    parts: CompilationUnitTypeParts,
    diagnostics: Vec<Diagnostic>,
    deferred_facts: Vec<trial::DeferredFact>,
    explicit_storage_failures: Option<BTreeMap<UnitTypeId, Span>>,
    storage_recovery_scopes: Vec<BTreeMap<UnitTypeId, Span>>,
    current_return_span: Option<Span>,
    loop_depth: usize,
    suppressed_loop_depth: usize,
    pending_loop_exits: Vec<while_flow::PendingLoopExits>,
    pending_boolean_edges: BTreeMap<UnitExpressionId, while_flow::PendingBooleanEdges>,
    pending_abrupt_sites: BTreeSet<while_flow::PendingControlSite>,
    pending_callable_returns: Option<call_writeback::PendingCallableReturns>,
    closure_effects: closure_writeback::ClosureEffects,
    closure_origins: closure_writeback::ClosureOrigins,
    call_writebacks: call_writeback::PendingWritebacks,
    discovered_call_writebacks: call_writeback::PendingWritebacks,
    discovered_loop_heads: BTreeSet<UnitStatementId>,
    loop_replay_sites: BTreeSet<UnitStatementId>,
    callable_loop_bases: Vec<usize>,
    active_nullable_whens: Vec<nullable_when::ActiveWhenFrame>,
    candidate_local_expected: bool,
    current_receiver: Option<UnitTypeId>,
    current_rejected_extension_receiver: bool,
    current_receiver_mode: Option<ParameterMode>,
    current_owner: Option<DeclarationId>,
    constant_prechecked: BTreeSet<UnitSymbolId>,
    checking_constants: bool,
    constant_inputs: super::constants::ConstantInputs,
    constant_values: BTreeMap<UnitSymbolId, crate::type_checking::ConstValue>,
}

impl<'a> BodyChecker<'a> {
    fn new(
        sources: &'a SourceMap,
        files: Vec<&'a ParsedFile>,
        names: &'a CompilationUnitNames,
        environment: &'a TypeEnvironment,
        signatures: CompilationUnitSignatures,
    ) -> Self {
        let references = names
            .references()
            .iter()
            .filter_map(|reference| {
                reference.namespace().map(|namespace| {
                    (
                        (
                            reference.source_unit(),
                            reference.span().start(),
                            reference.span().end(),
                            namespace_rank(namespace),
                        ),
                        reference.target().clone(),
                    )
                })
            })
            .collect();
        let symbols_by_span = names
            .source_units()
            .iter()
            .flat_map(|unit| {
                unit.resolution().symbols().iter().map(move |symbol| {
                    (
                        (
                            unit.source_unit(),
                            symbol.span().start(),
                            symbol.span().end(),
                            namespace_rank(symbol.namespace()),
                        ),
                        UnitSymbolId::new(unit.source_unit(), symbol.id()),
                    )
                })
            })
            .collect();
        let stable_flow_symbols = flow::collect_stable_flow_symbols(&files, names);
        let expression_uses = files
            .iter()
            .map(|file| collect_expression_uses(file))
            .collect();
        Self {
            sources,
            names,
            environment,
            files,
            expression_uses,
            signatures,
            references,
            symbols_by_span,
            stable_flow_symbols,
            flow_facts: BTreeMap::new(),
            flow_versions: FlowVersions::default(),
            pending_immutable_bindings: BTreeSet::new(),
            replacement_values: BTreeMap::new(),
            parts: CompilationUnitTypeParts::default(),
            diagnostics: Vec::new(),
            deferred_facts: Vec::new(),
            explicit_storage_failures: None,
            storage_recovery_scopes: Vec::new(),
            current_return_span: None,
            loop_depth: 0,
            suppressed_loop_depth: 0,
            pending_loop_exits: Vec::new(),
            pending_boolean_edges: BTreeMap::new(),
            pending_abrupt_sites: BTreeSet::new(),
            pending_callable_returns: None,
            closure_effects: BTreeMap::new(),
            closure_origins: BTreeMap::new(),
            call_writebacks: BTreeSet::new(),
            discovered_call_writebacks: BTreeSet::new(),
            discovered_loop_heads: BTreeSet::new(),
            loop_replay_sites: BTreeSet::new(),
            callable_loop_bases: Vec::new(),
            active_nullable_whens: Vec::new(),
            candidate_local_expected: false,
            current_receiver: None,
            current_rejected_extension_receiver: false,
            current_receiver_mode: None,
            current_owner: None,
            constant_prechecked: BTreeSet::new(),
            checking_constants: false,
            constant_inputs: BTreeMap::new(),
            constant_values: BTreeMap::new(),
        }
    }

    fn run(self) -> Result<CompilationUnitTypes, CompilationUnitTypeError> {
        self.run_with_generic_limit(crate::type_checking::MAX_UNIT_GENERIC_INSTANCES)
    }

    fn check_source_bodies(&mut self) -> Result<(), CompilationUnitTypeError> {
        self.parts.constant_declaration_count = self
            .names
            .index()
            .source_units()
            .iter()
            .map(|source| {
                self.file(source.id())
                    .ast()
                    .items()
                    .iter()
                    .filter(|(_, node)| matches!(node.payload(), Item::Constant { .. }))
                    .count()
            })
            .sum();
        self.precheck_constant_dependencies()?;
        let declarations = self.names.index().declarations().to_vec();
        for declaration in declarations {
            let signature = self.signatures.declaration(declaration.id()).cloned();
            let callable = signature
                .as_ref()
                .and_then(|signature| signature.callable())
                .cloned();
            let nominal = signature
                .as_ref()
                .and_then(|signature| signature.nominal())
                .cloned();
            let file = self.files[declaration.source_unit().index()];
            let item = unwrapped_item(file, declaration.root())?.clone();
            if matches!(item, Item::Variable { .. } | Item::Constant { .. }) {
                let item_span = unwrapped_item_span(file, declaration.root())?;
                self.check_top_level_initializer(
                    declaration.id(),
                    declaration.source_unit(),
                    &item,
                    item_span,
                )?;
                continue;
            }
            if let Some(callable) = callable {
                let Item::Function { form, .. } = item else {
                    return Err(CompilationUnitTypeError::UnsupportedBody(
                        callable.name_span(),
                    ));
                };
                self.check_function(declaration.source_unit(), form, &callable)?;
                continue;
            }
            if let Some(nominal) = nominal {
                let Item::Classifier(classifier) = item else {
                    return Err(CompilationUnitTypeError::UnsupportedBody(
                        file.ast()
                            .items()
                            .get(declaration.root())
                            .map_err(TypeCheckingError::from)?
                            .span(),
                    ));
                };
                self.check_classifier_members(declaration.source_unit(), &classifier, &nominal)?;
                continue;
            }
            if item_requires_body_check(&item) {
                return Err(CompilationUnitTypeError::UnsupportedBody(
                    file.ast()
                        .items()
                        .get(declaration.root())
                        .map_err(TypeCheckingError::from)?
                        .span(),
                ));
            }
        }
        self.check_missing_name_roots()?;
        Ok(())
    }

    fn run_with_generic_limit(
        mut self,
        generic_limit: usize,
    ) -> Result<CompilationUnitTypes, CompilationUnitTypeError> {
        self = self.check_bodies_to_fixed_point()?;
        self.materialize_generic_body_types_with_limit(generic_limit)?;
        self.materialize_runtime_field_layouts()?;
        self.check_range_types()?;
        self.parts.constants = self.build_constant_facts();
        // Publication gates see every local rejection. Only the sealed product
        // coalesces occurrences proven to share an explicit storage error.
        let repeated = &self.parts.repeated_storage_diagnostics;
        if !repeated.is_empty() {
            self.diagnostics = self
                .diagnostics
                .into_iter()
                .enumerate()
                .filter_map(|(index, diagnostic)| {
                    (!repeated.contains_key(&index)).then_some(diagnostic)
                })
                .collect();
        }
        let source_units = self.names.index().source_units();
        let body_diagnostics =
            ordered_unit_diagnostics(self.sources, source_units, &self.diagnostics)?
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
        let mut diagnostics = self.signatures.diagnostics().to_vec();
        diagnostics.extend(body_diagnostics.iter().cloned());
        let diagnostics = ordered_unit_diagnostics(self.sources, source_units, &diagnostics)?
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        self.parts
            .integer_range_operations
            .sort_by_key(|fact| fact.expression());
        self.parts.iterations.sort_by_key(|plan| plan.statement());
        self.parts
            .map_descriptors
            .iterations
            .sort_by_key(|plan| plan.statement());
        self.parts
            .ownership_primitives
            .sort_by_key(|fact| fact.expression());
        self.parts
            .integer_operations
            .sort_by_key(|fact| fact.expression());
        self.finish_nullable_when_facts();
        // 排序与全部 trial 完成后才分流。诊断候选仍属原 owner，不重新发布执行表。
        if !self.signatures.is_valid()
            || diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity() == Severity::Error)
        {
            self.parts.nullable.whens.clear();
            self.parts.nullable.when_uses.clear();
            self.parts.retain_ownership_recovery();
            self.parts.integer_range_operations.clear();
        }
        Ok(CompilationUnitTypes::new(
            self.signatures,
            self.parts,
            body_diagnostics,
            diagnostics,
        ))
    }

    /// 在 body traversal 结束后的最终 unit type graph 上物化 ordinary-class field layouts。
    fn materialize_runtime_field_layouts(&mut self) -> Result<(), CompilationUnitTypeError> {
        if self
            .signatures
            .diagnostics()
            .iter()
            .chain(&self.diagnostics)
            .any(|diagnostic| diagnostic.severity() == Severity::Error)
        {
            return Ok(());
        }
        // 冻结候选 owner 集合，避免 Grow<T> -> Grow<List<T>> 一类合法 heap 递归无限扩张。
        let owner_count = self.signatures.types().len();
        let mut concrete = type_graph::ConcreteTypes::default();
        let mut index = 0;
        'owners: while index < owner_count {
            let owner_type = UnitTypeId::new(index);
            index += 1;
            let Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) = self.signatures.types().get(owner_type).cloned()
            else {
                continue;
            };
            let nominal = self
                .signatures
                .declaration(declaration)
                .and_then(|signature| signature.nominal())
                .cloned()
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            if nominal.kind() != NominalKind::Class {
                continue;
            }
            if nominal.type_parameters().len() != arguments.len() {
                return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
            }
            let mut owner_is_concrete = true;
            for argument in &arguments {
                owner_is_concrete &= concrete.is_concrete(self.signatures.types(), *argument)?;
            }
            if !owner_is_concrete {
                continue;
            }
            let substitutions = nominal
                .type_parameters()
                .iter()
                .copied()
                .zip(arguments.iter().copied())
                .collect::<BTreeMap<_, _>>();
            let mut fields = Vec::with_capacity(nominal.fields().len());
            for field in nominal.fields() {
                let concrete_type = self.substitute_type(field.ty(), &substitutions)?;
                if !concrete.is_concrete(self.signatures.types(), concrete_type)? {
                    // A valid name can still have an unbound/recovery field type. Omit the
                    // entire owner recipe rather than publishing a prefix or an internal error.
                    continue 'owners;
                }
                fields.push(UnitRuntimeFieldLayoutField::new(
                    field.symbol(),
                    field.ty(),
                    concrete_type,
                    field.span(),
                ));
            }
            self.parts
                .runtime_field_layouts
                .push(UnitRuntimeFieldLayoutDescriptor::new(
                    owner_type,
                    declaration,
                    arguments,
                    fields,
                ));
        }
        Ok(())
    }

    fn check_statement(
        &mut self,
        source: SourceUnitId,
        statement: StatementId,
        return_type: UnitTypeId,
        return_span: Option<Span>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let node = self
            .file(source)
            .ast()
            .statements()
            .get(statement)
            .map_err(TypeCheckingError::from)?;
        let span = node.span();
        let payload = node.payload().clone();
        let unit = self.builtin(BuiltinType::Unit);
        match payload {
            Statement::Block { elements } => {
                let mut falls_through = true;
                let mut pending_exit = true;
                for element in elements {
                    let result = self.check_loop_reachable(pending_exit, |checker| {
                        checker.check_statement(source, element, return_type, return_span)
                    })?;
                    falls_through &= result.falls_through;
                    pending_exit &= self.has_pending_statement_exit(source, element, result);
                }
                self.note_pending_statement_exit(source, statement, pending_exit);
                Ok(ExpressionCheck {
                    ty: unit,
                    falls_through,
                })
            }
            Statement::Expression { expression } => {
                let result =
                    self.check_expression(source, expression, None, return_span, return_type)?;
                let pending_exit = self.has_pending_expression_exit(source, expression, result)?;
                self.note_pending_statement_exit(source, statement, pending_exit);
                Ok(result)
            }
            Statement::LocalVariable { declaration } => {
                self.check_local_variable(source, statement, declaration, return_type)
            }
            Statement::LocalDestructuring {
                bindings,
                left_paren_span,
                right_paren_span,
                initializer,
                ..
            } => self.check_local_destructuring(
                source,
                statement,
                &bindings,
                left_paren_span,
                right_paren_span,
                initializer,
                return_type,
            ),
            Statement::While {
                condition, body, ..
            } => self.check_while_statement(
                source,
                statement,
                condition,
                body,
                return_type,
                return_span,
            ),
            Statement::For {
                binding,
                source: iteration_source,
                body,
                ..
            } => self.check_for_statement(
                source,
                statement,
                &binding,
                iteration_source,
                body,
                return_type,
                return_span,
            ),
            Statement::Loop { body, .. } => {
                self.check_loop_statement(source, statement, body, return_type, return_span)
            }
            Statement::Error => Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            }),
            _ => Err(CompilationUnitTypeError::UnsupportedBody(span)),
        }
    }

    fn check_local_variable(
        &mut self,
        source: SourceUnitId,
        statement: StatementId,
        declaration: ItemId,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let Item::Variable {
            kind,
            name,
            type_ref,
            initializer,
            ..
        } = unwrapped_item(self.file(source), declaration)?.clone()
        else {
            let span = self
                .file(source)
                .ast()
                .items()
                .get(declaration)
                .map_err(TypeCheckingError::from)?
                .span();
            return Err(CompilationUnitTypeError::UnsupportedBody(span));
        };
        let expected = type_ref
            .map(|type_ref| self.resolve_body_type_ref(source, type_ref))
            .transpose()?;
        let expected_span = type_ref
            .map(|type_ref| {
                self.file(source)
                    .ast()
                    .type_refs()
                    .get(type_ref)
                    .map(|node| node.span())
                    .map_err(TypeCheckingError::from)
            })
            .transpose()?;
        let deferred = self.deferred_facts.len();
        let initializer_id = UnitExpressionId::new(source, initializer);
        let initializer =
            self.check_expression(source, initializer, expected, expected_span, return_type)?;
        let pending_exit =
            self.has_pending_expression_exit(source, initializer_id.expression(), initializer)?;
        self.note_pending_statement_exit(source, statement, pending_exit);
        if let NameMarker::Present(span) = name
            && let Some(symbol) = self.symbol_at(source, span, Namespace::Value)
        {
            let ty = expected.unwrap_or(initializer.ty);
            self.record_symbol_type(symbol, ty);
            // An annotation can mask an unresolved initializer. A concrete
            // result may also depend on a pending operand.
            let pending = self.contains_unresolved_call_type(ty)
                || self.replacement_value_pending(initializer_id, initializer.ty, deferred);
            if kind == VariableKind::Val {
                if !self.is_error(initializer.ty) {
                    self.note_closure_binding(symbol, initializer_id);
                }
                self.set_immutable_binding_pending(symbol, pending);
            } else if kind == VariableKind::Var {
                self.flow_versions.declare_mutable(symbol, pending);
            }
        }
        Ok(ExpressionCheck {
            ty: self.builtin(BuiltinType::Unit),
            falls_through: initializer.falls_through,
        })
    }

    fn name_type(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        if let Some(ty) = self.check_selected_constant(source, expression, span)? {
            return Ok(ty);
        }
        let target = self.reference(source, span, Namespace::Value);
        if let Some(UnitReferenceTarget::Symbol(symbol)) = target
            && let Some(ty) = self.flow_facts.get(&FlowKey::Symbol(*symbol)).copied()
        {
            self.note_pending_binding_read(UnitExpressionId::new(source, expression), *symbol, ty);
            return Ok(ty);
        }
        match target {
            Some(UnitReferenceTarget::Declaration(declaration)) => {
                let signature = self
                    .signatures
                    .declaration(*declaration)
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                Ok(self
                    .parts
                    .symbol_types
                    .get(&signature.symbol())
                    .copied()
                    .unwrap_or(signature.ty()))
            }
            Some(UnitReferenceTarget::Symbol(symbol)) => {
                let symbol = *symbol;
                let ty = self
                    .parts
                    .symbol_types
                    .get(&symbol)
                    .copied()
                    .or_else(|| self.signatures.symbol_type(symbol))
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                let kind = self.names.source_units()[symbol.source_unit().index()]
                    .resolution()
                    .symbols()
                    .get(symbol.symbol().index())
                    .map(|symbol| symbol.kind());
                if kind == Some(SymbolKind::Field)
                    && let Some(owner) = self.current_owner
                {
                    self.parts
                        .aggregate_projections
                        .push(UnitAggregateProjectionDescriptor::new(
                            UnitExpressionId::new(source, expression),
                            UnitAggregateProjectionReceiver::This(owner),
                            symbol,
                            ty,
                            UnitAggregateProjectionKind::Field,
                        ));
                }
                self.note_pending_binding_read(
                    UnitExpressionId::new(source, expression),
                    symbol,
                    ty,
                );
                Ok(ty)
            }
            Some(UnitReferenceTarget::Unresolved | UnitReferenceTarget::LaterLocal(_)) => {
                Ok(self.error_type())
            }
            Some(UnitReferenceTarget::Symbols(symbols)) => {
                let symbols = symbols.clone();
                self.resolve_bare_symbol_candidates(source, expression, span, &symbols)
            }
            Some(UnitReferenceTarget::External(external)) => {
                match self.environment.binding(*external).cloned() {
                    Some(ExternalTypeBinding::Value(ty)) => {
                        Ok(self.normalize_environment_type(&ty))
                    }
                    Some(ExternalTypeBinding::Function(signature)) => {
                        // UnitTypeKind::Function only preserves the callable ABI shape. Until the
                        // unit model can carry compiler-bound identity/effects through a function
                        // value, accepting an effectful external here would silently erase
                        // cross-thread, abort, or stdout facts. Direct calls are handled in calls.rs
                        // and retain those effects.
                        if !signature.effects.is_empty() {
                            return Err(CompilationUnitTypeError::UnsupportedBody(span));
                        }
                        let parameters = signature
                            .parameters
                            .iter()
                            .map(|parameter| {
                                let ty = self.normalize_environment_type(&parameter.ty);
                                UnitFunctionParameterType::new(parameter.mode, ty)
                            })
                            .collect();
                        let return_type = self.normalize_environment_type(&signature.return_type);
                        Ok(self.signatures.types_mut().intern(UnitTypeKind::Function {
                            move_only: false,
                            parameters,
                            return_type,
                        }))
                    }
                    None => Ok(self.deferred_type(DeferredReason::UnboundExternalType)),
                    Some(
                        ExternalTypeBinding::Builtin(_)
                        | ExternalTypeBinding::Capability(_)
                        | ExternalTypeBinding::Intrinsic(_)
                        | ExternalTypeBinding::IntrinsicCallable(_),
                    ) => Err(CompilationUnitTypeError::UnsupportedBody(span)),
                }
            }
            _ => match self.reference(source, span, Namespace::Type) {
                Some(UnitReferenceTarget::Declaration(declaration)) => self
                    .signatures
                    .declaration(*declaration)
                    .map(|signature| signature.ty())
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol),
                _ => Err(CompilationUnitTypeError::UnsupportedBody(span)),
            },
        }
    }

    fn reference(
        &self,
        source: SourceUnitId,
        span: Span,
        namespace: Namespace,
    ) -> Option<&UnitReferenceTarget> {
        self.references
            .get(&(source, span.start(), span.end(), namespace_rank(namespace)))
    }

    pub(super) fn assignable(&self, actual: UnitTypeId, expected: UnitTypeId) -> bool {
        unit_assignable(self.signatures.types(), actual, expected)
    }

    fn builtin_kind(&self, ty: UnitTypeId) -> Option<BuiltinType> {
        match self.signatures.types().get(ty) {
            Some(UnitTypeKind::Builtin(builtin)) => Some(*builtin),
            _ => None,
        }
    }

    pub(super) fn is_builtin(&self, ty: UnitTypeId, builtin: BuiltinType) -> bool {
        self.signatures.types().get(ty) == Some(&UnitTypeKind::Builtin(builtin))
    }

    pub(super) fn is_error(&self, ty: UnitTypeId) -> bool {
        matches!(self.signatures.types().get(ty), Some(UnitTypeKind::Error))
    }

    pub(super) fn is_deferred(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.signatures.types().get(ty),
            Some(UnitTypeKind::Deferred(_))
        )
    }

    fn builtin(&self, builtin: BuiltinType) -> UnitTypeId {
        self.signatures
            .types()
            .builtin(builtin)
            .expect("UnitTypeTable seeds every builtin")
    }

    pub(super) fn error_type(&mut self) -> UnitTypeId {
        self.signatures.types_mut().intern(UnitTypeKind::Error)
    }

    pub(super) fn deferred_type(
        &mut self,
        reason: crate::type_checking::DeferredReason,
    ) -> UnitTypeId {
        self.signatures
            .types_mut()
            .intern(UnitTypeKind::Deferred(reason))
    }

    pub(super) fn type_name(&self, ty: UnitTypeId) -> String {
        match self.signatures.types().get(ty) {
            Some(UnitTypeKind::Builtin(builtin)) => builtin.name().to_owned(),
            Some(UnitTypeKind::Nullable(inner)) => format!("{}?", self.type_name(*inner)),
            Some(UnitTypeKind::Nominal { declaration, .. }) => {
                format!("declaration#{}", declaration.index())
            }
            Some(UnitTypeKind::Function { .. }) => "function type".to_owned(),
            Some(UnitTypeKind::Error) | None => "<error>".to_owned(),
            Some(kind) => format!("{kind:?}"),
        }
    }

    pub(super) fn emit(
        &mut self,
        code: &str,
        message: &str,
        primary: Span,
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(code)?;
        self.diagnostics.push(Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            message,
            primary,
        )?);
        Ok(())
    }

    pub(super) fn note_repeated_storage_error(
        &mut self,
        expression: UnitExpressionId,
        diagnostic: usize,
        origin: Span,
    ) {
        self.parts
            .storage_recovery
            .rejected_origins
            .insert(expression, origin);
        self.parts
            .repeated_storage_diagnostics
            .insert(diagnostic, origin);
    }

    pub(super) fn emit_maybe_label(
        &mut self,
        code: &str,
        message: &str,
        primary: Span,
        label: Option<Span>,
        label_message: impl Into<String>,
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(code)?;
        let mut diagnostic =
            Diagnostic::new(self.sources, Severity::Error, code, message, primary)?;
        if let Some(label) = label {
            diagnostic.add_label(self.sources, label, label_message)?;
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn file(&self, source: SourceUnitId) -> &'a ParsedFile {
        self.files[source.index()]
    }

    pub(super) fn expression_use(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> ExpressionUse {
        self.expression_uses[source.index()][expression.index()]
    }
}

fn namespace_rank(namespace: Namespace) -> u8 {
    match namespace {
        Namespace::Type => 0,
        Namespace::Value => 1,
    }
}

fn unwrapped_item(parsed: &ParsedFile, id: crate::ast::ItemId) -> Result<&Item, TypeCheckingError> {
    let mut item = parsed.ast().items().get(id)?.payload();
    while let Item::Modified { declaration, .. } = item {
        item = parsed.ast().items().get(*declaration)?.payload();
    }
    Ok(item)
}

fn unwrapped_item_span(
    parsed: &ParsedFile,
    id: crate::ast::ItemId,
) -> Result<Span, TypeCheckingError> {
    let mut node = parsed.ast().items().get(id)?;
    while let Item::Modified { declaration, .. } = node.payload() {
        node = parsed.ast().items().get(*declaration)?;
    }
    Ok(node.span())
}

fn item_requires_body_check(item: &Item) -> bool {
    match item {
        Item::Variable { .. } | Item::Constant { .. } => true,
        Item::Classifier(_) => false,
        Item::Modified { .. } => unreachable!("unwrapped_item removes modifiers"),
        Item::Error | Item::Function { .. } | Item::Companion(_) | Item::Deinit { .. } => false,
    }
}

/// 与 primitive 结构验证复用同一 unit 类型相容关系。
pub(super) fn unit_assignable(
    types: &crate::type_checking::UnitTypeTable,
    actual: UnitTypeId,
    expected: UnitTypeId,
) -> bool {
    actual == expected
        || matches!(types.get(actual), Some(UnitTypeKind::Error))
        || matches!(types.get(expected), Some(UnitTypeKind::Error))
        || matches!(
            types.get(actual),
            Some(UnitTypeKind::Builtin(BuiltinType::Nothing))
        )
        || matches!(
            types.get(actual),
            Some(UnitTypeKind::EnumCase { root, .. }) if *root == expected
        )
        || matches!(
            types.get(actual),
            Some(UnitTypeKind::StaticSelf(interface)) if *interface == expected
        )
        || matches!(
            types.get(expected),
            Some(UnitTypeKind::Nullable(inner))
                if actual == *inner
                    || matches!(
                        types.get(actual),
                        Some(UnitTypeKind::EnumCase { root, .. }) if root == inner
                    )
        )
}
