//! Test-only closed-body template, not a loop producer, phi, or termination proof.
//! Real straight-line producer events supply every transition. No owner may cross the
//! template boundary. Repetition gives each static definition a fresh dynamic instance.
//! This checks transport only: absence of a published return event cannot prove a
//! reachable backedge. A future producer must separately establish continuation.
use super::*;

const BODY: &str = r#"fun body(): Unit {
var root = "old"
val incoming = "next"
val previous = replace(&root, incoming)
val forwarded = (previous)
}"#;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Instance {
    round: usize,
    definition: UnitOwnerId,
}

#[derive(Debug)]
struct Replacement {
    round: usize,
    previous: Instance,
    incoming: Instance,
}

#[derive(Default, Debug)]
struct Trace {
    created: Vec<Instance>,
    dropped: Vec<Instance>,
    replacements: Vec<Replacement>,
}

impl Trace {
    fn conserved(&self) -> bool {
        self.created.len() == self.dropped.len()
            && self.created.iter().enumerate().all(|(index, instance)| {
                !self.created[..index].contains(instance)
                    && self.dropped.iter().filter(|drop| *drop == instance).count() == 1
            })
            && self.replacements.iter().all(|replacement| {
                replacement.previous != replacement.incoming
                    && replacement.previous.round == replacement.round
                    && replacement.incoming.round == replacement.round
                    && self.created.contains(&replacement.previous)
                    && self.created.contains(&replacement.incoming)
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Uncreated,
    Result(UnitExpressionId),
    Named(UnitSymbolId),
    Pending(UnitExpressionId, UnitExpressionId),
    Dropped,
}

#[derive(Clone)]
struct ClosedBody {
    catalog: UnitOwnerObservation,
}

impl ClosedBody {
    fn from_source(text: &str) -> Self {
        let owned = checked(text);
        Self {
            catalog: owned.owner_observations()[0].clone(),
        }
    }

    // This is a static template check, including for a requested zero-length prefix.
    // Its temporary trace is discarded: it does not claim a dynamic iteration occurred.
    fn validate(&self) -> bool {
        self.catalog.status() == UnitOwnerObservationStatus::Complete
            && self.catalog.is_consistent()
            && !self.catalog.definitions().is_empty()
            && self
                .catalog
                .definitions()
                .iter()
                .all(|definition| matches!(definition.origin(), UnitOwnerOrigin::String(_)))
            && self.replay_round(0, &mut Trace::default())
    }

    fn prefix(&self, rounds: usize) -> Option<Trace> {
        if !self.validate() {
            return None;
        }
        let mut trace = Trace::default();
        for round in 0..rounds {
            if !self.replay_round(round, &mut trace) {
                return None;
            }
        }
        trace.conserved().then_some(trace)
    }

    fn replay_round(&self, round: usize, trace: &mut Trace) -> bool {
        let mut slots = vec![Slot::Uncreated; self.catalog.definitions().len()];
        let instance = |owner: &UnitOwnerId| Instance {
            round,
            definition: owner.clone(),
        };
        for event in self.catalog.events() {
            let valid = match event {
                UnitOwnerTransportEvent::Result { expression, owner } => {
                    if self.catalog.definition(owner.clone()).is_none()
                        || slots.iter().enumerate().any(|(index, slot)| {
                            index != owner.index && *slot == Slot::Result(*expression)
                        })
                    {
                        return false;
                    }
                    match slots[owner.index] {
                        Slot::Uncreated
                            if self.catalog.definitions()[owner.index].origin()
                                == UnitOwnerOrigin::String(*expression) =>
                        {
                            trace.created.push(instance(owner));
                        }
                        Slot::Result(_) => {}
                        _ => return false,
                    }
                    slots[owner.index] = Slot::Result(*expression);
                    true
                }
                UnitOwnerTransportEvent::Binding {
                    symbol,
                    initializer,
                    owner,
                } => self.transfer(
                    &mut slots,
                    owner,
                    Slot::Result(*initializer),
                    Slot::Named(*symbol),
                ),
                UnitOwnerTransportEvent::Move {
                    expression,
                    symbol,
                    owner,
                } => self.transfer(
                    &mut slots,
                    owner,
                    Slot::Named(*symbol),
                    Slot::Result(*expression),
                ),
                UnitOwnerTransportEvent::Pending {
                    call,
                    argument,
                    owner,
                } => self.transfer(
                    &mut slots,
                    owner,
                    Slot::Result(*argument),
                    Slot::Pending(*call, *argument),
                ),
                UnitOwnerTransportEvent::Replace {
                    call,
                    root,
                    argument,
                    previous,
                    incoming,
                } => {
                    if self.catalog.definition(previous.clone()).is_none()
                        || self.catalog.definition(incoming.clone()).is_none()
                        || previous == incoming
                        || slots[previous.index] != Slot::Named(*root)
                        || slots[incoming.index] != Slot::Pending(*call, *argument)
                        || slots.contains(&Slot::Result(*call))
                    {
                        return false;
                    }
                    // Read both incoming locations before committing either destination.
                    slots[previous.index] = Slot::Result(*call);
                    slots[incoming.index] = Slot::Named(*root);
                    trace.replacements.push(Replacement {
                        round,
                        previous: instance(previous),
                        incoming: instance(incoming),
                    });
                    true
                }
                UnitOwnerTransportEvent::Drop { symbol, owner, .. } => {
                    if self.catalog.definition(owner.clone()).is_none()
                        || slots[owner.index] != Slot::Named(*symbol)
                    {
                        return false;
                    }
                    slots[owner.index] = Slot::Dropped;
                    trace.dropped.push(instance(owner));
                    true
                }
                // Branch, capture, return delivery, providers and carried state need
                // independent contracts. Do not inherit their admission from the catalog.
                _ => false,
            };
            if !valid {
                return false;
            }
        }
        // Result is not an implicit return or temporary destructor. The backedge is
        // allowed only after an actual Drop for every instance created in this body.
        slots.iter().all(|slot| *slot == Slot::Dropped)
    }

    fn transfer(&self, slots: &mut [Slot], owner: &UnitOwnerId, from: Slot, to: Slot) -> bool {
        if self.catalog.definition(owner.clone()).is_none()
            || slots[owner.index] != from
            || slots.contains(&to)
        {
            return false;
        }
        slots[owner.index] = to;
        true
    }
}

#[test]
fn closed_body_prefixes_create_fresh_instances_and_release_before_each_backedge() {
    let fixture = Fixture::new(&[("root.ko", BODY)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let body = ClosedBody {
        catalog: owned.owner_observations()[0].clone(),
    };
    let old = literal_owner(&fixture, 0, &body.catalog, "\"old\"");
    let next = literal_owner(&fixture, 0, &body.catalog, "\"next\"");
    let definitions = body.catalog.definitions().len();
    assert_eq!(definitions, 2);
    for rounds in [0, 1, 2, 7] {
        let trace = body.prefix(rounds).expect("closed local-owner template");
        assert_eq!(trace.created.len(), rounds * definitions);
        assert_eq!(trace.dropped.len(), rounds * definitions);
        assert_eq!(trace.replacements.len(), rounds);
        for round in 0..rounds {
            let created = trace
                .created
                .iter()
                .filter(|instance| instance.round == round)
                .collect::<Vec<_>>();
            let dropped = trace
                .dropped
                .iter()
                .filter(|instance| instance.round == round)
                .collect::<Vec<_>>();
            assert_eq!(created.len(), definitions);
            assert_eq!(dropped.len(), definitions);
            assert!(created.iter().all(|instance| dropped.contains(instance)));
            assert_eq!(trace.replacements[round].previous.round, round);
            assert_eq!(trace.replacements[round].incoming.round, round);
            assert_eq!(trace.replacements[round].previous.definition, old);
            assert_eq!(trace.replacements[round].incoming.definition, next);
        }
    }
}

#[test]
fn stale_previous_round_instance_cannot_satisfy_current_drop_or_replace() {
    let body = ClosedBody::from_source(BODY);
    let mut trace = body.prefix(2).unwrap();
    let previous = trace
        .dropped
        .iter()
        .find(|instance| instance.round == 0)
        .unwrap()
        .clone();
    let current = trace
        .dropped
        .iter_mut()
        .find(|instance| instance.round == 1)
        .unwrap();
    *current = previous;
    assert!(!trace.conserved());
    let mut trace = body.prefix(2).unwrap();
    trace.replacements[1].previous = trace.replacements[0].previous.clone();
    assert!(!trace.conserved());
    trace.replacements[1].incoming = trace.replacements[0].incoming.clone();
    assert!(
        !trace.conserved(),
        "two stale operands do not become a current commit"
    );
    let mut trace = body.prefix(2).unwrap();
    trace.replacements[1].incoming = trace.replacements[0].incoming.clone();
    assert!(
        !trace.conserved(),
        "a stale incoming is rejected independently of a valid previous operand"
    );
}

#[test]
fn missing_drop_or_duplicate_creation_rejects_even_a_zero_length_prefix() {
    let body = ClosedBody::from_source(BODY);
    let mut missing = body.clone();
    let index = missing
        .catalog
        .events
        .iter()
        .position(|event| matches!(event, UnitOwnerTransportEvent::Drop { .. }))
        .unwrap();
    missing.catalog.events.remove(index);
    assert!(missing.prefix(0).is_none());
    assert!(missing.prefix(2).is_none());
    let mut duplicate = body.clone();
    let repeated = duplicate.catalog.events.clone();
    duplicate.catalog.events.extend(repeated);
    assert!(duplicate.prefix(0).is_none());
    assert!(duplicate.prefix(2).is_none());
}

#[test]
fn pending_and_replace_must_belong_to_this_body_execution() {
    let body = ClosedBody::from_source(BODY);
    let mut missing = body.clone();
    missing
        .catalog
        .events
        .retain(|event| !matches!(event, UnitOwnerTransportEvent::Pending { .. }));
    assert!(missing.prefix(2).is_none());
    let mut swapped = body;
    for event in &mut swapped.catalog.events {
        if let UnitOwnerTransportEvent::Replace {
            previous, incoming, ..
        } = event
        {
            std::mem::swap(previous, incoming);
        }
    }
    assert!(swapped.prefix(2).is_none());
}

#[test]
fn foreign_analysis_token_and_unreleased_result_are_not_iteration_locals() {
    let mut body = ClosedBody::from_source(BODY);
    let foreign = ClosedBody::from_source(BODY).catalog.definitions()[0].id();
    for event in &mut body.catalog.events {
        if let UnitOwnerTransportEvent::Result { owner, .. } = event {
            *owner = foreign.clone();
            break;
        }
    }
    assert!(body.prefix(1).is_none());
    // Production straight-line consistency permits a result without claiming its drop.
    let result = ClosedBody::from_source("fun body(): Unit { \"temporary\" }");
    assert_eq!(
        result.catalog.status(),
        UnitOwnerObservationStatus::Complete
    );
    assert!(result.catalog.is_consistent());
    assert!(result.prefix(1).is_none());
}

#[test]
fn parameter_capture_and_branch_catalogs_do_not_grant_closed_loop_admission() {
    for source in [
        "fun body(own input: String): Unit { val local = input }",
        "fun body(): Unit { val input = \"source\"; val closure = move { input } }",
        "fun body(flag: Boolean): Unit { if (flag) { val left = \"left\" } else { val right = \"right\" } }",
    ] {
        let body = ClosedBody::from_source(source);
        assert_eq!(body.catalog.status(), UnitOwnerObservationStatus::Complete);
        assert!(body.catalog.is_consistent());
        assert!(body.prefix(0).is_none());
        assert!(body.prefix(2).is_none());
    }
}

#[test]
fn closed_transport_alone_does_not_prove_a_reachable_backedge() {
    // Return has no explicit delivery/terminator event in the current observation.
    // A closed transport sequence may therefore validate despite the real exit.
    // This model must never become a production loop-admission predicate.
    let body =
        ClosedBody::from_source("fun body(): Unit { val local = \"before return\"; return }");
    assert_eq!(body.catalog.status(), UnitOwnerObservationStatus::Complete);
    assert!(body.prefix(2).is_some());
    // The actual while-with-return source remains in LOOPS and must be Unsupported.
    assert!(
        super::loop_boundaries::LOOPS
            .iter()
            .any(|source| source.contains("return body"))
    );
}
