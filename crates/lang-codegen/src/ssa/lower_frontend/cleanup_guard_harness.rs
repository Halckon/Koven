//! Private guard tests start with ordinary, complete source and verified lowering.
//! Recovered checkpoints use actual SSA identities; no frontend fact is synthesized.
use super::*;
use crate::ssa::model::{Definition, Module, Program, SsaTypeKind};
use lang_frontend::{
    lexer::lex,
    name_resolution::{ReferenceTarget, resolve_names},
    ownership_checking::check_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{IntrinsicTypeConstructor, check_types, complete_file_types, standard_environments},
};

pub(super) struct Fixture {
    pub(super) sources: SourceMap,
    pub(super) parsed: ParsedFile,
    pub(super) names: NameResolution,
    pub(super) typed: TypedFile,
    pub(super) owned: OwnershipCheckedFile,
    pub(super) program: Program,
    pub(super) type_ids: BTreeMap<TypeId, SsaTypeId>,
}

impl Fixture {
    pub(super) fn analyze(text: &str) -> Self {
        let mut sources = SourceMap::new();
        let source = sources.add_source("private-cleanup-guard.ko", text).unwrap();
        let lexed = lex(&sources, source).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        let names = resolve_names(&sources, &parsed, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty());
        complete_file_types(&sources, &parsed, &names, &types, &typed).unwrap();
        let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
        assert!(owned.diagnostics().is_empty());
        let program = orchestrate::lower_scalar_file(&sources, &parsed, &names, &typed, &owned)
            .expect("real complete/ownership/lowering baseline");
        crate::ssa::verify::verify_program(&program).unwrap();
        let mut type_ids = BTreeMap::new();
        for ty in names.symbols().iter().filter_map(|symbol| typed.symbol_type(symbol.id()))
            .chain(parsed.ast().expressions().iter().filter_map(|(id, _)| typed.expression_type(id)))
        {
            map_type(&program.modules[0], &names, &typed, ty, &mut type_ids);
        }
        Self { sources, parsed, names, typed, owned, program, type_ids }
    }

    pub(super) fn entry(&self) -> &Function {
        self.program.modules[0].functions.iter().find(|function| function.name == "entry").unwrap()
    }

    pub(super) fn symbol(&self, name: &str) -> SymbolId {
        let mut matches = self.names.symbols().iter().filter(|symbol| symbol.name() == name);
        let symbol = matches.next().unwrap().id();
        assert!(matches.next().is_none(), "fixture symbol must be unambiguous: {name}");
        symbol
    }

    /// Empty runtime state is intentional. Each test installs its observed checkpoint.
    pub(super) fn with_lowerer<T>(&self, function: &mut Function, check: impl FnOnce(&mut ExpressionLowerer<'_>) -> T) -> T {
        let module = &self.program.modules[0];
        let references = self.names.references().iter().filter_map(|reference| match reference.target() {
            ReferenceTarget::Symbol(symbol) => Some((span_key(reference.span()), *symbol)),
            _ => None,
        }).collect();
        let heap_payloads = module.types.iter().enumerate().filter_map(|(index, kind)| match kind {
            SsaTypeKind::HeapOwner { payload: Some(payload), .. } => Some((SsaTypeId { module: module.id, index }, *payload)),
            _ => None,
        }).collect();
        let function_ids = BTreeMap::new();
        let source_closures = source_closure::CallableLayouts::default();
        let instance_plan = instances::FunctionInstancePlan::default();
        let enum_payloads = BTreeMap::new();
        let substitutions = BTreeMap::new();
        let block = function.entry_block().unwrap();
        let mut lowerer = ExpressionLowerer {
            map_loop_owners: map_loop_owner::Carriers::default(),
            parsed: &self.parsed, names: &self.names, typed: &self.typed, owned: &self.owned,
            source_text: self.sources.source_text(self.parsed.source_id()).unwrap(),
            references: &references, function_ids: &function_ids, source_closures: &source_closures,
            instance_plan: &instance_plan, source_token: None, thunk_expression: None,
            capture_loans: BTreeMap::new(), type_ids: &self.type_ids, heap_payloads: &heap_payloads,
            map_results: &module.map_results, ssa_types: &module.types, enum_payloads: &enum_payloads,
            substitutions: &substitutions, function, block, bindings: BTreeMap::new(),
            borrow_bindings: BTreeMap::new(), result_source_loans: BTreeMap::new(),
            non_null_bindings: BTreeMap::new(), temporaries: BTreeMap::new(),
            pending_call_loans: PendingCallLoans::default(),
            return_type: self.typed.types().builtin(BuiltinType::Int).unwrap(),
            readonly_receiver: None, loops: Vec::new(),
        };
        check(&mut lowerer)
    }
}

// Read only the real module's already-emitted types, including canonical nominal names.
fn map_type(module: &Module, names: &NameResolution, typed: &TypedFile, ty: TypeId, mapped: &mut BTreeMap<TypeId, SsaTypeId>) -> Option<SsaTypeId> {
    if let Some(id) = mapped.get(&ty) { return Some(*id); }
    let kind = match typed.types().get(ty)? {
        TypeKind::Builtin(builtin) => match builtin {
            BuiltinType::Int => SsaTypeKind::Integer { bits: 32, signed: true },
            BuiltinType::Long => SsaTypeKind::Integer { bits: 64, signed: true },
            BuiltinType::Boolean => SsaTypeKind::Boolean,
            BuiltinType::String => SsaTypeKind::StringOwner,
            BuiltinType::Unit => SsaTypeKind::Unit,
            _ => return None,
        },
        TypeKind::Nullable(inner) => SsaTypeKind::NullableHandle { inner: map_type(module, names, typed, *inner, mapped)? },
        TypeKind::Nominal { nominal, .. } => {
            let name = format!("{}#t{}", names.symbols()[nominal.symbol().index()].name(), ty.index());
            let id = *module.named_type_ids.get(&name)?;
            mapped.insert(ty, id);
            return Some(id);
        }
        TypeKind::Intrinsic { constructor, arguments } => match constructor {
            IntrinsicTypeConstructor::List | IntrinsicTypeConstructor::Array | IntrinsicTypeConstructor::MutableList => {
                let [element] = arguments.as_slice() else { return None; };
                let kind = match constructor {
                    IntrinsicTypeConstructor::List => crate::ssa::model::SequentialContainerKind::List,
                    IntrinsicTypeConstructor::Array => crate::ssa::model::SequentialContainerKind::Array,
                    _ => crate::ssa::model::SequentialContainerKind::MutableList,
                };
                SsaTypeKind::SequentialContainer { kind, element: map_type(module, names, typed, *element, mapped)? }
            }
            IntrinsicTypeConstructor::Box | IntrinsicTypeConstructor::Rc => {
                let prefix = if *constructor == IntrinsicTypeConstructor::Box { "Box" } else { "Rc" };
                let id = *module.named_type_ids.get(&format!("{prefix}#t{}", ty.index()))?;
                mapped.insert(ty, id);
                return Some(id);
            }
            _ => return None,
        },
        _ => return None,
    };
    let id = *module.type_ids.get(&kind)?;
    mapped.insert(ty, id);
    Some(id)
}

pub(super) fn parameter(function: &Function, entity: EntityId) -> (BlockId, usize) {
    let Definition::BlockParameter { block, index } = function.entity(entity).unwrap().definition else { panic!("actual block parameter required") };
    assert_eq!(function.block(block).unwrap().parameters[index], entity);
    (block, index)
}

pub(super) fn edges(kind: &TerminatorKind) -> Vec<&Edge> {
    match kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional { when_true, when_false, .. } => vec![when_true, when_false],
        TerminatorKind::NullableBranch { when_null, when_non_null, .. } => vec![when_null, when_non_null],
        _ => Vec::new(),
    }
}

pub(super) fn edges_mut(kind: &mut TerminatorKind) -> Vec<&mut Edge> {
    match kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional { when_true, when_false, .. } => vec![when_true, when_false],
        TerminatorKind::NullableBranch { when_null, when_non_null, .. } => vec![when_null, when_non_null],
        _ => Vec::new(),
    }
}

pub(super) fn incoming(function: &Function, target: BlockId) -> Vec<(BlockId, Edge)> {
    function.blocks.iter().flat_map(|block| block.terminator.iter().flat_map(|term| edges(&term.kind)).filter(move |edge| edge.target == target).map(move |edge| (block.id, edge.clone()))).collect()
}

pub(super) fn unsupported<T: std::fmt::Debug>(result: Result<T, LoweringError>, span: Span) {
    let error = result.expect_err("private guard must reject before final verification");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(span));
}
