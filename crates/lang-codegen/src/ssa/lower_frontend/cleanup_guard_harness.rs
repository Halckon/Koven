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
    type_checking::{
        IntrinsicTypeConstructor, check_types, complete_file_types, standard_environments,
    },
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
        let source = sources
            .add_source("private-cleanup-guard.ko", text)
            .unwrap();
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
        {
            map_type(&program.modules[0], &names, &typed, ty, &mut type_ids);
        }
        Self {
            sources,
            parsed,
            names,
            typed,
            owned,
            program,
            type_ids,
        }
    }

    pub(super) fn entry(&self) -> &Function {
        self.program.modules[0]
            .functions
            .iter()
            .find(|function| function.name == "entry")
            .unwrap()
    }

    pub(super) fn symbol(&self, name: &str) -> SymbolId {
        let mut matches = self
            .names
            .symbols()
            .iter()
            .filter(|symbol| symbol.name() == name);
        let symbol = matches.next().unwrap().id();
        assert!(
            matches.next().is_none(),
            "fixture symbol must be unambiguous: {name}"
        );
        symbol
    }

    /// Empty runtime state is intentional. Each test installs its observed checkpoint.
    pub(super) fn with_lowerer<T>(
        &self,
        function: &mut Function,
        check: impl FnOnce(&mut ExpressionLowerer<'_>) -> T,
    ) -> T {
        let module = &self.program.modules[0];
        let references = self
            .names
            .references()
            .iter()
            .filter_map(|reference| match reference.target() {
                ReferenceTarget::Symbol(symbol) => Some((span_key(reference.span()), *symbol)),
                _ => None,
            })
            .collect();
        let heap_payloads = module
            .types
            .iter()
            .enumerate()
            .filter_map(|(index, kind)| match kind {
                SsaTypeKind::HeapOwner {
                    payload: Some(payload),
                    ..
                } => Some((
                    SsaTypeId {
                        module: module.id,
                        index,
                    },
                    *payload,
                )),
                _ => None,
            })
            .collect();
        let function_ids = BTreeMap::new();
        let source_closures = source_closure::CallableLayouts::default();
        let instance_plan = instances::FunctionInstancePlan::default();
        let enum_payloads = BTreeMap::new();
        let substitutions = BTreeMap::new();
        let block = function.entry_block().unwrap();
        let mut lowerer = ExpressionLowerer {
            map_loop_owners: map_loop_owner::Carriers::default(),
            parsed: &self.parsed,
            names: &self.names,
            typed: &self.typed,
            owned: &self.owned,
            source_text: self.sources.source_text(self.parsed.source_id()).unwrap(),
            references: &references,
            function_ids: &function_ids,
            source_closures: &source_closures,
            instance_plan: &instance_plan,
            source_token: None,
            thunk_expression: None,
            capture_loans: BTreeMap::new(),
            type_ids: &self.type_ids,
            heap_payloads: &heap_payloads,
            map_results: &module.map_results,
            nullable_maps: &module.nullable_maps,
            ssa_types: &module.types,
            enum_payloads: &enum_payloads,
            substitutions: &substitutions,
            function,
            block,
            bindings: BTreeMap::new(),
            borrow_bindings: BTreeMap::new(),
            result_source_loans: BTreeMap::new(),
            non_null_bindings: BTreeMap::new(),
            temporaries: BTreeMap::new(),
            pending_call_loans: PendingCallLoans::default(),
            return_type: self.typed.types().builtin(BuiltinType::Int).unwrap(),
            readonly_receiver: None,
            loops: Vec::new(),
        };
        check(&mut lowerer)
    }
}

// Read only the real module's already-emitted types, including canonical nominal names.
fn map_type(
    module: &Module,
    names: &NameResolution,
    typed: &TypedFile,
    ty: TypeId,
    mapped: &mut BTreeMap<TypeId, SsaTypeId>,
) -> Option<SsaTypeId> {
    if let Some(id) = mapped.get(&ty) {
        return Some(*id);
    }
    let kind = match typed.types().get(ty)? {
        TypeKind::Builtin(builtin) => match builtin {
            BuiltinType::Int => SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            },
            BuiltinType::Long => SsaTypeKind::Integer {
                bits: 64,
                signed: true,
            },
            BuiltinType::Boolean => SsaTypeKind::Boolean,
            BuiltinType::String => SsaTypeKind::StringOwner,
            BuiltinType::Unit => SsaTypeKind::Unit,
            _ => return None,
        },
        TypeKind::Nullable(inner) => SsaTypeKind::NullableHandle {
            inner: map_type(module, names, typed, *inner, mapped)?,
        },
        TypeKind::Nominal { nominal, .. } => {
            let name = format!(
                "{}#t{}",
                names.symbols()[nominal.symbol().index()].name(),
                ty.index()
            );
            let id = *module.named_type_ids.get(&name)?;
            mapped.insert(ty, id);
            return Some(id);
        }
        TypeKind::Intrinsic {
            constructor,
            arguments,
        } => match constructor {
            IntrinsicTypeConstructor::List
            | IntrinsicTypeConstructor::Array
            | IntrinsicTypeConstructor::MutableList => {
                let [element] = arguments.as_slice() else {
                    return None;
                };
                let kind = match constructor {
                    IntrinsicTypeConstructor::List => {
                        crate::ssa::model::SequentialContainerKind::List
                    }
                    IntrinsicTypeConstructor::Array => {
                        crate::ssa::model::SequentialContainerKind::Array
                    }
                    _ => crate::ssa::model::SequentialContainerKind::MutableList,
                };
                SsaTypeKind::SequentialContainer {
                    kind,
                    element: map_type(module, names, typed, *element, mapped)?,
                }
            }
            IntrinsicTypeConstructor::Box | IntrinsicTypeConstructor::Rc => {
                let prefix = if *constructor == IntrinsicTypeConstructor::Box {
                    "Box"
                } else {
                    "Rc"
                };
                let id = *module
                    .named_type_ids
                    .get(&format!("{prefix}#t{}", ty.index()))?;
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
    let Definition::BlockParameter { block, index } = function.entity(entity).unwrap().definition
    else {
        panic!("actual block parameter required")
    };
    assert_eq!(function.block(block).unwrap().parameters[index], entity);
    (block, index)
}

pub(super) fn edges(kind: &TerminatorKind) -> Vec<&Edge> {
    match kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => vec![when_null, when_non_null],
        _ => Vec::new(),
    }
}

pub(super) fn edges_mut(kind: &mut TerminatorKind) -> Vec<&mut Edge> {
    match kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => vec![when_null, when_non_null],
        _ => Vec::new(),
    }
}

pub(super) fn incoming(function: &Function, target: BlockId) -> Vec<(BlockId, Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .terminator
                .iter()
                .flat_map(|term| edges(&term.kind))
                .filter(move |edge| edge.target == target)
                .map(move |edge| (block.id, edge.clone()))
        })
        .collect()
}

pub(super) fn unsupported<T: std::fmt::Debug>(result: Result<T, LoweringError>, span: Span) {
    let error = result.expect_err("private guard must reject before final verification");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(span));
}

/// Recover only the active outer context queried by these cleanup guards. Every slot
/// is the actual preheader operand; current/items/total are the fixture's live locals.
pub(super) fn outer_context(
    lowerer: &mut ExpressionLowerer<'_>,
    root: SymbolId,
    statement: StatementId,
) -> NonNullProof {
    let span = lowerer
        .parsed
        .ast()
        .statements()
        .get(statement)
        .unwrap()
        .span();
    let (header, owner, view, body) = lowerer
        .function
        .blocks
        .iter()
        .find_map(|block| {
            let term = block.terminator.as_ref()?;
            if term.origin != Origin::Source(span) {
                return None;
            }
            let TerminatorKind::NullableBranch {
                owner,
                view,
                when_non_null,
                ..
            } = &term.kind
            else {
                return None;
            };
            Some((block.id, *owner, *view, when_non_null.target))
        })
        .unwrap();
    let owner_column = parameter(lowerer.function, EntityId::Value(owner)).1;
    let incoming = incoming(lowerer.function, header);
    let [(preheader, edge)] = incoming.as_slice() else {
        panic!("bounded source has one real preheader")
    };
    assert_eq!(*preheader, lowerer.function.entry_block().unwrap());
    let mut carried = lowerer
        .linear_binding_slots(&BTreeMap::new(), span)
        .unwrap();
    carried.slots = edge
        .arguments
        .iter()
        .enumerate()
        .map(|(column, &source)| {
            let ty = lowerer.function.entity(source).unwrap().ty;
            assert_eq!(
                lowerer
                    .function
                    .entity(lowerer.function.block(header).unwrap().parameters[column])
                    .unwrap()
                    .ty,
                ty
            );
            let symbol = if column == owner_column {
                root
            } else {
                let matches = lowerer
                    .names
                    .symbols()
                    .iter()
                    .filter(|symbol| {
                        matches!(symbol.name(), "items" | "total")
                            && lowerer
                                .typed
                                .symbol_type(symbol.id())
                                .and_then(|ty| lowerer.type_ids.get(&ty))
                                .copied()
                                == Some(ty.semantic_type())
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    matches.len(),
                    1,
                    "real preheader column has one fixture local"
                );
                matches[0].id()
            };
            control::LinearBindingSlot::new(Some(symbol), source, ty)
        })
        .collect();
    lowerer.loops.push(loop_control::LoopContext {
        entry_views: BTreeMap::new(),
        outer_borrowed_while: None,
        header,
        carried,
        continues: Vec::new(),
        breaks: Vec::new(),
        for_loop: None,
        numeric_loop: None,
    });
    let EntityId::Value(body_owner) =
        lowerer.function.block(body).unwrap().parameters[owner_column]
    else {
        panic!("real owner column")
    };
    let EntityType::Loan {
        target: inner,
        kind: LoanKind::Shared,
    } = lowerer.function.entity(EntityId::Loan(view)).unwrap().ty
    else {
        panic!("real nullable branch view")
    };
    assert_eq!(parameter(lowerer.function, EntityId::Loan(view)).0, body);
    NonNullProof::introduced(
        view,
        body_owner,
        inner,
        non_null_proof::ProofScope::While(statement),
    )
}

/// Stop immediately before the real adjacent end/drop pair, retaining the complete
/// earlier instruction prefix and every other block/edge. No predecessor is removed.
pub(super) fn before_cleanup(
    lowerer: &mut ExpressionLowerer<'_>,
    block: BlockId,
    root: SymbolId,
    span: Span,
    inherited: Option<NonNullProof>,
) -> ValueId {
    let instructions = lowerer.function.block(block).unwrap().instructions.clone();
    let drops = instructions
        .iter()
        .enumerate()
        .filter_map(|(index, id)| {
            let instruction = lowerer.function.instruction(*id).unwrap();
            let Operation::Drop { owner } = instruction.operation else {
                return None;
            };
            (instruction.origin == Origin::Source(span)
                && lowerer.function.entity(EntityId::Value(owner)).unwrap().ty
                    == EntityType::Value(
                        lowerer.type_ids[&lowerer.typed.symbol_type(root).unwrap()],
                    ))
            .then_some((index, owner))
        })
        .collect::<Vec<_>>();
    let [(index, owner)] = drops.as_slice() else {
        panic!("one real current-owner cleanup")
    };
    let mut prefix = *index;
    if let Some(mut proof) = inherited {
        assert!(*index > 0);
        let end = lowerer
            .function
            .instruction(instructions[index - 1])
            .unwrap();
        let Operation::BorrowEnd { loan } = end.operation else {
            panic!("actual owner cleanup must first retire its view")
        };
        assert_eq!(end.origin, Origin::Source(span));
        assert_eq!(parameter(lowerer.function, EntityId::Loan(loan)).0, block);
        proof.owner = *owner;
        proof.loan = loan;
        lowerer.non_null_bindings.insert(root, proof);
        prefix -= 1;
    }
    assert_eq!(
        parameter(lowerer.function, EntityId::Value(*owner)).0,
        block
    );
    lowerer.function.blocks[block.index()]
        .instructions
        .truncate(prefix);
    lowerer.function.blocks[block.index()].terminator = None;
    lowerer.block = block;
    lowerer.bindings.insert(root, LoweredValue::Value(*owner));
    // Borrow Boolean parameters are independent, caller-owned, and remain live.
    let entry = lowerer.function.entry_block().unwrap();
    for &entity in &lowerer.function.block(entry).unwrap().parameters {
        if let EntityId::Loan(loan) = entity {
            let parameter = lowerer
                .names
                .symbols()
                .iter()
                .find(|symbol| symbol.name() == "flag")
                .unwrap()
                .id();
            assert_eq!(
                lowerer.function.entity(entity).unwrap().ty,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: lowerer.type_ids[&lowerer.typed.symbol_type(parameter).unwrap()]
                }
            );
            lowerer.borrow_bindings.insert(parameter, loan);
        }
    }
    *owner
}
