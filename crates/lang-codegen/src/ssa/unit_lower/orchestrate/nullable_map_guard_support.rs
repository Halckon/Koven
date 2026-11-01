//! CI-only recovery from six frozen, fully checked ordinary p/q programs.
//! This is a bounded checkpoint reconstruction, never an in-flight/global hook.
use super::super::*;
use crate::ssa::{
    lowering_support::callable_instances::SourceToken,
    model::{MapContainerKind, Module, SsaTypeKind},
    nullable_map_consume_lowering_tests::nullable_map_guard_sources,
    unit_lower_test_support::declaration,
};
use lang_frontend::{
    lexer::lex,
    name_resolution::{ScopeKind, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{ValidatedCompilationUnitOwnership, check_compilation_unit_ownership},
    parser::{ParsedFile, parse_file},
    type_checking::{
        IntrinsicTypeConstructor, ValidatedCompilationUnitTypes, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

use std::collections::BTreeSet;

mod checkpoint;
mod prelude;
pub(in crate::ssa::unit_lower) use checkpoint::{
    Checkpoint, Stage, exit_text, state_text, unchanged,
};

pub(in crate::ssa::unit_lower) struct Fixture {
    pub id: &'static str,
    pub sources: SourceMap,
    pub files: Vec<ParsedFile>,
    pub names: ValidatedCompilationUnitNames,
    pub typed: ValidatedCompilationUnitTypes,
    pub owned: ValidatedCompilationUnitOwnership,
    pub program: Program,
    pub target: FunctionId,
    target_declaration: DeclarationId,
    pub source: SourceUnitId,
    pub type_ids: BTreeMap<UnitTypeId, SsaTypeId>,
}

impl Fixture {
    pub fn new(id: &'static str) -> Self {
        let cases = nullable_map_guard_sources();
        let (_, _, _, provider, consumer) = cases.iter().find(|case| case.0 == id).unwrap();
        let mut sources = SourceMap::new();
        let mut files = Vec::new();
        for (path, text) in [("p/provider.ko", *provider), ("q/consumer.ko", *consumer)] {
            let source = sources.add_source(path, text).unwrap();
            let lexed = lex(&sources, source).unwrap();
            assert!(lexed.diagnostics().is_empty(), "{id}: lex diagnostics");
            let parsed = parse_file(&sources, &lexed).unwrap();
            assert!(parsed.diagnostics().is_empty(), "{id}: parse diagnostics");
            files.push(parsed);
        }
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", files[0].source_id(), &files[0]),
            SourceUnitInput::new("root", "q/consumer.ko", files[1].source_id(), &files[1]),
        ];
        let (names_environment, types_environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &names_environment)
            .unwrap()
            .validate()
            .expect("ordinary names before mutation");
        let typed =
            check_compilation_unit_types(&sources, &inputs, &names, &types_environment).unwrap();
        assert!(typed.diagnostics().is_empty());
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &types_environment, &typed)
                .expect("ordinary type completion before mutation");
        assert!(complete.is_same_analysis(&typed));
        assert!(std::ptr::eq(complete.types(), &typed));
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types_environment, &typed)
                .unwrap();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.deferred().is_empty());
        let owned = owned.validate().unwrap();
        let (program, entry) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types_environment,
            &typed,
            &owned,
            declaration(&names, "q", "entry"),
        )
        .expect("ordinary p/q lowering must succeed before checkpoint recovery");
        verify_program(&program).expect("unmodified ordinary baseline must verify");
        assert_eq!(program.modules.len(), 1);
        let units = names.names().index().source_units();
        assert_eq!(units.len(), 2);
        let provider_units = units
            .iter()
            .filter(|unit| {
                unit.source_id() == files[0].source_id()
                    && unit.key().logical_path().expect("rooted source").as_str() == "p/provider.ko"
            })
            .collect::<Vec<_>>();
        let [provider_unit] = provider_units.as_slice() else {
            panic!("unique provider identity")
        };
        let source = provider_unit.id();
        assert_ne!(
            source,
            units
                .iter()
                .find(|unit| unit.source_id() == files[1].source_id())
                .unwrap()
                .id()
        );
        let target_declaration = declaration(&names, "p", if id == "C3" { "run" } else { "main" });
        let target_data = &names.names().index().declarations()[target_declaration.index()];
        assert_eq!(target_data.source_unit(), source);
        let (_, _, target_span) = unwrap_modified(&files[0], target_data.root()).unwrap();
        let matches = program.modules[0]
            .functions
            .iter()
            .filter(|function| function.origin == Origin::Source(target_span))
            .collect::<Vec<_>>();
        let [function] = matches.as_slice() else {
            panic!("one emitted source function")
        };
        let target = function.id;
        assert_ne!(target, entry);
        let mut type_ids = BTreeMap::new();
        for ty in typed.types().body_symbol_types().values().copied() {
            map_type(&program.modules[0], typed.types(), ty, &mut type_ids);
        }
        for unit in units {
            let parsed = files
                .iter()
                .find(|parsed| parsed.source_id() == unit.source_id())
                .unwrap();
            for (expression, _) in parsed.ast().expressions().iter() {
                if let Some(ty) = typed
                    .types()
                    .expression_type(UnitExpressionId::new(unit.id(), expression))
                {
                    map_type(&program.modules[0], typed.types(), ty, &mut type_ids);
                }
            }
        }
        Self {
            id,
            sources,
            files,
            names,
            typed,
            owned,
            program,
            target,
            target_declaration,
            source,
            type_ids,
        }
    }

    pub fn parsed(&self) -> &ParsedFile {
        &self.files[0]
    }
    pub fn function(&self) -> &Function {
        self.program.modules[0].function(self.target).unwrap()
    }
    pub fn symbol(&self, name: &str) -> UnitSymbolId {
        let source = self
            .names
            .names()
            .source_units()
            .iter()
            .find(|unit| unit.source_unit() == self.source)
            .unwrap();
        let typed = self.typed.types();
        let signature = unit_callable_signature(
            typed,
            UnitCallableTarget::Declaration(self.target_declaration),
        )
        .unwrap();
        let Origin::Source(function_span) = self.function().origin else {
            panic!("actual target source origin")
        };
        // Parameters are published by signatures, not necessarily by body typing.
        let candidates = typed
            .body_symbol_types()
            .keys()
            .copied()
            .chain(
                signature
                    .parameters()
                    .iter()
                    .filter_map(|parameter| parameter.symbol()),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|symbol| {
                if symbol.source_unit() != self.source {
                    return false;
                }
                let resolution = source.resolution();
                let data = &resolution.symbols()[symbol.symbol().index()];
                if data.name() != name {
                    return false;
                }
                let mut scope = Some(data.scope());
                while let Some(id) = scope {
                    let current = &resolution.scopes()[id.index()];
                    if current.kind() == ScopeKind::Lambda {
                        return false;
                    }
                    if current.kind() == ScopeKind::Function {
                        return current.span() == Some(function_span);
                    }
                    scope = current.parent();
                }
                false
            })
            .collect::<Vec<_>>();
        let [symbol] = candidates.as_slice() else {
            panic!("unique actual source symbol {name}")
        };
        assert!(
            typed.symbol_type(*symbol).is_some(),
            "actual typed source symbol {name}"
        );
        *symbol
    }

    pub fn rejected<T>(
        &self,
        result: Result<T, LoweringError>,
        kind: LoweringErrorKind,
        span: Span,
        label: &str,
    ) {
        let Err(error) = result else {
            panic!("{} {label}: changed checkpoint accepted", self.id)
        };
        assert_eq!(error.kind, kind, "{} {label}", self.id);
        assert_eq!(error.span, Some(span), "{} {label}", self.id);
        assert_eq!(
            self.sources.source_name(span.source_id()).unwrap(),
            "p/provider.ko"
        );
        let expected = self.sources.slice(span).unwrap();
        assert!(!expected.is_empty());
        assert_eq!(self.sources.slice(error.span.unwrap()).unwrap(), expected);
        eprintln!(
            "unit {} {label}: {:?}, p/provider.ko: {:?}",
            self.id, kind, expected
        );
    }

    /// Rebuild the real planner/ABI identity graph; only runtime state is restored later.
    pub fn with_lowerer<T>(
        &self,
        stage: Stage,
        function: &mut Function,
        type_ids: &BTreeMap<UnitTypeId, SsaTypeId>,
        nullable_maps: &BTreeMap<SsaTypeId, SsaTypeId>,
        check: impl FnOnce(&mut UnitExpressionLowerer<'_>, SourceToken) -> T,
    ) -> T {
        assert_eq!(function.id, self.target);
        let module = &self.program.modules[0];
        let typed = self.typed.types();
        let owned = self.owned.ownership();
        let parsed_files = self
            .names
            .names()
            .index()
            .source_units()
            .iter()
            .map(|unit| {
                self.files
                    .iter()
                    .find(|file| file.source_id() == unit.source_id())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let (instances, _, source_plan) = plan_unit_instances_from_facts(
            &parsed_files,
            &self.names,
            typed,
            owned,
            declaration(&self.names, "q", "entry"),
            MAX_UNIT_GENERIC_INSTANCES,
        )
        .unwrap()
        .into_parts();
        assert!(owned.closures().is_empty());
        assert!(owned.borrow_return_origins().is_empty());
        assert!(owned.borrow_results().range_return_origins().is_empty());
        assert!(owned.borrow_results().range_uses().is_empty());
        assert!(module.deinits.is_empty());
        assert!(
            self.files
                .iter()
                .all(
                    |file| file.ast().expressions().iter().all(|(_, node)| !matches!(
                        node.payload(),
                        Expression::Lambda { .. } | Expression::CallableReference { .. }
                    ))
                )
        );
        assert_eq!(instances.len(), module.functions.len());
        let mut function_ids = BTreeMap::new();
        for instance in &instances {
            assert!(instance.key().deinit_owner().is_none());
            assert!(instance.key().callable_arguments().is_empty());
            assert!(instance.substitutions().is_empty());
            assert!(instance.key().static_self().is_none());
            assert_eq!(
                source_plan.source(instance.source_token()),
                Some(instance.key())
            );
            let parsed = parsed_files[instance.source_unit().index()];
            assert_eq!(instance.span().source_id(), parsed.source_id());
            let candidates = module
                .functions
                .iter()
                .filter(|emitted| {
                    emitted.name
                        == instance_function_name(&self.names, instance).expect("named package")
                        && emitted.origin == Origin::Source(instance.span())
                })
                .collect::<Vec<_>>();
            let [emitted] = candidates.as_slice() else {
                panic!("exact source-instance emitted identity")
            };
            let signature = unit_callable_signature(typed, instance.key().target()).unwrap();
            assert!(signature.receiver().is_none());
            assert!(signature.range_extension().is_none());
            assert!(signature.borrow_return().is_none());
            assert!(
                emitted.receiver.is_none()
                    && emitted.borrow_return.is_none()
                    && emitted.carrier_return.is_none()
            );
            let parameters = &emitted
                .block(emitted.entry_block().unwrap())
                .unwrap()
                .parameters;
            assert_eq!(parameters.len(), signature.parameters().len());
            for (parameter, entity) in signature.parameters().iter().zip(parameters) {
                assert_eq!(parameter.mode(), ParameterMode::Value);
                assert_eq!(
                    emitted.entity(*entity).unwrap().ty,
                    EntityType::Value(self.type_ids[&parameter.ty()])
                );
                assert_eq!(entity.function(), emitted.id);
            }
            assert_eq!(
                builtin_type(typed, signature.return_type()),
                Some(BuiltinType::Unit)
            );
            assert!(emitted.return_types.is_empty());
            assert!(
                function_ids
                    .insert(instance.key().clone(), emitted.id)
                    .is_none()
            );
        }
        let instance = instances
            .iter()
            .find(|instance| function_ids.get(instance.key()) == Some(&self.target))
            .unwrap();
        assert_eq!(instance.source_unit(), self.source);
        let other = instances
            .iter()
            .find(|candidate| candidate.source_unit() != self.source)
            .unwrap();
        assert_ne!(function_ids[other.key()], self.target);
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
        assert_eq!(
            abi_module, *module,
            "ABI reconstruction adds no types/functions/registries"
        );
        let references = symbol_references(&self.names, self.source, Namespace::Value);
        let type_references = symbol_references(&self.names, self.source, Namespace::Type);
        let signature = unit_callable_signature(typed, instance.key().target()).unwrap();
        let block = function.entry_block().unwrap();
        let mut bindings = BTreeMap::new();
        for (parameter, entity) in signature
            .parameters()
            .iter()
            .zip(&function.block(block).unwrap().parameters)
        {
            let EntityId::Value(value) = *entity else {
                panic!("own-only entry ABI")
            };
            bindings.insert(parameter.symbol().unwrap(), LoweredValue::Value(value));
        }
        let empty = BTreeMap::new();
        let enum_payloads = BTreeMap::new();
        let field_indices = BTreeMap::new();
        let callable_plans = BTreeMap::new();
        let mut lowerer = UnitExpressionLowerer {
            sources: &self.sources,
            parsed: self.parsed(),
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
            heap_payloads: &empty,
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
            borrow_bindings: BTreeMap::new(),
            entry_boolean_roots: Default::default(),
            entry_string_roots: Default::default(),
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
            closure_scope: self.target,
            temporaries: BTreeMap::new(),
            pending_operands: Vec::new(),
            pending_call_frames: Vec::new(),
            loops: Vec::new(),
            map_local_continuation: None,
            return_type: signature.return_type(),
        };
        let checkpoint = Checkpoint::read(self);
        let prelude_bindings = prelude::recover(self, &lowerer, &checkpoint);
        checkpoint.install(&mut lowerer, stage, prelude_bindings);
        lowerer.type_ids = type_ids;
        lowerer.nullable_maps = nullable_maps;
        check(&mut lowerer, other.source_token())
    }
}

/// Look up only existing ordinary-lowering storage; never intern/register test types.
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
            let candidates = module
                .nullable_maps
                .iter()
                .filter_map(|(id, target)| (*target == inner).then_some(*id))
                .collect::<Vec<_>>();
            let [id] = candidates.as_slice() else {
                return None;
            };
            mapped.insert(ty, *id);
            return Some(*id);
        }
        UnitTypeKind::Intrinsic {
            constructor,
            arguments,
        } if matches!(
            constructor,
            IntrinsicTypeConstructor::Map | IntrinsicTypeConstructor::MutableMap
        ) =>
        {
            let [key, value] = arguments.as_slice() else {
                panic!("Map K/V")
            };
            SsaTypeKind::MapContainer {
                kind: if *constructor == IntrinsicTypeConstructor::Map {
                    MapContainerKind::Map
                } else {
                    MapContainerKind::MutableMap
                },
                key: map_type(module, typed, *key, mapped)?,
                value: map_type(module, typed, *value, mapped)?,
            }
        }
        _ => return None,
    };
    let id = *module.type_ids.get(&kind)?;
    assert_eq!(module.type_kind(id), Some(&kind));
    mapped.insert(ty, id);
    Some(id)
}
