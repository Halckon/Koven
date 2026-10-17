//! CI-only recovery of bounded ordinary B/C checkpoints, never a lowering input seam.
//! Each fixture first completes the unchanged FE/ownership/SSA pipeline.
mod checkpoint;
mod identity;
mod prelude;
mod state;

use super::super::declarations::collect_functions;
use super::super::instances::{FunctionTemplate, plan_instances};
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
pub(in crate::ssa::lower_frontend) use state::Snapshot;

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
