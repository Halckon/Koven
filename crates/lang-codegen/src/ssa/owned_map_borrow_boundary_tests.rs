//! Normal source negative controls; no malformed facts or IR.
use super::unit_lower_test_support::parsed;
use crate::test_support::diagnostics::diagnostic;
use lang_frontend::{
    analysis::{UnitSourceDescriptor, analyze_parsed_unit_names, analyze_unit_recovery},
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_ownership,
    type_checking::{
        CompilationUnitTypes, FinalTypeAdmissionError, TypeEnvironment,
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

fn diagnostics(source: &str, unit: bool) -> String {
    if !unit {
        let mut sources = lang_frontend::source::SourceMap::new();
        let (id, file) = parsed(&mut sources, "negative.ko", source);
        let (environment, types) = standard_environments();
        let snapshot = analyze_parsed_unit_names(
            sources,
            vec![(UnitSourceDescriptor::memory_document(id), file)],
            environment,
        )
        .unwrap();
        assert!(
            snapshot.names().diagnostics().is_empty(),
            "{:?}",
            snapshot.names().diagnostics()
        );
        let names = snapshot.validated_names().unwrap();
        let inputs = snapshot.inputs();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].source_id(), id);
        assert!(std::ptr::eq(
            inputs[0].parsed(),
            &snapshot.parsed_files()[0]
        ));
        assert_eq!(snapshot.sources().source_text(id), Ok(source));
        // Preserve the original parallel type/ownership observation even when
        // raw types reject. This public producer continues these exact parsed
        // and name products; it does not reparse or fabricate a qualification.
        let (raw, owned) = analyze_unit_recovery(&snapshot, &types).unwrap();
        observe_complete_types(snapshot.sources(), &inputs, names, &types, &raw);
        let diagnostics = format!(
            "types={:?}; ownership={:?}",
            raw.diagnostics(),
            owned.diagnostics()
        );
        match raw.validate() {
            Ok(typed) => {
                assert!(owned.is_compatible_with(&typed));
            }
            Err(raw) => {
                diagnostic!(
                    "MemoryDocument basic qualification rejected: {:?}",
                    raw.diagnostics()
                );
            }
        }
        return diagnostics;
    }
    let mut sources = lang_frontend::source::SourceMap::new();
    let (provider, entry) = source.split_once("fun entry()").unwrap();
    let (p, provider) = parsed(
        &mut sources,
        "provider.ko",
        &format!("package p\n{provider}"),
    );
    let (q, entry) = parsed(
        &mut sources,
        "entry.ko",
        &format!("package q\nimport p.*\nfun entry(){entry}"),
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", p, &provider),
        SourceUnitInput::new("root", "q/entry.ko", q, &entry),
    ];
    let (environment, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    observe_complete_types(&sources, &inputs, &names, &types, &typed);
    if !typed.diagnostics().is_empty() {
        return format!("types={:?}", typed.diagnostics());
    }
    let typed = typed.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(owned.is_compatible_with(&typed));
    format!("ownership={:?}", owned.diagnostics())
}

// Observe complete admission without replacing the original raw diagnostics
// or granting a capability. Basic and complete are distinct qualifications:
// recovery diagnostics neither require complete success nor imply its failure.
fn observe_complete_types(
    sources: &lang_frontend::source::SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &lang_frontend::name_resolution::ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    raw: &CompilationUnitTypes,
) {
    assert!(raw.is_compatible_with(sources, inputs, names, environment));
    match complete_compilation_unit_types(sources, inputs, names, environment, raw) {
        Ok(complete) => {
            assert!(complete.is_same_analysis(raw));
            assert!(std::ptr::eq(complete.types(), raw));
        }
        Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
            diagnostic!(
                "Unit complete admission diagnostics (not the raw oracle): {diagnostics:?}"
            );
        }
        Err(error) => panic!("original Unit complete admission failed internally: {error:?}"),
    }
}

#[test]
fn owned_map_borrow_frontend_negative_controls() {
    let prefix = "fun observe(value: String) { println(value) }\nfun entry() { var values = mutableMapOf<Int, String>(); values.put(1, \"value\".clone()); val removed = values.remove(1); ";
    for (label, source) in [
        ("get-move-only", "fun entry() { val values = mapOf<Int, String>(); val item = values.get(1) }".into()),
        ("no-proof", format!("{prefix} observe(removed) }}")),
        ("wrong-edge", format!("{prefix} if (removed == null) {{ observe(removed) }} }}")),
        ("proof-ended", format!("{prefix} if (removed != null) {{ observe(removed) }}; observe(removed) }}")),
        ("other-symbol", format!("{prefix} val other = values.remove(2); if (removed != null) {{ observe(other) }} }}")),
        ("consumed-owner", format!("{prefix} val taken = removed!!; observe(taken); val again = removed!!; observe(again) }}")),
        ("borrow-then-move", "fun conflict(first: String, own second: String) { println(first); println(second) }\nfun entry() { var values = mutableMapOf<Int, String>(); values.put(1, \"value\".clone()); val removed = values.remove(1); if (removed != null) { conflict(removed, removed) } }".into()),
        ("callee-consumes", "fun consume(own value: String) {}\nfun observe(value: String) { consume(value) }\nfun entry() {}".into()),
    ] {
        for unit in [false, true] {
            let diagnostics = diagnostics(&source, unit);
            diagnostic!("{label}/unit={unit}: {diagnostics}");
            assert!(diagnostics.contains("Diagnostic {"), "must reject {label}/unit={unit}");
            let expected = match label {
                "get-move-only" => "L0136", "consumed-owner" => "L0131",
                "borrow-then-move" => "L0135", "callee-consumes" => "L0133", _ => "L0084",
            };
            assert!(diagnostics.contains(expected), "{label}/unit={unit}: expected {expected}");
        }
    }
}

#[test]
fn owned_map_borrow_unit_bounded_result_and_nested_positions() {
    let setup = "var values = mutableMapOf<Int, String>(); values.put(1, \"value\".clone()); val removed = values.remove(1);";
    for (label, source) in [
        (
            "borrow-return",
            format!(
                "fun view(value: String): borrow String from value = value\nfun entry() {{ {setup} if (removed != null) {{ borrow val item = view(removed); println(item) }} }}"
            ),
        ),
        (
            "require-key",
            format!(
                "fun key(value: String): String = value.clone()\nfun entry() {{ {setup} val other = mapOf<String, String>(); if (removed != null) {{ borrow val item = other.requireValue(key(removed)); println(item) }} }}"
            ),
        ),
        (
            "container-index",
            format!(
                "fun index(value: String): Int {{ println(value); return 0 }}\nfun entry() {{ {setup} val array = arrayOf(7); if (removed != null) {{ val selected = array[index(removed)]; if (selected == 7) {{ println(\"found\") }} }} }}"
            ),
        ),
    ] {
        let error = super::borrow_storage_contract_tests::unit(&source)
            .err()
            .expect("bounded source must reject");
        diagnostic!("{label}: {error:?}; source={source}");
        assert_eq!(
            error.kind,
            super::LoweringErrorKind::UnsupportedNode,
            "{label}"
        );
        assert!(error.span.is_some(), "source-located boundary {label}");
    }
}

pub(in crate::ssa) mod source_closure {
    //! Source-qualified concrete callable layouts, capture formation, and per-source lookup.

    use crate::ssa::lower_frontend::instances::{FunctionInstancePlan, resolve_concrete_type};
    use crate::ssa::lower_frontend::{
        ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, builtin_type, error,
        nominal::NominalTypeMapper, value,
    };
    use crate::ssa::{
        lowering_support::callable_instances::{CallableKey, CallableToken, SourceToken},
        model::{
            ClosureCaptureMode as SsaCaptureMode, ClosureCaptureType, EntityId, EntityType,
            FunctionId, LoanKind, Module, Operation, Origin, SsaTypeId,
        },
    };
    use lang_frontend::{
        ast::{ExpressionId, StatementId},
        name_resolution::{NameResolution, SymbolId, SymbolKind},
        ownership_checking::{
            CallableOrigin, ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource,
            OwnershipCheckedFile,
        },
        parser::{Expression, ParsedFile, Statement},
        source::Span,
        type_checking::{BuiltinType, ParameterMode, TypeId, TypeKind, TypedFile},
    };
    use std::collections::BTreeMap;

    pub(in crate::ssa) struct Inputs<'a> {
        pub(in crate::ssa) parsed: &'a ParsedFile,
        pub(in crate::ssa) names: &'a NameResolution,
        pub(in crate::ssa) typed: &'a TypedFile,
        pub(in crate::ssa) owned: &'a OwnershipCheckedFile,
        pub(in crate::ssa) instances: &'a FunctionInstancePlan,
        pub(in crate::ssa) templates:
            &'a [crate::ssa::lower_frontend::instances::FunctionTemplate],
    }

    pub(in crate::ssa) fn declare(
        module: &mut Module,
        mapper: &mut NominalTypeMapper,
        inputs: Inputs<'_>,
    ) -> Result<CallableLayouts, LoweringError> {
        let Inputs {
            parsed,
            names,
            typed,
            owned,
            instances,
            templates,
        } = inputs;
        let mut layouts = CallableLayouts::default();
        for (ordinal, instance) in instances.instances().iter().enumerate() {
            let owner_span = templates[instance.template_index].span;
            let lambdas = parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(expression, node)| {
                    (contains(owner_span, node.span())
                        && matches!(node.payload(), Expression::Lambda { .. })
                        && owned.callable_origin(expression).is_some_and(|fact| {
                            fact.origin() == CallableOrigin::Lambda(expression)
                        }))
                    .then_some((expression, node.span()))
                })
                .collect::<Vec<_>>();
            for &(expression, span) in &lambdas {
                if lambdas
                    .iter()
                    .any(|&(other, outer)| other != expression && contains(outer, span))
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                let node = parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
                let Expression::Lambda {
                    opener_span,
                    parameters,
                    arrow_span,
                    body,
                    ..
                } = node.payload()
                else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                owned
                    .closure(expression)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let ty = typed
                    .expression_type(expression)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let ty = resolve_concrete_type(typed, ty, &instance.substitutions, span)?;
                let Some(TypeKind::Function {
                    parameters: signature,
                    return_type,
                    ..
                }) = typed.types().get(ty)
                else {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                };
                let parameter_spans =
                    if arrow_span.is_none() && parameters.is_empty() && signature.len() == 1 {
                        std::slice::from_ref(opener_span)
                    } else {
                        parameters.as_slice()
                    };
                if parameter_spans.len() != signature.len() {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let mut symbols = Vec::new();
                let mut parameter_types = Vec::new();
                for (parameter, &parameter_span) in signature.iter().zip(parameter_spans) {
                    let symbol = names
                        .symbols()
                        .iter()
                        .find(|symbol| {
                            symbol.span() == parameter_span
                                && symbol.kind() == SymbolKind::LambdaParameter
                        })
                        .map(|symbol| symbol.id())
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, parameter_span))?;
                    symbols.push(symbol);
                    let target =
                        mapper.intern(module, names, typed, parameter.ty, parameter_span)?;
                    parameter_types.push(match parameter.mode {
                        ParameterMode::Value => EntityType::Value(target),
                        ParameterMode::Borrow => EntityType::Loan {
                            kind: LoanKind::Shared,
                            target,
                        },
                        ParameterMode::Inout => {
                            return Err(error(LoweringErrorKind::UnsupportedNode, parameter_span));
                        }
                    });
                }
                let returns = if builtin_type(typed, *return_type) == Some(BuiltinType::Unit) {
                    Vec::new()
                } else {
                    vec![mapper.intern(module, names, typed, *return_type, span)?]
                };
                let mut captures = Vec::new();
                let mut fields = Vec::new();
                for capture in owned.captures_of(expression) {
                    let ClosureCaptureSource::Symbol(symbol) = capture.source() else {
                        return Err(error(
                            LoweringErrorKind::UnsupportedNode,
                            capture.reference_span(),
                        ));
                    };
                    let concrete = resolve_concrete_type(
                        typed,
                        capture.ty(),
                        &instance.substitutions,
                        capture.reference_span(),
                    )?;
                    let target =
                        mapper.intern(module, names, typed, concrete, capture.reference_span())?;
                    let mode = match capture.mode() {
                        ClosureCaptureMode::Owned => SsaCaptureMode::Owned,
                        ClosureCaptureMode::Shared => SsaCaptureMode::Shared,
                    };
                    fields.push(if mode == SsaCaptureMode::Shared {
                        module
                            .add_shared_reference_type(target)
                            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?
                    } else {
                        target
                    });
                    captures.push(CapturePlan {
                        symbol,
                        ty: target,
                        field_type: *fields
                            .last()
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?,
                        mode,
                        effect: capture.effect(),
                        span: capture.reference_span(),
                    });
                }
                let name = format!("lambda{}.s{}", expression.index(), ordinal);
                let environment = if captures.is_empty() {
                    None
                } else {
                    Some(
                        module
                            .add_aggregate_type(format!("{name}.environment"), fields)
                            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?,
                    )
                };
                let callable = if let Some(environment) = environment {
                    module.add_concrete_closure_type_with_parameters(
                        format!("{name}.closure"),
                        parameter_types.clone(),
                        returns.clone(),
                        environment,
                        captures
                            .iter()
                            .map(|capture| ClosureCaptureType {
                                mode: capture.mode,
                                ty: capture.ty,
                            })
                            .collect(),
                    )
                } else {
                    module.add_function_pointer_type_with_parameters(
                        parameter_types.clone(),
                        returns.clone(),
                    )
                }
                .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
                let thunk = module
                    .add_function(format!("{name}.thunk"), returns, Origin::Source(span))
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
                if let Some(target) = environment {
                    parameter_types.insert(
                        0,
                        EntityType::Loan {
                            kind: LoanKind::Shared,
                            target,
                        },
                    );
                }
                module
                    .function_mut(thunk)
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
                    .add_block(parameter_types, Origin::Source(span))
                    .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
                let result_expression = tail(parsed, *body, span)?;
                layouts.lambdas.insert(
                    (instance.source, expression.index()),
                    ClosurePlan {
                        owner: instance.source,
                        expression,
                        callable,
                        thunk,
                        body: *body,
                        span,
                        captures,
                        parameter_symbols: symbols,
                        return_type: *return_type,
                        result_expression,
                        substitutions: instance.substitutions.clone(),
                    },
                );
            }
        }
        Ok(layouts)
    }

    fn tail(
        parsed: &ParsedFile,
        body: StatementId,
        span: Span,
    ) -> Result<Option<ExpressionId>, LoweringError> {
        let node = parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Statement::LambdaBody { elements } = node.payload() else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        elements
            .last()
            .map(|statement| {
                parsed
                    .ast()
                    .statements()
                    .get(*statement)
                    .map(|node| match node.payload() {
                        Statement::Expression { expression } => Some(*expression),
                        _ => None,
                    })
                    .map_err(|_| error(LoweringErrorKind::MissingFact, span))
            })
            .transpose()
            .map(Option::flatten)
    }

    fn contains(owner: Span, child: Span) -> bool {
        owner.source_id() == child.source_id()
            && owner.start() <= child.start()
            && child.end() <= owner.end()
    }

    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct CapturePlan {
        pub(in crate::ssa) symbol: SymbolId,
        pub(in crate::ssa) ty: SsaTypeId,
        pub(in crate::ssa) field_type: SsaTypeId,
        pub(in crate::ssa) mode: SsaCaptureMode,
        pub(in crate::ssa) effect: ClosureCaptureEffect,
        pub(in crate::ssa) span: Span,
    }

    #[derive(Clone)]
    pub(in crate::ssa) struct ClosurePlan {
        pub(in crate::ssa) owner: SourceToken,
        pub(in crate::ssa) expression: ExpressionId,
        pub(in crate::ssa) callable: SsaTypeId,
        pub(in crate::ssa) thunk: FunctionId,
        pub(in crate::ssa) body: StatementId,
        pub(in crate::ssa) span: Span,
        pub(in crate::ssa) captures: Vec<CapturePlan>,
        pub(in crate::ssa) parameter_symbols: Vec<SymbolId>,
        pub(in crate::ssa) return_type: TypeId,
        pub(in crate::ssa) result_expression: Option<ExpressionId>,
        pub(in crate::ssa) substitutions: BTreeMap<SymbolId, TypeId>,
    }

    #[derive(Default)]
    pub(in crate::ssa) struct CallableLayouts {
        pub(in crate::ssa) lambdas: BTreeMap<(SourceToken, usize), ClosurePlan>,
        pub(in crate::ssa) named: BTreeMap<SourceToken, SsaTypeId>,
    }

    impl CallableLayouts {
        pub(in crate::ssa) fn token_type(
            &self,
            instances: &FunctionInstancePlan,
            token: CallableToken,
            span: Span,
        ) -> Result<SsaTypeId, LoweringError> {
            match instances
                .callable(token)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            {
                CallableKey::Lambda { owner, expression } => self
                    .lambdas
                    .get(&(*owner, expression.expression().index()))
                    .map(|plan| plan.callable),
                CallableKey::KnownFunction { function } => self.named.get(function).copied(),
            }
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
        }

        pub(in crate::ssa) fn expression_type(
            &self,
            lowerer: &ExpressionLowerer<'_>,
            expression: ExpressionId,
            span: Span,
        ) -> Result<SsaTypeId, LoweringError> {
            let parsed = lowerer.parsed;
            let typed = lowerer.typed;
            let owned = lowerer.owned;
            let instances = lowerer.instance_plan;
            let source = lowerer
                .source_token
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            let node = parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            if let Expression::Lambda { .. } = node.payload() {
                return self
                    .lambdas
                    .get(&(source, expression.index()))
                    .map(|plan| plan.callable)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span));
            }
            if let Expression::Group { expression } = node.payload() {
                return self.expression_type(lowerer, *expression, span);
            }
            let origin = owned
                .callable_origin(expression)
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?
                .origin();
            match origin {
                CallableOrigin::Lambda(lambda) => self
                    .lambdas
                    .get(&(source, lambda.index()))
                    .map(|plan| plan.callable)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span)),
                CallableOrigin::Parameter(parameter) => {
                    let key = instances
                        .source(source)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let signature = typed
                        .callables()
                        .iter()
                        .find(|signature| signature.symbol() == key.symbol())
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let slot = signature
                        .parameter_symbols()
                        .iter()
                        .position(|symbol| *symbol == Some(parameter))
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let token = key
                        .callable_arguments()
                        .iter()
                        .find(|(index, _)| *index == slot)
                        .map(|(_, token)| *token)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    self.token_type(instances, token, span)
                }
                CallableOrigin::KnownFunction(function) => {
                    let instance = instances
                        .instances()
                        .iter()
                        .find(|instance| {
                            instance.key.symbol() == function
                                && instance.key.type_arguments().is_empty()
                                && instance.key.callable_arguments().is_empty()
                        })
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    self.named
                        .get(&instance.source)
                        .copied()
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))
                }
                CallableOrigin::FactoryResult(call) => {
                    let key = instances
                        .call_site(source, call)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let factory = instances
                        .instances()
                        .iter()
                        .find(|instance| &instance.key == key)
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let token = instances
                        .pointer_return(factory.source)
                        .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
                    self.token_type(instances, token, span)
                }
            }
        }
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn lower_source_closure(
            &mut self,
            expression: ExpressionId,
            span: Span,
        ) -> Result<LoweredValue, LoweringError> {
            let owner = self
                .source_token
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            let plan = self
                .source_closures
                .lambdas
                .get(&(owner, expression.index()))
                .cloned()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let mut captures = Vec::new();
            for (field, capture) in plan.captures.iter().enumerate() {
                if capture.mode == SsaCaptureMode::Shared {
                    let loan = if let Some(&source) = self.borrow_bindings.get(&capture.symbol) {
                        let (_, results) = self.append(
                            Operation::SharedReborrow { source },
                            vec![EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: capture.ty,
                            }],
                            capture.span,
                        )?;
                        require_loan(results[0], capture.span)?
                    } else {
                        let Some(LoweredValue::Value(owner)) =
                            self.bindings.get(&capture.symbol).copied()
                        else {
                            return Err(error(LoweringErrorKind::MissingFact, capture.span));
                        };
                        let (_, results) = self.append(
                            Operation::RootPlace { owner },
                            vec![EntityType::Place(capture.ty)],
                            capture.span,
                        )?;
                        let EntityId::Place(place) = results[0] else {
                            return Err(error(LoweringErrorKind::InvalidModel, capture.span));
                        };
                        let (_, results) = self.append(
                            Operation::BorrowBegin {
                                place,
                                kind: LoanKind::Shared,
                            },
                            vec![EntityType::Loan {
                                kind: LoanKind::Shared,
                                target: capture.ty,
                            }],
                            capture.span,
                        )?;
                        require_loan(results[0], capture.span)?
                    };
                    self.capture_loans.insert((expression.index(), field), loan);
                    captures.push(crate::ssa::model::ClosureCaptureOperand::Shared(loan));
                } else {
                    let operand = match capture.effect {
                        ClosureCaptureEffect::Copy => {
                            if let Some(LoweredValue::Value(source)) =
                                self.bindings.get(&capture.symbol).copied()
                            {
                                let (_, results) = self.append(
                                    Operation::Copy { source },
                                    vec![EntityType::Value(capture.ty)],
                                    capture.span,
                                )?;
                                value(results[0])
                            } else if let Some(&source) = self.borrow_bindings.get(&capture.symbol)
                            {
                                let (_, results) = self.append(
                                    Operation::Read {
                                        source: crate::ssa::model::PlaceAccess::Loan(source),
                                    },
                                    vec![EntityType::Value(capture.ty)],
                                    capture.span,
                                )?;
                                value(results[0])
                            } else {
                                return Err(error(LoweringErrorKind::MissingFact, capture.span));
                            }
                        }
                        ClosureCaptureEffect::Move => {
                            let Some(LoweredValue::Value(owner)) =
                                self.bindings.remove(&capture.symbol)
                            else {
                                return Err(error(LoweringErrorKind::MissingFact, capture.span));
                            };
                            self.forget_delivered_owners(&[owner]);
                            owner
                        }
                        ClosureCaptureEffect::Borrow | ClosureCaptureEffect::Unknown => {
                            return Err(error(LoweringErrorKind::UnsupportedNode, capture.span));
                        }
                    };
                    captures.push(crate::ssa::model::ClosureCaptureOperand::Owned(operand));
                }
            }
            let operation = if captures.is_empty() {
                Operation::FunctionAddress { target: plan.thunk }
            } else {
                Operation::ClosureConstruct {
                    closure: plan.callable,
                    thunk: plan.thunk,
                    captures,
                }
            };
            let (_, results) =
                self.append(operation, vec![EntityType::Value(plan.callable)], span)?;
            Ok(LoweredValue::Value(value(results[0])))
        }

        /// Lambda tail evaluation preserves ordinary statement/drop facts and the void Unit ABI.
        pub(in crate::ssa) fn lower_lambda_body(
            &mut self,
            plan: &ClosurePlan,
        ) -> Result<LoweredValue, LoweringError> {
            let node = self
                .parsed
                .ast()
                .statements()
                .get(plan.body)
                .map_err(|_| error(LoweringErrorKind::MissingFact, plan.span))?;
            let lang_frontend::parser::Statement::LambdaBody { elements } = node.payload() else {
                return Err(error(LoweringErrorKind::MissingFact, plan.span));
            };
            let elements = elements.clone();
            let mut result = LoweredValue::Unit;
            for statement in elements {
                result = self.lower_statement(statement)?;
                if matches!(result, LoweredValue::Diverged) {
                    return Ok(result);
                }
            }
            self.emit_drops(
                lang_frontend::ownership_checking::DropPoint::AfterStatement(plan.body),
            )?;
            if builtin_type(self.typed, plan.return_type) == Some(BuiltinType::Unit) {
                Ok(LoweredValue::Unit)
            } else if plan.result_expression.is_some() {
                Ok(result)
            } else {
                Err(error(LoweringErrorKind::MissingFact, plan.span))
            }
        }

        pub(in crate::ssa) fn bind_capture_views(
            &mut self,
            plan: &ClosurePlan,
        ) -> Result<(), LoweringError> {
            let parameters = self
                .function
                .block(self.block)
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, plan.span))?
                .parameters
                .clone();
            let offset = usize::from(!plan.captures.is_empty());
            for (&symbol, &parameter) in plan.parameter_symbols.iter().zip(&parameters[offset..]) {
                match parameter {
                    EntityId::Value(owner) => {
                        self.bindings.insert(symbol, LoweredValue::Value(owner));
                    }
                    EntityId::Loan(loan) => {
                        self.borrow_bindings.insert(symbol, loan);
                    }
                    EntityId::Place(_) => {
                        return Err(error(LoweringErrorKind::InvalidModel, plan.span));
                    }
                }
            }
            if plan.captures.is_empty() {
                return Ok(());
            }
            let environment = require_loan(parameters[0], plan.span)?;
            for (field, capture) in plan.captures.iter().enumerate() {
                let (_, results) = self.append(
                    Operation::SharedFieldLoan {
                        base: environment,
                        field,
                    },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: capture.field_type,
                    }],
                    capture.span,
                )?;
                let slot = require_loan(results[0], capture.span)?;
                let view = if capture.mode == SsaCaptureMode::Shared {
                    self.capture_loans
                        .insert((plan.expression.index(), field), slot);
                    let (_, results) = self.append(
                        Operation::SharedReferenceFollow { source: slot },
                        vec![EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: capture.ty,
                        }],
                        capture.span,
                    )?;
                    require_loan(results[0], capture.span)?
                } else {
                    slot
                };
                self.borrow_bindings.insert(capture.symbol, view);
            }
            Ok(())
        }

        /// Close thunk-local views in child-before-parent order, using current CFG rebound IDs.
        pub(in crate::ssa) fn end_thunk_capture_views(
            &mut self,
            span: Span,
        ) -> Result<(), LoweringError> {
            let Some(expression) = self.thunk_expression else {
                return Ok(());
            };
            let owner = self
                .source_token
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let plan = self
                .source_closures
                .lambdas
                .get(&(owner, expression.index()))
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let captures = plan
                .captures
                .iter()
                .enumerate()
                .map(|(field, capture)| (field, capture.symbol))
                .collect::<Vec<_>>();
            for (field, symbol) in captures.into_iter().rev() {
                let loan = self
                    .borrow_bindings
                    .remove(&symbol)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                if let Some(loan) = self.capture_loans.remove(&(expression.index(), field)) {
                    self.append(Operation::BorrowEnd { loan }, Vec::new(), span)?;
                }
            }
            Ok(())
        }

        /// SSA Drop already releases a closure's shared dependencies. Retire only the carrier slots.
        /// Emitting another BorrowEnd would double-end the current capture loan.
        pub(in crate::ssa) fn release_owner_capture_loans(
            &mut self,
            owner: crate::ssa::model::ValueId,
            span: Span,
        ) -> Result<(), LoweringError> {
            let Some(EntityType::Value(ty)) = self
                .function
                .entity(EntityId::Value(owner))
                .map(|entity| entity.ty)
            else {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            };
            let Some(plan) = self
                .source_closures
                .lambdas
                .values()
                .find(|plan| plan.callable == ty && !plan.captures.is_empty())
            else {
                return Ok(());
            };
            let keys = plan
                .captures
                .iter()
                .enumerate()
                .filter_map(|(field, capture)| {
                    (capture.mode == SsaCaptureMode::Shared)
                        .then_some((plan.expression.index(), field))
                })
                .collect::<Vec<_>>();
            for key in keys.into_iter().rev() {
                self.capture_loans
                    .remove(&key)
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            }
            Ok(())
        }
    }

    fn require_loan(
        entity: EntityId,
        span: Span,
    ) -> Result<crate::ssa::model::LoanId, LoweringError> {
        match entity {
            EntityId::Loan(loan) => Ok(loan),
            _ => Err(error(LoweringErrorKind::InvalidModel, span)),
        }
    }
}
