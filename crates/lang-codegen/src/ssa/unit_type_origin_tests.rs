//! Real Unit lowering origins; no manually populated type-origin table or native execution.
use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::{
    name_resolution::{SourceUnitInput, SymbolKind},
    parser::ParsedFile,
    source::{SourceMap, Span},
    type_checking::standard_environments,
};

use super::{
    model::{Program, SsaTypeId, SsaTypeKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;

fn name(ty: &SsaTypeKind) -> Option<&str> {
    match ty {
        SsaTypeKind::Aggregate { name, .. }
        | SsaTypeKind::TaggedUnion { name, .. }
        | SsaTypeKind::HeapOwner { name, .. }
        | SsaTypeKind::SharedOwner { name, .. } => Some(name),
        _ => None,
    }
}

fn assert_real_primary(sources: &SourceMap, parsed: &ParsedFile, span: Span) {
    assert_eq!(span.source_id(), parsed.source_id());
    assert!(!sources.slice(span).expect("valid source span").is_empty());
    // The origin must be an actual AST use/declaration span, not a manufactured range.
    assert!(
        parsed
            .ast()
            .expressions()
            .iter()
            .any(|(_, node)| node.span() == span)
            || parsed
                .ast()
                .items()
                .iter()
                .any(|(_, node)| node.span() == span)
            || parsed
                .ast()
                .type_refs()
                .iter()
                .any(|(_, node)| node.span() == span),
        "primary must preserve an original AST span: {span:?}"
    );
}

#[test]
fn unit_storage_origins_preserve_real_nominal_and_wrapper_declarations() {
    for (definition, expression, storage_count) in [
        ("class Item(val n: Int)", "Item(1)", 2),
        ("value class Item(val n: Int)", "Item(1)", 1),
        ("enum class Item { Full(n: Int), Empty }", "Item.Full(1)", 3),
        ("value class Item(val n: Int)", "Box(Item(1))", 2),
        ("class Item(val n: Int)", "Rc(Item(1))", 3),
    ] {
        let text = format!("{definition}\nfun entry(): Unit {{ val item = {expression} }}");
        let unit =
            UnitSource::analyze("origins.ko", &text).expect("strict original Unit admission");
        unit.assert_frontend_clean();
        let (program, _) = unit
            .lower_entry(unit.declaration("entry", SymbolKind::Function))
            .expect("real Unit source lowers and verifies");
        let declaration = unit
            .names()
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "Item")
            .expect("real nominal declaration")
            .name_span();
        assert_eq!(unit.sources().slice(declaration).unwrap(), "Item");
        let module = &program.modules[0];
        let mut count = 0;
        for (index, kind) in module.types.iter().enumerate() {
            let Some(storage_name) = name(kind) else {
                continue;
            };
            let ty = SsaTypeId {
                module: module.id,
                index,
            };
            let origin = program
                .type_origin(ty)
                .expect("every lowered storage type has an origin");
            assert_real_primary(unit.sources(), unit.parsed(), origin.primary);
            let expected = if storage_name.contains(".case0.payload") {
                let start = text.find("Full(n:").unwrap();
                unit.sources()
                    .span(unit.parsed().source_id(), start, start + 4)
                    .unwrap()
            } else if storage_name.contains(".case1.payload") {
                let start = text.find("Empty }").unwrap();
                unit.sources()
                    .span(unit.parsed().source_id(), start, start + 5)
                    .unwrap()
            } else {
                declaration
            };
            assert_eq!(origin.declaration, expected, "{storage_name}");
            if let SsaTypeKind::HeapOwner {
                payload: Some(payload),
                ..
            } = kind
                && storage_name.starts_with("class#")
            {
                assert_eq!(program.type_origin(*payload), Some(origin));
            }
            count += 1;
        }
        assert_eq!(count, storage_count, "{text}");
    }
}

fn origins(program: &Program) -> Vec<(String, Span, Span)> {
    let module = &program.modules[0];
    module
        .types
        .iter()
        .enumerate()
        .filter_map(|(index, kind)| {
            let name = name(kind)?;
            let origin = program
                .type_origin(SsaTypeId {
                    module: module.id,
                    index,
                })
                .expect("real nominal origin");
            Some((name.to_owned(), origin.primary, origin.declaration))
        })
        .collect()
}

#[test]
fn same_named_classes_keep_source_qualified_origins_under_input_permutation() {
    let mut sources = SourceMap::new();
    let (left_id, left) = parsed(
        &mut sources,
        "left/origins.ko",
        "package left\nclass Item(val n: Int)\nfun create(): Item = Item(1)",
    );
    let (right_id, right) = parsed(
        &mut sources,
        "right/origins.ko",
        "package right\nimport left.create\nclass Item(val n: Int)\nfun entry(): Unit { val a = create(); val b = Item(2) }",
    );
    let inputs = [
        SourceUnitInput::new("root", "left/origins.ko", left_id, &left),
        SourceUnitInput::new("root", "right/origins.ko", right_id, &right),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (ne, te) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
    let entry = declaration(&names, "right", "entry");
    let (forward, _) =
        lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
            .expect("two source same-name classes lower");
    let (backward, _) =
        lower_scalar_unit_with_entry(&sources, &reversed, &names, &te, &typed, &owned, entry)
            .expect("input permutation lowers");
    assert_text_eq!(render_program(&forward), render_program(&backward));
    assert_eq!(origins(&forward), origins(&backward));
    let module = &forward.modules[0];
    let mut owners = 0;
    for package in ["left", "right"] {
        let id = declaration(&names, package, "Item");
        let expected = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|candidate| candidate.id() == id)
            .unwrap()
            .name_span();
        assert_eq!(sources.slice(expected).unwrap(), "Item");
        assert_eq!(
            expected.source_id(),
            if package == "left" { left_id } else { right_id }
        );
        let prefix = format!("class#d{}.", id.index());
        let matching = module.types.iter().enumerate().filter(|(_, kind)| {
            matches!(kind, SsaTypeKind::HeapOwner { name, .. } if name.starts_with(&prefix))
        }).collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        let (
            index,
            SsaTypeKind::HeapOwner {
                payload: Some(payload),
                ..
            },
        ) = matching[0]
        else {
            panic!("defined class storage")
        };
        let origin = forward
            .type_origin(SsaTypeId {
                module: module.id,
                index,
            })
            .unwrap();
        assert_eq!(origin.declaration, expected);
        assert_eq!(forward.type_origin(*payload), Some(origin));
        let source_file = if origin.primary.source_id() == left_id {
            &left
        } else {
            &right
        };
        assert_real_primary(&sources, source_file, origin.primary);
        owners += 1;
    }
    assert_eq!(owners, 2);
}

mod lambda_preflight_tests {
    //! CI-only, real-source private preflight checkpoints; no edited facts or fabricated callables.
    //! Supported cases also traverse the sealed Unit entry, verifier and LLVM independently.

    use std::collections::BTreeMap;

    use lang_frontend::{
        lexer::lex,
        name_resolution::{
            DeclarationId, SourceUnitId, SourceUnitInput, UnitSymbolId, index_compilation_unit,
            resolve_compilation_unit_names,
        },
        ownership_checking::{
            OwnedCompilationUnitView, UnitCallableOrigin, check_compilation_unit_ownership,
            owned_compilation_unit_view,
        },
        parser::{FunctionBody, FunctionForm, NameMarker, Statement, parse_file},
        source::SourceMap,
        type_checking::{
            UnitTypeRefId, check_compilation_unit_types, complete_compilation_unit_types,
            standard_environments,
        },
    };

    use crate::ssa::unit_lower::type_plan::*;
    use crate::ssa::{
        model::{EntityType, MapContainerKind, Operation, Origin, Ownership, Program, SsaTypeKind},
        unit_lower::{
            FunctionPlan, FunctionPlanBody, callable_abi, closure, instance_function_name,
            lower_owned_unit_with_entry, unit_callable_signature,
        },
        unit_plan::{UnitCallablePlan, UnitFunctionInstanceKey, plan_unit_instances},
    };

    const PATH: &str = "lambda_formed_preflight.ko";

    struct Facts<'view, 'parsed: 'view> {
        view: &'view OwnedCompilationUnitView<'view, 'parsed>,
        parsed: &'parsed ParsedFile,
        entry: DeclarationId,
        instances: Vec<UnitPlannedInstance>,
        source_plan: UnitCallablePlan,
    }

    struct Local {
        symbol: UnitSymbolId,
        scope: ScopeId,
        initializer: UnitExpressionId,
        origin: Span,
        storage: UnitTypeId,
    }

    struct Checkpoint {
        program: Program,
        mapper: UnitTypeLowering,
        callables: BTreeMap<closure::CallablePlanKey, closure::CallablePlan>,
    }

    fn with_source(text: &str, check: impl FnOnce(Facts<'_, '_>)) {
        let mut sources = SourceMap::new();
        let source = sources.add_source(PATH, text).expect("one original source");
        let lexed = lex(&sources, source).expect("lex");
        assert!(lexed.diagnostics().is_empty());
        let parsed = parse_file(&sources, &lexed).expect("parse");
        assert!(parsed.diagnostics().is_empty());
        let inputs = [SourceUnitInput::memory_document(source, &parsed)];
        assert!(std::ptr::eq(inputs[0].parsed(), &parsed));
        assert_eq!(sources.source_text(source).unwrap(), text);
        let (environment, type_environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).expect("index");
        assert!(index.diagnostics().is_empty());
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("resolve names");
        assert!(names.diagnostics().is_empty());
        let names = names.validate().expect("validate names");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("check types");
        assert!(typed.diagnostics().is_empty());
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &type_environment, &typed)
                .expect("complete the original analysis");
        assert!(complete.is_same_analysis(&typed));
        assert!(std::ptr::eq(complete.types(), &typed));
        let typed = typed.validate().expect("validate types");
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
                .expect("check ownership");
        assert!(owned.diagnostics().is_empty());
        assert!(owned.deferred().is_empty());
        let owned = owned.validate().expect("validate ownership");
        let view =
            owned_compilation_unit_view(&sources, &inputs, &names, &type_environment, &typed, &owned)
                .expect("same-object sealed view");
        assert!(std::ptr::eq(view.sources(), &sources));
        assert!(std::ptr::eq(view.inputs(), inputs.as_slice()));
        assert!(std::ptr::eq(view.names(), &names));
        assert!(std::ptr::eq(view.types(), typed.types()));
        assert!(std::ptr::eq(view.ownership(), owned.ownership()));
        let declarations = names.names().index().declarations();
        let [main] = declarations else {
            panic!("each checkpoint has exactly one real main declaration");
        };
        assert_eq!(main.name(), "main");
        assert_eq!(main.kind(), SymbolKind::Function);
        let entry = main.id();
        // The complete production instance planner runs before any mapper is created.
        let (instances, demands, source_plan) = plan_unit_instances(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
        .expect("complete real unit plan")
        .into_parts();
        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].key(),
            &UnitFunctionInstanceKey::for_entry(entry)
        );
        assert!(demands.is_empty());
        check(Facts {
            view: &view,
            parsed: &parsed,
            entry,
            instances,
            source_plan,
        });
    }

    impl Facts<'_, '_> {
        fn source(&self) -> SourceUnitId {
            self.instances[0].source_unit()
        }

        fn resolution(&self) -> &NameResolution {
            let sources = self.view.names().names().source_units();
            assert_eq!(sources.len(), 1);
            assert_eq!(sources[0].source_unit(), self.source());
            assert_eq!(sources[0].resolution().source_id(), self.parsed.source_id());
            sources[0].resolution()
        }

        fn main_span(&self) -> Span {
            self.instances[0].span()
        }

        // Full NameMarker identity selects the initializer; no type or spelling guesses its span.
        fn local(&self, name: &str, within: Span) -> Local {
            let resolution = self.resolution();
            let symbols = resolution
                .symbols()
                .iter()
                .filter(|symbol| {
                    symbol.kind() == SymbolKind::Variable
                        && !symbol.is_synthetic()
                        && symbol.name() == name
                        && span_contains(within, symbol.span())
                })
                .collect::<Vec<_>>();
            let [symbol] = symbols.as_slice() else {
                panic!("one real local {name} in the selected source scope");
            };
            let declarations = self
                .parsed
                .ast()
                .statements()
                .iter()
                .filter_map(|(_, statement)| {
                    let Statement::LocalVariable { declaration } = statement.payload() else {
                        return None;
                    };
                    let node = self.parsed.ast().items().get(*declaration).unwrap();
                    match node.payload() {
                        Item::Variable {
                            name: NameMarker::Present(marker),
                            initializer,
                            type_ref: Some(type_ref),
                            ..
                        } if *marker == symbol.span() => Some((*initializer, *type_ref)),
                        _ => None,
                    }
                })
                .collect::<Vec<_>>();
            let [(initializer, type_ref)] = declarations.as_slice() else {
                panic!("one exact annotated local declaration");
            };
            let symbols = self
                .view
                .types()
                .body_symbol_types()
                .keys()
                .copied()
                .filter(|id| id.source_unit() == self.source() && id.symbol() == symbol.id())
                .collect::<Vec<_>>();
            let [qualified] = symbols.as_slice() else {
                panic!("one original source-qualified local identity");
            };
            let initializer = UnitExpressionId::new(self.source(), *initializer);
            let origin = self
                .parsed
                .ast()
                .expressions()
                .get(initializer.expression())
                .unwrap()
                .span();
            let storage = self.view.types().symbol_type(*qualified).unwrap();
            assert_eq!(
                self.view
                    .types()
                    .type_ref_type(UnitTypeRefId::new(self.source(), *type_ref)),
                Some(storage)
            );
            assert_eq!(
                self.view.types().expression_type(initializer),
                Some(storage)
            );
            assert_eq!(origin.source_id(), self.parsed.source_id());
            Local {
                symbol: *qualified,
                scope: symbol.scope(),
                initializer,
                origin,
                storage,
            }
        }

        fn lambda(&self, action: &str, within: Span, formed: bool) -> Local {
            let local = self.local(action, within);
            assert!(matches!(
                self.parsed
                    .ast()
                    .expressions()
                    .get(local.initializer.expression())
                    .unwrap()
                    .payload(),
                Expression::Lambda { .. }
            ));
            let origin = self.view.ownership().callable_origin(local.initializer);
            if formed {
                assert_eq!(
                    origin.unwrap().origin(),
                    UnitCallableOrigin::Lambda(local.initializer)
                );
                assert!(self.view.ownership().closure(local.initializer).is_some());
            } else {
                assert!(origin.is_none());
            }
            local
        }

        fn checkpoint(&self) -> Checkpoint {
            let typed = self.view.types();
            let owned = self.view.ownership();
            let instance = &self.instances[0];
            let mut program = Program::default();
            let id = program.add_module("lambda-preflight");
            let module = program.module_mut(id).unwrap();
            let mut mapper = UnitTypeLowering::new(self.view.names());
            let parsed_by_source = [self.parsed];
            intern_map_result_types(
                module,
                &parsed_by_source,
                &self.instances,
                typed,
                &mut mapper,
            )
            .expect("ordinary map-result preplanning");
            let layouts = closure::layout::declare(
                module,
                &parsed_by_source,
                &self.instances,
                self.view.names(),
                typed,
                owned,
                &mut mapper,
            )
            .expect("real callable layouts");
            let _abi = callable_abi::CallableAbi::declare(
                module,
                &self.instances,
                &self.source_plan,
                &layouts,
                typed,
                &mut mapper,
            )
            .expect("real callable ABI");
            let signature = unit_callable_signature(typed, instance.key().target()).unwrap();
            assert!(signature.parameters().is_empty());
            assert!(signature.receiver().is_none());
            assert!(instance.owner().is_none());
            assert!(instance.substitutions().is_empty());
            assert!(instance.key().static_self().is_none());
            let return_type = signature.return_type();
            assert_eq!(Some(return_type), typed.types().builtin(BuiltinType::Unit));
            let node = self.parsed.ast().items().get(instance.item()).unwrap();
            assert_eq!(node.span(), instance.span());
            let Item::Function {
                form:
                    FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    },
                parameters,
                ..
            } = node.payload()
            else {
                panic!("real zero-argument Unit main block");
            };
            assert!(parameters.is_empty());
            let function = module
                .add_function(
                    instance_function_name(self.view.names(), instance).unwrap(),
                    Vec::new(),
                    Origin::Source(instance.span()),
                )
                .unwrap();
            module
                .function_mut(function)
                .unwrap()
                .add_block(Vec::new(), Origin::Source(instance.span()))
                .unwrap();
            // Only this source's real main is reconstructed; CallablePlan is never constructed here.
            let plan = FunctionPlan {
                id: function,
                instance: instance.clone(),
                function_item: instance.item(),
                body: FunctionPlanBody::Block(*body),
                receiver: None,
                parameter_symbols: Vec::new(),
                inout_parameter: None,
                return_type,
            };
            intern_body_scalar_types(module, self.parsed, instance, typed, &mut mapper)
                .expect("ordinary supported body types");
            intern_formed_nullable_map_types(
                module,
                self.parsed,
                &plan,
                self.view.names(),
                typed,
                owned,
                &mut mapper,
            )
            .expect("main must not absorb a lambda-owned unsupported demand");
            let callables = closure::declare(module, &[plan], layouts).expect("real declared thunks");
            Checkpoint {
                program,
                mapper,
                callables,
            }
        }

        fn verified_program(&self) -> Program {
            let (program, entry) = lower_owned_unit_with_entry(self.view, self.entry)
                .expect("independent complete Unit lowering");
            crate::ssa::verify::verify_program(&program).expect("ordinary source SSA verifies");
            crate::llvm::render_verified_program_with_entry(&program, entry)
                .expect("ordinary source reaches LLVM");
            program
        }

        fn rejected(&self, error: LoweringError, origin: Span, text: &str) {
            assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
            assert_eq!(error.span, Some(origin));
            assert_eq!(
                self.view.sources().source_name(origin.source_id()).unwrap(),
                PATH
            );
            assert_eq!(self.view.sources().slice(origin).unwrap(), text);
        }
    }

    impl Checkpoint {
        fn run(
            &mut self,
            facts: &Facts<'_, '_>,
            lambda: UnitExpressionId,
        ) -> Result<(), LoweringError> {
            let matches = self
                .callables
                .values()
                .filter(|plan| plan.expression == lambda)
                .collect::<Vec<_>>();
            let [plan] = matches.as_slice() else {
                panic!("exact real declared lambda plan");
            };
            assert_eq!(plan.source_unit, facts.source());
            assert_eq!(plan.source_token, facts.instances[0].source_token());
            assert_eq!(plan.substitutions, *facts.instances[0].substitutions());
            assert_eq!(plan.static_self, facts.instances[0].key().static_self());
            let module = &mut self.program.modules[0];
            assert_eq!(
                module.function(plan.thunk).unwrap().origin,
                Origin::Source(plan.span)
            );
            intern_lambda_formed_nullable_map_types(
                module,
                facts.parsed,
                plan,
                facts.view.names(),
                facts.view.types(),
                facts.view.ownership(),
                &mut self.mapper,
            )
        }
    }

    fn nullable_map(facts: &Facts<'_, '_>, local: &Local, nullable_value: bool) {
        let typed = facts.view.types();
        let Some(UnitTypeKind::Nullable(inner)) = typed.types().get(local.storage) else {
            panic!("real nullable Map local");
        };
        let Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::MutableMap,
            arguments,
        }) = typed.types().get(*inner)
        else {
            panic!("exact MutableMap storage");
        };
        let [key, value] = arguments.as_slice() else {
            panic!("exact K/V arguments");
        };
        assert_eq!(Some(*key), typed.types().builtin(BuiltinType::Int));
        if nullable_value {
            assert!(
                matches!(typed.types().get(*value), Some(UnitTypeKind::Nullable(inner))
                if Some(*inner) == typed.types().builtin(BuiltinType::Int))
            );
        } else {
            assert_eq!(Some(*value), typed.types().builtin(BuiltinType::Int));
        }
        assert!(typed.map_constructions().is_empty());
        assert!(typed.map_gets().is_empty());
        assert!(typed.map_removes().is_empty());
    }

    #[test]
    fn unit_lambda_formed_nullable_map_preflight_reuses_existing_identity() {
        with_source(
            r#"fun main(): Unit {
      val outer: MutableMap<Int, Int>? = null
      val action: () -> Unit = {
        val inner: MutableMap<Int, Int>? = null
      }
    }
    "#,
            |facts| {
                let action = facts.lambda("action", facts.main_span(), true);
                let outer = facts.local("outer", facts.main_span());
                let inner = facts.local("inner", action.origin);
                nullable_map(&facts, &inner, false);
                assert_eq!(outer.storage, inner.storage);
                assert!(
                    facts
                        .view
                        .ownership()
                        .formed_owned_locals()
                        .contains(&inner.symbol)
                );
                assert!(
                    formed_in_function(facts.resolution(), outer.scope, facts.main_span()).unwrap()
                );
                assert!(
                    !formed_in_function(facts.resolution(), inner.scope, facts.main_span()).unwrap()
                );
                assert!(formed_in_lambda(facts.resolution(), inner.scope, action.origin).unwrap());
                assert!(!formed_in_lambda(facts.resolution(), outer.scope, action.origin).unwrap());
                let mut checkpoint = facts.checkpoint();
                assert_eq!(checkpoint.callables.len(), 1);
                let id = *checkpoint.mapper.type_ids().get(&inner.storage).unwrap();
                let module = &checkpoint.program.modules[0];
                let payload = module.nullable_map_inner(id).unwrap();
                let (kind, key, value) = module.map_container(payload).unwrap();
                assert_eq!(kind, MapContainerKind::MutableMap);
                assert_eq!(key, value);
                assert_eq!(
                    module.type_kind(key),
                    Some(&SsaTypeKind::Integer {
                        bits: 32,
                        signed: true,
                    })
                );
                assert_eq!(module.nullable_maps.len(), 1);
                assert_eq!(module.type_ownership(id), Some(Ownership::MoveOnly));
                assert!(module.map_results.is_empty());
                let before = (
                    module.types.clone(),
                    module.nullable_maps.clone(),
                    checkpoint.mapper.type_ids().clone(),
                );
                for _ in 0..2 {
                    checkpoint.run(&facts, action.initializer).unwrap();
                    let module = &checkpoint.program.modules[0];
                    assert_eq!(
                        before,
                        (
                            module.types.clone(),
                            module.nullable_maps.clone(),
                            checkpoint.mapper.type_ids().clone(),
                        )
                    );
                    assert!(module.map_results.is_empty());
                }
                let program = facts.verified_program();
                let module = &program.modules[0];
                assert_eq!(module.nullable_maps.len(), 1);
                assert!(module.map_results.is_empty());
                let thunks = module
                    .functions
                    .iter()
                    .filter(|function| function.origin == Origin::Source(action.origin))
                    .collect::<Vec<_>>();
                let [thunk] = thunks.as_slice() else {
                    panic!("one actual lambda body");
                };
                let (&wrapper, _) = module.nullable_maps.first_key_value().unwrap();
                let nulls = thunk
                    .instructions
                    .iter()
                    .filter(|instruction| {
                        instruction.origin == Origin::Source(inner.origin)
                            && matches!(instruction.operation,
                            Operation::NullableNull { nullable } if nullable == wrapper)
                    })
                    .collect::<Vec<_>>();
                let [null] = nulls.as_slice() else {
                    panic!("the real inner initializer produces one existing nullable Map identity");
                };
                assert_eq!(null.results.len(), 1);
                assert_eq!(
                    thunk.entity(null.results[0]).unwrap().ty,
                    EntityType::Value(wrapper)
                );
            },
        );
    }

    #[test]
    fn unit_lambda_formed_nullable_map_preflight_excludes_dead_local() {
        with_source(
            r#"fun main(): Unit {
      val action: () -> Unit = {
        return
        val dead: MutableMap<Int, Int?>? = null
      }
    }
    "#,
            |facts| {
                let action = facts.lambda("action", facts.main_span(), true);
                let dead = facts.local("dead", action.origin);
                nullable_map(&facts, &dead, true);
                assert!(
                    !facts
                        .view
                        .ownership()
                        .formed_owned_locals()
                        .contains(&dead.symbol)
                );
                assert!(formed_in_lambda(facts.resolution(), dead.scope, action.origin).unwrap());
                let mut checkpoint = facts.checkpoint();
                assert_eq!(checkpoint.callables.len(), 1);
                assert!(!checkpoint.mapper.type_ids().contains_key(&dead.storage));
                let before = checkpoint.program.modules[0].types.clone();
                checkpoint.run(&facts, action.initializer).unwrap();
                assert_eq!(checkpoint.program.modules[0].types, before);
                assert!(!checkpoint.mapper.type_ids().contains_key(&dead.storage));
                let program = facts.verified_program();
                let module = &program.modules[0];
                assert!(module.nullable_maps.is_empty());
                let thunks = module
                    .functions
                    .iter()
                    .filter(|function| function.origin == Origin::Source(action.origin))
                    .collect::<Vec<_>>();
                let [thunk] = thunks.as_slice() else {
                    panic!("the live lambda is still emitted");
                };
                assert!(
                    thunk
                        .instructions
                        .iter()
                        .all(|instruction| { instruction.origin != Origin::Source(dead.origin) })
                );
            },
        );
    }

    #[test]
    fn unit_lambda_formed_nullable_map_preflight_excludes_unplanned_lambda() {
        with_source(
            r#"fun main(): Unit {
      return
      val action: () -> Unit = {
        val dead: MutableMap<Int, Int?>? = null
      }
    }
    "#,
            |facts| {
                let action = facts.lambda("action", facts.main_span(), false);
                let dead = facts.local("dead", action.origin);
                nullable_map(&facts, &dead, true);
                assert!(
                    !facts
                        .view
                        .ownership()
                        .formed_owned_locals()
                        .contains(&action.symbol)
                );
                assert!(
                    !facts
                        .view
                        .ownership()
                        .formed_owned_locals()
                        .contains(&dead.symbol)
                );
                let checkpoint = facts.checkpoint();
                assert!(checkpoint.callables.is_empty());
                assert!(!checkpoint.mapper.type_ids().contains_key(&dead.storage));
                let program = facts.verified_program();
                assert_eq!(program.modules[0].functions.len(), 1);
                assert!(program.modules[0].nullable_maps.is_empty());
            },
        );
    }

    #[test]
    fn unit_lambda_formed_nullable_map_preflight_uses_sibling_initializer() {
        with_source(
            r#"fun main(): Unit {
      val good: () -> Unit = {
        val cache: MutableMap<Int, Int>? = null
      }
      val bad: () -> Unit = {
        val cache: MutableMap<Int, Int?>? = (null)
      }
    }
    "#,
            |facts| {
                let good = facts.lambda("good", facts.main_span(), true);
                let bad = facts.lambda("bad", facts.main_span(), true);
                let supported = facts.local("cache", good.origin);
                let rejected = facts.local("cache", bad.origin);
                nullable_map(&facts, &supported, false);
                nullable_map(&facts, &rejected, true);
                assert_ne!(supported.symbol, rejected.symbol);
                assert_ne!(supported.storage, rejected.storage);
                assert!(
                    facts
                        .view
                        .ownership()
                        .formed_owned_locals()
                        .contains(&supported.symbol)
                );
                assert!(
                    facts
                        .view
                        .ownership()
                        .formed_owned_locals()
                        .contains(&rejected.symbol)
                );
                assert!(formed_in_lambda(facts.resolution(), supported.scope, good.origin).unwrap());
                assert!(!formed_in_lambda(facts.resolution(), rejected.scope, good.origin).unwrap());
                assert!(formed_in_lambda(facts.resolution(), rejected.scope, bad.origin).unwrap());
                assert!(
                    !formed_in_function(facts.resolution(), rejected.scope, facts.main_span()).unwrap()
                );
                let mut checkpoint = facts.checkpoint();
                assert_eq!(checkpoint.callables.len(), 2);
                assert!(!checkpoint.mapper.type_ids().contains_key(&rejected.storage));
                checkpoint.run(&facts, good.initializer).unwrap();
                assert!(!checkpoint.mapper.type_ids().contains_key(&rejected.storage));
                let error = checkpoint.run(&facts, bad.initializer).unwrap_err();
                facts.rejected(error, rejected.origin, "(null)");
                let error = match lower_owned_unit_with_entry(facts.view, facts.entry) {
                    Ok(_) => panic!("the unsupported sibling nullable Map must be rejected"),
                    Err(error) => error,
                };
                facts.rejected(error, rejected.origin, "(null)");
            },
        );
    }

    #[test]
    fn unit_lambda_formed_nullable_map_preflight_keeps_nested_scope_boundary() {
        with_source(
            r#"fun main(): Unit {
      val outer: () -> Unit = {
        val outerMap: MutableMap<Int, Int>? = null
        val inner: () -> Unit = {
          val innerMap: MutableMap<Int, Int?>? = null
        }
      }
    }
    "#,
            |facts| {
                let outer = facts.lambda("outer", facts.main_span(), true);
                let inner = facts.lambda("inner", outer.origin, true);
                let outer_map = facts.local("outerMap", outer.origin);
                let inner_map = facts.local("innerMap", inner.origin);
                nullable_map(&facts, &outer_map, false);
                nullable_map(&facts, &inner_map, true);
                assert!(
                    facts
                        .view
                        .ownership()
                        .formed_owned_locals()
                        .contains(&inner_map.symbol)
                );
                assert!(formed_in_lambda(facts.resolution(), inner_map.scope, inner.origin).unwrap());
                assert!(!formed_in_lambda(facts.resolution(), inner_map.scope, outer.origin).unwrap());
                assert!(!formed_in_lambda(facts.resolution(), outer_map.scope, inner.origin).unwrap());
                assert!(formed_in_lambda(facts.resolution(), outer_map.scope, outer.origin).unwrap());
                assert!(
                    !formed_in_function(facts.resolution(), inner_map.scope, facts.main_span())
                        .unwrap()
                );
                assert!(
                    !formed_in_function(facts.resolution(), outer_map.scope, facts.main_span())
                        .unwrap()
                );
                // Nested lambdas remain outside the existing callable-layout capability.
                // No synthetic nested CallablePlan is created to exercise the new preflight.
                let error = match lower_owned_unit_with_entry(facts.view, facts.entry) {
                    Ok(_) => panic!("nested lambdas remain unsupported"),
                    Err(error) => error,
                };
                facts.rejected(
                    error,
                    inner.origin,
                    "{\n      val innerMap: MutableMap<Int, Int?>? = null\n    }",
                );
            },
        );
    }
}
