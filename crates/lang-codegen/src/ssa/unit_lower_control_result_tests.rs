use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{Function, Operation, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn lowers_fact_backed_move_only_control_results_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun choose(own flag: Boolean): String {\n\
             val left = \"left-owner\"\n\
             val right = \"right-owner\"\n\
             val selected = if (flag) { left } else { right }\n\
             return selected\n\
         }\n\
         fun route(own flag: Boolean): String = when (flag) {\n\
             true -> \"when-a\" + \"when-b\"\n\
             false -> \"when-c\" + \"when-d\"\n\
         }\n\
         fun nested(own flag: Boolean): String = if (flag) {\n\
             when (flag) {\n\
                 true -> \"nested-a\" + \"nested-b\"\n\
                 false -> \"nested-c\" + \"nested-d\"\n\
             }\n\
         } else { \"outer-a\" + \"outer-b\" }\n\
         fun diverging(own flag: Boolean): String {\n\
             val selected = if (flag) { \"normal-a\" + \"normal-b\" } else { return \"early\" }\n\
             return selected\n\
         }\n\
         fun label(number: Int): String = \"captured\"",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Unit {\n\
             val chosen = p.choose(true)\n\
             val routed = p.route(false)\n\
             val nested = p.nested(true)\n\
             val escaped = p.diverging(false)\n\
             val action: move (borrow Boolean) -> String = move { flag ->\n\
                 when (flag) {\n\
                     true -> \"lambda-a\" + \"lambda-b\"\n\
                     false -> \"lambda-c\" + \"lambda-d\"\n\
                 }\n\
             }\n\
             val invoked = action(true)\n\
             val offset = 2\n\
             val captured: move (borrow Boolean) -> String = move { flag ->\n\
                 if (flag) { p.label(offset) } else { p.label(offset) }\n\
             }\n\
             val capturedResult = captured(false)\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (reversed_names, reversed_typed, reversed_owned) =
        analyze(&sources, &reversed, &name_environment, &type_environment);

    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("fact-backed MoveOnly control results lower to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reversed_names,
        &type_environment,
        &reversed_typed,
        &reversed_owned,
        declaration(&reversed_names, "q", "entry"),
    )
    .expect("input permutation preserves control-result SSA");
    assert_text_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let choose = function(module, "p.choose");
    assert_eq!(operation_count(choose, is_string_literal), 2);
    assert_eq!(operation_count(choose, is_drop), 2);
    assert_eq!(terminator_count(choose, is_conditional), 1);

    let route = function(module, "p.route");
    assert_eq!(operation_count(route, is_string_concat), 2);
    assert_eq!(operation_count(route, is_drop), 4);

    let nested = function(module, "p.nested");
    assert_eq!(operation_count(nested, is_string_concat), 3);
    assert_eq!(operation_count(nested, is_drop), 6);
    assert_eq!(terminator_count(nested, is_conditional), 2);

    let diverging = function(module, "p.diverging");
    assert_eq!(operation_count(diverging, is_string_concat), 1);
    assert_eq!(operation_count(diverging, is_drop), 2);
    assert_eq!(terminator_count(diverging, is_return), 2);

    let composite_thunk = module
        .functions
        .iter()
        .find(|function| operation_count(function, is_string_concat) == 2)
        .expect("composite control-result lambda thunk exists");
    assert_eq!(operation_count(composite_thunk, is_drop), 4);
    let captured_thunk = module
        .functions
        .iter()
        .find(|function| operation_count(function, is_shared_field_loan) == 1)
        .expect("captured control-result lambda thunk exists");
    assert_eq!(terminator_count(captured_thunk, is_conditional), 1);
    assert_eq!(
        operation_count(function(module, "q.entry"), is_closure_construct),
        1
    );
}

fn function<'a>(module: &'a super::model::Module, name: &str) -> &'a Function {
    module
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .unwrap_or_else(|| panic!("reachable function {name} exists"))
}

fn operation_count(function: &Function, predicate: fn(&Operation) -> bool) -> usize {
    function
        .instructions
        .iter()
        .filter(|instruction| predicate(&instruction.operation))
        .count()
}

fn terminator_count(function: &Function, predicate: fn(&TerminatorKind) -> bool) -> usize {
    function
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .filter(|terminator| predicate(&terminator.kind))
        .count()
}

fn is_string_literal(operation: &Operation) -> bool {
    matches!(operation, Operation::StringLiteral { .. })
}

fn is_string_concat(operation: &Operation) -> bool {
    matches!(operation, Operation::StringConcat { .. })
}

fn is_drop(operation: &Operation) -> bool {
    matches!(operation, Operation::Drop { .. })
}

fn is_closure_construct(operation: &Operation) -> bool {
    matches!(operation, Operation::ClosureConstruct { .. })
}

fn is_shared_field_loan(operation: &Operation) -> bool {
    matches!(operation, Operation::SharedFieldLoan { .. })
}

fn is_conditional(terminator: &TerminatorKind) -> bool {
    matches!(terminator, TerminatorKind::Conditional { .. })
}

fn is_return(terminator: &TerminatorKind) -> bool {
    matches!(terminator, TerminatorKind::Return { .. })
}

pub(in crate::ssa) mod map_iteration_formed {
    //! General ordinary local formation uses original Expression ownership, not a lexical carrier.
    pub(in crate::ssa) use crate::ssa::lower_frontend::map_iteration::local::retained;
    use crate::ssa::lower_frontend::map_iteration::*;

    use crate::ssa::map_source::{
        BindingDeclaration, Birth, Formation, OwnerUpdate, named_formation,
    };
    use lang_frontend::{
        name_resolution::ReferenceTarget,
        ownership_checking::{
            DropFact, DropTarget, IterationSource, MapIterationComponent, OwnershipBindingKind,
        },
        parser::{NameMarker, VariableKind},
        type_checking::{ExpressionCategory, IntrinsicTypeConstructor, MapIterationKind},
    };
    pub(in crate::ssa) use retained as epoch;

    pub(in crate::ssa) fn formation(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        root: SymbolId,
        storage: TypeId,
        span: Span,
    ) -> Result<Formation<SymbolId>, LoweringError> {
        let mut formation = raw_formation(parsed, names, typed, owned, root, storage, span)?;
        for event in &formation.history {
            let resolved = epoch::resolve(
                (parsed, names, typed, owned),
                root,
                storage,
                event.rhs,
                span,
            )?;
            formation
                .births
                .push((event.rhs, resolved.root, resolved.birth));
        }
        Ok(formation)
    }

    pub(in crate::ssa) fn raw_formation(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        root: SymbolId,
        storage: TypeId,
        span: Span,
    ) -> Result<Formation<SymbolId>, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        named_formation(
            parsed,
            span,
            root,
            |root| {
                let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
                if symbol.kind() == SymbolKind::ValueParameter {
                    let marker =
                        owned_parameter((parsed, names, typed, owned), root, storage, span)?;
                    return Ok(BindingDeclaration::EntryParameter { marker });
                }
                if symbol.kind() != SymbolKind::Variable {
                    return Err(unsupported());
                }
                if symbol.is_synthetic()
                    || typed.symbol_type(root) != Some(storage)
                    || !owned.formed_owned_locals().contains(&root)
                {
                    return Err(missing());
                }
                let item =
                    unique(parsed.ast().items().iter().filter_map(
                        |(_, node)| match node.payload() {
                            Item::Variable {
                                kind,
                                name: NameMarker::Present(marker),
                                initializer,
                                ..
                            } if *marker == symbol.span() => Some((*kind, *marker, *initializer)),
                            _ => None,
                        },
                    ))
                    .ok_or_else(missing)?;
                if !matches!(item.0, VariableKind::Val | VariableKind::Var) {
                    return Err(unsupported());
                }
                if typed.expression_type(item.2) != Some(storage) {
                    return Err(missing());
                }
                Ok(BindingDeclaration::Local {
                    marker: item.1,
                    initializer: item.2,
                })
            },
            |span| {
                let reference = unique(names.references().iter().filter(|r| r.span() == span))
                    .ok_or_else(missing)?;
                match reference.target() {
                    ReferenceTarget::Symbol(root) => Ok(*root),
                    _ => Err(missing()),
                }
            },
            |id| typed.expression_type(id).zip(typed.expression_category(id)),
            |id| match parsed
                .ast()
                .expressions()
                .get(id)
                .map_err(|_| missing())?
                .payload()
            {
                Expression::Call { .. } => Ok(Birth::Call(id)),
                _ => Err(unsupported()),
            },
            typed
                .map_puts()
                .iter()
                .map(|d| OwnerUpdate {
                    expression: d.expression(),
                    receiver: d.receiver(),
                })
                .chain(typed.map_removes().iter().map(|d| OwnerUpdate {
                    expression: d.expression(),
                    receiver: d.receiver(),
                })),
        )
    }

    /// The parameter belongs to this execution body, rather than an enclosing capture.
    pub(in crate::ssa) fn owned_parameter(
        frontend: (
            &ParsedFile,
            &NameResolution,
            &TypedFile,
            &OwnershipCheckedFile,
        ),
        root: SymbolId,
        storage: TypeId,
        span: Span,
    ) -> Result<Span, LoweringError> {
        let (parsed, names, typed, owned) = frontend;
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
        let function = parsed
            .ast()
            .items()
            .iter()
            .filter(|(_, node)| {
                matches!(node.payload(), Item::Function { .. })
                    && node.span().source_id() == span.source_id()
                    && node.span().start() <= span.start()
                    && span.end() <= node.span().end()
            })
            .min_by_key(|(_, node)| node.span().end() - node.span().start())
            .ok_or_else(unsupported)?;
        let Item::Function { parameters, .. } = function.1.payload() else {
            return Err(missing());
        };
        if parsed.ast().expressions().iter().any(|(_, node)| {
            matches!(node.payload(), Expression::Lambda { .. })
                && node.span().start() <= span.start()
                && span.end() <= node.span().end()
        }) {
            return Err(unsupported());
        }
        if symbol.kind() != SymbolKind::ValueParameter
            || symbol.is_synthetic()
            || !parameters
                .iter()
                .any(|p| p.name == NameMarker::Present(symbol.span()))
            || typed.symbol_type(root) != Some(storage)
            || typed.parameter_mode(root) != Some(ParameterMode::Value)
            || owned.binding_kind(root) != Some(OwnershipBindingKind::Owned)
        {
            return Err(missing());
        }
        Ok(symbol.span())
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
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        let d = plan.descriptor();
        let Some(LoanTarget::Place(place)) = plan.source() else {
            return Err(unsupported());
        };
        let root = place.root();
        if !place.is_root()
            || plan.source_kind()
                != &IterationSource::BorrowedSource(LoanTarget::Place(place.clone()))
            || plan.source_access() != MapIterationSourceAccess::Owned
            || typed.copyability(d.source_type()) != Some(Copyability::MoveOnly)
            || typed.expression_category(d.source()) != Some(ExpressionCategory::Place)
            || typed.expression_type(d.source()) != Some(d.source_type())
            || d.key_binding() == d.value_binding()
            || d.bindings().contains(&root)
        {
            return Err(missing());
        }
        let resource = typed
            .is_resource_type(d.source_type())
            .ok_or_else(missing)?;
        if qualify::has_enclosing_control(parsed, span) {
            return Err(unsupported());
        }
        let constructor = match d.kind() {
            MapIterationKind::Map => IntrinsicTypeConstructor::Map,
            MapIterationKind::MutableMap => IntrinsicTypeConstructor::MutableMap,
        };
        if !matches!(typed.types().get(d.source_type()),Some(TypeKind::Intrinsic{constructor:actual,arguments}) if *actual==constructor && arguments.as_slice()==[d.key_type(),d.value_type()])
        {
            return Err(missing());
        }
        for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .enumerate()
        {
            let binding = plan.bindings()[index];
            let loan = &plan.loans()[index];
            if binding.symbol() != d.bindings()[index]
                || binding.kind() != OwnershipBindingKind::Shared
                || loan.binding() != binding.symbol()
                || loan.component() != component
                || loan.statement() != d.statement()
                || loan.source() != d.source()
                || Some(loan.origin()) != plan.source()
                || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
                || typed.symbol_type(binding.symbol())
                    != Some([d.key_type(), d.value_type()][index])
            {
                return Err(missing());
            }
        }
        let formation = formation(parsed, names, typed, owned, root, d.source_type(), span)?;
        if let Birth::Call(producer) = formation.birth {
            local::producer(parsed, names, typed, owned, producer, d.source_type(), span)?;
        }
        // Other epochs retain ordinary call delivery, not provider capability.
        // Only the current epoch can be an already-proved alias Name.
        for epoch in &formation.history {
            let outer = parsed
                .ast()
                .expressions()
                .get(epoch.rhs)
                .map_err(|_| missing())?;
            if outer.span() == formation.binding {
                continue;
            }
            let resolved = epoch::resolve(
                (parsed, names, typed, owned),
                root,
                d.source_type(),
                epoch.rhs,
                span,
            )?;
            let Birth::Call(call) = resolved.birth else {
                return Err(unsupported());
            };
            if typed.map_construction(call).is_some() || typed.map_consume(call).is_some() {
                local::producer(parsed, names, typed, owned, call, d.source_type(), span)?;
                continue;
            }
            if typed.map_with_value(call).is_some() {
                return Err(unsupported());
            }
            let ordinary = typed.call(call).ok_or_else(missing)?;
            if !matches!(
                ordinary.target(),
                lang_frontend::type_checking::CallableTarget::Source(_)
            ) {
                return Err(unsupported());
            }
        }
        let source_identity = SourceIdentity::Formed {
            symbol: root,
            birth: formation.birth,
        };
        if let Some(shape) =
            conditional::shape(parsed, names, typed, *d, body, source_identity, span)?
        {
            let owner = conditional::qualify(parsed, names, typed, owned, plan, shape, span)?;
            return Ok(qualify::Qualified {
                plan,
                source: source_identity,
                body,
                origin: names
                    .symbols()
                    .get(root.index())
                    .ok_or_else(missing)?
                    .span(),
                owner,
                body_exit: IterationExitKind::Fallthrough,
                conditional: Some(shape),
                nesting: nested::Nesting { parent: None },
                local: None,
            });
        }
        let body_exit = exit_shape::body_exit(parsed, names, typed, *d, body, span)?;
        exit_shape::cleanup(owned, plan, body, body_exit, span)?;
        let owner = if resource {
            resource::qualify(
                parsed,
                names,
                typed,
                owned,
                plan,
                (root, Some(&formation)),
                body_exit,
            )?
        } else {
            source_owner::qualify_formed(
                parsed,
                names,
                owned,
                plan,
                (root, &formation),
                body,
                body_exit,
            )?
        };
        Ok(qualify::Qualified {
            plan,
            source: SourceIdentity::Formed {
                symbol: root,
                birth: formation.birth,
            },
            body,
            origin: names
                .symbols()
                .get(root.index())
                .ok_or_else(missing)?
                .span(),
            owner: Some(owner),
            body_exit,
            conditional: None,
            nesting: nested::Nesting { parent: None },
            local: None,
        })
    }

    /// Finishing the next replacement RHS still cleans the current binding epoch.
    /// The enclosing assignment installs the new owner only after this exact event.
    pub(in crate::ssa) fn replacement_point(
        formation: Option<&Formation<SymbolId>>,
        point: DropPoint,
    ) -> bool {
        matches!((formation.and_then(|f|f.replacement),point),
            (Some(expected),DropPoint::AfterExpression(actual)) if expected==actual)
    }

    /// Preflight a published non-current epoch without claiming the provider's current Drop.
    pub(in crate::ssa) fn other_epoch_drop(
        frontend: (
            &ParsedFile,
            &NameResolution,
            &TypedFile,
            &OwnershipCheckedFile,
        ),
        root: SymbolId,
        storage: TypeId,
        fact: lang_frontend::ownership_checking::DropFact,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let (parsed, names, typed, owned) = frontend;
        if fact.target() != lang_frontend::ownership_checking::DropTarget::Named(root)
            || !owned.drops().contains(&fact)
        {
            return Ok(false);
        }
        let formation = formation(parsed, names, typed, owned, root, storage, span)?;
        // This checks every original fact, including the other epoch's exact public
        // owner, complete incoming provenance, condition and original Drop marker.
        let current = drops(parsed, owned, root, &formation)?;
        let event = crate::ssa::map_source::drop_event(parsed, fact.point())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, fact.value_origin()))?;
        Ok(!current.contains(&fact)
            && !crate::ssa::map_source::in_formation(parsed, &formation, event)?)
    }

    /// Partition one named root by real binding/replacement events, never by spelling or adjacency.
    pub(in crate::ssa) fn drops(
        parsed: &ParsedFile,
        owned: &OwnershipCheckedFile,
        root: SymbolId,
        formation: &Formation<SymbolId>,
    ) -> Result<Vec<lang_frontend::ownership_checking::DropFact>, LoweringError> {
        use lang_frontend::ownership_checking::{CleanupCondition, CleanupOwnerValue};
        let mut facts = Vec::new();
        for fact in
            owned.drops().iter().copied().filter(|f| {
                f.target() == lang_frontend::ownership_checking::DropTarget::Named(root)
            })
        {
            let bad = || error(LoweringErrorKind::MissingFact, fact.value_origin());
            let point = crate::ssa::map_source::drop_event(parsed, fact.point())
                .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, fact.value_origin()))?;
            let epoch = crate::ssa::map_source::epoch_at(parsed, formation, point)?;
            if fact.capture_slot().is_some() || fact.instance_address().is_some() {
                return Err(bad());
            }
            if crate::ssa::map_source::in_formation(parsed, formation, point)? {
                if fact.value_origin() != formation.declaration {
                    return Err(bad());
                }
                facts.push(fact);
            } else {
                let (_, epoch_root, birth) = unique(
                    formation
                        .births
                        .iter()
                        .copied()
                        .filter(|(rhs, _, _)| *rhs == epoch.rhs),
                )
                .ok_or_else(bad)?;
                let Birth::Call(producer) = birth else {
                    return Err(error(LoweringErrorKind::UnsupportedNode, point.span));
                };
                let owner = fact.owner().ok_or_else(bad)?;
                let conditions = owned.cleanup_conditions();
                match conditions.owner_value(owner).ok_or_else(bad)? {
                    CleanupOwnerValue::Expression { expression, origin } => {
                        let node = parsed
                            .ast()
                            .expressions()
                            .get(producer)
                            .map_err(|_| bad())?;
                        if fact.value_origin() != epoch.origin
                            || *expression != producer
                            || *origin != node.span()
                            || fact.condition().is_some_and(|c| {
                                conditions.get(c) != Some(&CleanupCondition::Always)
                            })
                        {
                            return Err(bad());
                        }
                    }
                    CleanupOwnerValue::IterationPhi { .. } => {
                        // The moved root may share this provider with other live roots.
                        // Select its exact phi binding and prove every incoming to the
                        // actual Move birth, rather than requiring a singleton set.
                        let available = source_owner::birth_condition(
                            parsed,
                            owned,
                            (epoch_root, birth),
                            owner,
                            point.span,
                            true,
                        )?;
                        if fact.value_origin()
                            != if epoch_root == root {
                                formation.declaration
                            } else {
                                epoch.origin
                            }
                            || fact.condition() != Some(available)
                        {
                            return Err(bad());
                        }
                    }
                    _ => return Err(bad()),
                }
            }
        }
        Ok(facts)
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn validate_formed_map_drop(
            &self,
            fact: DropFact,
            owner: ValueId,
        ) -> Result<bool, LoweringError> {
            let DropTarget::Named(root) = fact.target() else {
                return Ok(false);
            };
            for plan in self.owned.map_iterations() {
                let descriptor = plan.descriptor();
                let qualified =
                    qualify::qualify(self.parsed, self.names, self.typed, self.owned, *descriptor)?;
                let SourceIdentity::Formed { symbol, birth } = qualified.source else {
                    continue;
                };
                if symbol != root || qualified.local.is_some() {
                    continue;
                }
                let span = self
                    .parsed
                    .ast()
                    .statements()
                    .get(descriptor.statement())
                    .map_err(|_| error(LoweringErrorKind::MissingFact, fact.value_origin()))?
                    .span();
                let formation = formation(
                    self.parsed,
                    self.names,
                    self.typed,
                    self.owned,
                    root,
                    descriptor.source_type(),
                    span,
                )?;
                if formation.birth != birth {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
                let bad = || error(LoweringErrorKind::MissingFact, fact.value_origin());
                let event = crate::ssa::map_source::drop_event(self.parsed, fact.point())
                    .ok_or_else(bad)?;
                if !crate::ssa::map_source::in_formation(self.parsed, &formation, event)? {
                    // Qualification above checked this epoch's exact public owner/condition.
                    let epoch = crate::ssa::map_source::epoch_at(self.parsed, &formation, event)?;
                    let resolved = epoch::resolve(
                        (self.parsed, self.names, self.typed, self.owned),
                        root,
                        descriptor.source_type(),
                        epoch.rhs,
                        event.span,
                    )?;
                    if let Some(formation) = resolved.formation {
                        self.formed_map_anchor(&formation, Some(owner), resolved.at)?;
                    } else {
                        let Birth::Call(producer) = resolved.birth else {
                            return Err(bad());
                        };
                        let anchor = self.map_call_birth(producer, event.span)?;
                        nested::transport(
                            self.function,
                            EntityId::Value(owner),
                            EntityId::Value(anchor),
                            event.span,
                        )?;
                    }
                    return Ok(true);
                }
                let matched = qualified
                    .owner
                    .map(|proof| proof.at_checked(fact.point(), self.owned))
                    .transpose()?
                    .flatten();
                if matched != Some(fact) {
                    return Err(bad());
                }
                let terminal = crate::ssa::map_source::continuation(
                    self.parsed,
                    &formation,
                    event,
                    span,
                    |id| {
                        self.typed
                            .expression_type(id)
                            .zip(self.typed.expression_category(id))
                    },
                    |at| self.references.get(&span_key(at)).copied().ok_or_else(bad),
                    self.typed
                        .map_puts()
                        .iter()
                        .map(|d| crate::ssa::map_source::OwnerUpdate {
                            expression: d.expression(),
                            receiver: d.receiver(),
                        })
                        .chain(self.typed.map_removes().iter().map(|d| {
                            crate::ssa::map_source::OwnerUpdate {
                                expression: d.expression(),
                                receiver: d.receiver(),
                            }
                        })),
                )?;
                if !formation.same_epoch(&terminal) {
                    return Err(bad());
                }
                self.formed_map_anchor(&terminal, Some(owner), event.span)?;
                return Ok(true);
            }
            for descriptor in self.typed.map_iterations() {
                let span = self
                    .parsed
                    .ast()
                    .statements()
                    .get(descriptor.statement())
                    .map_err(|_| error(LoweringErrorKind::MissingFact, fact.value_origin()))?
                    .span();
                let Some(plan) = self.owned.map_iteration(descriptor.statement()) else {
                    continue;
                };
                if let Some(retained) = retained::proof(
                    (self.parsed, self.names, self.typed, self.owned),
                    plan,
                    fact,
                    span,
                )? {
                    if let Some(formation) = retained.formation {
                        self.formed_map_anchor(&formation, Some(owner), span)?;
                    } else {
                        let SourceIdentity::Parameter(root) = retained.source else {
                            return Err(error(LoweringErrorKind::MissingFact, span));
                        };
                        let anchor = *self
                            .map_iteration_state
                            .entries
                            .get(&root)
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        let entry = self
                            .function
                            .entry_block()
                            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                        if !matches!(self.function.entity(anchor).map(|e|e.definition),Some(Definition::BlockParameter{block,..}) if block==entry)
                            || !matches!(anchor, EntityId::Value(_))
                            || self
                                .bindings
                                .values()
                                .any(|v| *v == LoweredValue::Value(owner))
                            || self.temporaries.values().any(|&v| v == owner)
                        {
                            return Err(error(LoweringErrorKind::MissingFact, span));
                        }
                        nested::transport(self.function, EntityId::Value(owner), anchor, span)?;
                    }
                    return Ok(true);
                }
            }
            Ok(false)
        }

        pub(in crate::ssa) fn formed_map_anchor(
            &self,
            formation: &Formation<SymbolId>,
            dropped: Option<ValueId>,
            span: Span,
        ) -> Result<ValueId, LoweringError> {
            let bad = || error(LoweringErrorKind::MissingFact, span);
            let anchor = match formation.birth {
                Birth::Call(producer) => self.map_call_birth(producer, span)?,
                Birth::Expression(_) => {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
                Birth::EntryParameter {
                    root,
                    reference,
                    origin,
                } => {
                    let symbol = self.names.symbols().get(root.index()).ok_or_else(bad)?;
                    if symbol.span() != origin
                        || self.typed.parameter_mode(root) != Some(ParameterMode::Value)
                        || self
                            .references
                            .get(&span_key(self.expression_span(reference)?))
                            != Some(&root)
                    {
                        return Err(bad());
                    }
                    let EntityId::Value(anchor) = *self
                        .map_iteration_state
                        .entries
                        .get(&root)
                        .ok_or_else(bad)?
                    else {
                        return Err(bad());
                    };
                    let entry = self.function.entry_block().ok_or_else(bad)?;
                    if !matches!(self.function.entity(EntityId::Value(anchor)).map(|e|e.definition),Some(Definition::BlockParameter{block,..}) if block==entry)
                    {
                        return Err(bad());
                    }
                    anchor
                }
            };
            let current = if let Some(owner) = dropped {
                owner
            } else {
                let root = formation.roots.first().ok_or_else(bad)?;
                match self.bindings.get(root) {
                    Some(LoweredValue::Value(owner)) => *owner,
                    _ => return Err(bad()),
                }
            };
            if self
                .temporaries
                .values()
                .any(|v| *v == current || *v == anchor)
                || self.bindings.iter().any(|(root, value)| {
                    Some(root) != formation.roots.first()
                        && matches!(value,LoweredValue::Value(v) if *v==current||*v==anchor)
                })
            {
                return Err(bad());
            }
            check_formed_transport(self, formation, current, anchor, span)?;
            Ok(anchor)
        }
    }

    fn check_formed_transport(
        lowerer: &ExpressionLowerer<'_>,
        formation: &Formation<SymbolId>,
        current: ValueId,
        anchor: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let function = &lowerer.function;
        let anchor = EntityId::Value(anchor);
        let mut updates = BTreeMap::new();
        for update in &formation.updates {
            let at = lowerer.expression_span(update.expression)?;
            let put = lowerer.typed.map_put(update.expression);
            let remove = lowerer.typed.map_remove(update.expression);
            if put.is_some() == remove.is_some()
                || put
                    .map(|d| d.receiver())
                    .or_else(|| remove.map(|d| d.receiver()))
                    != Some(update.receiver)
            {
                return Err(bad());
            }
            let instruction = unique(function.instructions.iter().filter(|i| {
                i.origin == Origin::Source(at)
                    && matches!(
                        i.operation,
                        Operation::MapPut { .. } | Operation::MapRemove { .. }
                    )
            }))
            .ok_or_else(bad)?;
            let owner = match instruction.operation {
                Operation::MapPut { owner, .. } if put.is_some() => owner,
                Operation::MapRemove { owner, .. } if remove.is_some() => owner,
                _ => return Err(bad()),
            };
            let Some(&result @ EntityId::Value(_)) = instruction.results.first() else {
                return Err(bad());
            };
            if updates.insert(result, EntityId::Value(owner)).is_some() {
                return Err(bad());
            }
        }
        crate::ssa::map_source::transport::check(
            function,
            EntityId::Value(current),
            anchor,
            &updates,
            span,
        )
    }
}
