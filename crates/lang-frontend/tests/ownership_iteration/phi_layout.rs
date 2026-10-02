use super::*;

#[test]
fn loop_phi_layout_uses_distinct_statement_sources_before_body_choices() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupOwnerValue, CleanupSelectorSource, IterationPhiBoundary,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flags: List<Boolean>) {
            var f: () -> Unit = {}
            for (flag in flags) { f = if (flag) ({ read(xs) }) else ({ read(ys) }) }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plan = &owned.iterations()[0];
    let statement = plan.descriptor().statement();
    let table = owned.cleanup_conditions();
    let body_choice = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            sources
                .slice(node.span())
                .unwrap()
                .starts_with("if (flag)")
                .then_some(id)
        })
        .unwrap();
    let body_selector = table
        .selectors()
        .iter()
        .position(|selector| selector.control() == Some(body_choice))
        .unwrap();
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let phis = plan
        .closure_phis()
        .iter()
        .filter(|phi| phi.symbol() == f)
        .collect::<Vec<_>>();
    assert_eq!(phis.len(), 2, "header and exit each own a separate f value");
    assert_eq!(phis[0].boundary(), IterationPhiBoundary::Header);
    assert_eq!(phis[1].boundary(), IterationPhiBoundary::Exit);
    assert_eq!(phis[0].symbol(), phis[1].symbol());
    assert_ne!(phis[0].owner(), phis[1].owner());
    for phi in phis {
        assert!(
            matches!(table.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi {
            statement: owner_loop, boundary, symbol, ..
        }) if *owner_loop == statement && *boundary == phi.boundary() && *symbol == phi.symbol())
        );
        let expected = match phi.boundary() {
            IterationPhiBoundary::Header => plan.closure_flow().header(),
            IterationPhiBoundary::Exit => plan.closure_flow().exit(),
        }
        .iter()
        .find(|row| row.symbol() == phi.symbol())
        .unwrap()
        .origins();
        assert_eq!(
            phi.origins()
                .iter()
                .map(|origin| origin.closure())
                .collect::<Vec<_>>(),
            expected
        );
        for origin in phi.origins() {
            let selector = table.selector(origin.selector()).unwrap();
            assert!(
                matches!(selector.source(), CleanupSelectorSource::IterationPhi {
                statement: owner_loop, boundary, ..
            } if owner_loop == statement && boundary == phi.boundary())
            );
            assert_eq!(
                selector.control(),
                None,
                "phi may not masquerade as a source expression"
            );
            assert!(
                origin.selector().index() < body_selector,
                "header and exit fields must exist before planning the body"
            );
            let Some(CleanupCondition::Choice { branches, .. }) = table.get(origin.condition())
            else {
                panic!("presence is a saved boolean choice")
            };
            assert_eq!(table.get(branches[0]), Some(&CleanupCondition::Never));
            assert_eq!(table.get(branches[1]), Some(&CleanupCondition::Always));
        }
    }
}

#[test]
fn loop_phi_layout_keeps_opaque_owned_parameters_and_bindings() {
    use lang_frontend::ownership_checking::IterationPhiBoundary;
    let (sources, parsed, owned) = checked(
        "fun run(own first: move () -> Unit, own second: move () -> Unit, flags: List<Boolean>) {
            var f = first
            var g = second
            for (_ in flags) {
                val saved = f
                { f = g }
                { g = saved }
            }
            val a = f()
            val b = g()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let plan = &owned.iterations()[0];
    for name in ["f", "g"] {
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id();
        for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
            assert!(
                plan.closure_phis().iter().any(|phi| {
                    phi.boundary() == boundary && phi.symbol() == symbol && phi.origins().is_empty()
                }),
                "opaque {name} needs an owner slot at {boundary:?}"
            );
        }
    }
}

#[test]
fn loop_phi_layout_keeps_uncaptured_owned_parameter_in_header() {
    use lang_frontend::ownership_checking::IterationPhiBoundary;
    let (sources, parsed, owned) = checked(
        "fun run(own action: move () -> Unit, flags: List<Boolean>) {
            for (_ in flags) { val marker = 1 }
            val used = action()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let symbol = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "action")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    assert!(plan.closure_phis().iter().any(|phi| {
        phi.boundary() == IterationPhiBoundary::Header
            && phi.symbol() == symbol
            && phi.origins().is_empty()
    }));
}

#[test]
fn loop_phi_layout_keeps_implicit_owned_lambda_parameter() {
    use lang_frontend::name_resolution::SymbolKind;
    use lang_frontend::ownership_checking::IterationPhiBoundary;
    for (parameter, body) in [
        ("it", "{ for (_ in flags) {}\nread(it) }"),
        ("arg", "{ arg -> for (_ in flags) {}\nread(arg) }"),
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flags: List<Boolean>) {{
                val action: (own List<Int>) -> Unit = {body}
                val invoked = action(xs)
            }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{parameter}: {:?}",
            owned.diagnostics()
        );
        let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| symbol.kind() == SymbolKind::LambdaParameter)
            .unwrap()
            .id();
        let plan = &owned.iterations()[0];
        for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
            assert!(
                plan.closure_phis().iter().any(|phi| {
                    phi.boundary() == boundary && phi.symbol() == symbol && phi.origins().is_empty()
                }),
                "{parameter} lacks {boundary:?} owner slot"
            );
        }
    }
}

#[test]
fn loop_phi_layout_does_not_own_shared_lambda_capture() {
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flags: List<Boolean>) {
            val action: () -> Unit = { for (_ in flags) {}\nread(xs) }
            val invoked = action()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let xs = names
        .symbols()
        .iter()
        .rev()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "xs")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    assert!(
        plan.closure_phis().iter().all(|phi| phi.symbol() != xs),
        "shared capture only borrows the outer owner"
    );
}

#[test]
fn phi_incoming_does_not_allocate_a_slot_for_copyable_capture() {
    use lang_frontend::ownership_checking::ClosureCaptureSource;

    let (sources, parsed, owned) = checked(
        "fun read(n: Int) {}\nfun run(flags: List<Int>) { val n = 1\nvar f: () -> Unit = { read(n) }\nfor (_ in flags) {}\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let n = names
        .symbols()
        .iter()
        .rfind(|symbol| sources.slice(symbol.span()).unwrap() == "n")
        .unwrap()
        .id();
    let inputs = owned
        .iterations()
        .iter()
        .flat_map(|plan| plan.closure_phi_incomings())
        .flat_map(|incoming| incoming.bindings())
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .filter(|input| input.input().source() == ClosureCaptureSource::Symbol(n))
        .collect::<Vec<_>>();
    assert!(
        !inputs.is_empty(),
        "the carried closure must retain its checked capture"
    );
    assert!(
        inputs
            .iter()
            .all(|input| input.target().is_none() && input.capture_slot().is_none())
    );
}

#[test]
fn loop_phi_layout_keeps_owned_root_in_elvis_null_rhs() {
    use lang_frontend::ownership_checking::IterationPhiBoundary;
    let (sources, parsed, owned) = checked(
        "class Node {}\nfun read(x: Node?) {}\nfun run(own x: Node?, flags: List<Boolean>, flag: Boolean) {
            val selected = x ?: if (flag) {
                for (_ in flags) { val used = read(x) }
                Node()
            } else Node()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let x = names
        .symbols()
        .iter()
        .rev()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "x")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
        assert!(
            plan.closure_phis().iter().any(|phi| {
                phi.boundary() == boundary && phi.symbol() == x && phi.origins().is_empty()
            }),
            "Elvis null RHS loses x at {boundary:?}"
        );
    }
}

#[test]
fn loop_phi_layout_separates_captured_owner_slots_for_prior_and_current_environments() {
    use lang_frontend::ownership_checking::{
        CleanupOwnerValue, ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource,
        IterationPhiBoundary,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            var g: () -> Unit = {}
            for (_ in flags) {
                val prior = f
                val xs = listOf(1)
                { g = prior }
                { f = ({ read(xs) }) }
            }
            val first = g()
            val second = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let plan = &owned.iterations()[0];
    let capture = owned
        .captures()
        .iter()
        .find(|capture| sources.slice(capture.reference_span()).unwrap() == "xs")
        .unwrap();
    let source = capture.source();
    assert!(matches!(source, ClosureCaptureSource::Symbol(_)));
    let mut owners = std::collections::BTreeSet::new();
    for name in ["f", "g"] {
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id();
        for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
            let phi = plan
                .closure_phis()
                .iter()
                .find(|phi| phi.symbol() == symbol && phi.boundary() == boundary)
                .unwrap();
            let origin = phi
                .origins()
                .iter()
                .find(|origin| origin.closure() == capture.lambda())
                .unwrap();
            let slot = origin
                .sources()
                .iter()
                .find(|slot| slot.source() == source)
                .unwrap();
            assert_eq!(slot.mode(), ClosureCaptureMode::Shared);
            assert_eq!(slot.effect(), ClosureCaptureEffect::Borrow);
            assert!(matches!(
                owned.cleanup_conditions().owner_value(slot.owner()),
                Some(CleanupOwnerValue::IterationPhiSourceOwner {
                    environment,
                    closure,
                    source: captured,
                    ..
                }) if *environment == phi.owner() && *closure == capture.lambda() && *captured == source
            ));
            assert!(
                owners.insert(slot.owner()),
                "{name} {boundary:?} shares a captured owner slot"
            );
        }
    }
    assert_eq!(owners.len(), 4);
}

#[test]
fn loop_phi_layout_does_not_own_a_borrowed_outer_element_capture() {
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(xs: List<List<Int>>, flags: List<Boolean>) {
            for (n in xs) {
                var f: () -> Unit = {}
                for (_ in flags) { f = ({ read(n) }) }
                val used = f()
            }
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let inner = owned
        .iterations()
        .iter()
        .find(|plan| {
            sources
                .slice(
                    parsed
                        .ast()
                        .statements()
                        .get(plan.descriptor().statement())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                .starts_with("for (_ in flags)")
        })
        .unwrap();
    let capture = owned
        .captures()
        .iter()
        .find(|capture| sources.slice(capture.reference_span()).unwrap() == "n")
        .unwrap();
    let origins = inner
        .closure_phis()
        .iter()
        .flat_map(|phi| phi.origins())
        .filter(|origin| origin.closure() == capture.lambda())
        .collect::<Vec<_>>();
    assert!(!origins.is_empty());
    assert!(
        origins.iter().all(|origin| origin.sources().is_empty()),
        "Borrow element has no owner to transport"
    );
}

#[test]
fn loop_phi_layout_distinguishes_owned_move_capture_source() {
    use lang_frontend::ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode};
    let (sources, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flags: List<Boolean>) {
            var f: move () -> Unit = move { read(xs) }
            for (_ in flags) { val invoked = f() }
            val after = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let capture = owned
        .captures()
        .iter()
        .find(|capture| sources.slice(capture.reference_span()).unwrap() == "xs")
        .unwrap();
    let plan = &owned.iterations()[0];
    let origins = plan
        .closure_phis()
        .iter()
        .flat_map(|phi| phi.origins())
        .filter(|origin| origin.closure() == capture.lambda())
        .collect::<Vec<_>>();
    assert_eq!(origins.len(), 2, "header and exit keep separate sources");
    for origin in origins {
        let slot = origin
            .sources()
            .iter()
            .find(|slot| slot.source() == capture.source())
            .unwrap();
        assert_eq!(slot.mode(), ClosureCaptureMode::Owned);
        assert_eq!(slot.effect(), ClosureCaptureEffect::Move);
    }
}

#[test]
fn loop_phi_source_slots_keep_checked_capture_order() {
    use lang_frontend::ownership_checking::ClosureCaptureSource;
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, label: Int, flags: List<Boolean>) {
            var f: () -> Unit = { val seen = label\nval first = read(ys)\nval second = read(xs) }
            for (_ in flags) { val invoked = f() }
            val after = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let plan = &owned.iterations()[0];
    let (phi, origin) = plan
        .closure_phis()
        .iter()
        .find_map(|phi| {
            phi.origins()
                .iter()
                .find(|origin| origin.sources().len() == 2)
                .map(|origin| (phi, origin))
        })
        .unwrap();
    assert_eq!(
        plan.capture_graph().nodes()[origin.node()]
            .sources()
            .iter()
            .map(|source| source.position())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        origin
            .sources()
            .iter()
            .map(|slot| match slot.source() {
                ClosureCaptureSource::Symbol(symbol) => {
                    sources
                        .slice(names.symbols()[symbol.index()].span())
                        .unwrap()
                }
                ClosureCaptureSource::This => "this",
            })
            .collect::<Vec<_>>(),
        ["ys", "xs"]
    );
    let environment = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            lang_frontend::ownership_checking::IterationCleanupAction::CreateClosureOwner {
                owner,
                closure,
            } if *closure == origin.closure() => Some(*owner),
            _ => None,
        })
        .expect("the phi source is a created closure environment");
    let label = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "label")
        .unwrap()
        .id();
    let label_slot = owned
        .cleanup_conditions()
        .capture_slot(environment, ClosureCaptureSource::Symbol(label))
        .expect("the non-phi capture still occupies its environment slot");
    assert_eq!(
        owned
            .cleanup_conditions()
            .capture_slot_value(label_slot)
            .unwrap()
            .position(),
        0
    );
    for (position, source) in origin.sources().iter().enumerate() {
        let slot = owned
            .cleanup_conditions()
            .capture_slot(environment, source.source())
            .expect("every checked capture has a static environment slot");
        assert_eq!(
            owned
                .cleanup_conditions()
                .capture_slot_value(slot)
                .unwrap()
                .position(),
            position + 1
        );
        let phi_slot = owned
            .cleanup_conditions()
            .phi_capture_slot(phi.owner(), origin.closure(), source.source())
            .expect("phi capture uses the full checked capture position");
        let mapped = phi
            .capture_layout()
            .iter()
            .find(|mapped| mapped.node() == origin.node() && mapped.position() == position + 1)
            .expect("the finite graph maps the original capture position");
        assert_eq!(mapped.slot(), phi_slot);
        assert_eq!(
            owned
                .cleanup_conditions()
                .capture_slot_value(phi_slot)
                .unwrap()
                .position(),
            position + 1
        );
    }
}

#[test]
fn phi_capture_slots_distinguish_same_source_in_alternative_lambdas() {
    use lang_frontend::ownership_checking::{
        ClosureCaptureSource, DropPoint, DropTarget, IterationPhiBoundary, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean, flags: List<Boolean>) {
            var f: move () -> Unit = if (flag) (move { read(xs) }) else (move { read(xs) })
            for (_ in flags) { val marker = 1 }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let xs = names
        .symbols()
        .iter()
        .rfind(|symbol| sources.slice(symbol.span()).unwrap() == "xs")
        .unwrap()
        .id();
    let source = ClosureCaptureSource::Symbol(xs);
    let plan = &owned.iterations()[0];
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == f)
        .unwrap();
    assert_eq!(exit.origins().len(), 2);
    let table = owned.cleanup_conditions();
    let slots = exit
        .origins()
        .iter()
        .map(|origin| {
            let slot = table
                .phi_capture_slot(exit.owner(), origin.closure(), source)
                .expect("each lambda candidate has its own phi capture slot");
            let layout = table.capture_slot_value(slot).unwrap();
            assert_eq!(layout.environment(), exit.owner());
            assert_eq!(layout.closure(), origin.closure());
            assert_eq!(layout.source(), source);
            slot
        })
        .collect::<Vec<_>>();
    assert_ne!(slots[0], slots[1]);
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let carried = exhaustion
        .bindings()
        .iter()
        .find(|binding| binding.target() == exit.owner())
        .unwrap();
    assert_eq!(carried.origins().len(), slots.len());
    for (index, origin) in carried.origins().iter().enumerate() {
        let incoming_slots = origin
            .environments()
            .iter()
            .flat_map(|environment| environment.sources())
            .filter(|input| input.input().source() == source)
            .map(|input| input.capture_slot())
            .collect::<Vec<_>>();
        assert_eq!(incoming_slots, [Some(slots[index])]);
    }
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, expression)| {
            (sources.slice(expression.span()).unwrap() == "f()").then_some(id)
        })
        .unwrap();
    let released = owned
        .drops()
        .iter()
        .filter(|fact| {
            fact.point() == DropPoint::CallReturn(call)
                && matches!(fact.target(), DropTarget::Captured { owner, source: actual, .. }
                    if owner == exit.owner() && actual == source)
        })
        .collect::<Vec<_>>();
    assert_eq!(released.len(), 2);
    for fact in released {
        let DropTarget::Captured { closure, .. } = fact.target() else {
            unreachable!();
        };
        assert_eq!(
            fact.capture_slot(),
            table.phi_capture_slot(exit.owner(), closure, source)
        );
    }
}

#[test]
fn nested_phi_capture_drop_uses_the_root_layout_slot() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, ClosureCaptureSource, DropPoint, DropTarget, IterationPhiBoundary,
        IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>) {
            var f: move () -> Unit = move {}
            for (_ in flags) { val xs = listOf(1)
                val g: move () -> Unit = move { read(xs) }
                { f = move { g() } } }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let xs = names
        .symbols()
        .iter()
        .rfind(|symbol| sources.slice(symbol.span()).unwrap() == "xs")
        .unwrap()
        .id();
    let source = ClosureCaptureSource::Symbol(xs);
    let plan = &owned.iterations()[0];
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == f)
        .unwrap();
    let header = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == f)
        .unwrap();
    let inner = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, expression)| {
            (sources.slice(expression.span()).unwrap() == "move { read(xs) }").then_some(id)
        })
        .unwrap();
    let slot = owned
        .cleanup_conditions()
        .phi_capture_slot(exit.owner(), inner, source)
        .expect("the nested source shares the root phi's finite layout");
    let layout = owned.cleanup_conditions().capture_slot_value(slot).unwrap();
    assert_eq!(layout.environment(), exit.owner());
    assert_eq!(layout.closure(), inner);
    let source_slot = owned
        .cleanup_conditions()
        .phi_capture_slot(header.owner(), inner, source)
        .expect("nested forwarding reads the header environment layout");
    for incoming in plan.closure_phi_incomings() {
        for binding in incoming.bindings() {
            let layout = plan
                .closure_phis()
                .iter()
                .find(|layout| layout.owner() == binding.target())
                .unwrap();
            assert_eq!(binding.selector_writes().len(), layout.origins().len());
            for (write, node) in binding.selector_writes().iter().zip(layout.origins()) {
                assert_eq!(
                    (write.node(), write.target()),
                    (node.node(), node.selector())
                );
            }
        }
    }
    let entry = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let header_input = entry
        .bindings()
        .iter()
        .find(|binding| binding.target() == header.owner())
        .unwrap();
    let inner_node = header
        .origins()
        .iter()
        .find(|origin| origin.closure() == inner)
        .unwrap();
    assert_eq!(header_input.selector_writes().len(), header.origins().len());
    assert_eq!(
        owned.cleanup_conditions().get(
            header_input
                .selector_writes()
                .iter()
                .find(|write| write.node() == inner_node.node())
                .unwrap()
                .condition()
        ),
        Some(&CleanupCondition::Never),
        "zero-round entry must clear the not-yet-formed child presence"
    );
    let incoming_slots = plan
        .closure_phi_incomings()
        .iter()
        .filter(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .flat_map(|incoming| incoming.bindings())
        .filter(|binding| binding.target() == exit.owner())
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .flat_map(|source| source.captured())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .filter(|input| input.input().source() == source)
        .map(|input| (input.capture_slot(), input.source_capture_slot()))
        .collect::<Vec<_>>();
    assert!(
        incoming_slots.contains(&(Some(slot), Some(source_slot))),
        "{incoming_slots:?}"
    );
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, expression)| {
            (sources.slice(expression.span()).unwrap() == "f()").then_some(id)
        })
        .unwrap();
    assert!(owned.drops().iter().any(|fact| {
        fact.point() == DropPoint::CallReturn(call)
            && matches!(fact.target(), DropTarget::Captured { owner, closure, source: actual, .. }
                if owner != exit.owner() && closure == inner && actual == source)
            && fact.capture_slot() == Some(slot)
    }), "nested captured drop must use its root phi layout slot: {:?}", owned.drops());
}
