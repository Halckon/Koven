//! SPEC-0306 boundaries use ordinary source only; no malformed SSA or mutation.
use super::{
    LoweringErrorKind, borrow_memory_test_support::single as memory_unit,
    borrow_storage_contract_tests::unit,
};
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_ownership,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

fn assert_frontend_rejection(text: &str, code: &str, primary: &str) {
    let mut sources = SourceMap::new();
    let (source, file) =
        super::unit_lower_test_support::parsed(&mut sources, "mixed-boundary.ko", text);
    let (environment, types) = standard_environments();
    let (provider, entry) = text.split_once("fun entry()").unwrap();
    let (p, provider) = super::unit_lower_test_support::parsed(
        &mut sources,
        "provider.ko",
        &format!("package p\n{provider}"),
    );
    let (q, entry) = super::unit_lower_test_support::parsed(
        &mut sources,
        "entry.ko",
        &format!("package q\nimport p.*\nfun entry(){entry}"),
    );
    let memory = [SourceUnitInput::memory_document(source, &file)];
    let rooted = [
        SourceUnitInput::new("root", "p/provider.ko", p, &provider),
        SourceUnitInput::new("root", "q/entry.ko", q, &entry),
    ];
    for inputs in [&memory[..], &rooted[..]] {
        let index = index_compilation_unit(&sources, inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, inputs, &names, &types, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
        assert_eq!(owned.diagnostics()[0].code().to_string(), code);
        assert_eq!(
            sources
                .slice(owned.diagnostics()[0].primary_span())
                .unwrap(),
            primary
        );
        assert!(
            owned.validate().is_err(),
            "invalid source cannot obtain a validated ownership unit"
        );
    }
}

#[test]
fn mixed_value_borrow_wrong_owner_overlap_and_consumed_value_rejected_at_frontend() {
    for (source, code, primary) in [
        (
            include_str!("../../tests/fixtures/mixed_borrow/wrong_origin_owned.ko"),
            "L0162",
            "other",
        ),
        (
            include_str!("../../tests/fixtures/mixed_borrow/source_live_move.ko"),
            "L0135",
            "source",
        ),
        (
            include_str!("../../tests/fixtures/mixed_borrow/owned_aux_reuse.ko"),
            "L0131",
            "other",
        ),
        (
            include_str!("../../tests/fixtures/mixed_borrow/overlap_owned_after_borrow.ko"),
            "L0135",
            "source",
        ),
        (
            include_str!("../../tests/fixtures/mixed_borrow/overlap_owned_before_borrow.ko"),
            "L0131",
            "source",
        ),
    ] {
        assert_frontend_rejection(source, code, primary);
    }
}

#[test]
fn mixed_value_borrow_inout_source_remains_frontend_deferred() {
    assert_frontend_rejection(
        include_str!("../../tests/fixtures/mixed_borrow/inout_result_source_boundary.ko"),
        "L0164",
        "borrow",
    );
}

#[test]
fn mixed_value_borrow_non_source_inout_remains_lowering_unsupported() {
    for source in [
        include_str!("../../tests/fixtures/mixed_borrow/non_source_inout.ko"),
        include_str!("../../tests/fixtures/mixed_borrow/non_source_inout_writeback.ko"),
    ] {
        for lowered in [memory_unit(source), unit(source)] {
            assert_eq!(
                lowered.err().unwrap().kind,
                LoweringErrorKind::UnsupportedNode
            );
        }
    }
}

#[test]
fn mixed_value_borrow_nested_result_binding_remains_frontend_deferred() {
    let source = "fun view(source: String, own other: Int): borrow String from source = source\nfun entry() { val source = \"kept\".clone(); borrow val item = view(view(source, 7), 8); println(item) }";
    assert_frontend_rejection(source, "L0164", "view(source, 7)");
}

#[test]
fn mixed_value_borrow_callable_value_remains_lowering_unsupported() {
    let source = "fun <T> view(source: String, own other: T): borrow String from source = source\nfun <T> inspect(source: String, own other: T) { borrow val item = view(source, other); println(item) }\nfun entry() { val source = \"kept\".clone(); val action: () -> Unit = { println(\"called\") }; inspect(source, action) }";
    for lowered in [memory_unit(source), unit(source)] {
        assert_eq!(
            lowered.err().unwrap().kind,
            LoweringErrorKind::UnsupportedNode
        );
    }
}

#[test]
fn mixed_value_borrow_wrapper_control_prefix_remains_lowering_unsupported() {
    for argument in ["-n", "if (true) { 1 } else { 2 }", "n + 1"] {
        let source = format!(
            "fun view(own other: Int, source: String): borrow String from source = source\nfun wrap(source: String, n: Int): borrow String from source = view({argument}, source)\nfun entry() {{ val source = \"kept\"; borrow val item = wrap(source, 7); println(item) }}"
        );
        for lowered in [memory_unit(&source), unit(&source)] {
            assert_eq!(
                lowered.err().unwrap().kind,
                LoweringErrorKind::UnsupportedNode
            );
        }
    }
}

#[test]
fn mixed_value_borrow_caller_cfg_remains_lowering_unsupported() {
    let source = "fun view(source: String, own other: Int): borrow String from source = source\nfun entry() { val source = \"kept\"; borrow val item = view(source, 7); if (true) { println(item) }; println(item) }";
    for lowered in [memory_unit(source), unit(source)] {
        assert_eq!(
            lowered.err().unwrap().kind,
            LoweringErrorKind::UnsupportedNode
        );
    }
}

pub(in crate::ssa) mod map_iteration_local {
    //! The bounded, ordinary named-local Map proof consumes only published FE facts.
    //! Formation history is an anchor; entry Expression owners and every incoming
    //! establish current liveness. Runtime still checks birth, transport and absence.
    pub(in crate::ssa) use crate::ssa::lower_frontend::map_iteration::local_runtime::conditions;
    pub(in crate::ssa) use crate::ssa::constant_materialization::map_local_identity as identity;
    pub(in crate::ssa) use identity::{lambda_body, producer};
    use crate::ssa::lower_frontend::map_iteration::source::topology;

    use crate::ssa::lower_frontend::map_iteration::*;
    use crate::ssa::lower_frontend::*;
    use lang_frontend::{
        name_resolution::{Namespace, ReferenceTarget},
        ownership_checking::{
            CleanupOwnerValueId, CleanupSelectorId, DropFact, DropTarget, LoanEndPoint, LoanKind,
        },
        parser::NameMarker,
        type_checking::{
            CallableResultSource, CallableTarget, ExpressionCategory, MapIterationDescriptor,
        },
    };

    pub(in crate::ssa) use self::consumer_prove as consumer;

    #[derive(Clone)]
    pub(in crate::ssa) struct Proof {
        pub(in crate::ssa) root: SymbolId,
        pub(in crate::ssa) initializer: ExpressionId,
        pub(in crate::ssa) consumer: ExpressionId,
        pub(in crate::ssa) birth: CleanupOwnerValueId,
        pub(in crate::ssa) header: CleanupOwnerValueId,
        pub(in crate::ssa) exit: CleanupOwnerValueId,
        /// The actual final consumption, not necessarily this provider's exhaustion.
        pub(in crate::ssa) drop: DropFact,
        pub(in crate::ssa) late_size: Option<ExpressionId>,
        pub(in crate::ssa) dead: Vec<DeadColumn>,
        pub(in crate::ssa) path: Option<Path>,
    }

    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct DeadColumn {
        pub(in crate::ssa) root: SymbolId,
        pub(in crate::ssa) initializer: ExpressionId,
        pub(in crate::ssa) previous: StatementId,
        pub(in crate::ssa) consumed: DropFact,
        pub(in crate::ssa) drop: DropFact,
        pub(in crate::ssa) header: CleanupOwnerValueId,
        pub(in crate::ssa) exit: CleanupOwnerValueId,
    }

    /// A precise FE control identity, never permission to assume a branch executed.
    /// The lowering carrier must prove this successful arm dominates the preheader.
    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct Path {
        pub(in crate::ssa) control: ExpressionId,
        pub(in crate::ssa) condition: ExpressionId,
        pub(in crate::ssa) selector: CleanupSelectorId,
        pub(in crate::ssa) branch: usize,
        pub(in crate::ssa) success: StatementId,
        pub(in crate::ssa) failure: StatementId,
        pub(in crate::ssa) size: ExpressionId,
        pub(in crate::ssa) previous: StatementId,
    }

    pub(in crate::ssa) fn qualify<'a>(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &'a OwnershipCheckedFile,
        plan: &'a MapIterationOwnershipPlan,
        body: StatementId,
        span: Span,
    ) -> Result<qualify::Qualified<'a>, LoweringError> {
        qualify_scope(parsed, names, typed, owned, plan, body, span).map_err(|failure| {
            if failure.kind == LoweringErrorKind::UnsupportedNode {
                error(LoweringErrorKind::UnsupportedNode, span)
            } else {
                failure
            }
        })
    }

    fn qualify_scope<'a>(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &'a OwnershipCheckedFile,
        plan: &'a MapIterationOwnershipPlan,
        body: StatementId,
        span: Span,
    ) -> Result<qualify::Qualified<'a>, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let sites = identity::collect(parsed, names, typed, owned, plan, span)?;
        let mut proofs = Vec::new();
        for (index, site) in sites.iter().enumerate() {
            let previous = index
                .checked_sub(1)
                .map(|index| (&sites[index], &proofs[index]));
            let proof = topology::prove(parsed, owned, site, previous)?;
            proofs.push(proof);
        }
        // Match the complete root-or-associated-owner Drop set, including later dead
        // columns. No unrelated target can hide behind a known owner identity.
        let mut expected = Vec::new();
        let mut owners = Vec::new();
        for proof in &proofs {
            expected.push(proof.drop);
            owners.extend([proof.birth, proof.header, proof.exit]);
            for dead in &proof.dead {
                expected.push(dead.drop);
                owners.extend([dead.header, dead.exit]);
            }
        }
        if expected
            .iter()
            .enumerate()
            .any(|(index, fact)| expected[..index].contains(fact))
        {
            return Err(missing());
        }
        let related = owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| {
                sites
                    .iter()
                    .any(|site| fact.target() == DropTarget::Named(site.root))
                    || fact.owner().is_some_and(|owner| owners.contains(&owner))
            })
            .collect::<Vec<_>>();
        if related.len() != expected.len()
            || expected
                .iter()
                .any(|fact| related.iter().filter(|actual| *actual == fact).count() != 1)
        {
            return Err(missing());
        }
        let index = sites
            .iter()
            .position(|site| site.plan.descriptor().statement() == plan.descriptor().statement())
            .ok_or_else(missing)?;
        let site = &sites[index];
        if site.body != body {
            return Err(missing());
        }
        let proof = proofs.remove(index);
        Ok(qualify::Qualified {
            plan,
            source: SourceIdentity::Local {
                symbol: proof.root,
                initializer: proof.initializer,
            },
            body,
            origin: site.origin,
            owner: Some(source_owner::Proof {
                flow: None,
                disposition: source_owner::Disposition::Drop(proof.drop),
                transfer: None,
                conditional_return: None,
            }),
            body_exit: IterationExitKind::Fallthrough,
            conditional: None,
            nesting: nested::Nesting { parent: None },
            local: Some(proof),
        })
    }

    /// Called only after the fallible whole-descriptor qualification preflight.
    /// A false result is not used to translate a qualification corruption into U.
    pub(in crate::ssa) fn supported_drop(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        fact: DropFact,
    ) -> bool {
        typed.map_iterations().iter().any(|descriptor| {
            let Ok(qualified) = qualify::qualify(parsed, names, typed, owned, *descriptor) else {
                return false;
            };
            qualified.local.is_some_and(|proof| {
                proof.drop == fact || proof.dead.iter().any(|dead| dead.drop == fact)
            })
        })
    }

    pub(in crate::ssa) mod consumer_prove {
        use crate::ssa::lower_frontend::map_iteration::*;
    use crate::ssa::lower_frontend::*;

        pub(in crate::ssa) fn prove(
            parsed: &ParsedFile,
            names: &NameResolution,
            typed: &TypedFile,
            owned: &OwnershipCheckedFile,
            descriptor: &MapIterationDescriptor,
            body: StatementId,
            span: Span,
        ) -> Result<ExpressionId, LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
            let body = parsed.ast().statements().get(body).map_err(|_| missing())?;
            let Statement::Block { elements } = body.payload() else {
                return Err(unsupported());
            };
            let [statement] = elements.as_slice() else {
                return Err(unsupported());
            };
            let statement = parsed
                .ast()
                .statements()
                .get(*statement)
                .map_err(|_| missing())?;
            let Statement::Expression { expression } = statement.payload() else {
                return Err(unsupported());
            };
            let node = parsed
                .ast()
                .expressions()
                .get(*expression)
                .map_err(|_| missing())?;
            let Expression::Call {
                callee,
                arguments,
                type_arguments,
                ..
            } = node.payload()
            else {
                return Err(unsupported());
            };
            if arguments.len() != 2
                || !type_arguments.is_empty()
                || arguments
                    .iter()
                    .any(|argument| argument.named_prefix.is_some() || argument.mode_marker.is_some())
            {
                return Err(unsupported());
            }
            let callee = parsed
                .ast()
                .expressions()
                .get(*callee)
                .map_err(|_| missing())?;
            if callee.payload() != &Expression::Name {
                return Err(unsupported());
            }
            let call = typed.call(*expression).ok_or_else(missing)?;
            let CallableTarget::Source(target) = call.target() else {
                return Err(unsupported());
            };
            let symbol = names.symbols().get(target.index()).ok_or_else(missing)?;
            let signature = unique(
                typed
                    .callables()
                    .iter()
                    .filter(|signature| signature.symbol() == target),
            )
            .ok_or_else(missing)?;
            if symbol.kind() != SymbolKind::Function
                || !parsed.roots().iter().any(|item| parsed.ast().items().get(*item).is_ok_and(|node| {
                    matches!(node.payload(), Item::Function { name: NameMarker::Present(marker), .. } if *marker == symbol.span())
                }))
                || call.receiver().is_some() || call.aborts() || call.prints_line()
                || !call.instance().type_arguments().is_empty() || signature.owner().is_some()
                || signature.receiver().is_some() || !signature.type_parameters().is_empty()
                || builtin_type(typed, call.return_type()) != Some(BuiltinType::Unit)
            {
                return Err(unsupported());
            }
            let reference = unique(names.references().iter().filter(|reference| {
                reference.span() == callee.span() && reference.namespace() == Namespace::Value
            }))
            .ok_or_else(missing)?;
            let selected = match reference.target() {
                ReferenceTarget::Symbol(symbol) => *symbol == target,
                ReferenceTarget::OverloadSet(symbols) => symbols.contains(&target),
                _ => false,
            };
            if !selected
                || call.expression() != *expression
                || call.result_source() != CallableResultSource::Owned
                || signature.result_source() != call.result_source()
                || signature.return_type() != call.return_type()
                || typed.expression_type(*expression) != Some(call.return_type())
                || call.arguments().len() != 2
                || signature.parameters().len() != 2
            {
                return Err(missing());
            }
            let loans = owned
                .loans()
                .iter()
                .filter(|loan| loan.call() == *expression)
                .collect::<Vec<_>>();
            let ends = owned
                .loan_ends()
                .iter()
                .filter(|end| end.call() == *expression)
                .collect::<Vec<_>>();
            if loans.len() != 2 || ends.len() != 2 {
                return Err(missing());
            }
            for (index, argument) in arguments.iter().enumerate() {
                let symbol = descriptor.bindings()[index];
                let ty = [descriptor.key_type(), descriptor.value_type()][index];
                let argument_node = parsed
                    .ast()
                    .expressions()
                    .get(argument.value)
                    .map_err(|_| missing())?;
                if argument_node.payload() != &Expression::Name {
                    return Err(unsupported());
                }
                let reference = unique(names.references().iter().filter(|reference| {
                    reference.span() == argument_node.span()
                        && reference.namespace() == Namespace::Value
                }))
                .ok_or_else(missing)?;
                let parameter = &signature.parameters()[index];
                let mapping = call.arguments()[index];
                let loan = unique(
                    loans
                        .iter()
                        .copied()
                        .filter(|loan| loan.argument() == argument.value),
                )
                .ok_or_else(missing)?;
                let end = unique(
                    ends.iter()
                        .copied()
                        .filter(|end| end.argument() == argument.value),
                )
                .ok_or_else(missing)?;
                if reference.target() != &ReferenceTarget::Symbol(symbol)
                    || parameter.mode != ParameterMode::Borrow
                    || parameter.ty != ty
                    || mapping.argument_index() != index
                    || mapping.parameter_index() != index
                    || mapping.mode() != ParameterMode::Borrow
                    || mapping.parameter_type() != ty
                    || mapping.crosses_thread()
                    || mapping.category() != ExpressionCategory::Place
                    || typed.expression_type(argument.value) != Some(ty)
                    || typed.expression_category(argument.value) != Some(ExpressionCategory::Place)
                    || loan.kind() != LoanKind::Shared
                    || loan.begin_span() != argument_node.span()
                    || loan.end_span() != node.span()
                    || loan.is_receiver_reservation()
                    || loan.activation_point().is_some()
                    || !matches!(loan.target(), LoanTarget::Place(place) if place.is_root() && place.root() == symbol)
                    || end.point() != LoanEndPoint::CallReturn(*expression)
                {
                    return Err(missing());
                }
            }
            Ok(*expression)
        }
    }

    pub(in crate::ssa) mod retained {
        //! Other live Map roots keep their own epoch and terminal fact across a provider.
        use crate::ssa::lower_frontend::map_iteration::formed::{
            drops, formation, owned_parameter, raw_formation, replacement_point,
        };
        use crate::ssa::lower_frontend::map_iteration::*;
    use crate::ssa::lower_frontend::*;
        use crate::ssa::map_source::{Birth, Formation};
        use lang_frontend::{
            ownership_checking::IterationPhiBoundary, type_checking::IntrinsicTypeConstructor,
        };

        pub(in crate::ssa) struct Epoch {
            pub root: SymbolId,
            pub birth: Birth<SymbolId>,
            pub formation: Option<Formation<SymbolId>>,
            pub at: Span,
        }

        pub(in crate::ssa) fn resolve(
            frontend: (
                &ParsedFile,
                &NameResolution,
                &TypedFile,
                &OwnershipCheckedFile,
            ),
            root: SymbolId,
            storage: TypeId,
            rhs: ExpressionId,
            span: Span,
        ) -> Result<Epoch, LoweringError> {
            let (parsed, names, typed, owned) = frontend;
            let bad = || error(LoweringErrorKind::MissingFact, span);
            let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
            let rhs = crate::ssa::map_source::transparent_expression(parsed, rhs, |id| {
                typed.expression_type(id).zip(typed.expression_category(id))
            })?;
            let node = parsed.ast().expressions().get(rhs).map_err(|_| bad())?;
            if typed.expression_type(rhs) != Some(storage) {
                return Err(bad());
            }
            match node.payload() {
                Expression::Call { .. } => Ok(Epoch {
                    root,
                    birth: Birth::Call(rhs),
                    formation: None,
                    at: node.span(),
                }),
                Expression::Name => {
                    if typed.expression_category(rhs) != Some(ExpressionCategory::Place) {
                        return Err(bad());
                    }
                    let reference = unique(
                        names
                            .references()
                            .iter()
                            .filter(|r| r.span() == node.span()),
                    )
                    .ok_or_else(bad)?;
                    let ReferenceTarget::Symbol(source) = reference.target() else {
                        return Err(bad());
                    };
                    if names
                        .symbols()
                        .get(source.index())
                        .is_some_and(|s| s.kind() == SymbolKind::ValueParameter)
                    {
                        let origin = owned_parameter(frontend, *source, storage, span)?;
                        return Ok(Epoch {
                            root: *source,
                            birth: Birth::EntryParameter {
                                root: *source,
                                reference: rhs,
                                origin,
                            },
                            formation: None,
                            at: node.span(),
                        });
                    }
                    let formation =
                        raw_formation(parsed, names, typed, owned, *source, storage, node.span())?;
                    Ok(Epoch {
                        root: *source,
                        birth: formation.birth,
                        formation: Some(formation),
                        at: node.span(),
                    })
                }
                _ => Err(unsupported()),
            }
        }

        /// Formation is present only for a real local epoch; parameters retain their entry slot.
        pub(in crate::ssa) struct Retained {
            pub(in crate::ssa) source: SourceIdentity,
            pub(in crate::ssa) formation: Option<Formation<SymbolId>>,
        }

        pub(in crate::ssa) fn proof(
            frontend: (
                &ParsedFile,
                &NameResolution,
                &TypedFile,
                &OwnershipCheckedFile,
            ),
            plan: &MapIterationOwnershipPlan,
            fact: DropFact,
            span: Span,
        ) -> Result<Option<Retained>, LoweringError> {
            let (parsed, names, typed, owned) = frontend;
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let DropTarget::Named(root) = fact.target() else {
                return Ok(None);
            };
            if !plan
                .closure_flow()
                .exit()
                .iter()
                .any(|binding| binding.symbol() == root)
                || matches!(plan.source(),Some(LoanTarget::Place(place)) if place.root()==root)
            {
                return Ok(None);
            }
            let storage = typed.symbol_type(root).ok_or_else(missing)?;
            if !matches!(
                typed.types().get(storage),
                Some(lang_frontend::type_checking::TypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Map | IntrinsicTypeConstructor::MutableMap,
                    ..
                })
            ) {
                return Ok(None);
            }
            let event = crate::ssa::map_source::drop_event(parsed, fact.point())
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
            if event.span.end() <= span.end() {
                return Ok(None);
            }
            let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
            let (source, formation, origin) = if symbol.kind() == SymbolKind::ValueParameter {
                let origin = owned_parameter((parsed, names, typed, owned), root, storage, span)?;
                (SourceIdentity::Parameter(root), None, origin)
            } else {
                let formation = formation(parsed, names, typed, owned, root, storage, span)?;
                if !drops(parsed, owned, root, &formation)?.contains(&fact) {
                    return Ok(None);
                }
                let origin = formation.declaration;
                (
                    SourceIdentity::Formed {
                        symbol: root,
                        birth: formation.birth,
                    },
                    Some(formation),
                    origin,
                )
            };
            let flow = source_owner::Flow {
                statement: plan.descriptor().statement(),
                source,
                span,
                late: Some(fact),
            };
            let exit = source_owner::phi(
                owned.cleanup_conditions(),
                plan,
                root,
                IterationPhiBoundary::Exit,
                span,
            )?;
            source_owner::flow_condition(parsed, names, owned, flow, exit.owner(), span)?;
            let live = source_owner::flow_condition(
                parsed,
                names,
                owned,
                flow,
                fact.owner().ok_or_else(missing)?,
                span,
            )?;
            if fact.value_origin() != origin
                || fact.capture_slot().is_some()
                || fact.instance_address().is_some()
                || fact.condition() != Some(live)
            {
                return Err(missing());
            }
            let future = crate::ssa::map_source::future_reference(
                parsed,
                span,
                event.span,
                names.references().iter().filter_map(|r| {
                    (r.target() == &ReferenceTarget::Symbol(root)).then_some(r.span())
                }),
            );
            if !future && !ancestor_fact((parsed, names, typed, owned), flow, fact, storage, span)? {
                let resource = typed.is_resource_type(storage).ok_or_else(missing)?;
                let scope = resource::scope(parsed, plan.descriptor().statement(), origin, span)?;
                if !resource
                    || (!resource::normal_point(parsed, scope, span, fact.point())
                        && !replacement_point(formation.as_ref(), fact.point()))
                {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            Ok(Some(Retained { source, formation }))
        }

        /// An active ancestor owns this other root, independently of the inner source.
        fn ancestor_fact(
            frontend: (
                &ParsedFile,
                &NameResolution,
                &TypedFile,
                &OwnershipCheckedFile,
            ),
            flow: source_owner::Flow,
            fact: DropFact,
            storage: TypeId,
            span: Span,
        ) -> Result<bool, LoweringError> {
            use lang_frontend::ownership_checking::CleanupOwnerValue;
            let (parsed, names, typed, owned) = frontend;
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let root = match flow.source {
                SourceIdentity::Parameter(root) | SourceIdentity::Formed { symbol: root, .. } => root,
                _ => return Err(missing()),
            };
            for ancestor in owned.map_iterations() {
                if ancestor.descriptor().statement() == flow.statement
                    || ancestor.source_access() != MapIterationSourceAccess::Owned
                    || !matches!(ancestor.source(),Some(LoanTarget::Place(place)) if place.is_root()&&place.root()==root)
                {
                    continue;
                }
                let node = parsed
                    .ast()
                    .statements()
                    .get(ancestor.descriptor().statement())
                    .map_err(|_| missing())?;
                let Statement::For { body, .. } = node.payload() else {
                    return Err(missing());
                };
                let body = parsed
                    .ast()
                    .statements()
                    .get(*body)
                    .map_err(|_| missing())?
                    .span();
                if body.source_id() != span.source_id()
                    || body.start() > span.start()
                    || span.end() > body.end()
                {
                    continue;
                }
                if ancestor.descriptor().source_type() != storage {
                    return Err(missing());
                }
                let parent_flow = source_owner::Flow {
                    statement: ancestor.descriptor().statement(),
                    source: flow.source,
                    span: node.span(),
                    late: None,
                };
                if ancestor
                    .exits()
                    .iter()
                    .any(|exit| exit.point() == fact.point())
                    && conditional::source_drop(owned, parent_flow, fact.point())? == Some(fact)
                {
                    source_owner::flow_condition(
                        parsed,
                        names,
                        owned,
                        parent_flow,
                        fact.owner().ok_or_else(missing)?,
                        span,
                    )?;
                    return Ok(true);
                }
                if typed.is_resource_type(storage) != Some(true) {
                    continue;
                }
                let exit = source_owner::phi(
                    owned.cleanup_conditions(),
                    ancestor,
                    root,
                    IterationPhiBoundary::Exit,
                    node.span(),
                )?;
                let live = source_owner::flow_condition(
                    parsed,
                    names,
                    owned,
                    parent_flow,
                    exit.owner(),
                    span,
                )?;
                let origin = names
                    .symbols()
                    .get(root.index())
                    .ok_or_else(missing)?
                    .span();
                let scope = resource::scope(
                    parsed,
                    ancestor.descriptor().statement(),
                    origin,
                    node.span(),
                )?;
                if fact.owner() != Some(exit.owner())
                    || fact.condition() != Some(live)
                    || !resource::normal_point(parsed, scope, node.span(), fact.point())
                {
                    continue;
                }
                if !matches!(owned.cleanup_conditions().owner_value(exit.owner()),Some(CleanupOwnerValue::IterationPhi{statement,boundary:IterationPhiBoundary::Exit,symbol,..})
                    if *statement==ancestor.descriptor().statement()&&*symbol==root)
                {
                    return Err(missing());
                }
                let cleanup = owned
                    .cleanup_steps()
                    .iter()
                    .filter_map(|(point, action)| (*point == fact.point()).then_some(*action))
                    .collect::<Vec<_>>();
                conditional::validate_drop_ledger(owned, fact.point(), &cleanup, span)?;
                return Ok(true);
            }
            Ok(false)
        }
    }
}
