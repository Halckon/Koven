//! Real ordinary-unit inputs and SSA identities for private guard checkpoints.
use super::super::super::*;
use crate::ssa::model::{Definition, Edge, Module, SsaTypeKind};
use crate::ssa::unit_lower_test_support::{declaration, parsed};
use lang_frontend::{
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{ValidatedCompilationUnitOwnership, check_compilation_unit_ownership},
    parser::ParsedFile,
    type_checking::{
        IntrinsicTypeConstructor, NominalKind, ValidatedCompilationUnitTypes,
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

pub(in crate::ssa::unit_lower) struct Fixture {
    pub sources: SourceMap,
    pub parsed: ParsedFile,
    pub names: ValidatedCompilationUnitNames,
    pub typed: ValidatedCompilationUnitTypes,
    pub owned: ValidatedCompilationUnitOwnership,
    pub program: Program,
    pub entry: FunctionId,
    pub source: SourceUnitId,
    pub type_ids: BTreeMap<UnitTypeId, SsaTypeId>,
}

impl Fixture {
    pub fn new(text: &str) -> Self {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{text}");
        let (source, parsed) = parsed(&mut sources, "p/guard.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/guard.ko", source, &parsed)];
        let (names_environment, types_environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &names_environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed =
            check_compilation_unit_types(&sources, &inputs, &names, &types_environment).unwrap();
        complete_compilation_unit_types(&sources, &inputs, &names, &types_environment, &typed)
            .expect("ordinary source completes before any guard mutation");
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types_environment, &typed)
                .unwrap()
                .validate()
                .unwrap();
        let (program, entry) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types_environment,
            &typed,
            &owned,
            declaration(&names, "p", "entry"),
        )
        .expect("ordinary source lowers before private checkpoint recovery");
        verify_program(&program).expect("unmodified ordinary-unit SSA baseline verifies");
        let source = names.names().source_units()[0].source_unit();
        let mut type_ids = BTreeMap::new();
        let references = symbol_references(&names, source, Namespace::Value);
        for ty in references
            .values()
            .filter_map(|symbol| typed.types().symbol_type(*symbol))
            .chain(parsed.ast().expressions().iter().filter_map(|(id, _)| {
                typed
                    .types()
                    .expression_type(UnitExpressionId::new(source, id))
            }))
        {
            map_type(&program.modules[0], typed.types(), ty, &mut type_ids);
        }
        Self {
            sources,
            parsed,
            names,
            typed,
            owned,
            program,
            entry,
            source,
            type_ids,
        }
    }

    pub fn function(&self) -> &Function {
        self.program.modules[0].function(self.entry).unwrap()
    }

    pub fn symbol(&self, name: &str) -> UnitSymbolId {
        let symbols = symbol_references(&self.names, self.source, Namespace::Value)
            .into_values()
            .filter(|symbol| {
                self.names.names().source_units()[symbol.source_unit().index()]
                    .resolution()
                    .symbols()[symbol.symbol().index()]
                .name()
                    == name
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(symbols.len(), 1, "unambiguous real source symbol {name}");
        *symbols.first().unwrap()
    }

    /// This is the real entry seed, not a claim to snapshot an in-flight lowerer.
    /// Each caller restores its documented checkpoint from the original SSA columns.
    pub fn with_lowerer<T>(
        &self,
        function: &mut Function,
        check: impl FnOnce(&mut UnitExpressionLowerer<'_>) -> T,
    ) -> T {
        assert_eq!(function.id, self.entry);
        let typed = self.typed.types();
        let owned = self.owned.ownership();
        let module = &self.program.modules[0];
        let parsed_files = [&self.parsed];
        let (instances, _, source_plan) = plan_unit_instances_from_facts(
            &parsed_files,
            &self.names,
            typed,
            owned,
            declaration(&self.names, "p", "entry"),
            MAX_UNIT_GENERIC_INSTANCES,
        )
        .unwrap()
        .into_parts();
        // These fixtures contain no closure, deinit, receiver, generic or range ABI.
        // Consequently the source-instance order is exactly the emitted function order.
        assert!(owned.closures().is_empty());
        assert!(owned.borrow_return_origins().is_empty());
        assert!(owned.borrow_results().range_return_origins().is_empty());
        assert!(owned.borrow_results().range_uses().is_empty());
        assert!(typed.map_constructions().is_empty());
        assert!(self.parsed.ast().expressions().iter().all(|(_, node)| {
            !matches!(
                node.payload(),
                Expression::Lambda { .. } | Expression::CallableReference { .. }
            )
        }));
        assert_eq!(instances.len(), module.functions.len());
        let mut function_ids = BTreeMap::new();
        for (instance, emitted) in instances.iter().zip(&module.functions) {
            assert!(instance.key().deinit_owner().is_none());
            assert!(instance.key().callable_arguments().is_empty());
            assert!(instance.substitutions().is_empty());
            assert!(instance.key().static_self().is_none());
            assert_eq!(
                source_plan.source(instance.source_token()),
                Some(instance.key())
            );
            assert_eq!(instance.source_unit(), self.source);
            assert_eq!(emitted.name, instance_function_name(&self.names, instance));
            assert_eq!(emitted.origin, Origin::Source(instance.span()));
            let signature = unit_callable_signature(typed, instance.key().target()).unwrap();
            assert!(signature.receiver().is_none());
            assert!(signature.range_extension().is_none());
            assert!(signature.borrow_return().is_none());
            assert!(emitted.borrow_return.is_none() && emitted.carrier_return.is_none());
            let parameters = &emitted
                .block(emitted.entry_block().unwrap())
                .unwrap()
                .parameters;
            assert_eq!(parameters.len(), signature.parameters().len());
            for (parameter, entity) in signature.parameters().iter().zip(parameters) {
                let target = self.type_ids[&parameter.ty()];
                let expected = match parameter.mode() {
                    ParameterMode::Value => EntityType::Value(target),
                    ParameterMode::Borrow => EntityType::Loan {
                        kind: LoanKind::Shared,
                        target,
                    },
                    ParameterMode::Inout => panic!("fixture has no Inout ABI"),
                };
                assert_eq!(emitted.entity(*entity).unwrap().ty, expected);
                assert_eq!(entity.function(), emitted.id);
            }
            if builtin_type(typed, signature.return_type()) == Some(BuiltinType::Unit) {
                assert!(emitted.return_types.is_empty());
            } else {
                assert_eq!(
                    emitted.return_types,
                    [self.type_ids[&signature.return_type()]]
                );
            }
            function_ids.insert(instance.key().clone(), emitted.id);
        }
        let instance = instances
            .iter()
            .find(|instance| function_ids.get(instance.key()) == Some(&self.entry))
            .unwrap();
        let mut abi_module = module.clone();
        let mut abi_types = type_lower::UnitTypeLowering::new();
        let layouts = closure::layout::declare(
            &mut abi_module,
            &parsed_files,
            &instances,
            &self.names,
            typed,
            owned,
            &mut abi_types,
        )
        .unwrap();
        assert!(layouts.is_empty());
        let callable_abi = callable_abi::CallableAbi::declare(
            &mut abi_module,
            &instances,
            &source_plan,
            &layouts,
            typed,
            &mut abi_types,
        )
        .unwrap();
        assert_eq!(abi_module.types, module.types);
        assert_eq!(abi_module.functions, module.functions);
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
        let mut field_indices = BTreeMap::new();
        for ty in self.type_ids.keys() {
            if let Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) = typed.types().get(*ty)
            {
                assert!(arguments.is_empty());
                let nominal = typed
                    .signatures()
                    .declaration(*declaration)
                    .unwrap()
                    .nominal()
                    .unwrap();
                assert_eq!(nominal.kind(), NominalKind::Class);
                for (index, field) in nominal.fields().iter().enumerate() {
                    field_indices.insert((*ty, field.symbol()), index);
                }
            }
        }
        let enum_payloads = BTreeMap::new();
        let callable_plans = BTreeMap::new();
        let references = symbol_references(&self.names, self.source, Namespace::Value);
        let type_references = symbol_references(&self.names, self.source, Namespace::Type);
        let signature = unit_callable_signature(typed, instance.key().target()).unwrap();
        let block = function.entry_block().unwrap();
        let mut bindings = BTreeMap::new();
        let mut borrow_bindings = BTreeMap::new();
        let mut entry_boolean_roots = std::collections::BTreeSet::new();
        for (parameter, entity) in signature
            .parameters()
            .iter()
            .zip(&function.block(block).unwrap().parameters)
        {
            let symbol = parameter.symbol().unwrap();
            match *entity {
                EntityId::Value(value) => {
                    bindings.insert(symbol, LoweredValue::Value(value));
                }
                EntityId::Loan(loan) => {
                    borrow_bindings.insert(symbol, loan);
                    if typed.types().get(parameter.ty())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                    {
                        entry_boolean_roots.insert(symbol);
                    }
                }
                EntityId::Place(_) => panic!("no Place entry ABI"),
            }
        }
        let mut lowerer = UnitExpressionLowerer {
            sources: &self.sources,
            parsed: &self.parsed,
            source_unit: self.source,
            names: &self.names,
            typed,
            owned,
            constant_owned: None,
            function_ids: &function_ids,
            source_plan: &source_plan,
            callable_abi: &callable_abi,
            source_token: instance.source_token(),
            type_ids: &self.type_ids,
            heap_payloads: &heap_payloads,
            map_results: &module.map_results,
            ssa_types: &module.types,
            enum_payloads: &enum_payloads,
            field_indices: &field_indices,
            substitutions: instance.substitutions(),
            static_self: instance.key().static_self(),
            references: &references,
            type_references: &type_references,
            function,
            block,
            bindings,
            borrow_bindings,
            entry_boolean_roots,
            nullable_proofs: BTreeMap::new(),
            map_borrow_guards: BTreeMap::new(),
            result_source_loans: BTreeMap::new(),
            temporary_range_slots: BTreeMap::new(),
            short_range_ends: BTreeMap::new(),
            current_receiver: None,
            consumed_receiver: None,
            closure_bindings: BTreeMap::new(),
            closure_binding_context: false,
            capture_loans: BTreeMap::new(),
            thunk_expression: None,
            callable_plans: &callable_plans,
            closure_scope: self.entry,
            temporaries: BTreeMap::new(),
            pending_operands: Vec::new(),
            pending_call_frames: Vec::new(),
            loops: Vec::new(),
            return_type: signature.return_type(),
        };
        check(&mut lowerer)
    }
}

fn map_type(
    module: &Module,
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    mapped: &mut BTreeMap<UnitTypeId, SsaTypeId>,
) -> Option<SsaTypeId> {
    if let Some(id) = mapped.get(&ty) {
        return Some(*id);
    }
    let kind = match typed.types().get(ty)? {
        UnitTypeKind::Builtin(builtin) => match builtin {
            BuiltinType::Int => SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            },
            BuiltinType::Boolean => SsaTypeKind::Boolean,
            BuiltinType::String => SsaTypeKind::StringOwner,
            BuiltinType::Unit => SsaTypeKind::Unit,
            _ => return None,
        },
        UnitTypeKind::Nullable(inner) => SsaTypeKind::NullableHandle {
            inner: map_type(module, typed, *inner, mapped)?,
        },
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            assert!(arguments.is_empty());
            let name = format!("class#d{}.u{}", declaration.index(), ty.index());
            let id = *module.named_type_ids.get(&name)?;
            let nominal = typed.signatures().declaration(*declaration)?.nominal()?;
            assert_eq!(nominal.kind(), NominalKind::Class);
            let SsaTypeKind::HeapOwner {
                name: actual,
                payload: Some(payload),
            } = module.type_kind(id)?
            else {
                panic!("canonical source class has a defined heap owner")
            };
            assert_eq!(actual, &name);
            let SsaTypeKind::Aggregate { fields, .. } = module.type_kind(*payload)? else {
                panic!("source class payload")
            };
            assert_eq!(fields.len(), nominal.fields().len());
            for (field, actual) in nominal.fields().iter().zip(fields) {
                assert_eq!(map_type(module, typed, field.ty(), mapped), Some(*actual));
            }
            mapped.insert(ty, id);
            return Some(id);
        }
        UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::List,
            arguments,
        } => {
            let [element] = arguments.as_slice() else {
                panic!("List element type")
            };
            SsaTypeKind::SequentialContainer {
                kind: crate::ssa::model::SequentialContainerKind::List,
                element: map_type(module, typed, *element, mapped)?,
            }
        }
        _ => return None,
    };
    let id = *module.type_ids.get(&kind)?;
    assert_eq!(module.type_kind(id), Some(&kind));
    mapped.insert(ty, id);
    Some(id)
}

pub(in crate::ssa::unit_lower) fn rejected<T>(
    result: Result<T, LoweringError>,
    kind: LoweringErrorKind,
    span: Span,
) {
    let Err(error) = result else {
        panic!("private guard accepted the changed identity")
    };
    assert_eq!(error.kind, kind);
    assert_eq!(error.span, Some(span));
}

pub(in crate::ssa::unit_lower) fn parameter(
    function: &Function,
    entity: EntityId,
) -> (BlockId, usize) {
    let Definition::BlockParameter { block, index } = function.entity(entity).unwrap().definition
    else {
        panic!("actual SSA parameter")
    };
    assert_eq!(function.block(block).unwrap().parameters[index], entity);
    (block, index)
}

pub(in crate::ssa::unit_lower) fn edges(kind: &TerminatorKind) -> Vec<&Edge> {
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

/// Only actual edge arguments establish a carried identity. No type-based search.
pub(in crate::ssa::unit_lower) fn rebound(
    function: &Function,
    birth: EntityId,
    target: BlockId,
) -> EntityId {
    let mut known = std::collections::BTreeSet::from([birth]);
    loop {
        let before = known.len();
        for block in &function.blocks {
            for edge in block.terminator.iter().flat_map(|term| edges(&term.kind)) {
                for (argument, parameter) in edge
                    .arguments
                    .iter()
                    .zip(&function.block(edge.target).unwrap().parameters)
                {
                    if known.contains(argument) {
                        known.insert(*parameter);
                    }
                }
            }
        }
        if known.len() == before {
            break;
        }
    }
    let matches = function
        .block(target)
        .unwrap()
        .parameters
        .iter()
        .filter(|entity| known.contains(entity))
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "one current column for {birth:?} at {target:?}"
    );
    matches[0]
}
