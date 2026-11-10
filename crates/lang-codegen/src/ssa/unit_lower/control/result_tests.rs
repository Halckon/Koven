//! Contextual null delivery must not lend another control's result identity.
use super::super::*;
use super::*;
use crate::ssa::model::{Module, SsaTypeKind};
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

struct Fixture {
    sources: SourceMap,
    parsed: ParsedFile,
    names: ValidatedCompilationUnitNames,
    typed: ValidatedCompilationUnitTypes,
    owned: ValidatedCompilationUnitOwnership,
    program: Program,
    entry: FunctionId,
    source: SourceUnitId,
    type_ids: BTreeMap<UnitTypeId, SsaTypeId>,
}

impl Fixture {
    fn new(text: &str) -> Self {
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
        let (program, entry) = crate::ssa::refine::with_per_family_lowering(|| {
            lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &types_environment,
                &typed,
                &owned,
                declaration(&names, "p", "entry"),
            )
        })
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

    fn function(&self) -> &Function {
        self.program.modules[0].function(self.entry).unwrap()
    }

    fn with_lowerer<T>(
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
        let mut function_ids = BTreeMap::new();
        for (instance, emitted) in instances.iter().zip(&module.functions) {
            function_ids.insert(instance.key().clone(), emitted.id);
        }
        let instance = instances
            .iter()
            .find(|instance| function_ids.get(instance.key()) == Some(&self.entry))
            .unwrap();
        let mut abi_module = Module {
            id: module.id,
            name: module.name.clone(),
            types: module.types.clone(),
            type_ids: module.type_ids.clone(),
            named_type_ids: module.named_type_ids.clone(),
            type_origins: module.type_origins.clone(),
            functions: module.functions.clone(),
            deinits: module.deinits.clone(),
            map_results: module.map_results.clone(),
            nullable_maps: module.nullable_maps.clone(),
            integer_ranges: module.integer_ranges.clone(),
        };
        let mut abi_types = type_lower::UnitTypeLowering::new(&self.names);
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
        let callable_abi = callable_abi::CallableAbi::declare(
            &mut abi_module,
            &instances,
            &source_plan,
            &layouts,
            typed,
            &mut abi_types,
        )
        .unwrap();
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
            migrated_family_compatibility: false,
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
            nullable_maps: &module.nullable_maps,
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
            current_receiver: None,
            consumed_receiver: None,
            closure_binding_context: false,
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
        UnitTypeKind::Nullable(inner) => {
            let inner = map_type(module, typed, *inner, mapped)?;
            if module.type_kind(inner) == Some(&SsaTypeKind::StringOwner) {
                SsaTypeKind::InlineNullable { inner, niche: true }
            } else {
                SsaTypeKind::NullableHandle { inner }
            }
        }
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => {
            assert!(arguments.is_empty());
            let name = format!("class#d{}.u{}", declaration.index(), ty.index());
            let id = *module.named_type_ids.get(&name)?;
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

#[test]
fn contextual_null_result_requires_exact_target_and_control_identity() {
    let fixture = Fixture::new(
        r#"fun consume(own v: String?): Unit {}
fun entry(flag: Boolean): Unit {
    val value = if (flag) { ("value" + "") } else { if (flag) { null } else { null } }
    consume(value)
}"#,
    );
    let controls = fixture
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::If { .. }).then_some((id, node.span()))
        })
        .collect::<Vec<_>>();
    let mut function = fixture.function().clone();
    fixture.with_lowerer(&mut function, |lowerer| {
        let control_with_inner = |builtin| {
            controls
                .iter()
                .copied()
                .find(|(expression, _)| {
                    let ty = lowerer
                        .typed
                        .expression_type(UnitExpressionId::new(lowerer.source_unit, *expression))
                        .unwrap();
                    matches!(lowerer.typed.types().get(ty), Some(UnitTypeKind::Nullable(inner))
                if builtin_type(lowerer.typed, *inner) == Some(builtin))
                })
                .unwrap()
        };
        let (outer, outer_span) = control_with_inner(BuiltinType::String);
        let (nested, nested_span) = control_with_inner(BuiltinType::Nothing);
        let outer_plan = lowerer.control_result(outer, None, outer_span).unwrap();
        let target = outer_plan.string.unwrap();
        let nested_plan = lowerer
            .control_result(nested, Some(target), nested_span)
            .unwrap();
        assert_eq!(
            nested_plan.control,
            UnitExpressionId::new(lowerer.source_unit, nested)
        );
        assert!(nested_plan.required);
        assert_eq!(nested_plan.string, Some(target));
        // An unresolved Nothing? has no stand-alone String? storage entitlement.
        assert!(matches!(
            lowerer.control_result(nested, None, nested_span),
            Err(LoweringError {
                kind: LoweringErrorKind::UnsupportedNode,
                ..
            })
        ));
        let boolean = lowerer.typed.types().builtin(BuiltinType::Boolean).unwrap();
        let boolean_storage = lowerer.type_ids[&boolean];
        let before = lowerer.function.clone();
        for (expression, expected, span) in [
            (nested, (target.0, boolean_storage), nested_span),
            (nested, (boolean, target.1), nested_span),
            (outer, target, outer_span),
        ] {
            assert!(matches!(
                lowerer.control_result(expression, Some(expected), span),
                Err(LoweringError {
                    kind: LoweringErrorKind::MissingFact,
                    ..
                })
            ));
        }
        let Expression::If {
            condition,
            then_branch,
            else_branch,
            ..
        } = *lowerer
            .parsed
            .ast()
            .expressions()
            .get(nested)
            .unwrap()
            .payload()
        else {
            unreachable!()
        };
        assert_eq!(
            lowerer
                .lower_conditional(
                    nested,
                    condition,
                    then_branch,
                    else_branch,
                    0,
                    1,
                    nested_span,
                    outer_plan
                )
                .unwrap_err()
                .kind,
            LoweringErrorKind::MissingFact
        );
        assert_eq!(
            *lowerer.function, before,
            "wrong control must fail before creating CFG or cleanup"
        );
    });
}
