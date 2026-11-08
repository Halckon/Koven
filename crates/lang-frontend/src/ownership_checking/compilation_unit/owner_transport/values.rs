//! Versioned value projection. Choice is not a producer definition.
//! Legacy projection is closed straight-line String transport; a separate sealed
//! planner transaction can publish the narrow single-binding Choice slice.
//! A structurally valid choice graph does not establish typed AST admission.
mod catalog;
pub use catalog::UnitOwnerProducerCatalogV1;

use super::{
    UnitOwnerCallable, UnitOwnerId, UnitOwnerObservation, UnitOwnerObservationStatus,
    UnitOwnerOrigin, UnitOwnerTransportEvent,
};
use crate::ownership_checking::UnitDropPoint;
use crate::{name_resolution::UnitSymbolId, type_checking::UnitExpressionId};

/// Opaque selection identity; never an owner producer.
#[derive(Clone, Debug)]
pub struct UnitOwnerChoiceIdV1 {
    pub(in crate::ownership_checking::compilation_unit) identity: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) callable: UnitOwnerCallable,
    pub(in crate::ownership_checking::compilation_unit) control: UnitExpressionId,
}

impl PartialEq for UnitOwnerChoiceIdV1 {
    fn eq(&self, other: &Self) -> bool {
        self.same(other)
    }
}
impl Eq for UnitOwnerChoiceIdV1 {}

impl UnitOwnerChoiceIdV1 {
    fn same(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.identity, &other.identity)
            && self.callable == other.callable
            && self.control == other.control
    }
}

/// A real definition or a separately identified selected value.
#[derive(Clone, Debug)]
pub enum UnitOwnerValueV1 {
    /// An unchanged real producer token.
    Definition(UnitOwnerId),
    /// A selection token, resolved separately on each branch.
    Choice(UnitOwnerChoiceIdV1),
}

/// Value conservation projection with actual named cleanup points.
#[derive(Clone, Debug)]
pub enum UnitOwnerValueEventV1 {
    /// A real String producer enters its expression result slot.
    Create(UnitExpressionId, UnitOwnerId),
    /// An existing expression result is forwarded without creating a definition.
    Forward(UnitExpressionId, UnitExpressionId, UnitOwnerValueV1),
    /// A consuming name takes a named owner into an expression result.
    Move(UnitExpressionId, UnitSymbolId, UnitOwnerValueV1),
    /// A binding receives an existing expression result.
    Bind(UnitSymbolId, UnitExpressionId, UnitOwnerValueV1),
    /// A call registers an evaluated value operand.
    Pending(UnitExpressionId, UnitExpressionId, UnitOwnerValueV1),
    /// Atomic exchange of two complete String roots, with no result or new producer.
    Swap {
        /// Checked source-qualified call.
        call: UnitExpressionId,
        /// Left root before exchange.
        left: UnitSymbolId,
        /// Right root before exchange.
        right: UnitSymbolId,
        /// Value at the left root before exchange.
        left_value: UnitOwnerValueV1,
        /// Value at the right root before exchange.
        right_value: UnitOwnerValueV1,
    },
    /// Atomic replacement exchanges the root and pending operand.
    Replace {
        /// Checked call identity.
        call: UnitExpressionId,
        /// Pending operand expression.
        argument: UnitExpressionId,
        /// Replaced root binding.
        root: UnitSymbolId,
        /// Previous root value, returned by replacement.
        previous: UnitOwnerValueV1,
        /// Incoming pending value, installed in the root.
        incoming: UnitOwnerValueV1,
    },
    /// A real named cleanup consumes its current value.
    Drop(UnitDropPoint, UnitSymbolId, UnitOwnerValueV1),
}

/// One real tail owner delivered along an ordered branch edge.
#[derive(Clone, Debug)]
pub struct UnitOwnerChoiceInputV1 {
    pub(in crate::ownership_checking::compilation_unit) edge: usize,
    pub(in crate::ownership_checking::compilation_unit) tail: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) owner: UnitOwnerId,
}

/// One non-nested two-way selection with complete alternative traces.
#[derive(Clone, Debug)]
pub struct UnitOwnerChoiceV1 {
    pub(in crate::ownership_checking::compilation_unit) id: UnitOwnerChoiceIdV1,
    pub(in crate::ownership_checking::compilation_unit) events: [Vec<UnitOwnerValueEventV1>; 2],
    pub(in crate::ownership_checking::compilation_unit) inputs: [UnitOwnerChoiceInputV1; 2],
}

/// Read-only V1 value projection. Its consistency does not grant syntax admission.
#[derive(Clone, Debug)]
pub struct UnitOwnerValueObservationV1 {
    pub(in crate::ownership_checking::compilation_unit) catalog: UnitOwnerProducerCatalogV1,
    pub(in crate::ownership_checking::compilation_unit) prefix: Vec<UnitOwnerValueEventV1>,
    pub(in crate::ownership_checking::compilation_unit) choice: Option<UnitOwnerChoiceV1>,
    pub(in crate::ownership_checking::compilation_unit) suffix: Vec<UnitOwnerValueEventV1>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ownership_checking::compilation_unit) enum Slot {
    Uncreated,
    Result(UnitExpressionId),
    Named(UnitSymbolId),
    Pending(UnitExpressionId, UnitExpressionId),
    Dropped,
}

impl UnitOwnerValueObservationV1 {
    fn valid_id(&self, id: &UnitOwnerId) -> bool {
        self.catalog.definition(id.clone()).is_some()
    }

    fn source(&self) -> crate::name_resolution::SourceUnitId {
        match self.catalog.callable {
            UnitOwnerCallable::Function(item) | UnitOwnerCallable::Deinit(item) => {
                item.source_unit()
            }
            UnitOwnerCallable::Lambda(expression) => expression.source_unit(),
        }
    }

    fn resolve(&self, value: &UnitOwnerValueV1, selected: Option<usize>) -> Option<usize> {
        match value {
            UnitOwnerValueV1::Definition(id) if self.valid_id(id) => Some(id.index),
            UnitOwnerValueV1::Choice(id) => {
                let choice = self.choice.as_ref()?;
                if !id.same(&choice.id) {
                    return None;
                }
                let input = choice.inputs.get(selected?)?;
                self.valid_id(&input.owner).then_some(input.owner.index)
            }
            _ => None,
        }
    }

    fn replay<'event>(
        &self,
        events: &'event [UnitOwnerValueEventV1],
        selected: Option<usize>,
        slots: &mut [Slot],
        mut preceding: Option<&'event UnitOwnerValueEventV1>,
    ) -> bool {
        for event in events {
            let valid = match event {
                UnitOwnerValueEventV1::Create(expression, owner) => {
                    self.valid_id(owner)
                        && expression.source_unit() == self.source()
                        && self.catalog.definitions[owner.index].origin
                            == UnitOwnerOrigin::String(*expression)
                        && slots[owner.index] == Slot::Uncreated
                        && !slots.contains(&Slot::Result(*expression))
                        && {
                            slots[owner.index] = Slot::Result(*expression);
                            true
                        }
                }
                UnitOwnerValueEventV1::Forward(from, to, value) => self.transition(
                    value,
                    selected,
                    slots,
                    Slot::Result(*from),
                    Slot::Result(*to),
                ),
                UnitOwnerValueEventV1::Move(expression, symbol, value) => self.transition(
                    value,
                    selected,
                    slots,
                    Slot::Named(*symbol),
                    Slot::Result(*expression),
                ),
                UnitOwnerValueEventV1::Bind(symbol, expression, value) => self.transition(
                    value,
                    selected,
                    slots,
                    Slot::Result(*expression),
                    Slot::Named(*symbol),
                ),
                UnitOwnerValueEventV1::Pending(call, argument, value) => self.transition(
                    value,
                    selected,
                    slots,
                    Slot::Result(*argument),
                    Slot::Pending(*call, *argument),
                ),
                UnitOwnerValueEventV1::Drop(point, symbol, value) => {
                    super::super::owner_transport_validation::point_source(*point) == self.source()
                        && match point {
                            UnitDropPoint::AfterStatement(_)
                            | UnitDropPoint::AfterExpression(_)
                            | UnitDropPoint::FunctionEntry(_) => true,
                            // A contiguous call-return cleanup group retains its checked
                            // commit witness, including both swapped roots.
                            UnitDropPoint::CallReturn(call) if self.choice.is_none() => {
                                let installed = match preceding {
                                    Some(UnitOwnerValueEventV1::Replace {
                                        call: c,
                                        root,
                                        incoming,
                                        ..
                                    }) if c == call && root == symbol => Some(incoming),
                                    Some(UnitOwnerValueEventV1::Swap {
                                        call: c,
                                        left,
                                        right,
                                        left_value,
                                        right_value,
                                    }) if c == call => {
                                        if left == symbol {
                                            Some(right_value)
                                        } else if right == symbol {
                                            Some(left_value)
                                        } else {
                                            None
                                        }
                                    }
                                    _ => None,
                                };
                                installed
                                    .and_then(|v| self.resolve(v, selected))
                                    .zip(self.resolve(value, selected))
                                    .is_some_and(|(installed, dropped)| installed == dropped)
                            }
                            _ => false,
                        }
                        && self.transition(
                            value,
                            selected,
                            slots,
                            Slot::Named(*symbol),
                            Slot::Dropped,
                        )
                }
                UnitOwnerValueEventV1::Swap {
                    call,
                    left,
                    right,
                    left_value,
                    right_value,
                } => {
                    let pair = self
                        .resolve(left_value, selected)
                        .zip(self.resolve(right_value, selected));
                    if let Some((l, r)) = pair {
                        self.choice.is_none()
                            && l != r
                            && left != right
                            && call.source_unit() == self.source()
                            && left.source_unit() == self.source()
                            && right.source_unit() == self.source()
                            && slots[l] == Slot::Named(*left)
                            && slots[r] == Slot::Named(*right)
                            && {
                                slots[l] = Slot::Named(*right);
                                slots[r] = Slot::Named(*left);
                                true
                            }
                    } else {
                        false
                    }
                }
                UnitOwnerValueEventV1::Replace {
                    call,
                    argument,
                    root,
                    previous,
                    incoming,
                } => {
                    let pair = self
                        .resolve(previous, selected)
                        .zip(self.resolve(incoming, selected));
                    if let Some((old, new)) = pair {
                        old != new
                            && call.source_unit() == self.source()
                            && argument.source_unit() == self.source()
                            && root.source_unit() == self.source()
                            && slots[old] == Slot::Named(*root)
                            && slots[new] == Slot::Pending(*call, *argument)
                            && !slots.contains(&Slot::Result(*call))
                            && {
                                slots[old] = Slot::Result(*call);
                                slots[new] = Slot::Named(*root);
                                true
                            }
                    } else {
                        false
                    }
                }
            };
            if !valid {
                return false;
            }
            if !matches!(
                event,
                UnitOwnerValueEventV1::Drop(UnitDropPoint::CallReturn(_), _, _)
            ) {
                preceding = Some(event);
            }
        }
        true
    }

    fn transition(
        &self,
        value: &UnitOwnerValueV1,
        selected: Option<usize>,
        slots: &mut [Slot],
        from: Slot,
        to: Slot,
    ) -> bool {
        let valid_slot = |slot| match slot {
            Slot::Named(id) => id.source_unit() == self.source(),
            Slot::Result(id) => id.source_unit() == self.source(),
            Slot::Pending(call, argument) => {
                call.source_unit() == self.source() && argument.source_unit() == self.source()
            }
            Slot::Dropped => true,
            Slot::Uncreated => false,
        };
        let Some(index) = self.resolve(value, selected) else {
            return false;
        };
        if !valid_slot(from)
            || !valid_slot(to)
            || slots[index] != from
            || (to != Slot::Dropped && from != to && slots.contains(&to))
        {
            return false;
        }
        slots[index] = to;
        true
    }

    /// Two complete alternative traces, not a union of their runtime owners.
    /// Deliberately only String definitions, one choice, no parameters/captures/exits.
    pub(in crate::ownership_checking::compilation_unit) fn validate(
        &self,
    ) -> Option<[Vec<Slot>; 2]> {
        if !self.catalog.is_consistent()
            || self
                .catalog
                .definitions
                .iter()
                .any(|definition| !matches!(definition.origin, UnitOwnerOrigin::String(_)))
        {
            return None;
        }
        let Some(choice) = self.choice.as_ref() else {
            let mut slots = vec![Slot::Uncreated; self.catalog.definitions.len()];
            if !self.suffix.is_empty()
                || !self.replay(&self.prefix, None, &mut slots, None)
                || slots.iter().any(|slot| *slot != Slot::Dropped)
            {
                return None;
            }
            return Some([slots.clone(), slots]);
        };
        if self.catalog.callable != choice.id.callable
            || !std::sync::Arc::ptr_eq(&self.catalog.identity, &choice.id.identity)
            || choice.id.control.source_unit() != self.source()
        {
            return None;
        }
        let UnitOwnerCallable::Function(function) = self.catalog.callable else {
            return None;
        };
        if function.source_unit() != self.source() {
            return None;
        }
        let count = self.catalog.definitions.len();
        // Each static producer belongs to exactly one execution region. In particular,
        // a branch-only owner cannot be manufactured again after the selection.
        let mut regions = vec![None; count];
        for (region, events) in [
            (0, self.prefix.as_slice()),
            (1, choice.events[0].as_slice()),
            (2, choice.events[1].as_slice()),
            (3, self.suffix.as_slice()),
        ] {
            for event in events {
                if let UnitOwnerValueEventV1::Create(_, id) = event
                    && (!self.valid_id(id) || regions[id.index].replace(region).is_some())
                {
                    return None;
                }
            }
        }
        if regions.iter().any(Option::is_none) {
            return None;
        }
        let mut base = vec![Slot::Uncreated; count];
        if !self.replay(&self.prefix, None, &mut base, None) {
            return None;
        }
        let mut arms = [base.clone(), base];
        for (edge, arm) in arms.iter_mut().enumerate() {
            let input = &choice.inputs[edge];
            if input.edge != edge
                || !self.valid_id(&input.owner)
                || input.tail.source_unit() != self.source()
                || !self.replay(&choice.events[edge], None, arm, None)
                || arm[input.owner.index] != Slot::Result(input.tail)
            {
                return None;
            }
            let result = Slot::Result(choice.id.control);
            if arm
                .iter()
                .enumerate()
                .any(|(index, slot)| index != input.owner.index && *slot == result)
            {
                return None;
            }
            arm[input.owner.index] = result;
        }
        // Apart from the selected result and branch-local producers, the outer state
        // must be identical. A different binding token is not a safe symbol-only join.
        for (index, region) in regions.iter().enumerate() {
            if *region == Some(0) && arms[0][index] != arms[1][index] {
                return None;
            }
        }
        for (edge, arm) in arms.iter_mut().enumerate() {
            if !self.replay(&self.suffix, Some(edge), arm, None) {
                return None;
            }
            for (index, slot) in arm.iter().enumerate() {
                let unselected = regions[index] == Some(2 - edge);
                if *slot
                    != if unselected {
                        Slot::Uncreated
                    } else {
                        Slot::Dropped
                    }
                {
                    return None;
                }
            }
        }
        Some(arms)
    }
}

impl UnitOwnerObservation {
    /// Read a sealed narrow Choice capability, or project closed straight-line Strings.
    /// Every real definition must end in named Drop; unconsumed Results are rejected.
    /// Unsupported controls, parameters and captures return no partial projection.
    /// Legacy definitions/events remain Unsupported for Choice-producing AST paths.
    pub fn value_observation_v1(&self) -> Option<UnitOwnerValueObservationV1> {
        if let Some(choice) = &self.choice_value {
            return (choice.catalog.callable == self.callable
                && std::sync::Arc::ptr_eq(&choice.catalog.identity, &self.identity)
                && choice.is_consistent())
            .then(|| (**choice).clone());
        }
        if self.status != UnitOwnerObservationStatus::Complete
            || !self.is_consistent()
            || self
                .definitions
                .iter()
                .any(|definition| !matches!(definition.origin, UnitOwnerOrigin::String(_)))
        {
            return None;
        }
        let mut graph = UnitOwnerValueObservationV1 {
            catalog: UnitOwnerProducerCatalogV1::from_legacy(self)?,
            prefix: Vec::new(),
            choice: None,
            suffix: Vec::new(),
        };
        let mut slots = vec![Slot::Uncreated; self.definitions.len()];
        for event in &self.events {
            let value = |owner: &UnitOwnerId| UnitOwnerValueV1::Definition(owner.clone());
            let projected = match event {
                UnitOwnerTransportEvent::Result { expression, owner } => match slots[owner.index] {
                    Slot::Uncreated => UnitOwnerValueEventV1::Create(*expression, owner.clone()),
                    Slot::Result(from) if from == *expression => continue,
                    Slot::Result(from) => {
                        UnitOwnerValueEventV1::Forward(from, *expression, value(owner))
                    }
                    _ => return None,
                },
                UnitOwnerTransportEvent::Move {
                    expression,
                    symbol,
                    owner,
                } => UnitOwnerValueEventV1::Move(*expression, *symbol, value(owner)),
                UnitOwnerTransportEvent::Binding {
                    symbol,
                    initializer,
                    owner,
                } => UnitOwnerValueEventV1::Bind(*symbol, *initializer, value(owner)),
                UnitOwnerTransportEvent::Pending {
                    call,
                    argument,
                    owner,
                } => UnitOwnerValueEventV1::Pending(*call, *argument, value(owner)),
                UnitOwnerTransportEvent::Replace {
                    call,
                    argument,
                    root,
                    previous,
                    incoming,
                } => UnitOwnerValueEventV1::Replace {
                    call: *call,
                    argument: *argument,
                    root: *root,
                    previous: value(previous),
                    incoming: value(incoming),
                },
                UnitOwnerTransportEvent::Drop {
                    point,
                    symbol,
                    owner,
                } => UnitOwnerValueEventV1::Drop(*point, *symbol, value(owner)),
                UnitOwnerTransportEvent::Swap {
                    call,
                    left,
                    right,
                    left_owner,
                    right_owner,
                } => UnitOwnerValueEventV1::Swap {
                    call: *call,
                    left: *left,
                    right: *right,
                    left_value: value(left_owner),
                    right_value: value(right_owner),
                },
                UnitOwnerTransportEvent::If { .. }
                | UnitOwnerTransportEvent::Capture { .. }
                | UnitOwnerTransportEvent::CaptureDrop { .. }
                | UnitOwnerTransportEvent::SharedCapture { .. }
                | UnitOwnerTransportEvent::SharedCaptureEnd { .. } => return None,
            };
            if !graph.replay(
                std::slice::from_ref(&projected),
                None,
                &mut slots,
                graph.prefix.iter().rev().find(|event| {
                    !matches!(
                        event,
                        UnitOwnerValueEventV1::Drop(UnitDropPoint::CallReturn(_), _, _)
                    )
                }),
            ) {
                return None;
            }
            graph.prefix.push(projected);
        }
        graph.validate()?;
        Some(graph)
    }
}

impl UnitOwnerValueObservationV1 {
    /// Independent real producer catalog. No Choice is inserted here.
    pub fn producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.catalog
    }
    /// Value transitions preceding an optional alternative.
    pub fn prefix(&self) -> &[UnitOwnerValueEventV1] {
        &self.prefix
    }
    /// Alternative selection, present only for the sealed single-binding Choice slice.
    pub fn choice(&self) -> Option<&UnitOwnerChoiceV1> {
        self.choice.as_ref()
    }
    /// Complete continuation replayed separately for each selected value.
    pub fn suffix(&self) -> &[UnitOwnerValueEventV1] {
        &self.suffix
    }
    /// Validate graph conservation, not unrecorded AST syntax or typed admission.
    pub fn is_consistent(&self) -> bool {
        self.validate().is_some()
    }
}

impl UnitOwnerChoiceV1 {
    /// Selection identity scoped to the real producer catalog.
    pub fn id(&self) -> &UnitOwnerChoiceIdV1 {
        &self.id
    }
    /// Ordered true/false traces, including their actual owner cleanup transitions.
    pub fn events(&self) -> &[Vec<UnitOwnerValueEventV1>; 2] {
        &self.events
    }
    /// Ordered true/false real tail producer inputs.
    pub fn inputs(&self) -> &[UnitOwnerChoiceInputV1; 2] {
        &self.inputs
    }
}
impl UnitOwnerChoiceIdV1 {
    /// Callable containing the selection.
    pub fn callable(&self) -> UnitOwnerCallable {
        self.callable
    }
    /// Actual control expression, never a producer identity.
    pub fn control(&self) -> UnitExpressionId {
        self.control
    }
}
impl UnitOwnerChoiceInputV1 {
    /// True is zero and false is one.
    pub fn edge(&self) -> usize {
        self.edge
    }
    /// Actual branch-tail expression whose Result is selected.
    pub fn tail(&self) -> UnitExpressionId {
        self.tail
    }
    /// Actual producer selected on this edge.
    pub fn owner(&self) -> UnitOwnerId {
        self.owner.clone()
    }
}
