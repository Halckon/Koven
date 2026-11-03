//! Original non-null assertion sources and oracles, checked in both Unit entry modes.
use super::codes;
use lang_frontend::ownership_checking::{UnitDropPoint as DropPoint, UnitDropTarget as DropTarget};

#[test]
fn non_null_assertion_consumes_owned_source_even_for_borrowed_result() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            super::nullable_unit::checked(text, "ownership.ko", rooted, super::environments())
        };

        // Borrowing the extracted result must not turn !! into a borrowed nullable view.
        let text = "class Resource {}
        fun read(item: Resource): Int = 0
        fun readNullable(item: Resource?): Int = 0
        fun test(own source: Resource?): Int {
            val first = read(source!!)
            val second = readNullable(source)
            return 0
        }";
        let (sources, _, owned) = checked(text);
        assert_eq!(codes(owned.diagnostics()), ["L0131"]);
        assert_eq!(
            sources
                .slice(owned.diagnostics()[0].primary_span())
                .unwrap(),
            "source"
        );
    }
}

#[test]
fn non_null_assertion_rejects_borrowed_and_partial_move_sources() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            super::nullable_unit::checked(text, "ownership.ko", rooted, super::environments())
        };

        // Extraction cannot leave a borrowed binding or a field with a missing owner.
        for (parameter, operand, expected) in [
            ("source: Resource?", "source", "L0133"),
            ("inout source: Resource?", "source", "L0133"),
            ("own source: Holder", "source.item", "L0132"),
            ("own source: Array<Resource?>", "source[0]", "L0136"),
        ] {
            let text = format!(
                "class Resource {{}}\nclass Holder(val item: Resource?) {{}}\nfun test({parameter}): Resource = {operand}!!"
            );
            let (sources, _, owned) = checked(&text);
            assert_eq!(codes(owned.diagnostics()), [expected], "{text}");
            // Field moves retain the shared diagnostic contract: highlight the field name.
            let expected_span = if expected == "L0132" { "item" } else { operand };
            assert_eq!(
                sources
                    .slice(owned.diagnostics()[0].primary_span())
                    .unwrap(),
                expected_span
            );
        }
    }
}

#[test]
fn non_null_assertion_copy_preserves_borrowed_source() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            super::nullable_unit::checked(text, "ownership.ko", rooted, super::environments())
        };

        // A Copyable extraction retains the source for a second extraction.
        let (_, _, owned) = checked(
            "fun test(source: Int?): Int { val first = source!!
return first + source!! }",
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    }
}

#[test]
fn non_null_assertion_rejects_active_loan() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            super::nullable_unit::checked(text, "ownership.ko", rooted, super::environments())
        };

        // A prior argument's shared loan lasts through extraction in the next argument.
        let text = "class Resource {}
        fun use(first: Resource?, second: Resource): Int = 0
        fun test(own source: Resource?): Int = use(source, source!!)";
        let (sources, _, owned) = checked(text);
        assert_eq!(codes(owned.diagnostics()), ["L0135"]);
        assert_eq!(
            sources
                .slice(owned.diagnostics()[0].primary_span())
                .unwrap(),
            "source"
        );
        assert!(owned.loans().is_empty());
        assert!(owned.drops().is_empty());
    }
}

#[test]
fn non_null_assertion_transfers_drop_to_borrowed_result() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            super::nullable_unit::checked(text, "ownership.ko", rooted, super::environments())
        };

        // The nullable root has transferred its obligation; only the extracted temporary is dropped.
        for operand in ["source", "create()"] {
            let text = format!(
                "class Resource {{}}
            fun create(): Resource? = Resource()
            fun read(item: Resource): Int = 0
            fun test(own source: Resource?): Int = read({operand}!!)"
            );
            let (sources, _, owned) = checked(&text);
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            let extraction = format!("{operand}!!");
            let drops = owned
                .drops()
                .iter()
                .filter(|fact| {
                    sources
                        .slice(fact.value_origin())
                        .is_ok_and(|span| span == extraction)
                })
                .collect::<Vec<_>>();
            assert_eq!(drops.len(), 1, "{operand}: {:?}", owned.drops());
            assert!(matches!(drops[0].target(), DropTarget::Temporary(_)));
            assert!(matches!(drops[0].point(), DropPoint::CallReturn(_)));
            if operand == "source" {
                assert!(
                    owned
                        .drops()
                        .iter()
                        .all(|fact| sources.slice(fact.value_origin()).unwrap() != "source")
                );
            }
        }
    }
}

#[test]
fn non_null_assertion_copy_from_field_during_shared_loan() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            super::nullable_unit::checked(text, "ownership.ko", rooted, super::environments())
        };

        // Copying the inner Int neither mutates the field nor conflicts with a shared root loan.
        let (_, _, owned) = checked(
            "class Holder(val item: Int?) {}
        fun pair(first: Holder, second: Int): Int = second
        fun test(holder: Holder): Int = pair(holder, holder.item!!)",
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    }
}

#[test]
fn non_null_assertion_plans_bind_success_transfer_and_abort_to_one_evaluation() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            super::nullable_unit::checked(text, "ownership.ko", rooted, super::environments())
        };

        use lang_frontend::{
            ownership_checking::NonNullAssertionTransferKind, type_checking::AssertionFailureEffect,
        };
        let text = "class Resource {}
        fun create(): Resource? = Resource()
        fun copy(source: Int?): Int = source!!
        fun extract(own source: Resource?): Resource = source!!
        fun temporary(): Resource = create()!!";
        let (sources, parsed, owned) = checked(text);
        assert!(owned.diagnostics().is_empty());
        let plans = owned.non_null_assertions();
        assert_eq!(plans.len(), 3);
        for (plan, (operand, kind, place)) in plans.iter().zip([
            ("source", NonNullAssertionTransferKind::Copy, true),
            ("source", NonNullAssertionTransferKind::Consume, true),
            ("create()", NonNullAssertionTransferKind::Consume, false),
        ]) {
            let descriptor = plan.descriptor();
            assert_eq!(
                sources
                    .slice(
                        parsed
                            .ast()
                            .expressions()
                            .get(descriptor.operand().expression())
                            .unwrap()
                            .span()
                    )
                    .unwrap(),
                operand
            );
            assert_eq!(plan.non_null_transfer(), kind);
            assert_eq!(plan.source_place().is_some(), place);
            // The closed Abort effect has neither a transfer operation nor an unwind cleanup list.
            assert_eq!(plan.null_effect(), AssertionFailureEffect::Abort);
            assert_eq!(
                owned.non_null_assertion(descriptor.expression()),
                Some(plan)
            );
        }
        let (_, _, again) = checked(text);
        // Span retains SourceMap owner identity; compare stable map-local identities as above.
        assert_eq!(
            format!("{plans:?}"),
            format!("{:?}", again.non_null_assertions())
        );
    }
}

#[test]
fn non_null_assertion_plans_are_cleared_after_any_ownership_error() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            super::nullable_unit::checked(text, "ownership.ko", rooted, super::environments())
        };

        // A valid earlier extraction cannot leave a partially executable plan after a later error.
        let (_, _, owned) = checked(
            "class Resource {}
        fun valid(source: Int?): Int = source!!
        fun bad(source: Resource?): Resource = source!!",
        );
        assert_eq!(codes(owned.diagnostics()), ["L0133"]);
        assert!(owned.non_null_assertions().is_empty());
    }
}

#[test]
fn non_null_assertion_nested_operand_transfers_without_temporary_drop() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            super::nullable_unit::checked(text, "ownership.ko", rooted, super::environments())
        };

        // Both conditional branches deliver their selected nullable owner to the one assertion.
        let text = "class Resource {}
        fun choose(flag: Boolean, own left: Resource?, own right: Resource?): Resource =
            (if (flag) { left } else { right })!!";
        let (_, _, owned) = checked(text);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert_eq!(owned.non_null_assertions().len(), 1);
        assert!(owned.non_null_assertions()[0].source_place().is_none());
        assert_eq!(
            owned.non_null_assertions()[0].non_null_transfer(),
            lang_frontend::ownership_checking::NonNullAssertionTransferKind::Consume
        );
        assert!(
            owned
                .drops()
                .iter()
                .all(|fact| !matches!(fact.target(), DropTarget::Temporary(_))),
            "{:?}",
            owned.drops()
        );
    }
}

#[test]
fn non_null_assertion_plans_skip_unreachable_and_record_loop_and_closure_bodies() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            super::nullable_unit::checked(text, "ownership.ko", rooted, super::environments())
        };

        // Plans describe executable AST sites, including deferred closure bodies, once per site.
        let (_, _, owned) = checked(
            "fun unreachable(source: Int?): Int { return 0
            val never = source!! }
        fun repeatRead(flag: Boolean, source: Int?): Int { while (flag) { val item = source!! }
            return 0 }
        fun closure(): Int { val callback: (Int?) -> Int = { source -> source!! }
            return callback(1) }",
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert_eq!(owned.non_null_assertions().len(), 2);
        assert!(
            owned
                .non_null_assertions()
                .iter()
                .all(|plan| plan.non_null_transfer()
                    == lang_frontend::ownership_checking::NonNullAssertionTransferKind::Copy)
        );
    }
}

#[test]
fn non_null_assertion_rejects_repeated_loop_consumption() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            super::nullable_unit::checked(text, "ownership.ko", rooted, super::environments())
        };

        // A backedge can reach the same extraction again, even without a later source-level use.
        for body in [
            "while (flag) { val item = source!! }",
            "while (flag) { val item = source!!\ncontinue }",
            "loop { val item = source!! }",
            "for (index in indices) { val item = source!! }",
            "while (check(source!!)) {}",
        ] {
            let text = format!(
                "class Resource {{}}\nfun check(item: Resource): Boolean = true\nfun test(flag: Boolean, indices: Array<Int>, own source: Resource?): Int {{ {body}\nreturn 0 }}"
            );
            let (sources, _, owned) = checked(&text);
            assert_eq!(codes(owned.diagnostics()), ["L0131"], "{body}");
            assert_eq!(
                sources
                    .slice(owned.diagnostics()[0].primary_span())
                    .unwrap(),
                "source"
            );
            assert!(owned.non_null_assertions().is_empty());
        }
    }
}

#[test]
fn non_null_assertion_loop_allows_single_exit_and_replenished_owner() {
    for rooted in [false, true] {
        let checked = |text: &str| {
            super::nullable_unit::checked(text, "ownership.ko", rooted, super::environments())
        };

        // Exit edges never repeat, while initialization restores the owner before its next use.
        for body in [
            "while (flag) { val item = source!!\nbreak }",
            "loop { val item = source!!\nreturn 0 }",
            "while (flag) { val local: Resource? = Resource()\nval item = local!! }",
            "var local = source\nwhile (flag) { val item = local!!\n{ local = Resource() } }",
            "var local = source\nwhile (flag) { { local = Resource() }\nval item = local!! }",
            "var local = source\nloop { { local = Resource() }\nif (flag) { break }\nval item = local!! }\nval result = local!!",
        ] {
            let text = format!(
                "class Resource {{}}\nfun test(flag: Boolean, own source: Resource?): Int {{ {body}\nreturn 0 }}"
            );
            let (_, _, owned) = checked(&text);
            assert!(
                owned.diagnostics().is_empty(),
                "{body}: {:?}",
                owned.diagnostics()
            );
        }
    }
}
