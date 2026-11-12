//! CI-only recovery of bounded ordinary B/C checkpoints, never a lowering input seam.
//! Each fixture first completes the unchanged FE/ownership/SSA pipeline.
mod checkpoint;
mod identity {
    use super::super::super::declarations::FunctionDeclaration;
    use super::*;

    pub(super) fn map_type(
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

    pub(super) fn plans(
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

use super::super::declarations::collect_functions;
use super::super::instances::{FunctionTemplate, plan_instances};
use super::super::pending_call::PendingFieldChild;
use super::super::*;
use super::{LoweredFile, lower_scalar_file_product, unwrap_modified, validate_inputs};
use crate::ssa::{
    lowering_support::callable_instances::SourceToken,
    model::{CallableSignature, Definition, InstructionId, MapContainerKind, Module, SsaTypeKind},
    nullable_map_consume_lowering_tests::nullable_map_guard_sources,
    verify::verify_program,
};
use lang_frontend::{
    lexer::lex,
    name_resolution::{ReferenceTarget, SymbolKind, resolve_names},
    ownership_checking::{DropTarget, check_ownership},
    parser::{Expression, FunctionBody, FunctionForm, NameMarker, Statement, parse_file},
    source::SourceMap,
    type_checking::{
        IntrinsicTypeConstructor, check_types, complete_file_types, standard_environments,
    },
};

pub(in crate::ssa::lower_frontend) use checkpoint::{Checkpoint, Point};

type PendingCallSnapshot = ((usize, usize), Option<LoanId>, Option<PendingFieldChild>);

#[derive(Debug, PartialEq, Eq)]
pub(in crate::ssa::lower_frontend) struct Snapshot {
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
    pub(in crate::ssa::lower_frontend) fn take(lowerer: &ExpressionLowerer<'_>) -> Self {
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

pub(in crate::ssa::lower_frontend) struct Fixture {
    pub(in crate::ssa::lower_frontend) id: &'static str,
    pub(in crate::ssa::lower_frontend) path: &'static str,
    pub(in crate::ssa::lower_frontend) sources: SourceMap,
    pub(in crate::ssa::lower_frontend) parsed: ParsedFile,
    pub(in crate::ssa::lower_frontend) names: NameResolution,
    pub(in crate::ssa::lower_frontend) typed: TypedFile,
    pub(in crate::ssa::lower_frontend) owned: OwnershipCheckedFile,
    product: LoweredFile,
    instances: instances::FunctionInstancePlan,
    layouts: source_closure::CallableLayouts,
    plans: BTreeMap<FunctionId, Plan>,
    references: BTreeMap<(usize, usize), SymbolId>,
    type_ids: BTreeMap<TypeId, SsaTypeId>,
}

struct Plan {
    source: SourceToken,
    body: StatementId,
    return_type: TypeId,
    entry_bindings: BTreeMap<SymbolId, LoweredValue>,
}

impl Fixture {
    pub(in crate::ssa::lower_frontend) fn analyze(id: &'static str) -> Self {
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

    pub(in crate::ssa::lower_frontend) fn other_source(
        &self,
        source: SourceToken,
    ) -> Option<SourceToken> {
        self.instances
            .instances()
            .iter()
            .find(|instance| instance.source != source)
            .map(|instance| instance.source)
    }

    pub(in crate::ssa::lower_frontend) fn span(&self, expression: ExpressionId) -> Span {
        self.parsed
            .ast()
            .expressions()
            .get(expression)
            .unwrap()
            .span()
    }

    pub(in crate::ssa::lower_frontend) fn symbol(&self, name: &str) -> SymbolId {
        let symbols = self
            .names
            .symbols()
            .iter()
            .filter(|symbol| symbol.name() == name && !symbol.is_synthetic())
            .collect::<Vec<_>>();
        assert_eq!(symbols.len(), 1, "{}: unique {name}", self.id);
        symbols[0].id()
    }

    pub(in crate::ssa::lower_frontend) fn reject<T>(
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

    pub(in crate::ssa::lower_frontend) fn with_lowerer<T>(
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
