//! CI-only synthetic verifier negative contracts. Never select in ordinary-source local manifests.
use super::{
    model::{EntityId, EntityType, Operation, Origin, Program, SsaTypeKind, TerminatorKind},
    verify::{VerifyErrorKind, verify_program},
};
use crate::test_support::diagnostics::assert_diagnostic;
use lang_frontend::source::SourceMap;

fn fixture(source: SsaTypeKind, results: Vec<SsaTypeKind>) -> Program {
    let mut sources = SourceMap::new();
    let source_id = sources.add_source("contract.ko", "contract").unwrap();
    let origin = Origin::Source(sources.span(source_id, 0, 8).unwrap());
    let mut program = Program::default();
    let module_id = program.add_module("conversion-contract");
    let module = program.module_mut(module_id).unwrap();
    let source = module.intern_type(source);
    let result_types = results
        .into_iter()
        .map(|ty| EntityType::Value(module.intern_type(ty)))
        .collect();
    let function_id = module
        .add_function("conversion-contract", vec![], origin.clone())
        .unwrap();
    let function = module.function_mut(function_id).unwrap();
    let entry = function
        .add_block(vec![EntityType::Value(source)], origin.clone())
        .unwrap();
    let EntityId::Value(operand) = function.block(entry).unwrap().parameters[0] else {
        panic!("value");
    };
    function
        .append_instruction(
            entry,
            Operation::CheckedIntegerConvert { operand },
            result_types,
            origin.clone(),
        )
        .unwrap();
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
        .unwrap();
    program
}

fn integer() -> SsaTypeKind {
    SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    }
}

fn assert_invalid(source: SsaTypeKind, results: Vec<SsaTypeKind>) {
    let errors =
        verify_program(&fixture(source, results)).expect_err("invalid conversion contract");
    assert_diagnostic!(
        errors
            .errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. })),
        "{errors:?}"
    );
}

#[test]
fn rejects_boolean_source() {
    assert_invalid(SsaTypeKind::Boolean, vec![integer(), SsaTypeKind::Boolean]);
}

#[test]
fn rejects_non_language_integer_source() {
    assert_invalid(
        SsaTypeKind::Integer {
            bits: 128,
            signed: true,
        },
        vec![integer(), SsaTypeKind::Boolean],
    );
}

#[test]
fn rejects_char_target() {
    assert_invalid(integer(), vec![SsaTypeKind::Char, SsaTypeKind::Boolean]);
}

#[test]
fn rejects_non_language_integer_target() {
    assert_invalid(
        integer(),
        vec![
            SsaTypeKind::Integer {
                bits: 7,
                signed: false,
            },
            SsaTypeKind::Boolean,
        ],
    );
}

#[test]
fn rejects_missing_failure_result() {
    assert_invalid(integer(), vec![integer()]);
}

#[test]
fn rejects_extra_result() {
    assert_invalid(
        integer(),
        vec![integer(), SsaTypeKind::Boolean, SsaTypeKind::Boolean],
    );
}

#[test]
fn rejects_non_boolean_failure_result() {
    assert_invalid(integer(), vec![integer(), integer()]);
}

pub(in crate::ssa) mod outer_borrowed_while {
    use crate::ssa::lower_frontend::container::access::outer_borrowed_source as source;
    pub(in crate::ssa) use crate::ssa::lower_frontend::*;

    use lang_frontend::{
        ownership_checking::{
            CleanupCondition, CleanupConditions, CleanupOwnerValue, CleanupSelection,
            CleanupSelectorSource, DropFact, DropTarget, IterationCleanupAction, IterationExitKind,
            IterationPhiBoundary, IterationPhiIncomingKind, IterationPhiPresenceSource,
            IterationSource, OwnershipBindingKind,
        },
        type_checking::{IterationProvider, SequentialContainerKind, SequentialIterationBinding},
    };

    /// Only complete published facts and the bounded source relation construct this token.
    #[derive(Clone, PartialEq, Eq)]
    pub(in crate::ssa) struct Relation {
        pub(in crate::ssa) outer: StatementId,
        pub(in crate::ssa) inner: StatementId,
        pub(in crate::ssa) breaking: ExpressionId,
        pub(in crate::ssa) root: SymbolId,
        pub(in crate::ssa) items: SymbolId,
        pub(in crate::ssa) item: SymbolId,
        pub(in crate::ssa) storage: TypeId,
        pub(in crate::ssa) parameter_slot: usize,
        pub(in crate::ssa) initializer: ExpressionId,
        pub(in crate::ssa) drops: [DropFact; 2],
    }

    pub(in crate::ssa) fn candidate(
        parsed: &ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        fact: DropFact,
    ) -> Option<Relation> {
        let DropPoint::LoopExit(outer) = fact.point() else {
            return None;
        };
        let plan = owned.iteration(outer)?;
        let descriptor = plan.descriptor();
        let IterationSource::BorrowedSource(LoanTarget::Place(place)) = plan.source_kind() else {
            return None;
        };
        let items = place.root();
        let SequentialIterationBinding::Name(item) = descriptor.binding() else {
            return None;
        };
        let item = *item;
        let conditions = owned.cleanup_conditions();
        let exhausted = unique(
            plan.exits()
                .iter()
                .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
        )?;
        let [
            IterationCleanupAction::FinishProvider(finish),
            IterationCleanupAction::EndSource(end),
            IterationCleanupAction::Drop(items_drop),
            IterationCleanupAction::Drop(current_drop),
        ] = exhausted.actions()
        else {
            return None;
        };
        let DropTarget::Named(root) = current_drop.target() else {
            return None;
        };
        let storage = typed.symbol_type(root)?;
        let TypeKind::Nullable(inner) = typed.types().get(storage)? else {
            return None;
        };
        let TypeKind::Nominal { nominal, arguments } = typed.types().get(*inner)? else {
            return None;
        };
        if ![current_drop, items_drop].contains(&&fact)
            || *finish != outer
            || *end != outer
            || exhausted.point() != fact.point()
            || descriptor.statement() != outer
            || descriptor.provider() != IterationProvider::Sequential(SequentialContainerKind::List)
            || descriptor.delivery() != ParameterMode::Borrow
            || !place.is_root()
            || root == items
            || item == root
            || item == items
            || typed.symbol_type(items) != Some(descriptor.source_type())
            || typed.symbol_type(item) != Some(descriptor.element_type())
            || builtin_type(typed, descriptor.element_type()) != Some(BuiltinType::Int)
            || descriptor.source_type() == storage
            || !arguments.is_empty()
            || typed
                .nominals()
                .iter()
                .find(|entry| entry.id() == *nominal)?
                .kind()
                != NominalKind::Class
            || [storage, *inner, descriptor.source_type()]
                .iter()
                .any(|ty| typed.copyability(*ty) != Some(Copyability::MoveOnly))
            || !plan.capture_graph().nodes().is_empty()
            || plan.bindings().len() != 1
            || plan.bindings()[0].symbol() != item
            || plan.bindings()[0].kind() != OwnershipBindingKind::Shared
            || owned
                .borrow_results()
                .bindings()
                .iter()
                .any(|binding| [root, items].contains(&binding.binding()))
            || plan.closure_phis().len() != 4
            || plan.closure_phi_incomings().len() != 3
            || plan.exits().len() != 2
        {
            return None;
        }
        let node = parsed.ast().statements().get(outer).ok()?;
        let Statement::For { source, body, .. } = node.payload() else {
            return None;
        };
        if *source != descriptor.source() {
            return None;
        }
        let span = node.span();
        let mut seeds = Vec::new();
        for (symbol, drop) in [(root, current_drop), (items, items_drop)] {
            let header = unique(plan.closure_phis().iter().filter(|phi| {
                phi.symbol() == symbol && phi.boundary() == IterationPhiBoundary::Header
            }))?;
            let exit = unique(plan.closure_phis().iter().filter(|phi| {
                phi.symbol() == symbol && phi.boundary() == IterationPhiBoundary::Exit
            }))?;
            if drop.target() != DropTarget::Named(symbol)
                || drop.owner() != Some(header.owner())
                || drop.condition() != Some(header.availability_condition())
                || drop.point() != fact.point()
                || drop.capture_slot().is_some()
                || drop.instance_address().is_some()
                || drop.value_origin() != names.symbols().get(symbol.index())?.span()
                || unique(
                    owned
                        .drops()
                        .iter()
                        .filter(|candidate| candidate.target() == DropTarget::Named(symbol)),
                ) != Some(drop)
                || !crate::ssa::lower_frontend::constant_presence::prove_graph(
                    owned,
                    header.owner(),
                    symbol,
                    drop.condition(),
                )
            {
                return None;
            }
            for phi in [header, exit] {
                if !phi.root_nodes().is_empty()
                    || !phi.capture_layout().is_empty()
                    || !phi.origins().is_empty()
                    || !matches!(conditions.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi { statement, boundary, symbol: identity, origin })
                        if *statement == outer && *boundary == phi.boundary() && *identity == symbol && *origin == span)
                {
                    return None;
                }
                let slot = plan
                    .closure_phis()
                    .iter()
                    .filter(|other| other.boundary() == phi.boundary())
                    .position(|other| other.owner() == phi.owner())?;
                let selector = conditions.selector(phi.availability_selector())?;
                if selector.source()
                    != (CleanupSelectorSource::IterationPhi {
                        statement: outer,
                        boundary: phi.boundary(),
                        slot,
                    })
                    || selector.selection() != CleanupSelection::IterationPresence
                    || selector.branch_count() != 2
                    || selector.origin() != span
                    || !matches!(conditions.get(phi.availability_condition()), Some(CleanupCondition::Choice { selector, branches })
                        if *selector == phi.availability_selector() && branches.len() == 2 && never(conditions, branches[0]) && always(conditions, branches[1]))
                {
                    return None;
                }
            }
            for kind in [
                IterationPhiIncomingKind::Entry,
                IterationPhiIncomingKind::Fallthrough,
                IterationPhiIncomingKind::Exhaustion,
            ] {
                let edge = unique(
                    plan.closure_phi_incomings()
                        .iter()
                        .filter(|edge| edge.kind() == kind),
                )?;
                let target = if kind == IterationPhiIncomingKind::Exhaustion {
                    exit
                } else {
                    header
                };
                let point = match kind {
                    IterationPhiIncomingKind::Entry => DropPoint::AfterExpression(*source),
                    IterationPhiIncomingKind::Fallthrough => DropPoint::AfterStatement(*body),
                    _ => fact.point(),
                };
                let binding = unique(
                    edge.bindings()
                        .iter()
                        .filter(|binding| binding.target() == target.owner()),
                )?;
                if edge.bindings().len() != 2
                    || edge.boundary() != target.boundary()
                    || edge.point() != point
                    || !always(conditions, edge.condition())
                    || binding.availability_selector() != target.availability_selector()
                    || binding.presence_source() != IterationPhiPresenceSource::StaticConditions
                    || !binding.capture_slots_to_clear().is_empty()
                    || !binding.root_sources().is_empty()
                    || !binding.selector_writes().is_empty()
                    || !binding.origins().is_empty()
                {
                    return None;
                }
                if kind == IterationPhiIncomingKind::Exhaustion {
                    if !binding.values().is_empty() || !never(conditions, binding.available_when())
                    {
                        return None;
                    }
                } else {
                    let [value] = binding.values() else {
                        return None;
                    };
                    if value.condition() != binding.available_when() {
                        return None;
                    }
                    if kind == IterationPhiIncomingKind::Entry {
                        if !always(conditions, binding.available_when()) {
                            return None;
                        }
                        seeds.push(value.source());
                    } else if value.source() != header.owner()
                        || binding.available_when() != header.availability_condition()
                    {
                        return None;
                    }
                }
            }
        }
        for (index, phi) in plan.closure_phis().iter().enumerate() {
            if plan.closure_phis()[..index].iter().any(|other| {
                other.owner() == phi.owner()
                    || other.availability_selector() == phi.availability_selector()
            }) {
                return None;
            }
        }
        for flow in [plan.closure_flow().header(), plan.closure_flow().exit()] {
            if flow.len() != 2
                || [root, items].iter().any(|symbol| {
                    unique(flow.iter().filter(|binding| binding.symbol() == *symbol))
                        .is_none_or(|binding| !binding.origins().is_empty())
                })
            {
                return None;
            }
        }
        let fallthrough = unique(
            plan.exits()
                .iter()
                .filter(|exit| exit.kind() == IterationExitKind::Fallthrough),
        )?;
        if fallthrough.point() != DropPoint::AfterStatement(*body)
            || !always(conditions, fallthrough.condition())
            || !always(conditions, exhausted.condition())
            || fallthrough.actions()
                != [
                    IterationCleanupAction::EndBinding {
                        statement: outer,
                        symbol: item,
                    },
                    IterationCleanupAction::EndElement(outer),
                ]
        {
            return None;
        }
        let [seed, items_seed] = seeds.as_slice() else {
            return None;
        };
        let CleanupOwnerValue::Parameter {
            symbol: parameter,
            origin,
        } = conditions.owner_value(*seed)?
        else {
            return None;
        };
        let CleanupOwnerValue::Expression {
            expression: initializer,
            origin: source_origin,
        } = conditions.owner_value(*items_seed)?
        else {
            return None;
        };
        if seed == items_seed
            || typed.symbol_type(*parameter) != Some(storage)
            || names.symbols().get(parameter.index())?.span() != *origin
            || parsed.ast().expressions().get(*initializer).ok()?.span() != *source_origin
            || unique(
                owned
                    .bindings()
                    .iter()
                    .filter(|binding| binding.symbol() == *parameter),
            )
            .is_none_or(|binding| binding.kind() != OwnershipBindingKind::Owned)
        {
            return None;
        }
        let (inner, breaking, parameter_slot) = source::qualify(
            parsed,
            names,
            typed,
            outer,
            [root, items, item, *parameter],
            *initializer,
        )?;
        Some(Relation {
            outer,
            inner,
            breaking,
            root,
            items,
            item,
            storage,
            parameter_slot,
            initializer: *initializer,
            drops: [*items_drop, *current_drop],
        })
    }
    pub(in crate::ssa) fn unique<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
        let first = items.next()?;
        items.next().is_none().then_some(first)
    }
    fn always(
        table: &CleanupConditions,
        id: lang_frontend::ownership_checking::CleanupConditionId,
    ) -> bool {
        matches!(table.get(id), Some(CleanupCondition::Always))
    }
    fn never(
        table: &CleanupConditions,
        id: lang_frontend::ownership_checking::CleanupConditionId,
    ) -> bool {
        matches!(table.get(id), Some(CleanupCondition::Never))
    }
}
