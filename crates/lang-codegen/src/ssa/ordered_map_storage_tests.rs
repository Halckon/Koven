//! Ordinary source to verified LLVM layout observations; no native or IR mutation.

use super::model::{MapContainerKind, SsaTypeKind};
use crate::test_support::diagnostics::{assert_eq_diagnostic, diagnostic};

#[test]
fn ordered_map_headers_preserve_shared_six_field_storage() {
    let sources = [
        r#"fun entry(): Unit {
 var values = mutableMapOf<Int, Int>()
 values.put(9, 90)
 values.put(1, 10)
 values.put(9, 91)
 values.remove(9)
 values.put(9, 92)
 if (values.size == 2) { println("values") }
 val empty = mapOf<Int, Int>()
 if (empty.size == 0) { println("empty") }
}"#,
        r#"fun entry(): Unit {
 var values = mutableMapOf<String, String>()
 values.put("second".clone(), "old".clone())
 values.put("first".clone(), "value".clone())
 values.put("second".clone(), "new".clone())
 values.remove("first")
 values.put("first".clone(), "again".clone())
 if (values.size == 2) { println("values") }
 val empty = mapOf<String, String>()
 if (empty.size == 0) { println("empty") }
}"#,
    ];
    let mut observations = Vec::new();
    for (source_index, source) in sources.iter().enumerate() {
        for unit in [false, true] {
            let program = super::owned_map_borrow_tests::lower(source, unit);
            let llvm = crate::llvm::render_verified_program(&program).unwrap();
            assert_eq!(program.modules.len(), 1);
            let maps: Vec<_> = program.modules[0]
                .types
                .iter()
                .enumerate()
                .filter_map(|(index, kind)| {
                    matches!(kind, SsaTypeKind::MapContainer { .. }).then_some(index)
                })
                .collect();
            assert_eq!(
                maps.len(),
                2,
                "Map and MutableMap must remain distinct SSA types"
            );
            let contracts: Vec<_> = maps
                .iter()
                .map(|index| match program.modules[0].types[*index] {
                    SsaTypeKind::MapContainer { kind, key, value } => (kind, key, value),
                    _ => unreachable!("filtered Map container type"),
                })
                .collect();
            let mut kinds: Vec<_> = contracts.iter().map(|entry| entry.0).collect();
            kinds.sort();
            assert_eq!(kinds, [MapContainerKind::Map, MapContainerKind::MutableMap]);
            assert_eq!(contracts[0].1, contracts[1].1, "same actual key type");
            assert_eq!(contracts[0].2, contracts[1].2, "same actual value type");
            for (_, key, value) in contracts {
                assert_eq!(key, value, "fixture K/V are identical");
                let actual = program.modules[0].type_kind(key).unwrap();
                if source_index == 0 {
                    assert_eq!(
                        actual,
                        &SsaTypeKind::Integer {
                            bits: 32,
                            signed: true
                        }
                    );
                } else {
                    assert_eq!(actual, &SsaTypeKind::StringOwner);
                }
            }
            let headers: Vec<Vec<String>> = maps
                .into_iter()
                .map(|index| {
                    let prefix = format!("%koven.map.t{index} = type {{ ");
                    let matches: Vec<_> = llvm
                        .lines()
                        .filter_map(|line| line.strip_prefix(&prefix))
                        .collect();
                    assert_eq_diagnostic!(matches.len(), 1, "{prefix}\n{llvm}");
                    matches[0]
                        .strip_suffix(" }")
                        .unwrap()
                        .split(", ")
                        .map(str::to_owned)
                        .collect()
                })
                .collect();
            diagnostic!("ORDERED MAP HEADER source={source_index} unit={unit}: {headers:?}");
            observations.push(headers);
        }
    }
    assert_eq!(observations.len(), 4);
    for headers in observations {
        assert_eq!(
            headers[0], headers[1],
            "same K/V must share physical storage"
        );
        for fields in headers {
            assert_eq!(
                fields.len(),
                6,
                "buffer,size,capacity,tombstones,first,last"
            );
            assert_eq!(fields[0], "ptr");
            assert!(fields[1].starts_with('i'));
            assert!(fields[1..].iter().all(|field| field == &fields[1]));
        }
    }
}

pub(in crate::ssa) mod source_owner_handoff {
    //! Ordinary Return/local Move terminal identities retain this provider's actual current owner.
    use crate::ssa::lower_frontend::map_iteration::*;

    use crate::ssa::map_source::handoff::{self, Handoff};
    use crate::ssa::tests::source_owner::*;
    use lang_frontend::{parser::NameMarker, type_checking::CallableResultSource};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::ssa) struct Proof {
        root: SymbolId,
        terminal: Handoff<SymbolId>,
        pub(in crate::ssa) origin: Span,
        drop_index: Option<usize>,
    }
    impl Proof {
        pub(in crate::ssa) fn drop(
            self,
            owned: &OwnershipCheckedFile,
        ) -> Result<Option<DropFact>, LoweringError> {
            let bad = || error(LoweringErrorKind::MissingFact, self.origin);
            let Handoff::Binding { root, marker, .. } = self.terminal else {
                return Ok(None);
            };
            let fact = *owned
                .drops()
                .get(self.drop_index.ok_or_else(bad)?)
                .ok_or_else(bad)?;
            if fact.target() != DropTarget::Named(root) || fact.value_origin() != marker {
                return Err(bad());
            }
            Ok(Some(fact))
        }
    }

    pub(in crate::ssa) fn qualify(
        frontend: (
            &ParsedFile,
            &NameResolution,
            &TypedFile,
            &OwnershipCheckedFile,
        ),
        flow: Flow,
        span: Span,
    ) -> Result<Option<Proof>, LoweringError> {
        let (parsed, names, typed, owned) = frontend;
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let root = match flow.source {
            SourceIdentity::Parameter(root) | SourceIdentity::Formed { symbol: root, .. } => root,
            _ => return Ok(None),
        };
        let plan = owned.map_iteration(flow.statement).ok_or_else(bad)?;
        let storage = plan.descriptor().source_type();
        if plan.source_access() != MapIterationSourceAccess::Owned
            || owned
                .drops()
                .iter()
                .any(|d| d.target() == DropTarget::Named(root))
        {
            return Ok(None);
        }
        let resolve = |at| {
            unique(names.references().iter().filter(|r| r.span() == at))
                .and_then(|r| match r.target() {
                    lang_frontend::name_resolution::ReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                })
                .ok_or_else(bad)
        };
        let declaration = |at| {
            unique(
                names
                    .symbols()
                    .iter()
                    .filter(|s| s.span() == at && !s.is_synthetic()),
            )
            .map(|s| s.id())
            .ok_or_else(bad)
        };
        let Some(terminal) =
            handoff::find(parsed, span, root, storage, resolve, declaration, |id| {
                typed.expression_type(id).zip(typed.expression_category(id))
            })?
        else {
            return Ok(None);
        };
        let exit = crate::ssa::tests::source_owner::phi(
            owned.cleanup_conditions(),
            plan,
            root,
            IterationPhiBoundary::Exit,
            span,
        )?;
        let live = crate::ssa::tests::source_owner::flow_condition(
            parsed,
            names,
            owned,
            flow,
            exit.owner(),
            span,
        )?;
        if live != exit.availability_condition()
            || typed.copyability(storage) != Some(Copyability::MoveOnly)
        {
            return Err(bad());
        }
        let drop_index = match terminal {
            Handoff::Return { .. } => {
                let function = parsed
                    .ast()
                    .items()
                    .iter()
                    .filter(|(_, n)| {
                        matches!(n.payload(), Item::Function { .. })
                            && n.span().start() <= span.start()
                            && span.end() <= n.span().end()
                    })
                    .min_by_key(|(_, n)| n.span().end() - n.span().start())
                    .ok_or_else(bad)?;
                let Item::Function {
                    name: NameMarker::Present(marker),
                    ..
                } = function.1.payload()
                else {
                    return Err(bad());
                };
                let symbol = declaration(*marker)?;
                let callable = unique(typed.callables().iter().filter(|c| c.symbol() == symbol))
                    .ok_or_else(bad)?;
                if callable.return_type() != storage
                    || callable.result_source() != CallableResultSource::Owned
                {
                    return Err(bad());
                }
                None
            }
            Handoff::Binding {
                root: successor,
                marker,
                ..
            } => {
                if successor == root
                    || typed.symbol_type(successor) != Some(storage)
                    || !owned.formed_owned_locals().contains(&successor)
                {
                    return Err(bad());
                }
                let (index, fact) = unique(
                    owned
                        .drops()
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| d.target() == DropTarget::Named(successor)),
                )
                .ok_or_else(bad)?;
                if fact.value_origin() != marker
                    || fact.owner() != Some(exit.owner())
                    || fact.condition() != Some(live)
                    || fact.capture_slot().is_some()
                    || fact.instance_address().is_some()
                    || !matches!(fact.point(), DropPoint::CallReturn(_))
                {
                    return Err(bad());
                }
                Some(index)
            }
        };
        Ok(Some(Proof {
            root,
            terminal,
            origin: names.symbols().get(root.index()).ok_or_else(bad)?.span(),
            drop_index,
        }))
    }

    pub(in crate::ssa) mod delivery {
        //! Selected Value delivery and future ASAP Drop qualification for retained Map sources.
        use crate::ssa::lower_frontend::map_iteration::*;

        use crate::ssa::tests::source_owner::*;
        use lang_frontend::{
            name_resolution::ReferenceTarget,
            type_checking::{CallableTarget, ExpressionCategory},
        };

        pub(in crate::ssa) mod future {
            use crate::ssa::lower_frontend::map_iteration::*;

            use crate::ssa::tests::source_owner::*;
            use lang_frontend::name_resolution::ReferenceTarget;

            pub(in crate::ssa) fn qualify(
                parsed: &ParsedFile,
                names: &NameResolution,
                owned: &OwnershipCheckedFile,
                plan: &MapIterationOwnershipPlan,
                root: (
                    SymbolId,
                    Option<&crate::ssa::map_source::Formation<SymbolId>>,
                ),
                body_exit: IterationExitKind,
                span: Span,
            ) -> Result<Proof, LoweringError> {
                let (symbol, birth) = root;
                let bad = || error(LoweringErrorKind::MissingFact, span);
                let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
                if !matches!(
                    body_exit,
                    IterationExitKind::Fallthrough
                        | IterationExitKind::Continue(_)
                        | IterationExitKind::Break(_)
                ) || plan.closure_phis().len() != 2
                    || plan.closure_phi_incomings().len() != 3
                    || !plan.capture_graph().nodes().is_empty()
                {
                    return Err(unsupported());
                }
                let drops = if let Some(formation) = birth {
                    formed::drops(parsed, owned, symbol, formation)?
                } else {
                    owned
                        .drops()
                        .iter()
                        .copied()
                        .filter(|f| f.target() == DropTarget::Named(symbol))
                        .collect::<Vec<_>>()
                };
                let [drop] = drops.as_slice() else {
                    return Err(unsupported());
                };
                let point = match drop.point() {
                    DropPoint::CallReturn(e)
                    | DropPoint::AfterExpression(e)
                    | DropPoint::ControlTransfer(e) => {
                        parsed.ast().expressions().get(e).map_err(|_| bad())?.span()
                    }
                    DropPoint::LoopExit(s) | DropPoint::AfterStatement(s) => {
                        parsed.ast().statements().get(s).map_err(|_| bad())?.span()
                    }
                    _ => return Err(unsupported()),
                };
                if !crate::ssa::map_source::future_reference(
                    parsed,
                    span,
                    point,
                    names.references().iter().filter_map(|r| {
                        (r.target() == &ReferenceTarget::Symbol(symbol)).then_some(r.span())
                    }),
                ) {
                    return Err(unsupported());
                }
                let origin = names.symbols().get(symbol.index()).ok_or_else(bad)?.span();
                let conditions = owned.cleanup_conditions();
                let live = |owner| match birth {
                    Some(birth) => crate::ssa::tests::source_owner::formed_condition(
                        parsed,
                        owned,
                        (symbol, birth.birth),
                        owner,
                        span,
                    ),
                    None => crate::ssa::tests::source_owner::provenance::parameter(
                        owned, symbol, origin, owner, span,
                    ),
                };
                for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
                    let phi = crate::ssa::tests::source_owner::phi(
                        conditions, plan, symbol, boundary, span,
                    )?;
                    live(phi.owner())?;
                }
                let condition = live(drop.owner().ok_or_else(bad)?)?;
                if drop.value_origin() != origin
                    || drop.condition() != Some(condition)
                    || drop.capture_slot().is_some()
                    || drop.instance_address().is_some()
                {
                    return Err(bad());
                }
                if plan.exits().iter().any(|exit| {
                    exit.actions().iter().any(|action| {
                        matches!(action, IterationCleanupAction::Drop(fact)
                            if fact.target() == DropTarget::Named(symbol))
                    })
                }) {
                    return Err(bad());
                }
                let exhaustion = unique(
                    plan.exits()
                        .iter()
                        .filter(|e| e.kind() == IterationExitKind::Exhaustion),
                )
                .ok_or_else(bad)?;
                if exhaustion.actions()
                    != [
                        IterationCleanupAction::FinishProvider(plan.descriptor().statement()),
                        IterationCleanupAction::EndSource(plan.descriptor().statement()),
                    ]
                {
                    return Err(bad());
                }
                Ok(Proof {
                    flow: None,
                    disposition: source_owner::Disposition::Drop(*drop),
                    transfer: None,
                    conditional_return: None,
                })
            }
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub(in crate::ssa) struct Delivery {
            pub(in crate::ssa) root: SymbolId,
            pub(in crate::ssa) call: ExpressionId,
            pub(in crate::ssa) argument: ExpressionId,
            pub(in crate::ssa) parameter: usize,
            pub(in crate::ssa) owner: lang_frontend::ownership_checking::CleanupOwnerValueId,
            pub(in crate::ssa) origin: Span,
        }

        pub(in crate::ssa) fn qualify(
            parsed: &ParsedFile,
            names: &NameResolution,
            typed: &TypedFile,
            owned: &OwnershipCheckedFile,
            flow: Flow,
            span: Span,
        ) -> Result<Delivery, LoweringError> {
            let bad = || error(LoweringErrorKind::MissingFact, span);
            let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
            let plan = owned.map_iteration(flow.statement).ok_or_else(bad)?;
            let root = match flow.source {
                SourceIdentity::Parameter(root)
                | SourceIdentity::Local { symbol: root, .. }
                | SourceIdentity::Formed { symbol: root, .. } => root,
                _ => return Err(unsupported()),
            };
            let has_drop = match flow.source {
                SourceIdentity::Formed { symbol, .. } => {
                    let formation = formed::formation(
                        parsed,
                        names,
                        typed,
                        owned,
                        symbol,
                        plan.descriptor().source_type(),
                        span,
                    )?;
                    !formed::drops(parsed, owned, symbol, &formation)?.is_empty()
                }
                _ => owned
                    .drops()
                    .iter()
                    .any(|f| f.target() == DropTarget::Named(root)),
            };
            if has_drop {
                return Err(unsupported());
            }
            let storage = plan.descriptor().source_type();
            let origin = names.symbols().get(root.index()).ok_or_else(bad)?.span();
            let exit = crate::ssa::tests::source_owner::phi(
                owned.cleanup_conditions(),
                plan,
                root,
                IterationPhiBoundary::Exit,
                span,
            )?;
            let live = crate::ssa::tests::source_owner::flow_condition(
                parsed,
                names,
                owned,
                flow,
                exit.owner(),
                span,
            )?;
            if live != exit.availability_condition()
                || owned.cleanup_conditions().get(live) == Some(&CleanupCondition::Never)
            {
                return Err(bad());
            }
            let mut candidates = Vec::new();
            for call in typed.calls() {
                let node = parsed
                    .ast()
                    .expressions()
                    .get(call.expression())
                    .map_err(|_| bad())?;
                if !crate::ssa::map_source::future_reference(
                    parsed,
                    span,
                    node.span(),
                    names.references().iter().filter_map(|r| {
                        (r.target() == &ReferenceTarget::Symbol(root)).then_some(r.span())
                    }),
                ) {
                    continue;
                }
                let Expression::Call { arguments, .. } = node.payload() else {
                    return Err(bad());
                };
                for mapping in call.arguments() {
                    let argument = arguments.get(mapping.argument_index()).ok_or_else(bad)?;
                    let canonical = crate::ssa::map_source::transparent_expression(
                        parsed,
                        argument.value,
                        |id| typed.expression_type(id).zip(typed.expression_category(id)),
                    )?;
                    let operand = parsed
                        .ast()
                        .expressions()
                        .get(canonical)
                        .map_err(|_| bad())?;
                    if operand.payload() != &Expression::Name {
                        continue;
                    }
                    let reference = unique(
                        names
                            .references()
                            .iter()
                            .filter(|r| r.span() == operand.span()),
                    )
                    .ok_or_else(bad)?;
                    if reference.target() != &ReferenceTarget::Symbol(root)
                        || mapping.mode() != ParameterMode::Value
                    {
                        continue;
                    }
                    if !matches!(call.target(), CallableTarget::Source(_)) || call.aborts() {
                        return Err(unsupported());
                    }
                    if mapping.parameter_type() != storage
                        || mapping.category() != ExpressionCategory::Place
                        || typed.expression_type(argument.value) != Some(storage)
                        || typed.copyability(storage) != Some(Copyability::MoveOnly)
                    {
                        return Err(bad());
                    }
                    candidates.push(Delivery {
                        root,
                        call: call.expression(),
                        argument: argument.value,
                        parameter: mapping.parameter_index(),
                        owner: exit.owner(),
                        origin,
                    });
                }
            }
            let [delivery] = candidates.as_slice() else {
                return Err(unsupported());
            };
            Ok(*delivery)
        }
    }
}
