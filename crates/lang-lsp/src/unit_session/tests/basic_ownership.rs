//! SPEC-0252：冻结 unit 基础所有权 gate、raw recovery 与候选 snapshot 原子性。
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{OwnershipDeferredReason, check_compilation_unit_ownership},
    parser::parse_file,
    type_checking::{check_compilation_unit_types, standard_environments},
};

use super::*;

const UNUSED_CONSTANT: &str = "\r\nconst val unused: Int = 42";
const DEFERRED: &str = "package app\r\n\
    import lib.pick\r\n\
    class Resource {}\r\n\
    class ResourceHolder(var payload: Resource)\r\n\
    fun deferred(holders: List<ResourceHolder>): Unit {\r\n\
        val projected = /* 界é😀 */ holders[0].payload\r\n\
    }\r\n\
    fun use(): Int = /* 界é😀 */ pick(1)\r\n";

// The full publication/navigation oracle owns a different SourceMap. Re-run the frozen old
// chain on the actual map as well, so full derived Eq compares every raw fact and diagnostic
// without stringifying or discarding SourceId provenance. Never route this oracle through the
// new analysis facade; independent standard-environment pairs stay within their own chain.
pub(super) fn assert_raw_products_match_old_pipeline(actual: &UnitSnapshot) {
    let owner = &actual.name_snapshot;
    let sources = owner.sources();
    let parsed = owner
        .descriptors()
        .iter()
        .map(|unit| {
            let lexed = lex(sources, unit.source_id()).unwrap();
            parse_file(sources, &lexed).unwrap()
        })
        .collect::<Vec<_>>();
    let inputs = owner
        .descriptors()
        .iter()
        .zip(&parsed)
        .map(|(unit, parsed)| {
            SourceUnitInput::new(
                unit.root_identity(),
                unit.logical_path(),
                unit.source_id(),
                parsed,
            )
        })
        .collect::<Vec<_>>();
    let (name_environment, type_environment) = standard_environments();
    let index = index_compilation_unit(sources, &inputs).unwrap();
    let names =
        resolve_compilation_unit_names(sources, &inputs, &index, &name_environment).unwrap();
    let (typed, owned) = if let Ok(names) = names.validate() {
        let typed =
            check_compilation_unit_types(sources, &inputs, &names, &type_environment).unwrap();
        let owned = typed.clone().validate().ok().map(|validated| {
            check_compilation_unit_ownership(
                sources,
                &inputs,
                &names,
                &type_environment,
                &validated,
            )
            .unwrap()
        });
        (Some(typed), owned)
    } else {
        (None, None)
    };
    assert_eq!(actual._typed, typed);
    assert_eq!(actual._owned, owned);
    if let Some(owned) = &actual._owned {
        let typed = actual._typed.as_ref().unwrap();
        let validated = typed.clone().validate().unwrap();
        assert!(validated.types().is_same_analysis(typed));
        assert!(owned.is_compatible_with(&validated));
    }
}

#[test]
fn unused_constant_in_other_source_blocks_whole_unit_ownership_without_l0131() {
    let provider = format!("{PROVIDER}{UNUSED_CONSTANT}");
    for (consumer, baseline_codes) in [(VALID, vec![]), (OWNERSHIP, vec!["L0131"])] {
        for reverse in [false, true] {
            let baseline = UnitSession::new(config(consumer, reverse)).unwrap();
            assert_old_outputs(&baseline.config, &baseline.overlays, &baseline.snapshot);
            assert_eq!(
                codes(&publications(baseline.publications().unwrap())),
                baseline_codes
            );
            assert!(baseline.snapshot._owned.is_some());

            let session =
                UnitSession::new(config_with_provider(&provider, consumer, reverse)).unwrap();
            assert_old_outputs(&session.config, &session.overlays, &session.snapshot);
            let typed = session.snapshot._typed.as_ref().unwrap();
            assert!(typed.diagnostics().is_empty());
            let constants = typed.constants().unwrap();
            assert_eq!(constants.declarations().len(), 1);
            assert!(constants.uses().is_empty());
            assert!(typed.clone().validate().is_err());
            assert!(typed.clone().validate_constants().is_ok());
            assert!(session.snapshot._owned.is_none());
            assert_eq!(
                publications(session.publications().unwrap()),
                [
                    PublishDiagnosticsParams::new(PROVIDER_URI.parse().unwrap(), vec![], None),
                    PublishDiagnosticsParams::new(CONSUMER_URI.parse().unwrap(), vec![], None),
                ]
            );
            if consumer == OWNERSHIP {
                assert_eq!(
                    session
                        .definition_locations(
                            &CONSUMER_URI.parse().unwrap(),
                            position(OWNERSHIP, "take", 2),
                        )
                        .unwrap(),
                    [location(CONSUMER_URI, OWNERSHIP, "take", 0)]
                );
            }
        }
    }
}

#[test]
fn diagnostic_free_deferred_ownership_keeps_raw_product_and_typed_navigation() {
    for reverse in [false, true] {
        let session = UnitSession::new(config(DEFERRED, reverse)).unwrap();
        assert_old_outputs(&session.config, &session.overlays, &session.snapshot);
        let typed = session.snapshot._typed.as_ref().unwrap();
        assert!(typed.diagnostics().is_empty());
        assert!(typed.clone().validate().is_ok());
        let owned = session.snapshot._owned.as_ref().unwrap();
        assert!(owned.diagnostics().is_empty());
        assert_eq!(owned.deferred().len(), 1);
        assert_eq!(
            owned.deferred()[0].reason(),
            OwnershipDeferredReason::IndexPlace
        );
        assert!(owned.clone().validate().is_err());
        assert!(owned.conditional_receiver_deliveries().is_empty());
        assert!(codes(&publications(session.publications().unwrap())).is_empty());
        let uri = CONSUMER_URI.parse().unwrap();
        for (needle, occurrence, target_uri, target_text, target, target_occurrence) in [
            ("pick", 1, PROVIDER_URI, PROVIDER, "pick", 0),
            ("payload", 1, CONSUMER_URI, DEFERRED, "payload", 0),
            ("holders", 1, CONSUMER_URI, DEFERRED, "holders", 0),
        ] {
            assert_eq!(
                session
                    .definition_locations(&uri, position(DEFERRED, needle, occurrence))
                    .unwrap(),
                [location(target_uri, target_text, target, target_occurrence)]
            );
        }
    }
}

#[test]
fn dropping_basic_recovery_candidates_preserves_complete_last_good_and_version() {
    let const_and_move = OWNERSHIP.replacen(
        "package app\r\n",
        &format!("package app{UNUSED_CONSTANT}\r\n"),
        1,
    );
    let uri = CONSUMER_URI.parse().unwrap();
    for stable in [VALID, OWNERSHIP, const_and_move.as_str(), DEFERRED] {
        let mut session = UnitSession::new(config(VALID, true)).unwrap();
        let opened = session.prepare_open(&uri, 5, stable.to_owned()).unwrap();
        assert_old_outputs(&session.config, &opened.overlays, &opened.snapshot);
        session.commit(opened);
        let previous = publications(session.publications().unwrap());
        let previous_typed = session.snapshot._typed.clone();
        let previous_owned = session.snapshot._owned.clone();
        let assert_unchanged = |session: &UnitSession| {
            assert_eq!(publications(session.publications().unwrap()), previous);
            assert_eq!(session.snapshot._typed, previous_typed);
            assert_eq!(session.snapshot._owned, previous_owned);
            assert!(
                session
                    .snapshot
                    ._typed
                    .as_ref()
                    .unwrap()
                    .is_same_analysis(previous_typed.as_ref().unwrap())
            );
            if let Some(owned) = &session.snapshot._owned {
                assert!(owned.is_same_analysis(previous_owned.as_ref().unwrap()));
            }
            assert_eq!(session.overlays.len(), 1);
            assert_eq!(session.overlays[uri.as_str()].text, stable);
            assert_eq!(session.overlays[uri.as_str()].version, 5);
            assert_old_outputs(&session.config, &session.overlays, &session.snapshot);
        };

        for candidate in [const_and_move.as_str(), DEFERRED, OWNERSHIP, TYPED_RECOVERY] {
            let update = session
                .prepare_change(&uri, 6, candidate.to_owned())
                .unwrap();
            assert_old_outputs(&session.config, &update.overlays, &update.snapshot);
            assert_eq!(update.publications()[1].version, Some(6));
            drop(update);
            assert_unchanged(&session);
        }
        let closed = session.prepare_close(&uri).unwrap();
        assert_old_outputs(&session.config, &closed.overlays, &closed.snapshot);
        assert_eq!(closed.publications()[1].version, None);
        drop(closed);
        assert_unchanged(&session);

        // A discarded candidate has not consumed v6, including after a dropped close.
        let retry = session
            .prepare_change(&uri, 6, DEFERRED.to_owned())
            .unwrap();
        session.commit(retry);
        assert_eq!(session.publications().unwrap()[1].version, Some(6));
        assert_eq!(session.overlays[uri.as_str()].text, DEFERRED);
        assert_old_outputs(&session.config, &session.overlays, &session.snapshot);
        assert!(
            !session
                .snapshot
                ._owned
                .as_ref()
                .unwrap()
                .deferred()
                .is_empty()
        );
    }
}
