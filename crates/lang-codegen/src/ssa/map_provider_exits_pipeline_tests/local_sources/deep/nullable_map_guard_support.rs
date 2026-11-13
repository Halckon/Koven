//! CI-only recovery of bounded ordinary B/C checkpoints, never a lowering input seam.
//! Each fixture first completes the unchanged FE/ownership/SSA pipeline.
pub(in crate::ssa) use crate::ssa::refine::build::owner_consumer_test_support::checkpoint;
mod identity {
    use super::*;
    use crate::ssa::lower_frontend::declarations::FunctionDeclaration;

    use crate::ssa::lower_frontend::*;

    pub(in crate::ssa) fn map_type(
        module: &Module,
        typed: &TypedFile,
        ty: TypeId,
        mapped: &mut BTreeMap<TypeId, SsaTypeId>,
    ) -> Option<SsaTypeId> {
        if let Some(&id) = mapped.get(&ty) {
            return Some(id);
        }
        let kind = match typed.types().get(ty)? {
            TypeKind::Builtin(BuiltinType::Int) => SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            },
            TypeKind::Builtin(BuiltinType::Boolean) => SsaTypeKind::Boolean,
            TypeKind::Builtin(BuiltinType::String) => SsaTypeKind::StringOwner,
            TypeKind::Builtin(BuiltinType::Unit) => SsaTypeKind::Unit,
            TypeKind::Intrinsic {
                constructor,
                arguments,
            } => {
                let kind = match constructor {
                    IntrinsicTypeConstructor::Map => MapContainerKind::Map,
                    IntrinsicTypeConstructor::MutableMap => MapContainerKind::MutableMap,
                    _ => return None,
                };
                let [key, value] = arguments.as_slice() else {
                    panic!("real Map K/V")
                };
                SsaTypeKind::MapContainer {
                    kind,
                    key: map_type(module, typed, *key, mapped).unwrap(),
                    value: map_type(module, typed, *value, mapped).unwrap(),
                }
            }
            TypeKind::Nullable(inner) => {
                let inner = map_type(module, typed, *inner, mapped).unwrap();
                let wrappers = module
                    .nullable_maps
                    .iter()
                    .filter_map(|(&wrapper, &target)| (target == inner).then_some(wrapper))
                    .collect::<Vec<_>>();
                let [wrapper] = wrappers.as_slice() else {
                    panic!("one existing exact-inner N")
                };
                assert_eq!(module.aggregate_fields(*wrapper).unwrap()[1], inner);
                assert!(!module.map_results.contains_key(wrapper));
                mapped.insert(ty, *wrapper);
                return Some(*wrapper);
            }
            TypeKind::Function { .. } => return None,
            _ => return None,
        };
        let id = *module
            .type_ids
            .get(&kind)
            .expect("no helper type registration");
        mapped.insert(ty, id);
        Some(id)
    }

    pub(in crate::ssa) fn plans(
        module: &Module,
        names: &NameResolution,
        typed: &TypedFile,
        declarations: &[FunctionDeclaration],
        instances: &instances::FunctionInstancePlan,
        function_ids: &BTreeMap<FunctionInstanceKey, FunctionId>,
        type_ids: &BTreeMap<TypeId, SsaTypeId>,
    ) -> (BTreeMap<FunctionId, Plan>, source_closure::CallableLayouts) {
        let mut plans = BTreeMap::new();
        let mut layouts = source_closure::CallableLayouts::default();
        assert_eq!(instances.instances().len(), function_ids.len());
        assert_eq!(module.functions.len(), function_ids.len());
        for instance in instances.instances() {
            let declaration = &declarations[instance.template_index];
            let key = instances.source(instance.source).unwrap();
            assert_eq!(key, &instance.key);
            assert_eq!(key.symbol(), declaration.symbol);
            assert!(key.type_arguments().is_empty());
            assert!(key.callable_arguments().is_empty());
            assert!(instance.substitutions.is_empty());
            let function = module.function(function_ids[key]).unwrap();
            assert_eq!(function.origin, Origin::Source(declaration.span));
            assert_eq!(function.name, names.symbols()[key.symbol().index()].name());
            assert!(function.receiver.is_none());
            assert!(function.borrow_return.is_none());
            assert!(function.carrier_return.is_none());
            assert!(declaration.callable.owner().is_none());
            assert!(declaration.callable.borrow_return().is_none());
            assert!(matches!(
                declaration.callable.result_source(),
                lang_frontend::type_checking::CallableResultSource::Owned
            ));
            let return_type = declaration.callable.return_type();
            let returns = if builtin_type(typed, return_type) == Some(BuiltinType::Unit) {
                Vec::new()
            } else {
                vec![type_ids[&return_type]]
            };
            assert_eq!(function.return_types, returns);
            let parameters = &function
                .block(function.entry_block().unwrap())
                .unwrap()
                .parameters;
            assert_eq!(parameters.len(), declaration.callable.parameters().len());
            let mut entry_bindings = BTreeMap::new();
            let mut parameter_types = Vec::new();
            for ((parameter, symbol), &entity) in declaration
                .callable
                .parameters()
                .iter()
                .zip(declaration.callable.parameter_symbols())
                .zip(parameters)
            {
                assert_eq!(parameter.mode, ParameterMode::Value);
                let symbol = symbol.expect("real parameter symbol");
                let ty = EntityType::Value(type_ids[&parameter.ty]);
                assert_eq!(function.entity(entity).unwrap().ty, ty);
                let EntityId::Value(value) = entity else {
                    panic!("ordinary own parameter")
                };
                entry_bindings.insert(symbol, LoweredValue::Value(value));
                parameter_types.push(ty);
            }
            let pointer = SsaTypeKind::FunctionPointer {
                signature: CallableSignature {
                    parameters: parameter_types,
                    returns,
                    never_returns: false,
                },
            };
            let pointer = *module
                .type_ids
                .get(&pointer)
                .expect("ordinary predeclared callable ABI");
            layouts.named.insert(instance.source, pointer);
            let Item::Function { form, .. } = &declaration.item else {
                panic!("source function")
            };
            let body = match form {
                FunctionForm::ImplicitUnitBlock(body)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(body),
                    ..
                } => *body,
                _ => panic!("bounded B/C fixtures have block functions"),
            };
            assert!(
                plans
                    .insert(
                        function.id,
                        Plan {
                            source: instance.source,
                            body,
                            return_type,
                            entry_bindings
                        }
                    )
                    .is_none()
            );
        }
        (plans, layouts)
    }
}

use crate::ssa::lower_frontend::declarations::collect_functions;
use crate::ssa::lower_frontend::instances::{FunctionTemplate, plan_instances};
use crate::ssa::lower_frontend::orchestrate::{
    LoweredFile, lower_scalar_file_product, validate_inputs,
};
use crate::ssa::lower_frontend::pending_call::PendingFieldChild;
use crate::ssa::lower_frontend::*;
use crate::ssa::{
    lowering_support::callable_instances::SourceToken,
    model::{CallableSignature, MapContainerKind, Module, SsaTypeKind},
    nullable_map_consume_lowering_tests::nullable_map_guard_sources,
    verify::verify_program,
};
use lang_frontend::{
    lexer::lex,
    name_resolution::{ReferenceTarget, resolve_names},
    ownership_checking::check_ownership,
    parser::{Expression, FunctionBody, FunctionForm, Statement, parse_file},
    source::SourceMap,
    type_checking::{
        IntrinsicTypeConstructor, check_types, complete_file_types, standard_environments,
    },
};

pub(in crate::ssa) use checkpoint::{Checkpoint, Point};

type PendingCallSnapshot = ((usize, usize), Option<LoanId>, Option<PendingFieldChild>);

#[derive(Debug, PartialEq, Eq)]
pub(in crate::ssa) struct Snapshot {
    function: Function,
    block: BlockId,
    source: Option<SourceToken>,
    function_ids: BTreeMap<FunctionInstanceKey, FunctionId>,
    type_ids: BTreeMap<TypeId, SsaTypeId>,
    nullable_maps: BTreeMap<SsaTypeId, SsaTypeId>,
    bindings: BTreeMap<SymbolId, LoweredValue>,
    proofs: BTreeMap<SymbolId, NonNullProof>,
    borrow: BTreeMap<SymbolId, LoanId>,
    result_loans: BTreeMap<SymbolId, Vec<LoanId>>,
    captures: BTreeMap<(usize, usize), LoanId>,
    temporaries: BTreeMap<usize, ValueId>,
    pending: Vec<PendingCallSnapshot>,
}

impl Snapshot {
    pub(in crate::ssa) fn take(lowerer: &ExpressionLowerer<'_>) -> Self {
        // These fixture-proven states cannot be written by the selected guards.
        assert!(lowerer.loops.is_empty());
        assert!(!lowerer.map_loop_owners.has_loop_carriers());
        assert!(lowerer.readonly_receiver.is_none());
        assert!(lowerer.thunk_expression.is_none());
        Self {
            function: lowerer.function.clone(),
            block: lowerer.block,
            source: lowerer.source_token,
            function_ids: lowerer.function_ids.clone(),
            type_ids: lowerer.type_ids.clone(),
            nullable_maps: lowerer.nullable_maps.clone(),
            bindings: lowerer.bindings.clone(),
            proofs: lowerer.non_null_bindings.clone(),
            borrow: lowerer.borrow_bindings.clone(),
            result_loans: lowerer.result_source_loans.clone(),
            captures: lowerer.capture_loans.clone(),
            temporaries: lowerer.temporaries.clone(),
            pending: lowerer
                .pending_call_loans
                .iter()
                .map(|(&key, &loan)| (key, loan, lowerer.pending_call_loans.child(&key)))
                .collect(),
        }
    }
}

pub(in crate::ssa) struct Fixture {
    pub(in crate::ssa) id: &'static str,
    pub(in crate::ssa) path: &'static str,
    pub(in crate::ssa) sources: SourceMap,
    pub(in crate::ssa) parsed: ParsedFile,
    pub(in crate::ssa) names: NameResolution,
    pub(in crate::ssa) typed: TypedFile,
    pub(in crate::ssa) owned: OwnershipCheckedFile,
    pub(in crate::ssa) product: LoweredFile,
    pub(in crate::ssa) instances: instances::FunctionInstancePlan,
    pub(in crate::ssa) layouts: source_closure::CallableLayouts,
    pub(in crate::ssa) plans: BTreeMap<FunctionId, Plan>,
    pub(in crate::ssa) references: BTreeMap<(usize, usize), SymbolId>,
    pub(in crate::ssa) type_ids: BTreeMap<TypeId, SsaTypeId>,
}

pub(in crate::ssa) struct Plan {
    pub(in crate::ssa) source: SourceToken,
    pub(in crate::ssa) body: StatementId,
    pub(in crate::ssa) return_type: TypeId,
    pub(in crate::ssa) entry_bindings: BTreeMap<SymbolId, LoweredValue>,
}

impl Fixture {
    pub(in crate::ssa) fn analyze(id: &'static str) -> Self {
        let (_, path, text, _, _) = nullable_map_guard_sources()
            .into_iter()
            .find(|case| case.0 == id)
            .expect("frozen B/C input");
        let mut sources = SourceMap::new();
        let source = sources.add_source(path, text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty(), "{id}: lexer diagnostics");
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty(), "{id}: parser diagnostics");
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &parsed, &environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{id}: name diagnostics");
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{id}: type diagnostics");
        let complete = complete_file_types(&sources, &parsed, &names, &types, &typed).unwrap();
        assert!(complete.is_same_analysis(&typed));
        assert!(std::ptr::eq(complete.types(), &typed));
        let owned = check_ownership(&sources, &parsed, &names, complete.types()).unwrap();
        assert!(
            owned.diagnostics().is_empty(),
            "{id}: ownership diagnostics"
        );
        assert!(owned.deferred().is_empty(), "{id}: deferred ownership");
        validate_inputs(&sources, &parsed, &names, &typed, &owned).unwrap();
        let product = lower_scalar_file_product(&sources, &parsed, &names, &typed, &owned)
            .expect("ordinary baseline must lower before private checkpoint recovery");
        verify_program(&product.program).expect("only the unmodified baseline is verified");
        assert_eq!(product.program.modules.len(), 1);
        let module = &product.program.modules[0];
        // The helper is deliberately not a generic/closure/Resource/loop harness.
        assert!(typed.nominals().is_empty());
        assert!(owned.iterations().is_empty());
        assert!(owned.captures().is_empty());
        assert!(owned.non_null_assertions().is_empty());
        assert!(module.deinits.is_empty());
        assert!(module.map_results.is_empty());
        assert!(parsed.ast().expressions().iter().all(|(_, node)| !matches!(
            node.payload(),
            Expression::Lambda { .. } | Expression::When { .. }
        )));
        assert!(parsed.ast().statements().iter().all(|(_, node)| !matches!(
            node.payload(),
            Statement::While { .. } | Statement::For { .. } | Statement::Loop { .. }
        )));
        let references = names
            .references()
            .iter()
            .filter_map(|reference| match reference.target() {
                ReferenceTarget::Symbol(symbol) => Some((span_key(reference.span()), *symbol)),
                _ => None,
            })
            .collect();
        let mut type_ids = BTreeMap::new();
        for ty in names
            .symbols()
            .iter()
            .filter_map(|symbol| typed.symbol_type(symbol.id()))
            .chain(
                parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter_map(|(id, _)| typed.expression_type(id)),
            )
            .chain(
                [
                    BuiltinType::Int,
                    BuiltinType::String,
                    BuiltinType::Boolean,
                    BuiltinType::Unit,
                ]
                .into_iter()
                .filter_map(|ty| typed.types().builtin(ty)),
            )
        {
            identity::map_type(module, &typed, ty, &mut type_ids);
        }
        let declarations = collect_functions(&parsed, &names, &typed).unwrap();
        assert!(
            declarations
                .iter()
                .all(|declaration| declaration.callable.type_parameters().is_empty())
        );
        let templates = declarations
            .iter()
            .map(|declaration| FunctionTemplate {
                symbol: declaration.symbol,
                type_parameters: declaration.callable.type_parameters().to_vec(),
                span: declaration.span,
            })
            .collect::<Vec<_>>();
        // Empty deinit spans are proven by the complete ordinary module and no nominals.
        let instances = plan_instances(&parsed, &typed, &owned, &templates, &[]).unwrap();
        let (plans, layouts) = identity::plans(
            module,
            &names,
            &typed,
            &declarations,
            &instances,
            &product.function_ids,
            &type_ids,
        );
        Self {
            id,
            path,
            sources,
            parsed,
            names,
            typed,
            owned,
            product,
            instances,
            layouts,
            plans,
            references,
            type_ids,
        }
    }

    pub(in crate::ssa) fn other_source(&self, source: SourceToken) -> Option<SourceToken> {
        self.instances
            .instances()
            .iter()
            .find(|instance| instance.source != source)
            .map(|instance| instance.source)
    }

    pub(in crate::ssa) fn span(&self, expression: ExpressionId) -> Span {
        self.parsed
            .ast()
            .expressions()
            .get(expression)
            .unwrap()
            .span()
    }

    pub(in crate::ssa) fn symbol(&self, name: &str) -> SymbolId {
        let symbols = self
            .names
            .symbols()
            .iter()
            .filter(|symbol| symbol.name() == name && !symbol.is_synthetic())
            .collect::<Vec<_>>();
        assert_eq!(symbols.len(), 1, "{}: unique {name}", self.id);
        symbols[0].id()
    }

    pub(in crate::ssa) fn reject<T>(
        &self,
        result: Result<T, LoweringError>,
        kind: LoweringErrorKind,
        span: Span,
    ) {
        let Err(failure) = result else {
            panic!("{}: direct guard accepted mutation", self.id)
        };
        assert_eq!(failure.kind, kind, "{}: direct guard kind", self.id);
        assert_eq!(failure.span, Some(span), "{}: exact source origin", self.id);
        assert_eq!(
            self.sources.source_name(span.source_id()).unwrap(),
            self.path
        );
        let text = self.sources.source_text(span.source_id()).unwrap();
        assert_eq!(
            self.sources.slice(span).unwrap(),
            &text[span.start()..span.end()]
        );
        assert!(!self.sources.slice(span).unwrap().is_empty());
    }

    pub(in crate::ssa) fn with_lowerer<T>(
        &self,
        state: &mut Checkpoint,
        check: impl FnOnce(&mut ExpressionLowerer<'_>) -> T,
    ) -> T {
        let module = &self.product.program.modules[0];
        let plan = &self.plans[&state.function.id];
        let empty = BTreeMap::new();
        let mut lowerer = ExpressionLowerer {
            // C3 still has the genuine entry input seed even after input moved to source.
            map_loop_owners: map_loop_owner::Carriers::new(&plan.entry_bindings),
            map_iteration_state: crate::ssa::lower_frontend::map_iteration::State::default(),
            parsed: &self.parsed,
            names: &self.names,
            typed: &self.typed,
            owned: &self.owned,
            source_text: self.sources.source_text(self.parsed.source_id()).unwrap(),
            references: &self.references,
            function_ids: &state.function_ids,
            source_closures: &self.layouts,
            instance_plan: &self.instances,
            source_token: state.source,
            thunk_expression: None,
            capture_loans: BTreeMap::new(),
            type_ids: &state.type_ids,
            heap_payloads: &empty,
            map_results: &module.map_results,
            nullable_maps: &state.nullable_maps,
            ssa_types: &module.types,
            enum_payloads: &BTreeMap::new(),
            substitutions: &BTreeMap::new(),
            function: &mut state.function,
            block: state.point.body,
            bindings: state.bindings.clone(),
            borrow_bindings: BTreeMap::new(),
            result_source_loans: BTreeMap::new(),
            non_null_bindings: BTreeMap::from([(state.point.root, state.point.proof)]),
            temporaries: BTreeMap::new(),
            pending_call_loans: PendingCallLoans::default(),
            return_type: plan.return_type,
            readonly_receiver: None,
            loops: Vec::new(),
        };
        check(&mut lowerer)
    }
}

pub(in crate::ssa) mod provenance {
    //! Complete published owner incoming graph, rooted at this actual owned parameter.
    use crate::ssa::lower_frontend::map_iteration::source_owner::*;
    use lang_frontend::ownership_checking::{CleanupConditionId, CleanupOwnerValueId};
    use std::collections::{BTreeMap, BTreeSet};

    pub(in crate::ssa) fn parameter(
        owned: &OwnershipCheckedFile,
        symbol: SymbolId,
        origin: Span,
        owner: CleanupOwnerValueId,
        span: Span,
    ) -> Result<CleanupConditionId, LoweringError> {
        rooted(
            owned,
            symbol,
            (symbol, origin, None),
            owner,
            span,
            false,
            None,
        )
    }

    pub(in crate::ssa) fn formed(
        owned: &OwnershipCheckedFile,
        symbol: SymbolId,
        birth: (ExpressionId, Span),
        owner: CleanupOwnerValueId,
        span: Span,
    ) -> Result<CleanupConditionId, LoweringError> {
        rooted(
            owned,
            symbol,
            (symbol, birth.1, Some(birth.0)),
            owner,
            span,
            false,
            None,
        )
    }

    /// Generic body paths retain all published incoming edges, including multiple jumps.
    pub(in crate::ssa) fn flow(
        owned: &OwnershipCheckedFile,
        symbol: SymbolId,
        origin: Span,
        expression: Option<ExpressionId>,
        owner: CleanupOwnerValueId,
        span: Span,
    ) -> Result<CleanupConditionId, LoweringError> {
        rooted(
            owned,
            symbol,
            (symbol, origin, expression),
            owner,
            span,
            true,
            None,
        )
    }

    pub(in crate::ssa) fn entry(
        owned: &OwnershipCheckedFile,
        symbol: SymbolId,
        birth: (SymbolId, Span),
        owner: CleanupOwnerValueId,
        span: Span,
        flow: bool,
    ) -> Result<CleanupConditionId, LoweringError> {
        rooted(
            owned,
            symbol,
            (birth.0, birth.1, None),
            owner,
            span,
            flow,
            None,
        )
    }

    /// Every published incoming path must pass this already proved Exit owner.
    /// Reaching the same birth along another path cannot substitute for continuation.
    pub(in crate::ssa) fn from_exit(
        owned: &OwnershipCheckedFile,
        symbol: SymbolId,
        owner: CleanupOwnerValueId,
        exit: CleanupOwnerValueId,
        span: Span,
    ) -> Result<CleanupConditionId, LoweringError> {
        rooted(
            owned,
            symbol,
            (symbol, span, None),
            owner,
            span,
            true,
            Some(exit),
        )
    }

    fn rooted(
        owned: &OwnershipCheckedFile,
        symbol: SymbolId,
        anchor: (SymbolId, Span, Option<ExpressionId>),
        owner: CleanupOwnerValueId,
        span: Span,
        flow: bool,
        cut: Option<CleanupOwnerValueId>,
    ) -> Result<CleanupConditionId, LoweringError> {
        let (seed_root, origin, expression) = anchor;
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        let conditions = owned.cleanup_conditions();
        let mut pending = vec![owner];
        let mut parents = BTreeMap::new();
        let mut available = BTreeMap::new();
        let mut seeds = BTreeSet::new();
        let mut values = Vec::new();
        while let Some(current) = pending.pop() {
            if available.contains_key(&current) {
                continue;
            }
            if cut == Some(current) {
                let Some(CleanupOwnerValue::IterationPhi {
                    statement,
                    boundary: IterationPhiBoundary::Exit,
                    symbol: actual,
                    origin,
                }) = conditions.owner_value(current)
                else {
                    return Err(bad());
                };
                if *actual != symbol {
                    return Err(bad());
                }
                let plan = owned.map_iteration(*statement).ok_or_else(bad)?;
                let phi = phi(
                    conditions,
                    plan,
                    symbol,
                    IterationPhiBoundary::Exit,
                    *origin,
                )?;
                if phi.owner() != current {
                    return Err(bad());
                }
                available.insert(current, phi.availability_condition());
                seeds.insert(current);
                continue;
            }
            match conditions.owner_value(current).ok_or_else(bad)? {
                CleanupOwnerValue::Parameter {
                    symbol: actual,
                    origin: at,
                } if cut.is_none()
                    && expression.is_none()
                    && *actual == seed_root
                    && *at == origin =>
                {
                    let entry = owned
                        .map_iterations()
                        .iter()
                        .flat_map(|p| p.closure_phi_incomings())
                        .flat_map(|i| i.bindings())
                        .flat_map(|b| b.values())
                        .find(|v| v.source() == current)
                        .ok_or_else(bad)?;
                    if conditions.get(entry.condition()) != Some(&CleanupCondition::Always) {
                        return Err(bad());
                    }
                    available.insert(current, entry.condition());
                    seeds.insert(current);
                }
                CleanupOwnerValue::Expression {
                    expression: actual,
                    origin: at,
                } if cut.is_none() && expression == Some(*actual) && *at == origin => {
                    let entry = owned
                        .map_iterations()
                        .iter()
                        .flat_map(|p| p.closure_phi_incomings())
                        .flat_map(|i| i.bindings())
                        .flat_map(|b| b.values())
                        .find(|v| v.source() == current)
                        .ok_or_else(bad)?;
                    if conditions.get(entry.condition()) != Some(&CleanupCondition::Always) {
                        return Err(bad());
                    }
                    available.insert(current, entry.condition());
                    seeds.insert(current);
                }
                CleanupOwnerValue::IterationPhi {
                    statement,
                    boundary,
                    symbol: actual,
                    ..
                } if *actual == symbol => {
                    let plan = owned.map_iteration(*statement).ok_or_else(bad)?;
                    let phi_node = phi(
                        conditions,
                        plan,
                        symbol,
                        *boundary,
                        conditions
                            .owner_value(current)
                            .and_then(|o| match o {
                                CleanupOwnerValue::IterationPhi { origin, .. } => Some(*origin),
                                _ => None,
                            })
                            .ok_or_else(bad)?,
                    )?;
                    if phi_node.owner() != current || !plan.capture_graph().nodes().is_empty() {
                        return Err(unsupported());
                    }
                    available.insert(current, phi_node.availability_condition());
                    let mut inputs = Vec::new();
                    let mut count = 0;
                    for incoming_edge in plan
                        .closure_phi_incomings()
                        .iter()
                        .filter(|i| i.boundary() == *boundary)
                    {
                        let kind = incoming_edge.kind();
                        let point = match kind {
                            IterationPhiIncomingKind::Entry => {
                                DropPoint::AfterExpression(plan.descriptor().source())
                            }
                            IterationPhiIncomingKind::Fallthrough => unique(
                                plan.exits()
                                    .iter()
                                    .filter(|e| e.kind() == IterationExitKind::Fallthrough),
                            )
                            .ok_or_else(bad)?
                            .point(),
                            IterationPhiIncomingKind::Continue(e)
                            | IterationPhiIncomingKind::Break(e) => DropPoint::ControlTransfer(e),
                            IterationPhiIncomingKind::Exhaustion => DropPoint::LoopExit(*statement),
                        };
                        let binding = if flow {
                            let selected = unique(
                                plan.closure_phi_incomings()
                                    .iter()
                                    .filter(|i| i.kind() == kind),
                            )
                            .ok_or_else(bad)?;
                            if selected.point() != point
                                || selected.boundary() != *boundary
                                || conditions.get(selected.condition()).is_none()
                            {
                                return Err(bad());
                            }
                            let binding = unique(
                                selected
                                    .bindings()
                                    .iter()
                                    .filter(|binding| binding.target() == phi_node.owner()),
                            )
                            .ok_or_else(bad)?;
                            if binding.target() != phi_node.owner()
                                || binding.availability_selector()
                                    != phi_node.availability_selector()
                                || binding.presence_source()
                                    != IterationPhiPresenceSource::StaticConditions
                                || !binding.capture_slots_to_clear().is_empty()
                                || !binding.root_sources().is_empty()
                                || !binding.selector_writes().is_empty()
                                || !binding.origins().is_empty()
                            {
                                return Err(bad());
                            }
                            binding
                        } else {
                            incoming(conditions, plan, phi_node, kind, point, span)?
                        };
                        count += 1;
                        match binding.values() {
                            [] if flow
                                && conditions.get(binding.available_when())
                                    == Some(&CleanupCondition::Never) => {}
                            [] => return Err(unsupported()),
                            [value] if value.condition() == binding.available_when() => {
                                let header = phi(
                                    conditions,
                                    plan,
                                    symbol,
                                    IterationPhiBoundary::Header,
                                    match conditions.owner_value(current) {
                                        Some(CleanupOwnerValue::IterationPhi {
                                            origin, ..
                                        }) => *origin,
                                        _ => return Err(bad()),
                                    },
                                )?;
                                if !flow
                                    && kind != IterationPhiIncomingKind::Entry
                                    && value.source() != header.owner()
                                {
                                    return Err(bad());
                                }
                                values.push((value.source(), value.condition()));
                                inputs.push(value.source());
                                pending.push(value.source());
                            }
                            _ => return Err(bad()),
                        }
                    }
                    let expected = if flow {
                        usize::from(*boundary == IterationPhiBoundary::Header)
                            + plan
                                .exits()
                                .iter()
                                .filter(|exit| match boundary {
                                    IterationPhiBoundary::Header => matches!(
                                        exit.kind(),
                                        IterationExitKind::Fallthrough
                                            | IterationExitKind::Continue(_)
                                    ),
                                    IterationPhiBoundary::Exit => matches!(
                                        exit.kind(),
                                        IterationExitKind::Break(_) | IterationExitKind::Exhaustion
                                    ),
                                })
                                .count()
                    } else if *boundary == IterationPhiBoundary::Header {
                        1 + usize::from(plan.exits().iter().any(|e| {
                            matches!(
                                e.kind(),
                                IterationExitKind::Fallthrough | IterationExitKind::Continue(_)
                            )
                        }))
                    } else {
                        1 + usize::from(
                            plan.exits()
                                .iter()
                                .any(|e| matches!(e.kind(), IterationExitKind::Break(_))),
                        )
                    };
                    if count != expected {
                        return Err(bad());
                    }
                    parents.insert(current, inputs);
                }
                CleanupOwnerValue::Parameter { .. } | CleanupOwnerValue::Expression { .. } => {
                    return Err(bad());
                }
                _ => return Err(unsupported()),
            }
        }
        if values.iter().any(|(source, condition)| {
            available.get(source).is_none_or(|known| {
                if flow {
                    !implies(conditions, *condition, *known)
                } else {
                    known != condition
                }
            })
        }) {
            return Err(bad());
        }
        loop {
            let before = seeds.len();
            for (target, inputs) in &parents {
                if inputs.iter().any(|source| seeds.contains(source)) {
                    seeds.insert(*target);
                }
            }
            if seeds.len() == before {
                break;
            }
        }
        if !available.keys().all(|id| seeds.contains(id)) {
            return Err(bad());
        }
        available.get(&owner).copied().ok_or_else(bad)
    }

    fn implies(
        conditions: &CleanupConditions,
        path: CleanupConditionId,
        live: CleanupConditionId,
    ) -> bool {
        fn visit(
            conditions: &CleanupConditions,
            path: CleanupConditionId,
            live: CleanupConditionId,
            seen: &mut BTreeSet<CleanupConditionId>,
        ) -> bool {
            if path == live
                || conditions.get(path) == Some(&CleanupCondition::Never)
                || conditions.get(live) == Some(&CleanupCondition::Always)
            {
                return true;
            }
            if !seen.insert(path) {
                return false;
            }
            let result = match (conditions.get(path), conditions.get(live)) {
                (
                    Some(CleanupCondition::Choice { selector, branches }),
                    Some(CleanupCondition::Choice {
                        selector: expected,
                        branches: live_branches,
                    }),
                ) => {
                    if selector == expected {
                        branches.len() == live_branches.len()
                            && branches
                                .iter()
                                .zip(live_branches)
                                .all(|(branch, live)| visit(conditions, *branch, *live, seen))
                    } else {
                        branches
                            .iter()
                            .all(|branch| visit(conditions, *branch, live, seen))
                    }
                }
                _ => false,
            };
            seen.remove(&path);
            result
        }
        visit(conditions, path, live, &mut BTreeSet::new())
    }

    pub(in crate::ssa) fn equal_on_path(
        conditions: &CleanupConditions,
        actual: CleanupConditionId,
        expected: CleanupConditionId,
        path: CleanupConditionId,
    ) -> bool {
        use lang_frontend::ownership_checking::CleanupSelectorId;
        enum Value {
            Known(bool),
            Select(CleanupSelectorId, usize),
        }
        fn value(
            conditions: &CleanupConditions,
            id: CleanupConditionId,
            selected: &BTreeMap<CleanupSelectorId, usize>,
        ) -> Option<Value> {
            match conditions.get(id)? {
                CleanupCondition::Always => Some(Value::Known(true)),
                CleanupCondition::Never => Some(Value::Known(false)),
                CleanupCondition::Choice { selector, branches } => match selected.get(selector) {
                    Some(index) => value(conditions, *branches.get(*index)?, selected),
                    None => Some(Value::Select(*selector, branches.len())),
                },
            }
        }
        fn compare(
            conditions: &CleanupConditions,
            actual: CleanupConditionId,
            expected: CleanupConditionId,
            path: CleanupConditionId,
            selected: &mut BTreeMap<CleanupSelectorId, usize>,
        ) -> bool {
            if actual == expected {
                return true;
            }
            let Some(path_value) = value(conditions, path, selected) else {
                return false;
            };
            if matches!(path_value, Value::Known(false)) {
                return true;
            }
            let Some(actual_value) = value(conditions, actual, selected) else {
                return false;
            };
            let Some(expected_value) = value(conditions, expected, selected) else {
                return false;
            };
            let pick = match (path_value, actual_value, expected_value) {
                (Value::Known(true), Value::Known(a), Value::Known(b)) => return a == b,
                (Value::Select(selector, count), _, _)
                | (_, Value::Select(selector, count), _)
                | (_, _, Value::Select(selector, count)) => (selector, count),
                _ => return false,
            };
            if pick.1 == 0 {
                return false;
            }
            for branch in 0..pick.1 {
                selected.insert(pick.0, branch);
                if !compare(conditions, actual, expected, path, selected) {
                    selected.remove(&pick.0);
                    return false;
                }
            }
            selected.remove(&pick.0);
            true
        }
        compare(conditions, actual, expected, path, &mut BTreeMap::new())
    }
}
