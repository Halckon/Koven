//! One complete two-version cleanup group and bounded constant-presence proof.
pub(in crate::ssa) mod constant_presence {
    pub(in crate::ssa) use crate::ssa::deinit_tests::constant_presence_source as source;
    use source::source_return;

    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupOwnerValue, DropFact,
        DropTarget, IterationPhiBoundary, IterationPhiIncomingKind, IterationPhiPresenceSource,
        LoanTarget, OwnershipCheckedFile,
    };
    use lang_frontend::type_checking::TypedFile;
    use std::collections::BTreeSet;

    fn implies(
        conditions: &CleanupConditions,
        path: CleanupConditionId,
        presence: CleanupConditionId,
        proven: &BTreeSet<CleanupConditionId>,
        visiting: &mut BTreeSet<(CleanupConditionId, CleanupConditionId)>,
    ) -> bool {
        if path == presence || proven.contains(&presence) {
            return true;
        }
        if !visiting.insert((path, presence)) {
            return false;
        }
        let always = CleanupCondition::Always;
        let path_node = if proven.contains(&path) {
            Some(&always)
        } else {
            conditions.get(path)
        };
        let result = match (path_node, conditions.get(presence)) {
            (_, Some(CleanupCondition::Always)) | (Some(CleanupCondition::Never), _) => true,
            (
                Some(CleanupCondition::Choice {
                    selector: a,
                    branches: x,
                }),
                Some(CleanupCondition::Choice {
                    selector: b,
                    branches: y,
                }),
            ) if a == b && x.len() == y.len() => x
                .iter()
                .zip(y)
                .all(|(x, y)| implies(conditions, *x, *y, proven, visiting)),
            (Some(CleanupCondition::Choice { branches, .. }), _) => branches
                .iter()
                .all(|branch| implies(conditions, *branch, presence, proven, visiting)),
            (_, Some(CleanupCondition::Choice { branches, .. })) => branches
                .iter()
                .all(|branch| implies(conditions, path, *branch, proven, visiting)),
            _ => false,
        };
        visiting.remove(&(path, presence));
        result
    }

    pub(in crate::ssa) fn constant_drop(
        parsed: &lang_frontend::parser::ParsedFile,
        names: &lang_frontend::name_resolution::NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        fact: DropFact,
    ) -> bool {
        let DropTarget::Named(symbol) = fact.target() else {
            return false;
        };
        let Some(owner) = fact.owner() else {
            return false;
        };
        let conditions = owned.cleanup_conditions();
        let Some(CleanupOwnerValue::IterationPhi {
            statement,
            symbol: identity,
            ..
        }) = conditions.owner_value(owner)
        else {
            return false;
        };
        if *identity != symbol {
            return false;
        }
        let Some(plan) = owned.iteration(*statement) else {
            return false;
        };
        let source = matches!(plan.source(), Some(LoanTarget::Place(path)) if path.is_root() && path.root() == symbol);
        let resource = typed
            .symbol_type(symbol)
            .is_some_and(|ty| typed.is_resource_type(ty) == Some(true));
        if !source && !resource {
            return false;
        }
        prove_graph(owned, owner, symbol, fact.condition())
            || source::prove(parsed, names, typed, owned, fact).is_some()
            || source_return::prove(parsed, names, typed, owned, fact).is_some()
    }

    pub(in crate::ssa) fn prove_graph(
        owned: &OwnershipCheckedFile,
        owner: lang_frontend::ownership_checking::CleanupOwnerValueId,
        symbol: lang_frontend::name_resolution::SymbolId,
        expected: Option<CleanupConditionId>,
    ) -> bool {
        let conditions = owned.cleanup_conditions();
        let mut pending = vec![owner];
        let mut visited = BTreeSet::new();
        let mut origins = BTreeSet::new();
        let mut proven = BTreeSet::new();
        let mut requirements = Vec::new();
        while let Some(current) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }
            let Some(CleanupOwnerValue::IterationPhi {
                statement,
                boundary,
                symbol: identity,
                ..
            }) = conditions.owner_value(current)
            else {
                return false;
            };
            if *identity != symbol {
                return false;
            }
            let Some(plan) = owned.iteration(*statement) else {
                return false;
            };
            let Some(target) = plan.closure_phis().iter().find(|phi| {
                phi.owner() == current && phi.boundary() == *boundary && phi.root_nodes().is_empty()
            }) else {
                return false;
            };
            if current == owner && expected != Some(target.availability_condition()) {
                return false;
            }
            let Some(header) = plan.closure_phis().iter().find(|phi| {
                phi.symbol() == symbol
                    && phi.boundary() == IterationPhiBoundary::Header
                    && phi.root_nodes().is_empty()
            }) else {
                return false;
            };
            pending.push(header.owner());
            proven.insert(header.availability_condition());
            proven.insert(target.availability_condition());
            let edges = plan
                .closure_phi_incomings()
                .iter()
                .filter(|edge| edge.boundary() == *boundary)
                .collect::<Vec<_>>();
            if edges.is_empty()
                || (*boundary == IterationPhiBoundary::Header
                    && edges
                        .iter()
                        .filter(|edge| edge.kind() == IterationPhiIncomingKind::Entry)
                        .count()
                        != 1)
            {
                return false;
            }
            for edge in edges {
                let Some(binding) = edge
                    .bindings()
                    .iter()
                    .find(|binding| binding.target() == current)
                else {
                    return false;
                };
                if binding.presence_source() != IterationPhiPresenceSource::StaticConditions
                    || binding.values().len() != 1
                {
                    return false;
                }
                let value = binding.values()[0];
                for condition in [binding.available_when(), value.condition()] {
                    requirements.push((edge.condition(), condition));
                }
                match conditions.owner_value(value.source()) {
                    Some(CleanupOwnerValue::IterationPhi {
                        symbol: source_symbol,
                        ..
                    }) if *source_symbol == symbol => pending.push(value.source()),
                    Some(
                        CleanupOwnerValue::Expression { .. } | CleanupOwnerValue::Parameter { .. },
                    ) => {
                        if *boundary != IterationPhiBoundary::Header
                            || edge.kind() != IterationPhiIncomingKind::Entry
                            || !matches!(
                                conditions.get(binding.available_when()),
                                Some(CleanupCondition::Always)
                            )
                            || !matches!(
                                conditions.get(value.condition()),
                                Some(CleanupCondition::Always)
                            )
                        {
                            return false;
                        }
                        origins.insert(value.source());
                    }
                    _ => return false,
                }
            }
        }
        origins.len() == 1
            && requirements.into_iter().all(|(path, presence)| {
                implies(conditions, path, presence, &proven, &mut BTreeSet::new())
            })
    }
}

pub(in crate::ssa) mod local_runtime {
    pub(in crate::ssa) mod conditions {
        use crate::ssa::lower_frontend::map_iteration::local::{Path, Proof, identity};
        use crate::ssa::lower_frontend::map_iteration::*;
        use lang_frontend::ownership_checking::{
            CleanupConditionId, CleanupConditions, CleanupSelection, CleanupSelectorId,
            CleanupSelectorSource,
        };

        #[derive(Clone, Copy)]
        pub(in crate::ssa) enum Expected {
            Path,
            Presence,
            Conjunction,
        }

        pub(in crate::ssa) fn path(
            parsed: &ParsedFile,
            conditions: &CleanupConditions,
            condition: CleanupConditionId,
            previous: Option<(&identity::Site<'_>, &Proof)>,
            span: Span,
        ) -> Result<Option<Path>, LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let Some((site, proof)) = previous.filter(|(site, _)| site.guard.is_some()) else {
                if conditions.get(condition) != Some(&CleanupCondition::Always) {
                    return Err(missing());
                }
                return Ok(None);
            };
            let guard = site.guard.ok_or_else(missing)?;
            if proof.late_size != Some(guard.size)
                || proof.drop.point() != DropPoint::CallReturn(guard.size)
            {
                return Err(missing());
            }
            let Some(CleanupCondition::Choice { selector, branches }) = conditions.get(condition)
            else {
                return Err(missing());
            };
            let [success, failure] = branches.as_slice() else {
                return Err(missing());
            };
            let control = parsed
                .ast()
                .expressions()
                .get(guard.control)
                .map_err(|_| missing())?;
            let layout = conditions.selector(*selector).ok_or_else(missing)?;
            if layout.source() != CleanupSelectorSource::Control(guard.control)
                || layout.selection() != CleanupSelection::Branch
                || layout.origin() != control.span()
                || layout.branch_count() != 2
                || conditions.get(*success) != Some(&CleanupCondition::Always)
                || conditions.get(*failure) != Some(&CleanupCondition::Never)
            {
                return Err(missing());
            }
            Ok(Some(Path {
                control: guard.control,
                condition: guard.condition,
                selector: *selector,
                branch: 0,
                success: guard.success,
                failure: guard.failure,
                size: guard.size,
                previous: site.plan.descriptor().statement(),
            }))
        }

        pub(in crate::ssa) fn check(
            conditions: &CleanupConditions,
            condition: CleanupConditionId,
            path: Option<Path>,
            presence: Option<CleanupSelectorId>,
            expected: Expected,
            span: Span,
        ) -> Result<(), LoweringError> {
            validate(
                conditions,
                condition,
                path,
                presence,
                &mut Vec::new(),
                &mut Vec::new(),
                span,
            )?;
            for path_live in [false, true] {
                for owner_live in [false, true] {
                    let actual = evaluate(
                        conditions,
                        condition,
                        path,
                        presence,
                        (path_live, owner_live),
                        conditions.nodes().len(),
                        span,
                    )?;
                    let reached = path.is_none() || path_live;
                    let expected = match expected {
                        Expected::Path => reached,
                        Expected::Presence => owner_live,
                        Expected::Conjunction => reached && owner_live,
                    };
                    if actual != expected {
                        return Err(error(LoweringErrorKind::MissingFact, span));
                    }
                }
            }
            Ok(())
        }

        fn evaluate(
            conditions: &CleanupConditions,
            condition: CleanupConditionId,
            path: Option<Path>,
            presence: Option<CleanupSelectorId>,
            assignment: (bool, bool),
            remaining: usize,
            span: Span,
        ) -> Result<bool, LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            if remaining == 0 {
                return Err(missing());
            }
            let (path_live, owner_live) = assignment;
            match conditions.get(condition).ok_or_else(missing)? {
                CleanupCondition::Always => Ok(true),
                CleanupCondition::Never => Ok(false),
                CleanupCondition::Choice { selector, branches } => {
                    let layout = conditions.selector(*selector).ok_or_else(missing)?;
                    if branches.len() != 2 || layout.branch_count() != 2 {
                        return Err(missing());
                    }
                    let branch = if Some(*selector) == presence {
                        if layout.selection() != CleanupSelection::IterationPresence {
                            return Err(missing());
                        }
                        usize::from(owner_live)
                    } else if let Some(path) = path.filter(|path| path.selector == *selector) {
                        if layout.source() != CleanupSelectorSource::Control(path.control)
                            || layout.selection() != CleanupSelection::Branch
                        {
                            return Err(missing());
                        }
                        if path_live {
                            path.branch
                        } else {
                            1 - path.branch
                        }
                    } else {
                        return Err(missing());
                    };
                    evaluate(
                        conditions,
                        branches[branch],
                        path,
                        presence,
                        assignment,
                        remaining - 1,
                        span,
                    )
                }
            }
        }

        fn validate(
            conditions: &CleanupConditions,
            condition: CleanupConditionId,
            path: Option<Path>,
            presence: Option<CleanupSelectorId>,
            active: &mut Vec<CleanupConditionId>,
            done: &mut Vec<CleanupConditionId>,
            span: Span,
        ) -> Result<(), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            if active.len() >= 3 || active.contains(&condition) {
                return Err(missing());
            }
            if done.contains(&condition) {
                return Ok(());
            }
            active.push(condition);
            match conditions.get(condition).ok_or_else(missing)? {
                CleanupCondition::Always | CleanupCondition::Never => {}
                CleanupCondition::Choice { selector, branches } => {
                    let layout = conditions.selector(*selector).ok_or_else(missing)?;
                    let valid = if Some(*selector) == presence {
                        layout.selection() == CleanupSelection::IterationPresence
                    } else {
                        path.is_some_and(|path| {
                            path.selector == *selector
                                && layout.source() == CleanupSelectorSource::Control(path.control)
                                && layout.selection() == CleanupSelection::Branch
                        })
                    };
                    if !valid || branches.len() != 2 || layout.branch_count() != 2 {
                        return Err(missing());
                    }
                    for branch in branches {
                        validate(conditions, *branch, path, presence, active, done, span)?;
                    }
                }
            }
            active.pop();
            done.push(condition);
            Ok(())
        }
    }
}
