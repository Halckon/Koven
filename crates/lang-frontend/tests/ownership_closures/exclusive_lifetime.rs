use super::{LoanKind, analyzed_unit, unit_codes};

#[test]
fn exclusive_capture_survives_later_call_operand_evaluation() {
    for rooted in [false, true] {
        for source in [
            "fun invoke(action: () -> Unit, done: Unit, seen: Int): Unit { action() }
             fun test(): Unit { var number = 1; val update: () -> Unit = { number = 2 }; invoke(update, update(), number) }",
            "fun observe(cb: (Unit, Int) -> Unit): Unit {}
             fun test(): Unit { var number = 1; val update: (Unit, Int) -> Unit = { done, seen -> number = seen }; update(observe(update), number) }",
            "fun test(): Unit { var number = 1; val update: () -> Unit = {
                 val read: () -> Int = { number }; number = 2; val seen = read()
             }; update() }",
        ] {
            let owned = analyzed_unit(source, rooted);
            assert_eq!(unit_codes(&owned), ["L0135"], "{source}");
            assert!(owned.validate().is_err(), "{source}");
        }
    }
}

#[test]
fn exclusive_capture_cannot_escape_through_value_delivery() {
    for rooted in [false, true] {
        for source in [
            "fun test(): Unit {
                 val factory: () -> () -> Unit = {
                     var number = 1; val update: () -> Unit = { number = 2 }; update
                 }
                 val f = factory(); f()
             }",
            "fun deliver(own cb: () -> Unit): Unit {}
             fun test(flag: Boolean): Unit {
                 var n = 1; val update: () -> Unit = { n = 2 }; deliver(if (flag) update else update)
             }",
            "class Holder(val cb: () -> Unit)
             fun test(flag: Boolean): Unit {
                 var n = 1; val update: () -> Unit = { n = 2 }; val holder = Holder(if (flag) update else update)
             }",
            "class Holder(var cb: () -> Unit)
             fun test(flag: Boolean): Unit {
                 var n = 1; val update: () -> Unit = { n = 2 }; val holder = Holder({})
                 holder.cb = if (flag) update else update
             }",
        ] {
            let owned = analyzed_unit(source, rooted);
            assert_eq!(unit_codes(&owned), ["L0137"], "{source}");
            assert!(owned.validate().is_err(), "{source}");
        }
    }
}

#[test]
fn exclusive_capture_loop_backedge_requires_a_proven_loan_state() {
    for rooted in [false, true] {
        let source = "fun read(n: Int): Unit {}
             fun test(flag: Boolean): Unit {
                 var n = 1; var f: () -> Unit = {}
                 while (flag) { read(n); f(); f = { n = 2 } }
             }";
        let owned = analyzed_unit(source, rooted);
        assert_eq!(unit_codes(&owned), ["L0164"], "{source}");
        assert!(owned.validate().is_err(), "{source}");

        let source = "fun test(flag: Boolean): Int {
             var n = 1
             while (flag) { val f: () -> Unit = { n = 2 }; f(); val seen = n }
             return n
         }";
        let owned = analyzed_unit(source, rooted);
        assert!(
            unit_codes(&owned).is_empty(),
            "{source}: {:?}",
            owned.diagnostics()
        );
        assert!(owned.validate().is_ok(), "{source}");
    }
}

#[test]
fn exclusive_capture_ends_on_branches_without_future_use() {
    for rooted in [false, true] {
        for action in [
            "if (flag) { f() }",
            "if (flag) { f() } else { val seen = n }",
        ] {
            let source = format!(
                "fun test(flag: Boolean): Int {{ var n = 1; val f: () -> Unit = {{ n = 2 }}; {action}; return n }}"
            );
            let owned = analyzed_unit(&source, rooted);
            assert!(
                unit_codes(&owned).is_empty(),
                "{source}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok(), "{source}");
        }
    }
}

#[test]
fn exclusive_capture_ends_after_the_last_direct_or_synchronous_call() {
    for rooted in [false, true] {
        for action in [
            "val update: () -> Unit = { number = number + 1 }; update()",
            "invoke({ number = number + 1 })",
            "val update: () -> Unit = { number = number + 1 }; val alias = update; alias()",
        ] {
            let source = format!(
                "fun invoke(action: () -> Unit): Unit {{ action() }}
                                 fun test(): Int {{ var number = 1; {action}; return number }}"
            );
            let owned = analyzed_unit(&source, rooted);
            assert!(
                owned.diagnostics().is_empty(),
                "{source}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok(), "{source}");
        }
    }
}

#[test]
fn live_exclusive_capture_rejects_an_outer_read_and_cannot_escape() {
    for rooted in [false, true] {
        for (source, code) in [
            (
                "fun test(): Int { var number = 1; val update: () -> Unit = { number = 2 }; val seen = number; update(); return seen }",
                "L0135",
            ),
            (
                "fun test(): () -> Unit { var number = 1; val update: () -> Unit = { number = 2 }; return update }",
                "L0137",
            ),
            (
                "fun test(): Unit { var number = 1; val update: () -> Unit = { number = 2 }; number = 3; update() }",
                "L0135",
            ),
        ] {
            let owned = analyzed_unit(source, rooted);
            assert_eq!(unit_codes(&owned), [code], "{source}");
            assert!(owned.captures().is_empty());
            assert!(owned.validate().is_err());
        }
    }
}

#[test]
fn exclusive_capture_demand_reports_an_existing_shared_capture_conflict() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            "fun test(): Int {
                var number = 1
                val read: () -> Int = { number }
                val write: () -> Unit = { number = 2 }
                return read()
            }",
            rooted,
        );
        assert_eq!(unit_codes(&owned), ["L0135"]);
        assert!(owned.captures().is_empty());
        assert!(owned.loans().is_empty());
        assert!(owned.validate().is_err());
    }
}

#[test]
fn exclusive_and_shared_capture_permissions_stay_distinct() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            "fun test(): Int {
                var number = 1
                val step = 2
                val update: () -> Unit = { number = number + step }
                update()
                return number
            }",
            rooted,
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert_eq!(
            owned
                .captures()
                .iter()
                .map(|capture| capture.borrow_kind())
                .collect::<Vec<_>>(),
            [Some(LoanKind::Exclusive), Some(LoanKind::Shared)]
        );
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn unreachable_exclusive_capture_demand_does_not_publish_a_shared_environment() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            "fun test(): Unit {
                var number = 1
                return
                val write: () -> Unit = { number = 2 }
            }",
            rooted,
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.captures().is_empty(), "{:?}", owned.captures());
        assert!(owned.closures().is_empty(), "{:?}", owned.closures());
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn exclusive_capture_cannot_escape_by_returning_a_captured_closure() {
    for rooted in [false, true] {
        for (modifier, body) in [
            ("move ", "update"),
            ("", "update"),
            ("move ", "return update"),
        ] {
            let source = format!(
                "fun test(): () -> Unit {{
                     var n = 1
                     val update: () -> Unit = {{ n = 2 }}
                     val forward: () -> () -> Unit = {modifier}{{ {body} }}
                     return forward()
                 }}"
            );
            let owned = analyzed_unit(&source, rooted);
            assert_eq!(unit_codes(&owned), ["L0137"], "{source}");
            assert!(owned.validate().is_err(), "{source}");
        }
    }
}

#[test]
fn exclusive_capture_ends_on_loop_exit_without_future_use() {
    for rooted in [false, true] {
        for repeat in [
            "while (flag) { f() }",
            "for (i in 0..<2) { f() }",
            "loop { f(); if (flag) { break } }",
        ] {
            let source = format!(
                "fun test(flag: Boolean): Int {{
                     var n = 1; val f: () -> Unit = {{ n = 2 }}
                     {repeat}
                     return n
                 }}"
            );
            let owned = analyzed_unit(&source, rooted);
            assert!(
                unit_codes(&owned).is_empty(),
                "{source}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok(), "{source}");
        }
    }
}

#[test]
fn explicit_borrow_of_exclusive_capture_needs_environment_transport() {
    for rooted in [false, true] {
        let source = "fun test(): Unit {
             var n = 1; val f: () -> Unit = { n = 2 }
             borrow val alias = f
             f(); val seen = n; alias()
         }";
        let owned = analyzed_unit(source, rooted);
        assert_eq!(unit_codes(&owned), ["L0164"], "{source}");
        assert!(owned.validate().is_err(), "{source}");
    }
}

#[test]
fn borrow_result_callback_requires_exclusive_environment_transport() {
    for rooted in [false, true] {
        for source in [
            "fun view(source: () -> Unit): borrow () -> Unit from source = source
             fun invoke(action: () -> Unit, seen: Int): Unit { action() }
             fun test(): Unit {
                 var n = 1; val f: () -> Unit = { n = 2 }
                 invoke(view(f), n)
             }",
            "fun select(before: Unit, source: () -> Unit): borrow () -> Unit from source = source
             fun test(): Unit {
                 var n = 1; var f: () -> Unit = {}
                 borrow val alias = select((f = { n = 2 }), f)
                 val seen = n; alias()
             }",
        ] {
            let owned = analyzed_unit(source, rooted);
            assert_eq!(unit_codes(&owned), ["L0164"], "{source}");
            assert!(owned.validate().is_err(), "{source}");
        }
    }
}

#[test]
fn nullable_callable_requires_exclusive_environment_transport() {
    for rooted in [false, true] {
        for delivery in [
            "return (if (flag) f else null)!!",
            "return (if (flag) f else null) ?: ({})",
            "val optional = if (flag) f else null; return optional!!",
        ] {
            let source = format!(
                "fun test(flag: Boolean): () -> Unit {{
                     var n = 1; val f: () -> Unit = {{ n = 2 }}
                     {delivery}
                 }}"
            );
            let owned = analyzed_unit(&source, rooted);
            assert_eq!(unit_codes(&owned), ["L0164"], "{source}");
            assert!(owned.validate().is_err(), "{source}");
        }
    }
}

#[test]
fn unused_nullable_shared_closure_keeps_its_existing_release_point() {
    for rooted in [false, true] {
        let source = "fun test(flag: Boolean): Unit {
             var n = 1; val f: () -> Int = { n }
             val optional = if (flag) f else null
             n = 3
         }";
        let owned = analyzed_unit(source, rooted);
        assert!(
            unit_codes(&owned).is_empty(),
            "{source}: {:?}",
            owned.diagnostics()
        );
        assert!(owned.validate().is_ok(), "{source}");
    }
}

#[test]
fn borrow_result_uses_the_rebound_environment() {
    for rooted in [false, true] {
        let source =
            "fun select(before: Unit, source: () -> Unit): borrow () -> Unit from source = source
             fun test(flag: Boolean): Unit {
                 var n = 1; var f: () -> Unit = { n = 2 }
                 borrow val alias = select(if (flag) { f(); f = {} } else { f(); f = {} }, f)
                 alias()
             }";
        let owned = analyzed_unit(source, rooted);
        assert!(
            unit_codes(&owned).is_empty(),
            "{source}: {:?}",
            owned.diagnostics()
        );
        assert!(owned.validate().is_ok(), "{source}");
    }
}

#[test]
fn branch_entry_drops_exclusive_environment_before_access_to_its_source() {
    use lang_frontend::ownership_checking::{UnitDropPoint, UnitDropTarget};
    for rooted in [false, true] {
        for control in [
            "if (flag) { edit() } else { return n }",
            "when { flag -> { edit() }; else -> { return n } }",
            "when (word) { \"call\" -> { edit() }; else -> { return n } }",
        ] {
            let source = format!(
                "fun test(flag: Boolean, word: String): Int {{ var n = 1; val edit: () -> Unit = {{ n = 2 }}; {control}; return n }}"
            );
            let owned = analyzed_unit(&source, rooted);
            assert!(
                unit_codes(&owned).is_empty(),
                "{source}: {:?}",
                owned.diagnostics()
            );
            let early = owned
                .drops()
                .iter()
                .filter(|fact| matches!(fact.point(), UnitDropPoint::BranchEntry { branch: 1, .. }))
                .collect::<Vec<_>>();
            let [early] = early.as_slice() else {
                panic!(
                    "the returning branch must release the environment before reading n: {:?}",
                    owned.drops()
                );
            };
            let UnitDropTarget::Named(holder) = early.target() else {
                panic!("the named environment is released");
            };
            let points = owned
                .drops()
                .iter()
                .filter_map(|fact| {
                    (fact.target() == UnitDropTarget::Named(holder)).then_some(fact.point())
                })
                .collect::<Vec<_>>();
            assert_eq!(points.len(), 2, "{points:?}");
            assert!(
                points
                    .iter()
                    .any(|point| matches!(point, UnitDropPoint::CallReturn(_)))
            );
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn when_only_releases_the_environment_on_the_match_edge_that_still_holds_it() {
    use lang_frontend::ownership_checking::{UnitDropPoint, UnitDropTarget};
    for rooted in [false, true] {
        let owned = analyzed_unit(
            "fun check(edit: () -> Unit): Boolean { edit(); return true }
             fun test(flag: Boolean): Int {
                 var n = 1
                 val edit: () -> Unit = { n = 2 }
                 when { flag, check(edit) -> { return n }; else -> { return n } }
             }",
            rooted,
        );
        assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
        let drops = owned.drops();
        let Some(matched) = drops.iter().find(|fact| {
            matches!(
                fact.point(),
                UnitDropPoint::WhenAlternativeMatch {
                    entry: 0,
                    alternative: 0,
                    ..
                }
            )
        }) else {
            panic!("only the first matching edge still owns edit: {drops:?}");
        };
        assert!(matches!(matched.target(), UnitDropTarget::Named(_)));
        let points = drops
            .iter()
            .filter_map(|fact| (fact.target() == matched.target()).then_some(fact.point()))
            .collect::<Vec<_>>();
        assert_eq!(points.len(), 2, "{points:?}");
        assert!(
            points
                .iter()
                .any(|point| matches!(point, UnitDropPoint::CallReturn(_)))
        );
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn branch_entry_releases_a_nested_environment_before_its_borrowed_callable() {
    use lang_frontend::ownership_checking::{UnitDropPoint, UnitDropTarget};
    for rooted in [false, true] {
        let source = "fun test(flag: Boolean): Int {
            var n = 1
            val edit: () -> Unit = { n = 2 }
            val outer: () -> Unit = { edit() }
            if (flag) { outer() } else { return n }
            return n
        }";
        let (sources, owned) = super::borrow_rejection_unit::checked(
            source,
            "closures.ko",
            rooted,
            super::environments(),
        );
        assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
        let holders = owned
            .drops()
            .iter()
            .filter(|fact| {
                matches!(fact.point(), UnitDropPoint::BranchEntry { branch: 1, .. })
                    && matches!(fact.target(), UnitDropTarget::Named(_))
            })
            .map(|fact| sources.slice(fact.value_origin()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(holders, ["outer", "edit"]);
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn branch_entry_with_different_joined_environments_defers_executable_cleanup() {
    for rooted in [false, true] {
        let owned = analyzed_unit(
            "fun test(flag: Boolean, call: Boolean): Int {
                var n = 1
                var edit: () -> Unit = {}
                if (flag) { edit = { n = 2 } } else { edit = { n = 3 } }
                if (call) { edit() } else { return n }
                return n
            }",
            rooted,
        );
        assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().iter().any(|fact| {
            fact.reason()
                == lang_frontend::ownership_checking::OwnershipDeferredReason::ResourceLifetime
        }));
        assert!(owned.drops().is_empty());
        assert!(owned.validate().is_err());
    }
}

#[test]
fn branch_entry_keeps_the_callable_borrowed_by_an_evaluated_wrapper() {
    use lang_frontend::ownership_checking::{UnitDropPoint, UnitDropTarget};
    for rooted in [false, true] {
        let source = "fun run(action: () -> Unit, arg: Int): Unit { action() }
            fun test(flag: Boolean): Int {
                var n = 1
                val edit: () -> Unit = { n = 2 }
                val outer: () -> Unit = { edit() }
                run({ edit() }, if (flag) { outer(); 1 } else 2)
                return n
            }";
        let (sources, owned) = super::borrow_rejection_unit::checked(
            source,
            "closures.ko",
            rooted,
            super::environments(),
        );
        assert!(unit_codes(&owned).is_empty(), "{:?}", owned.diagnostics());
        let entry_holders = owned
            .drops()
            .iter()
            .filter(|fact| {
                matches!(fact.point(), UnitDropPoint::BranchEntry { branch: 1, .. })
                    && matches!(fact.target(), UnitDropTarget::Named(_))
            })
            .map(|fact| sources.slice(fact.value_origin()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(entry_holders, ["outer"]);
        let edits = owned
            .drops()
            .iter()
            .enumerate()
            .filter(|(_, fact)| {
                matches!(fact.target(), UnitDropTarget::Named(_))
                    && sources.slice(fact.value_origin()).unwrap() == "edit"
            })
            .collect::<Vec<_>>();
        let [(edit_index, edit)] = edits.as_slice() else {
            panic!("one cleanup for edit: {edits:?}");
        };
        assert!(matches!(edit.point(), UnitDropPoint::CallReturn(_)));
        assert!(owned.drops()[..*edit_index].iter().any(|fact| {
            fact.point() == edit.point() && matches!(fact.target(), UnitDropTarget::Temporary(_))
        }));
        assert!(owned.validate().is_ok());
    }
}
