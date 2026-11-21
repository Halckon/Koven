//! CI-only synthetic verifier negatives; never select in ordinary-source local manifests.
use super::{
    model::{EntityId, EntityType, Operation, Origin, Program, SsaTypeKind, TerminatorKind},
    verify::{VerifyErrorKind, verify_program},
};
use crate::test_support::diagnostics::assert_diagnostic;
use lang_frontend::source::SourceMap;

#[derive(Clone, Copy)]
enum Invalid {
    Unregistered,
    BadEmptyField,
    MissingField,
    UnsignedEndpoint,
    MixedConstructor,
    WrongMember,
    MixedEquality,
    NoResult,
    ExtraResult,
    PlaceResult,
}

fn fixture(case: Invalid) -> Program {
    let mut sources = SourceMap::new();
    let source = sources.add_source("range-contract.ko", "contract").unwrap();
    let origin = Origin::Source(sources.span(source, 0, 8).unwrap());
    let mut program = Program::default();
    let module_id = program.add_module("range-contract");
    let module = program.module_mut(module_id).unwrap();
    let int = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let long = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let unsigned = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: false,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let range = match case {
        Invalid::Unregistered => module
            .add_aggregate_type("lookalike", vec![int, int, boolean])
            .unwrap(),
        Invalid::BadEmptyField | Invalid::MissingField | Invalid::UnsignedEndpoint => {
            let endpoint = if matches!(case, Invalid::UnsignedEndpoint) {
                unsigned
            } else {
                int
            };
            let fields = match case {
                Invalid::BadEmptyField => vec![endpoint, endpoint, int],
                Invalid::MissingField => vec![endpoint, boolean],
                _ => vec![endpoint, endpoint, boolean],
            };
            let range = module
                .add_aggregate_type("invalid-registry", fields)
                .unwrap();
            module.integer_ranges.insert(range, endpoint);
            range
        }
        _ => module.add_integer_range_type(int).unwrap(),
    };
    let long_range = module.add_integer_range_type(long).unwrap();
    let function_id = module
        .add_function("contract", vec![], origin.clone())
        .unwrap();
    let function = module.function_mut(function_id).unwrap();
    let block = function
        .add_block(
            vec![
                EntityType::Value(int),
                EntityType::Value(long),
                EntityType::Value(range),
                EntityType::Value(long_range),
            ],
            origin.clone(),
        )
        .unwrap();
    let values = function
        .block(block)
        .unwrap()
        .parameters
        .iter()
        .map(|entity| {
            let EntityId::Value(value) = entity else {
                panic!("value parameter");
            };
            *value
        })
        .collect::<Vec<_>>();
    let operation = match case {
        Invalid::WrongMember => Operation::IntegerRangeContains {
            value: values[1],
            range: values[2],
        },
        Invalid::MixedEquality => Operation::IntegerRangeEqual {
            left: values[2],
            right: values[3],
        },
        _ => Operation::IntegerRangeConstruct {
            first: values[0],
            bound: if matches!(case, Invalid::MixedConstructor) {
                values[1]
            } else {
                values[0]
            },
            inclusive: true,
        },
    };
    let results = match case {
        Invalid::WrongMember | Invalid::MixedEquality => vec![EntityType::Value(boolean)],
        Invalid::NoResult => vec![],
        Invalid::ExtraResult => vec![EntityType::Value(range), EntityType::Value(range)],
        Invalid::PlaceResult => vec![EntityType::Place(range)],
        _ => vec![EntityType::Value(range)],
    };
    function
        .append_instruction(block, operation, results, origin.clone())
        .unwrap();
    function
        .set_terminator(block, TerminatorKind::Return { values: vec![] }, origin)
        .unwrap();
    program
}

fn rejected(case: Invalid) {
    let errors = verify_program(&fixture(case)).expect_err("invalid numeric Range contract");
    assert_diagnostic!(
        errors
            .errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. })),
        "{errors:?}"
    );
    if matches!(
        case,
        Invalid::BadEmptyField | Invalid::MissingField | Invalid::UnsignedEndpoint
    ) {
        assert_diagnostic!(
            errors
                .errors
                .iter()
                .any(|error| matches!(error.kind, VerifyErrorKind::InvalidTypeDefinition { .. })),
            "{errors:?}"
        );
    }
}

#[test]
fn rejects_unregistered_range_lookalike() {
    rejected(Invalid::Unregistered);
}
#[test]
fn rejects_range_registry_malformed_fields() {
    rejected(Invalid::BadEmptyField);
    rejected(Invalid::MissingField);
}
#[test]
fn rejects_range_registry_unsigned_endpoint() {
    rejected(Invalid::UnsignedEndpoint);
}
#[test]
fn rejects_mixed_constructor_endpoints() {
    rejected(Invalid::MixedConstructor);
}
#[test]
fn rejects_membership_wrong_endpoint() {
    rejected(Invalid::WrongMember);
}
#[test]
fn rejects_equality_different_range_identity() {
    rejected(Invalid::MixedEquality);
}
#[test]
fn rejects_range_result_count_and_entity_kind() {
    rejected(Invalid::NoResult);
    rejected(Invalid::ExtraResult);
    rejected(Invalid::PlaceResult);
}

pub(in crate::ssa) mod nullable_borrowed_cleanup {
    //! One readonly nullable owner across one independently owned List provider.
    //! Published presence and actual owner/view/provider columns are separate proofs.
    pub(in crate::ssa) use crate::ssa::borrow_memory_test_support::borrowed_cleanup_carrier as carrier;
    use carrier::source;

    use crate::ssa::lower_frontend::*;
    use lang_frontend::{
        ownership_checking::{
            CleanupCondition, CleanupConditionId, CleanupConditions, CleanupOwnerValue,
            CleanupSelection, CleanupSelectorSource, DropFact, DropTarget, IterationCleanupAction,
            IterationClosurePhiBinding, IterationExitKind, IterationPhiBoundary,
            IterationPhiIncomingBinding, IterationPhiIncomingKind, IterationPhiPresenceSource,
            IterationSource, OwnershipBindingKind,
        },
        type_checking::{IterationProvider, SequentialContainerKind, SequentialIterationBinding},
    };

    pub(in crate::ssa) struct Plan {
        pub(in crate::ssa) fact: DropFact,
        pub(in crate::ssa) root: SymbolId,
        pub(in crate::ssa) storage: TypeId,
        pub(in crate::ssa) statement: StatementId,
        pub(in crate::ssa) outer: StatementId,
        pub(in crate::ssa) source_root: SymbolId,
        pub(in crate::ssa) element: SymbolId,
        pub(in crate::ssa) source_drop: DropFact,
    }

    /// Admission never installs a proof, rewrites a condition or admits an SSA value.
    pub(in crate::ssa) fn candidate(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        fact: DropFact,
    ) -> Option<Plan> {
        let DropTarget::Named(root) = fact.target() else {
            return None;
        };
        let DropPoint::AfterExpression(member) = fact.point() else {
            return None;
        };
        let conditions = owned.cleanup_conditions();
        let CleanupOwnerValue::IterationPhi {
            statement,
            boundary: IterationPhiBoundary::Exit,
            symbol,
            ..
        } = conditions.owner_value(fact.owner()?)?
        else {
            return None;
        };
        let statement = *statement;
        let plan = owned.iteration(statement)?;
        let descriptor = plan.descriptor();
        let storage = typed.symbol_type(root)?;
        let TypeKind::Nullable(inner) = typed.types().get(storage)? else {
            return None;
        };
        let TypeKind::Nominal { nominal, arguments } = typed.types().get(*inner)? else {
            return None;
        };
        let IterationSource::BorrowedSource(LoanTarget::Place(place)) = plan.source_kind() else {
            return None;
        };
        let source_root = place.root();
        let SequentialIterationBinding::Name(element) = descriptor.binding() else {
            return None;
        };
        let element = *element;
        if *symbol != root
            || !arguments.is_empty()
            || typed
                .nominals()
                .iter()
                .find(|item| item.id() == *nominal)?
                .kind()
                != NominalKind::Class
            || typed.copyability(storage) != Some(Copyability::MoveOnly)
            || typed.copyability(*inner) != Some(Copyability::MoveOnly)
            || fact.capture_slot().is_some()
            || fact.instance_address().is_some()
            || fact.value_origin() != names.symbols().get(root.index())?.span()
            || owned
                .drops()
                .iter()
                .filter(|drop| drop.point() == fact.point() && drop.target() == fact.target())
                .count()
                != 1
            || !owned.cleanup_steps().iter().any(|(point, action)| {
                *point == fact.point() && *action == IterationCleanupAction::Drop(fact)
            })
            || descriptor.statement() != statement
            || descriptor.provider() != IterationProvider::Sequential(SequentialContainerKind::List)
            || descriptor.delivery() != ParameterMode::Borrow
            || builtin_type(typed, descriptor.element_type()) != Some(BuiltinType::Int)
            || typed.symbol_type(element) != Some(descriptor.element_type())
            || !place.is_root()
            || source_root == root
            || element == root
            || element == source_root
            || typed.symbol_type(source_root) != Some(descriptor.source_type())
            || descriptor.source_type() == storage
            || typed.copyability(descriptor.source_type()) != Some(Copyability::MoveOnly)
            || !plan.capture_graph().nodes().is_empty()
            || plan.bindings().len() != 1
            || plan.bindings()[0].symbol() != element
            || plan.bindings()[0].kind() != OwnershipBindingKind::Shared
            || owned
                .borrow_results()
                .bindings()
                .iter()
                .any(|binding| [root, source_root].contains(&binding.binding()))
            || plan.closure_phis().len() != 4
            || plan.closure_phi_incomings().len() != 4
            || plan.exits().len() != 3
        {
            return None;
        }
        let node = parsed.ast().statements().get(statement).ok()?;
        let Statement::For { source, body, .. } = node.payload() else {
            return None;
        };
        if *source != descriptor.source() {
            return None;
        }
        let span = node.span();
        let phi = |symbol, boundary| {
            unique(
                plan.closure_phis()
                    .iter()
                    .filter(|phi| phi.symbol() == symbol && phi.boundary() == boundary),
            )
        };
        let header = phi(root, IterationPhiBoundary::Header)?;
        let exit = phi(root, IterationPhiBoundary::Exit)?;
        let source_header = phi(source_root, IterationPhiBoundary::Header)?;
        let source_exit = phi(source_root, IterationPhiBoundary::Exit)?;
        for (index, phi) in plan.closure_phis().iter().enumerate() {
            if !phi.capture_layout().is_empty()
                || !phi.root_nodes().is_empty()
                || !phi.origins().is_empty()
                || plan.closure_phis()[..index].iter().any(|other| {
                    other.owner() == phi.owner()
                        || other.availability_selector() == phi.availability_selector()
                })
                || !matches!(conditions.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi {
                    statement: id, boundary, symbol, origin }) if *id == statement && *boundary == phi.boundary()
                    && *symbol == phi.symbol() && *origin == span)
            {
                return None;
            }
            let slot = plan.closure_phis()[..index]
                .iter()
                .filter(|other| other.boundary() == phi.boundary())
                .count();
            let selector = conditions.selector(phi.availability_selector())?;
            if selector.source()
                != (CleanupSelectorSource::IterationPhi {
                    statement,
                    boundary: phi.boundary(),
                    slot,
                })
                || selector.selection() != CleanupSelection::IterationPresence
                || selector.branch_count() != 2
                || selector.origin() != span
                || !matches!(conditions.get(phi.availability_condition()), Some(CleanupCondition::Choice { selector, branches })
                    if *selector == phi.availability_selector() && branches.len() == 2
                    && never(conditions, branches[0]) && always(conditions, branches[1]))
            {
                return None;
            }
        }
        if exit.owner() != fact.owner()? || fact.condition()? != exit.availability_condition() {
            return None;
        }
        for flow in [plan.closure_flow().header(), plan.closure_flow().exit()] {
            if flow.len() != 2
                || [root, source_root].iter().any(|symbol| {
                    unique(flow.iter().filter(|binding| binding.symbol() == *symbol))
                        .is_none_or(|binding| !binding.origins().is_empty())
                })
            {
                return None;
            }
        }
        let mut seeds = None;
        let mut actual = Vec::new();
        for incoming in plan.closure_phi_incomings() {
            if incoming.bindings().len() != 2 {
                return None;
            }
            let targets = match incoming.kind() {
                IterationPhiIncomingKind::Entry => {
                    if incoming.point() != DropPoint::AfterExpression(descriptor.source())
                        || !always(conditions, incoming.condition())
                        || seeds.is_some()
                    {
                        return None;
                    }
                    [header, source_header]
                }
                IterationPhiIncomingKind::Continue(expression) => {
                    if incoming.point() != DropPoint::ControlTransfer(expression)
                        || !matches!(
                            source::expr(parsed, expression)?,
                            Expression::Continue { .. }
                        )
                    {
                        return None;
                    }
                    actual.push((
                        IterationExitKind::Continue(expression),
                        incoming.point(),
                        incoming.condition(),
                    ));
                    [header, source_header]
                }
                IterationPhiIncomingKind::Fallthrough => {
                    if incoming.point() != DropPoint::AfterStatement(*body) {
                        return None;
                    }
                    actual.push((
                        IterationExitKind::Fallthrough,
                        incoming.point(),
                        incoming.condition(),
                    ));
                    [header, source_header]
                }
                IterationPhiIncomingKind::Exhaustion => {
                    if incoming.point() != DropPoint::LoopExit(statement)
                        || !always(conditions, incoming.condition())
                    {
                        return None;
                    }
                    actual.push((
                        IterationExitKind::Exhaustion,
                        incoming.point(),
                        incoming.condition(),
                    ));
                    [exit, source_exit]
                }
                IterationPhiIncomingKind::Break(_) => return None,
            };
            let mut entry = Vec::new();
            for (target, header) in targets.into_iter().zip([header, source_header]) {
                let binding = unique(
                    incoming
                        .bindings()
                        .iter()
                        .filter(|binding| binding.target() == target.owner()),
                )?;
                if incoming.boundary() != target.boundary() || !plain_binding(binding, target) {
                    return None;
                }
                if incoming.kind() == IterationPhiIncomingKind::Exhaustion
                    && target.symbol() == source_root
                {
                    if !binding.values().is_empty() || !never(conditions, binding.available_when())
                    {
                        return None;
                    }
                    continue;
                }
                let [value] = binding.values() else {
                    return None;
                };
                if value.condition() != binding.available_when() {
                    return None;
                }
                match incoming.kind() {
                    IterationPhiIncomingKind::Entry => {
                        if !always(conditions, binding.available_when()) {
                            return None;
                        }
                        entry.push(value.source());
                    }
                    IterationPhiIncomingKind::Exhaustion => {
                        if value.source() != header.owner()
                            || binding.available_when() != header.availability_condition()
                        {
                            return None;
                        }
                    }
                    _ => {
                        if value.source() != header.owner()
                            || !header_conjunction(
                                conditions,
                                header,
                                binding.available_when(),
                                incoming.condition(),
                            )
                        {
                            return None;
                        }
                    }
                }
            }
            if incoming.kind() == IterationPhiIncomingKind::Entry {
                let [current, source] = entry.as_slice() else {
                    return None;
                };
                seeds = Some((*current, *source));
            }
        }
        let (seed, source_seed) = seeds?;
        let CleanupOwnerValue::Parameter {
            symbol: parameter,
            origin,
        } = conditions.owner_value(seed)?
        else {
            return None;
        };
        let CleanupOwnerValue::Expression {
            expression: source_seed_expression,
            origin: source_seed_origin,
        } = conditions.owner_value(source_seed)?
        else {
            return None;
        };
        if parsed
            .ast()
            .expressions()
            .get(*source_seed_expression)
            .ok()?
            .span()
            != *source_seed_origin
            || unique(
                owned
                    .bindings()
                    .iter()
                    .filter(|binding| binding.symbol() == *parameter),
            )
            .is_none_or(|binding| binding.kind() != OwnershipBindingKind::Owned)
            || typed.symbol_type(*parameter) != Some(storage)
            || names.symbols().get(parameter.index())?.span() != *origin
            || seed == source_seed
            || actual.len() != 3
            || actual.iter().enumerate().any(|(index, item)| {
                actual[..index]
                    .iter()
                    .any(|other| other.0 == item.0 || other.1 == item.1)
            })
            || actual.iter().any(|item| {
                plan.exits()
                    .iter()
                    .filter(|exit| (exit.kind(), exit.point(), exit.condition()) == *item)
                    .count()
                    != 1
            })
        {
            return None;
        }
        let source_drop = *unique(owned.drops().iter().filter(|drop| {
            drop.target() == DropTarget::Named(source_root)
                && drop.owner() == Some(source_header.owner())
        }))?;
        if source_drop.point() != DropPoint::LoopExit(statement)
            || source_drop.condition() != Some(source_header.availability_condition())
            || source_drop.value_origin() != names.symbols().get(source_root.index())?.span()
            || !crate::ssa::lower_frontend::constant_presence::constant_drop(
                parsed,
                names,
                typed,
                owned,
                source_drop,
            )
        {
            return None;
        }
        for edge in plan.exits() {
            let expected = match edge.kind() {
                IterationExitKind::Continue(_) | IterationExitKind::Fallthrough => vec![
                    IterationCleanupAction::EndBinding {
                        statement,
                        symbol: element,
                    },
                    IterationCleanupAction::EndElement(statement),
                ],
                IterationExitKind::Exhaustion => vec![
                    IterationCleanupAction::FinishProvider(statement),
                    IterationCleanupAction::EndSource(statement),
                    IterationCleanupAction::Drop(source_drop),
                ],
                _ => return None,
            };
            if edge.actions() != expected.as_slice() {
                return None;
            }
        }
        let outer = source::qualify(
            parsed,
            names,
            typed,
            statement,
            member,
            root,
            *parameter,
            source_root,
            element,
            *source_seed_expression,
        )?;
        for (symbol, seed, final_drop) in
            [(root, seed, fact), (source_root, source_seed, source_drop)]
        {
            let drops = owned
                .drops()
                .iter()
                .filter(|drop| drop.target() == DropTarget::Named(symbol))
                .collect::<Vec<_>>();
            if drops.len() != 2
                || drops.iter().any(|drop| {
                    **drop != final_drop
                        && !(drop.point() == DropPoint::LoopExit(outer)
                            && drop.owner() == Some(seed)
                            && drop.condition().is_none()
                            && drop.capture_slot().is_none()
                            && drop.instance_address().is_none())
                })
            {
                return None;
            }
        }
        Some(Plan {
            fact,
            root,
            storage,
            statement,
            outer,
            source_root,
            element,
            source_drop,
        })
    }

    pub(in crate::ssa) fn unique<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
        let first = items.next()?;
        items.next().is_none().then_some(first)
    }
    fn always(table: &CleanupConditions, id: CleanupConditionId) -> bool {
        matches!(table.get(id), Some(CleanupCondition::Always))
    }
    fn never(table: &CleanupConditions, id: CleanupConditionId) -> bool {
        matches!(table.get(id), Some(CleanupCondition::Never))
    }
    fn header_conjunction(
        table: &CleanupConditions,
        header: &IterationClosurePhiBinding,
        condition: CleanupConditionId,
        edge: CleanupConditionId,
    ) -> bool {
        matches!(table.get(condition), Some(CleanupCondition::Choice { selector, branches })
            if *selector == header.availability_selector() && branches.len() == 2
            && never(table, branches[0]) && branches[1] == edge)
    }
    fn plain_binding(
        binding: &IterationPhiIncomingBinding,
        target: &IterationClosurePhiBinding,
    ) -> bool {
        binding.target() == target.owner()
            && binding.availability_selector() == target.availability_selector()
            && binding.presence_source() == IterationPhiPresenceSource::StaticConditions
            && binding.capture_slots_to_clear().is_empty()
            && binding.root_sources().is_empty()
            && binding.selector_writes().is_empty()
            && binding.origins().is_empty()
    }
}
