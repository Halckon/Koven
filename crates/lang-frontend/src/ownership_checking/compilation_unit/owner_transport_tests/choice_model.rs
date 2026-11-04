//! Test-only value-choice model. This is NOT a production admission or a public protocol.
//! Definitions retain their real producer identity; a selection never adds a definition.
mod cases;

use super::*;

pub(in crate::ownership_checking::compilation_unit) const CONSUMERS: &[&str] = &[
    "var a = \"old\"\nval old = replace(&a, if (flag) \"new\" else \"other\")",
    "var a = if (flag) \"old\" else \"other\"\nval old = replace(&a, \"new\")",
];

pub(in crate::ownership_checking::compilation_unit) fn consumer_source(body: &str) -> String {
    format!(
        "fun run(flag: Boolean): Unit {{\n{body}\n}}\nfun sibling(): Unit {{ val x = \"safe\" }}"
    )
}

#[derive(Clone, Debug)]
struct ChoiceId {
    identity: std::sync::Arc<()>,
    callable: UnitOwnerCallable,
    control: UnitExpressionId,
}

impl ChoiceId {
    fn same(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.identity, &other.identity)
            && self.callable == other.callable
            && self.control == other.control
    }
}

#[derive(Clone, Debug)]
enum Value {
    Definition(UnitOwnerId),
    Choice(ChoiceId),
}

#[derive(Clone, Debug)]
enum Event {
    Create(UnitExpressionId, UnitOwnerId),
    Forward(UnitExpressionId, UnitExpressionId, Value),
    Move(UnitExpressionId, UnitSymbolId, Value),
    Bind(UnitSymbolId, UnitExpressionId, Value),
    Pending(UnitExpressionId, UnitExpressionId, Value),
    Replace {
        call: UnitExpressionId,
        argument: UnitExpressionId,
        root: UnitSymbolId,
        previous: Value,
        incoming: Value,
    },
    Drop(UnitSymbolId, Value),
}

#[derive(Clone, Debug)]
struct Input {
    edge: usize,
    tail: UnitExpressionId,
    owner: UnitOwnerId,
}

#[derive(Clone, Debug)]
struct Choice {
    id: ChoiceId,
    events: [Vec<Event>; 2],
    inputs: [Input; 2],
}

#[derive(Clone, Debug)]
struct Model {
    catalog: UnitOwnerObservation,
    prefix: Vec<Event>,
    choice: Choice,
    suffix: Vec<Event>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Uncreated,
    Result(UnitExpressionId),
    Named(UnitSymbolId),
    Pending(UnitExpressionId, UnitExpressionId),
    Dropped,
}

impl Model {
    fn valid_id(&self, id: &UnitOwnerId) -> bool {
        self.catalog.definition(id.clone()).is_some()
    }

    fn source(&self) -> crate::name_resolution::SourceUnitId {
        self.choice.id.control.source_unit()
    }

    fn resolve(&self, value: &Value, selected: Option<usize>) -> Option<usize> {
        match value {
            Value::Definition(id) if self.valid_id(id) => Some(id.index),
            Value::Choice(id) if id.same(&self.choice.id) => {
                let input = &self.choice.inputs[selected?];
                self.valid_id(&input.owner).then_some(input.owner.index)
            }
            _ => None,
        }
    }

    fn replay(&self, events: &[Event], selected: Option<usize>, slots: &mut [Slot]) -> bool {
        for event in events {
            let valid = match event {
                Event::Create(expression, owner) => {
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
                Event::Forward(from, to, value) => self.transition(
                    value,
                    selected,
                    slots,
                    Slot::Result(*from),
                    Slot::Result(*to),
                ),
                Event::Move(expression, symbol, value) => self.transition(
                    value,
                    selected,
                    slots,
                    Slot::Named(*symbol),
                    Slot::Result(*expression),
                ),
                Event::Bind(symbol, expression, value) => self.transition(
                    value,
                    selected,
                    slots,
                    Slot::Result(*expression),
                    Slot::Named(*symbol),
                ),
                Event::Pending(call, argument, value) => self.transition(
                    value,
                    selected,
                    slots,
                    Slot::Result(*argument),
                    Slot::Pending(*call, *argument),
                ),
                Event::Drop(symbol, value) => {
                    self.transition(value, selected, slots, Slot::Named(*symbol), Slot::Dropped)
                }
                Event::Replace {
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
        }
        true
    }

    fn transition(
        &self,
        value: &Value,
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
    fn validate(&self) -> Option<[Vec<Slot>; 2]> {
        if self.catalog.status != UnitOwnerObservationStatus::Complete
            || !self.catalog.is_consistent()
            || self.catalog.callable != self.choice.id.callable
            || !std::sync::Arc::ptr_eq(&self.catalog.identity, &self.choice.id.identity)
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
            (1, self.choice.events[0].as_slice()),
            (2, self.choice.events[1].as_slice()),
            (3, self.suffix.as_slice()),
        ] {
            for event in events {
                if let Event::Create(_, id) = event
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
        if !self.replay(&self.prefix, None, &mut base) {
            return None;
        }
        let mut arms = [base.clone(), base];
        for (edge, arm) in arms.iter_mut().enumerate() {
            let input = &self.choice.inputs[edge];
            if input.edge != edge
                || !self.valid_id(&input.owner)
                || input.tail.source_unit() != self.source()
                || !self.replay(&self.choice.events[edge], None, arm)
                || arm[input.owner.index] != Slot::Result(input.tail)
            {
                return None;
            }
            let result = Slot::Result(self.choice.id.control);
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
            if !self.replay(&self.suffix, Some(edge), arm) {
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
