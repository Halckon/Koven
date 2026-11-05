//! Private edge/terminator contract sketches, not emitted production CFG facts.
//! The fixture supplies real source identities; the topology below is an independent,
//! manually declared test oracle. Passing it cannot authorize production loop owners.
use super::*;

const NORMAL: &str = r#"while (flag) { val local = "local" }"#;
const INNER: &str = "loop { if (flag) { continue } else { break } }";
const OUTER: &str = r#"while (flag) {
loop { if (flag) { continue } else { break } }
return "done"
}"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoopKind {
    While,
    Infinite,
}

#[derive(Clone, Debug)]
struct Loop {
    id: UnitStatementId,
    parent: Option<UnitStatementId>,
    kind: LoopKind,
    // Abstract cleanup frontier, not a claim about production lexical depth values.
    depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Entry(UnitStatementId),
    UnconditionalBody(UnitStatementId),
    ConditionTrue(UnitStatementId),
    ConditionFalse(UnitStatementId),
    Fallthrough(UnitStatementId),
    Continue(UnitExpressionId),
    Break(UnitExpressionId),
    Return(UnitExpressionId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Destination {
    Header(UnitStatementId),
    Body(UnitStatementId),
    AfterLoop(UnitStatementId),
    FunctionExit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cleanup {
    id: usize,
    depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    PayloadCompleted(UnitExpressionId),
    Deliver(UnitExpressionId),
    Cleanup(Cleanup),
    Transfer(Destination),
}

#[derive(Clone, Debug)]
struct Edge {
    // Product-local witness index; role alone is insufficient when paths share a point.
    index: usize,
    identity: std::sync::Arc<()>,
    callable: UnitOwnerCallable,
    role: Role,
    stack: Vec<UnitStatementId>,
    actions: Vec<Action>,
}

#[derive(Clone)]
struct Witness {
    role: Role,
    stack: Vec<UnitStatementId>,
    // Actual producer must supply this ordered pending cleanup snapshot. Here it is
    // deliberately independent model input; numeric depth is not an owner identity.
    obligations: Vec<Cleanup>,
}

#[derive(Clone)]
struct Contract {
    identity: std::sync::Arc<()>,
    source: crate::name_resolution::SourceUnitId,
    callable: UnitOwnerCallable,
    loops: Vec<Loop>,
    return_sites: Vec<(UnitExpressionId, Option<UnitExpressionId>)>,
    witnesses: Vec<Witness>,
}

impl Contract {
    fn loop_info(&self, id: UnitStatementId) -> Option<&Loop> {
        self.loops.iter().find(|item| item.id == id)
    }

    fn expected(&self, witness: &Witness) -> Option<Vec<Action>> {
        if witness
            .obligations
            .iter()
            .enumerate()
            .any(|(index, cleanup)| {
                cleanup.depth == 0
                    || witness.obligations[..index]
                        .iter()
                        .any(|prior| prior.id == cleanup.id)
            })
        {
            return None;
        }
        for (index, &id) in witness.stack.iter().enumerate() {
            let info = self.loop_info(id)?;
            if id.source_unit() != self.source
                || info.parent != index.checked_sub(1).map(|parent| witness.stack[parent])
            {
                return None;
            }
        }
        let nearest = witness.stack.last().copied();
        let (destination, frontier, payload) = match witness.role {
            Role::Entry(id) => {
                let info = self.loop_info(id)?;
                if id.source_unit() != self.source || info.parent != nearest {
                    return None;
                }
                (Destination::Header(id), None, None)
            }
            Role::UnconditionalBody(id) => {
                if nearest != Some(id) || self.loop_info(id)?.kind != LoopKind::Infinite {
                    return None;
                }
                (Destination::Body(id), None, None)
            }
            Role::ConditionTrue(id) | Role::ConditionFalse(id) => {
                let info = self.loop_info(id)?;
                if nearest != Some(id) || info.kind != LoopKind::While {
                    return None;
                }
                let destination = if matches!(witness.role, Role::ConditionTrue(_)) {
                    Destination::Body(id)
                } else {
                    Destination::AfterLoop(id)
                };
                (destination, None, None)
            }
            Role::Fallthrough(id) => {
                if nearest != Some(id) {
                    return None;
                }
                (
                    Destination::Header(id),
                    Some(self.loop_info(id)?.depth),
                    None,
                )
            }
            Role::Continue(expression) | Role::Break(expression) => {
                if expression.source_unit() != self.source {
                    return None;
                }
                let id = nearest?;
                let destination = if matches!(witness.role, Role::Continue(_)) {
                    Destination::Header(id)
                } else {
                    Destination::AfterLoop(id)
                };
                (destination, Some(self.loop_info(id)?.depth), None)
            }
            Role::Return(expression) => {
                if expression.source_unit() != self.source {
                    return None;
                }
                (
                    Destination::FunctionExit,
                    Some(0),
                    self.return_sites
                        .iter()
                        .find(|(site, _)| *site == expression)?
                        .1,
                )
            }
        };
        let mut actions = Vec::new();
        if let Some(expression) = payload {
            if expression.source_unit() != self.source {
                return None;
            }
            actions.push(Action::PayloadCompleted(expression));
            actions.push(Action::Deliver(expression));
        }
        if let Some(frontier) = frontier {
            // Snapshot is in completion/declaration order. Unwind reverses it and
            // preserves outer-loop obligations at break/continue/fallthrough.
            actions.extend(witness.obligations.iter().rev().filter_map(|cleanup| {
                (cleanup.depth > frontier).then_some(Action::Cleanup(*cleanup))
            }));
        }
        actions.push(Action::Transfer(destination));
        Some(actions)
    }

    fn edges(&self) -> Vec<Edge> {
        self.witnesses
            .iter()
            .enumerate()
            .map(|(index, witness)| Edge {
                index,
                identity: self.identity.clone(),
                callable: self.callable,
                role: witness.role,
                stack: witness.stack.clone(),
                actions: self
                    .expected(witness)
                    .expect("valid manually declared topology"),
            })
            .collect()
    }

    fn validates(&self, edges: &[Edge]) -> bool {
        if self.loops.iter().enumerate().any(|(index, item)| {
            item.id.source_unit() != self.source
                || self.loops[..index].iter().any(|prior| prior.id == item.id)
                || item.parent.is_some_and(|parent| {
                    self.loop_info(parent)
                        .is_none_or(|info| info.depth >= item.depth)
                })
        }) {
            return false;
        }
        if edges.len() != self.witnesses.len() {
            return false;
        }
        let mut seen = vec![false; self.witnesses.len()];
        for edge in edges {
            let Some(witness) = self.witnesses.get(edge.index) else {
                return false;
            };
            if seen[edge.index]
                || !std::sync::Arc::ptr_eq(&edge.identity, &self.identity)
                || edge.callable != self.callable
                || edge.role != witness.role
                || edge.stack != witness.stack
                || self.expected(witness).as_ref() != Some(&edge.actions)
            {
                return false;
            }
            seen[edge.index] = true;
        }
        seen.into_iter().all(|value| value)
    }
}

fn return_site(fixture: &Fixture, text: &str) -> (UnitExpressionId, Option<UnitExpressionId>) {
    let expression = fixture.expression(0, text);
    let crate::parser::Expression::Return { value, .. } = fixture.parsed[0]
        .ast()
        .expressions()
        .get(expression)
        .unwrap()
        .payload()
    else {
        panic!("a real return expression supplies its payload identity");
    };
    (
        UnitExpressionId::new(fixture.source(0), expression),
        value.map(|value| UnitExpressionId::new(fixture.source(0), value)),
    )
}

fn fixture() -> (Contract, UnitStatementId, UnitStatementId, UnitStatementId) {
    let text = format!("fun run(flag: Boolean): String {{\n{NORMAL}\n{OUTER}\nreturn \"zero\"\n}}");
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    assert!(matches!(
        owned.owner_observations()[0].status(),
        UnitOwnerObservationStatus::Unsupported { .. }
    ));
    assert!(owned.owner_observations()[0].events().is_empty());
    let statement = |text| {
        let matches = fixture.parsed[0]
            .ast()
            .statements()
            .iter()
            .filter_map(|(id, node)| {
                (fixture.sources.slice(node.span()).unwrap() == text).then_some(id)
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1);
        UnitStatementId::new(fixture.source(0), matches[0])
    };
    let expression = |text| UnitExpressionId::new(fixture.source(0), fixture.expression(0, text));
    let normal = statement(NORMAL);
    let outer = statement(OUTER);
    let inner = statement(INNER);
    assert_eq!(
        owned.owner_observations()[0].status(),
        UnitOwnerObservationStatus::Unsupported {
            point: UnitDropPoint::AfterStatement(normal),
            reason: UnitOwnerUnsupportedReason::Syntax,
        }
    );
    // Only this first loop is the production rejection witness. Later constructs
    // supply source identities to the model, not independent admission evidence.
    let obligations = vec![
        Cleanup { id: 0, depth: 1 },
        Cleanup { id: 1, depth: 2 },
        Cleanup { id: 2, depth: 3 },
    ];
    let witness = |role, stack| Witness {
        role,
        stack,
        obligations: obligations.clone(),
    };
    let contract = Contract {
        identity: std::sync::Arc::new(()),
        source: fixture.source(0),
        callable: owned.owner_observations()[0].callable(),
        return_sites: vec![
            return_site(&fixture, "return \"done\""),
            return_site(&fixture, "return \"zero\""),
        ],
        loops: vec![
            Loop {
                id: normal,
                parent: None,
                kind: LoopKind::While,
                depth: 1,
            },
            Loop {
                id: outer,
                parent: None,
                kind: LoopKind::While,
                depth: 1,
            },
            Loop {
                id: inner,
                parent: Some(outer),
                kind: LoopKind::Infinite,
                depth: 2,
            },
        ],
        witnesses: vec![
            witness(Role::Entry(normal), vec![]),
            witness(Role::ConditionTrue(normal), vec![normal]),
            witness(Role::ConditionFalse(normal), vec![normal]),
            witness(Role::Fallthrough(normal), vec![normal]),
            witness(Role::Entry(outer), vec![]),
            witness(Role::ConditionTrue(outer), vec![outer]),
            witness(Role::ConditionFalse(outer), vec![outer]),
            witness(Role::Entry(inner), vec![outer]),
            witness(Role::Continue(expression("continue")), vec![outer, inner]),
            witness(Role::Break(expression("break")), vec![outer, inner]),
            witness(Role::Return(expression("return \"done\"")), vec![outer]),
            witness(Role::Return(expression("return \"zero\"")), vec![]),
            witness(Role::UnconditionalBody(inner), vec![outer, inner]),
        ],
    };
    (contract, normal, outer, inner)
}

#[test]
fn real_source_identities_do_not_change_production_loop_rejection() {
    let (contract, _, _, _) = fixture();
    assert!(contract.validates(&contract.edges()));
}

#[test]
fn zero_iteration_has_condition_false_edge_but_no_body_or_backedge() {
    let (contract, normal, _, inner) = fixture();
    let edges = contract.edges();
    assert_eq!(
        edges[0].actions,
        [Action::Transfer(Destination::Header(normal))]
    );
    assert_eq!(
        edges[2].actions,
        [Action::Transfer(Destination::AfterLoop(normal))]
    );
    // This is a possible two-edge path, not a claim that flag is false at runtime.
    let mut invalid = contract.clone();
    invalid.witnesses.push(Witness {
        role: Role::ConditionFalse(inner),
        stack: vec![contract.loops[1].id, inner],
        obligations: vec![],
    });
    assert!(
        invalid
            .expected(invalid.witnesses.last().unwrap())
            .is_none()
    );
}

#[test]
fn nested_continue_and_break_keep_outer_obligations_and_target_nearest_loop() {
    let (contract, _, outer, inner) = fixture();
    let edges = contract.edges();
    for (index, destination) in [
        (8, Destination::Header(inner)),
        (9, Destination::AfterLoop(inner)),
    ] {
        assert_eq!(
            edges[index].actions,
            [
                Action::Cleanup(Cleanup { id: 2, depth: 3 }),
                Action::Transfer(destination)
            ]
        );
        let mut wrong = edges.clone();
        *wrong[index].actions.last_mut().unwrap() = Action::Transfer(Destination::Header(outer));
        assert!(!contract.validates(&wrong));
    }
}

#[test]
fn return_delivers_before_reverse_cleanup_and_never_reaches_header_or_loop_exit() {
    let (contract, _, outer, _) = fixture();
    let edges = contract.edges();
    assert!(matches!(
        edges[10].actions.first(),
        Some(Action::PayloadCompleted(_))
    ));
    assert_eq!(
        &edges[10].actions[2..],
        [
            Action::Cleanup(Cleanup { id: 2, depth: 3 }),
            Action::Cleanup(Cleanup { id: 1, depth: 2 }),
            Action::Cleanup(Cleanup { id: 0, depth: 1 }),
            Action::Transfer(Destination::FunctionExit)
        ]
    );
    let mut late = edges.clone();
    late[10].actions.swap(1, 2);
    assert!(!contract.validates(&late));
    let mut missing = edges.clone();
    missing[10].actions.remove(1);
    assert!(!contract.validates(&missing));
    let mut incomplete = edges.clone();
    incomplete[10].actions.remove(0);
    assert!(!contract.validates(&incomplete));
    let (return_node, payload) = contract.return_sites[0];
    assert_ne!(Some(return_node), payload);
    assert_eq!(edges[10].actions[1], Action::Deliver(payload.unwrap()));
    let mut mistaken_node = edges.clone();
    mistaken_node[10].actions[1] = Action::Deliver(return_node);
    assert!(!contract.validates(&mistaken_node));
    let mut wrong_completion = edges.clone();
    wrong_completion[10].actions[0] = Action::PayloadCompleted(return_node);
    assert!(!contract.validates(&wrong_completion));
    let mut delivery_first = edges.clone();
    delivery_first[10].actions.swap(0, 1);
    assert!(!contract.validates(&delivery_first));
    let mut other_payload = edges.clone();
    other_payload[10].actions[1] = Action::Deliver(contract.return_sites[1].1.unwrap());
    assert!(!contract.validates(&other_payload));
    let mut wrong = edges;
    *wrong[10].actions.last_mut().unwrap() = Action::Transfer(Destination::Header(outer));
    assert!(!contract.validates(&wrong));
}

#[test]
fn each_required_edge_is_unique_product_local_and_has_one_final_transfer() {
    let (contract, _, _, _) = fixture();
    let edges = contract.edges();
    let mut missing = edges.clone();
    missing.remove(2);
    assert!(!contract.validates(&missing));
    let mut duplicate = edges.clone();
    duplicate[2] = duplicate[1].clone();
    assert!(!contract.validates(&duplicate));
    let mut foreign = edges.clone();
    foreign[0].identity = std::sync::Arc::new(());
    assert!(!contract.validates(&foreign));
    let mut post_transfer = edges.clone();
    post_transfer[0]
        .actions
        .push(Action::Cleanup(Cleanup { id: 4, depth: 3 }));
    assert!(!contract.validates(&post_transfer));
    let mut repeated_transfer = edges.clone();
    repeated_transfer[0]
        .actions
        .extend(edges[0].actions.clone());
    assert!(!contract.validates(&repeated_transfer));
    let mut rebound = edges;
    rebound[8].stack.pop();
    assert!(!contract.validates(&rebound));
}

#[test]
fn fallthrough_is_a_cleanup_then_backedge_not_an_exhaustion_exit() {
    let (contract, normal, _, inner) = fixture();
    let edges = contract.edges();
    assert_eq!(
        edges[3].actions,
        [
            Action::Cleanup(Cleanup { id: 2, depth: 3 }),
            Action::Cleanup(Cleanup { id: 1, depth: 2 }),
            Action::Transfer(Destination::Header(normal)),
        ]
    );
    assert_eq!(
        edges[12].actions,
        [Action::Transfer(Destination::Body(inner))]
    );
    let mut wrong = edges.clone();
    *wrong[3].actions.last_mut().unwrap() = Action::Transfer(Destination::AfterLoop(normal));
    assert!(!contract.validates(&wrong));
    let mut early = edges;
    early[3].actions.swap(0, 2);
    assert!(!contract.validates(&early));
}

#[test]
fn payload_free_return_needs_no_delivery_and_is_callable_local() {
    let fixture = Fixture::new(&[
        ("root.ko", "fun empty(): Unit { return }"),
        ("other.ko", "fun other(): Unit {}"),
    ]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let expression = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "return"));
    let contract = Contract {
        identity: std::sync::Arc::new(()),
        source: fixture.source(0),
        callable: owned.owner_observations().iter().find(|observation| {
            matches!(observation.callable(), UnitOwnerCallable::Function(item) if item.source_unit() == fixture.source(0))
        }).unwrap().callable(),
        return_sites: vec![return_site(&fixture, "return")],
        loops: vec![],
        witnesses: vec![Witness {
            role: Role::Return(expression),
            stack: vec![],
            obligations: vec![],
        }],
    };
    let edges = contract.edges();
    assert_eq!(
        edges[0].actions,
        [Action::Transfer(Destination::FunctionExit)]
    );
    assert!(contract.validates(&edges));
    let mut fabricated = edges.clone();
    fabricated[0].actions.insert(0, Action::Deliver(expression));
    assert!(!contract.validates(&fabricated));
    let mut foreign = edges;
    foreign[0].callable = UnitOwnerCallable::Lambda(expression);
    assert!(!contract.validates(&foreign));
    assert_ne!(fixture.source(0), fixture.source(1));
    let mut foreign_source = contract.clone();
    let foreign_return = UnitExpressionId::new(fixture.source(1), expression.expression());
    foreign_source.witnesses[0].role = Role::Return(foreign_return);
    foreign_source.return_sites[0].0 = foreign_return;
    // Preserve lookup consistency so rejection needs the source boundary itself.
    assert!(
        foreign_source
            .expected(&foreign_source.witnesses[0])
            .is_none()
    );
}

#[test]
fn duplicate_cleanup_obligations_and_invalid_loop_parent_are_not_valid_inputs() {
    let (contract, _, outer, _) = fixture();
    let edges = contract.edges();
    let mut duplicate = contract.clone();
    let repeated = duplicate.witnesses[8].obligations[0];
    duplicate.witnesses[8].obligations.push(repeated);
    assert!(!duplicate.validates(&edges));
    let mut cyclic = contract;
    cyclic.loops[1].parent = Some(outer);
    assert!(!cyclic.validates(&edges));
}
