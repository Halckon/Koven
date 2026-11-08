use lang_frontend::ownership_checking::{
    UnitClosureCaptureSource, UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as Action,
    UnitIterationExitKind,
};

use super::{LoanKind, analyzed_unit, unit_codes};

#[test]
fn move_only_capture_replaces_old_storage_only_on_normal_rhs_edges() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            "class Resource {}
             fun stop(): Nothing
             fun next(current: Resource): Resource = Resource()
             fun test(flag: Boolean): Unit {
                 var item = Resource()
                 val update: () -> Unit = {
                     item = next(item)
                     item = if (flag) Resource() else { return }
                     item = stop()
                 }
                 update()
             }",
            rooted,
        );
        assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
        let exclusive = owned
            .captures()
            .iter()
            .filter(|capture| capture.borrow_kind() == Some(LoanKind::Exclusive))
            .collect::<Vec<_>>();
        let [capture] = exclusive.as_slice() else {
            panic!("one exclusive captured root");
        };
        let UnitClosureCaptureSource::Symbol(symbol) = capture.source() else {
            panic!("captured root");
        };
        let replacements = owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.point(), UnitDropPoint::BeforeReplacement(_)))
            .collect::<Vec<_>>();
        assert_eq!(replacements.len(), 2, "{:?}", owned.drops());
        for fact in replacements {
            assert!(matches!(
                (fact.point(), fact.target()),
                (UnitDropPoint::BeforeReplacement(point),
                 UnitDropTarget::ReplacedCapture { closure, assignment, symbol: root })
                    if point == assignment && closure == capture.lambda() && root == symbol
            ));
        }
        let named = owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == UnitDropTarget::Named(symbol))
            .collect::<Vec<_>>();
        assert_eq!(named.len(), 1, "the caller still owns the captured place");
        assert!(matches!(named[0].point(), UnitDropPoint::CallReturn(_)));
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn move_only_capture_cannot_deliver_its_old_value_as_owned() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            "class Resource {}
             fun next(own current: Resource): Resource = Resource()
             fun test(): Unit {
                 var item = Resource()
                 val update: () -> Unit = { item = next(item) }
                 update()
             }",
            rooted,
        );
        assert_eq!(unit_codes(&owned), ["L0133"]);
        assert!(owned.captures().is_empty());
        assert!(owned.drops().is_empty());
        assert!(owned.validate().is_err());
    }
}

#[test]
fn callable_capture_replacement_requires_environment_writeback() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            "fun seed(): Int = 0
             fun test(): Int {
                 var action: () -> Int = seed
                 val update: () -> Unit = {
                     val local = 7
                     action = { local }
                 }
                 update()
                 return action()
             }",
            rooted,
        );
        assert_eq!(unit_codes(&owned), ["L0164"]);
        assert!(owned.captures().is_empty());
        assert!(owned.drops().is_empty());
        assert!(owned.validate().is_err());
    }
}

#[test]
fn evaluated_inline_callback_keeps_every_borrowed_root_through_later_arguments() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            r#"fun run(action: () -> Unit, argument: Int): Unit { action() }
               fun test(flag: Boolean): Unit {
                   var text = "old"
                   val suffix = "suffix"
                   run({ text = suffix + "!" }, if (flag) { 1 } else { 2 })
               }"#,
            rooted,
        );
        assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
        assert_eq!(owned.captures().len(), 2);
        for capture in owned.captures() {
            let UnitClosureCaptureSource::Symbol(symbol) = capture.source() else {
                panic!("captured root");
            };
            let root_drops = owned
                .drops()
                .iter()
                .filter(|fact| fact.target() == UnitDropTarget::Named(symbol))
                .collect::<Vec<_>>();
            let [root_drop] = root_drops.as_slice() else {
                panic!("one caller-side cleanup for {capture:?}: {root_drops:?}");
            };
            let UnitDropPoint::CallReturn(call) = root_drop.point() else {
                panic!("captured storage outlives later argument branches: {root_drop:?}");
            };
            assert!(owned.drops().iter().any(|fact| {
                fact.point() == UnitDropPoint::CallReturn(call)
                    && matches!(fact.target(), UnitDropTarget::Temporary(_))
            }));
        }
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn move_only_mutating_closure_requires_a_tracked_environment_transport() {
    for rooted in [false, true] {
        for action in [
            "var edit: () -> Unit = { item = Resource() }; edit()",
            "var edit: () -> Unit = {}; if (flag) { edit = { item = Resource() } }; edit()",
            "val edit: () -> Unit = { item = Resource() }; val fallback: () -> Unit = {}; val selected = if (flag) edit else fallback; selected()",
            "val edit: () -> Unit = { item = Resource() }; val nested: () -> Unit = { edit() }; nested()",
        ] {
            let source = format!(
                "class Resource {{}}; fun test(flag: Boolean): Unit {{ var item = Resource(); {action} }}"
            );
            let owned = analyzed_unit(&source, rooted);
            assert_eq!(unit_codes(&owned), ["L0164"], "{source}");
            assert!(owned.validate().is_err(), "{source}");
        }
    }
}

#[test]
fn repeated_call_and_nested_rhs_replace_the_current_capture_slot() {
    for rooted in [false, true] {
        let source = r#"fun inspect(text: String): Unit {}
             fun test(flag: Boolean): Unit {
                 var text = "old"
                 val edit: () -> Unit = {
                     text = if (flag) { text = "inner"; "outer" } else { "other" }
                 }
                 val alias = edit
                 alias()
                 alias()
                 inspect(text)
             }"#;
        let (sources, owned) = super::borrow_rejection_unit::checked(
            source,
            "closures.ko",
            rooted,
            super::environments(),
        );
        assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
        let origins = owned
            .drops()
            .iter()
            .filter_map(|fact| {
                matches!(fact.target(), UnitDropTarget::ReplacedCapture { .. })
                    .then_some(fact.value_origin().start())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            origins,
            [
                source.find("text = \"inner\"").unwrap(),
                source.find("text = if").unwrap()
            ],
            "the inner commit destroys the original and the outer commit destroys the inner value"
        );
        assert!(
            owned
                .drops()
                .iter()
                .all(|fact| sources.slice(fact.value_origin()).is_ok())
        );
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn abandoned_inline_callback_cleans_up_before_its_borrowed_roots() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            r#"fun run(action: () -> Unit, argument: Int): Unit { action() }
               fun test(flag: Boolean): Unit {
                   var text = "old"
                   val suffix = "suffix"
                   run({ text = suffix + "!" }, if (flag) 1 else { return })
               }"#,
            rooted,
        );
        assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
        let cleanup = owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.point(), UnitDropPoint::ControlTransfer(_)))
            .collect::<Vec<_>>();
        assert_eq!(cleanup.len(), 3, "{cleanup:?}");
        let [callback, first, second] = cleanup.as_slice() else {
            unreachable!("verified cleanup size");
        };
        assert!(matches!(callback.target(), UnitDropTarget::Temporary(_)));
        assert_eq!(first.point(), callback.point());
        assert_eq!(second.point(), callback.point());
        let roots = owned
            .captures()
            .iter()
            .map(|capture| {
                let UnitClosureCaptureSource::Symbol(symbol) = capture.source() else {
                    panic!("captured root");
                };
                UnitDropTarget::Named(symbol)
            })
            .collect::<Vec<_>>();
        assert_ne!(first.target(), second.target());
        assert!(roots.contains(&first.target()));
        assert!(roots.contains(&second.target()));
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn inner_iteration_break_keeps_the_outer_callback_and_its_sources_live() {
    for rooted in [false, true] {
        for (binding, callback) in [
            ("", r#"{ text = suffix + "!" }"#),
            (r#"val edit: () -> Unit = { text = suffix + "!" }"#, "edit"),
        ] {
            let source = format!(
                r#"fun run(action: () -> Unit, argument: Int): Unit {{ action() }}
                   fun test(flag: Boolean): Unit {{
                       var text = "old"
                       val suffix = "suffix"
                       {binding}
                       run({callback}, if (flag) {{ for (i in 0..<2) {{ break }}; 1 }} else 2)
                   }}"#
            );
            let owned = analyzed_unit(&source, rooted);
            assert!(
                unit_codes(&owned).is_empty(),
                "{source}: {:?}",
                owned.diagnostics()
            );
            for capture in owned.captures() {
                let UnitClosureCaptureSource::Symbol(symbol) = capture.source() else {
                    panic!("captured root");
                };
                let drops = owned
                    .drops()
                    .iter()
                    .filter(|fact| fact.target() == UnitDropTarget::Named(symbol))
                    .collect::<Vec<_>>();
                assert!(
                    matches!(drops.as_slice(), [fact] if matches!(fact.point(), UnitDropPoint::CallReturn(_))),
                    "{source}: {drops:?}"
                );
            }
            assert!(owned.validate().is_ok(), "{source}");
        }
    }
}

#[test]
fn later_argument_cannot_move_the_already_evaluated_callee() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            r#"fun test(flag: Boolean): Unit {
                   var text = "old"
                   val edit: (Int) -> Unit = { n -> text = "new" }
                   edit(if (flag) { val moved = edit; 1 } else 2)
               }"#,
            rooted,
        );
        assert_eq!(unit_codes(&owned), ["L0135"]);
        assert!(owned.drops().is_empty());
        assert!(owned.validate().is_err());
    }
}

#[test]
fn returning_across_nested_providers_releases_capture_roots_at_their_loop_depth() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            r#"fun run(action: () -> Unit, argument: Int): Unit { action() }
               fun test(flag: Boolean): Unit {
                   for (i in 0..<2) {
                       var text = "old"
                       run({ text = "new" }, if (flag) {
                           for (j in 0..<2) { return }
                           1
                       } else 2)
                   }
               }"#,
            rooted,
        );
        assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
        let [capture] = owned.captures() else {
            panic!("one captured root");
        };
        let UnitClosureCaptureSource::Symbol(symbol) = capture.source() else {
            panic!("captured root");
        };
        let actions = owned
            .iterations()
            .iter()
            .flat_map(|plan| plan.exits())
            .find(|exit| matches!(exit.kind(), UnitIterationExitKind::Return(_)))
            .expect("return through two providers")
            .actions();
        let order = actions.iter().filter_map(|action| match action {
            Action::Drop(fact) if matches!(fact.target(), UnitDropTarget::Temporary(_)) => {
                Some("callback")
            }
            Action::Drop(fact) if fact.target() == UnitDropTarget::Named(symbol) => Some("root"),
            Action::FinishProvider(_) => Some("provider"),
            _ => None,
        });
        assert_eq!(
            order.collect::<Vec<_>>(),
            ["provider", "callback", "root", "provider"],
            "{actions:?}"
        );
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn captured_resource_replaces_old_value_but_keeps_lexical_final_cleanup() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            "class Resource { deinit() {} }
             fun test(): Unit {
                 var item = Resource()
                 val edit: () -> Unit = { item = Resource() }
                 edit()
             }",
            rooted,
        );
        assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty());
        let [capture] = owned.captures() else {
            panic!("one captured root");
        };
        let UnitClosureCaptureSource::Symbol(symbol) = capture.source() else {
            panic!("captured root");
        };
        let old = owned.drops().iter().filter(|fact| {
            matches!(fact.target(), UnitDropTarget::ReplacedCapture { symbol: root, .. } if root == symbol)
        }).collect::<Vec<_>>();
        assert!(
            matches!(old.as_slice(), [fact] if matches!(fact.point(), UnitDropPoint::BeforeReplacement(_)))
        );
        let final_value = owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == UnitDropTarget::Named(symbol))
            .collect::<Vec<_>>();
        assert!(
            matches!(final_value.as_slice(), [fact] if matches!(fact.point(), UnitDropPoint::AfterStatement(_)))
        );
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn inline_copyable_exclusive_capture_keeps_shared_move_only_source_until_call() {
    for argument in [
        "if (flag) { 1 } else { 2 }",
        "when (suffix) { \"suffix\" -> 1; else -> 2 }",
    ] {
        for rooted in [false, true] {
            let source = format!(
                r#"fun inspect(suffix: String): Unit {{}}
               fun run(action: () -> Unit, argument: Int): Unit {{ action() }}
               fun test(flag: Boolean): Unit {{
                   var n = 0
                   val suffix = "suffix"
                   run({{ inspect(suffix); n = 1 }}, {argument})
               }}"#
            );
            let owned = analyzed_unit(&source, rooted);
            assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
            let capture = owned
                .captures()
                .iter()
                .find(|capture| capture.borrow_kind() == Some(LoanKind::Shared))
                .expect("shared String capture");
            let UnitClosureCaptureSource::Symbol(symbol) = capture.source() else {
                panic!("captured root");
            };
            let drops = owned
                .drops()
                .iter()
                .filter(|fact| fact.target() == UnitDropTarget::Named(symbol))
                .collect::<Vec<_>>();
            assert!(
                matches!(drops.as_slice(), [fact] if matches!(fact.point(), UnitDropPoint::CallReturn(_))),
                "{drops:?}"
            );
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn branch_entry_preserves_the_move_only_source_used_by_the_selected_body() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            "class Resource {}
             fun inspect(value: Resource): Unit {}
             fun test(flag: Boolean): Unit {
                 var item = Resource()
                 val edit: () -> Unit = { item = Resource() }
                 if (flag) { edit() } else { inspect(item); return }
             }",
            rooted,
        );
        assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
        let [capture] = owned.captures() else {
            panic!("one captured root");
        };
        let UnitClosureCaptureSource::Symbol(source) = capture.source() else {
            panic!("captured root");
        };
        let entry = owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.point(), UnitDropPoint::BranchEntry { branch: 1, .. }))
            .collect::<Vec<_>>();
        assert!(
            matches!(entry.as_slice(), [fact]
            if matches!(fact.target(), UnitDropTarget::Named(holder) if holder != source)),
            "{entry:?}"
        );
        assert!(owned.drops().iter().any(|fact| {
            fact.target() == UnitDropTarget::Named(source)
                && matches!(fact.point(), UnitDropPoint::CallReturn(_))
        }));
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn owned_inline_wrapper_keeps_nested_exclusive_environment_sources_until_call() {
    for rooted in [false, true] {
        let (sources, owned) = super::borrow_rejection_unit::checked(
            r#"fun inspect(suffix: String): Unit {}
               fun run(action: () -> Unit, argument: Int): Unit { action() }
               fun test(flag: Boolean): Int {
                   var n = 1
                   val suffix = "suffix"
                   val edit: () -> Unit = { inspect(suffix); n = 2 }
                   run(move { edit() }, if (flag) 1 else 2)
                   return n
               }"#,
            "closures.ko",
            rooted,
            super::environments(),
        );
        assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
        let drops = owned
            .drops()
            .iter()
            .enumerate()
            .filter(|(_, fact)| {
                matches!(fact.target(), UnitDropTarget::Named(_))
                    && sources.slice(fact.value_origin()).unwrap() == "suffix"
            })
            .collect::<Vec<_>>();
        let [(position, suffix)] = drops.as_slice() else {
            panic!("one cleanup for the borrowed suffix: {drops:?}");
        };
        assert!(
            matches!(suffix.point(), UnitDropPoint::CallReturn(_)),
            "{suffix:?}"
        );
        assert!(owned.drops()[..*position].iter().any(|fact| {
            fact.point() == suffix.point() && matches!(fact.target(), UnitDropTarget::Temporary(_))
        }));
        assert!(owned.validate().is_ok());
    }
}
