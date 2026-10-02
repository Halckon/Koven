//! SPEC-0251：真实 session 与冻结旧全链的可观察行为差分。
use lsp_types::{NumberOrString, PublishDiagnosticsParams, Range};
use serde_json::json;

use super::*;

mod manual;

const PROVIDER_URI: &str = "untitled:/z-provider.ko";
const CONSUMER_URI: &str = "file:///a-consumer.ko";
const PROVIDER: &str = "package lib\r\n/* 界é😀 */ public class Holder(val field: Int) { fun read(): Int = this.field }\r\npublic fun pick(item: Int): Int = item\r\npublic fun pick(item: String): Int = 2";
const VALID: &str = "package app\r\nimport lib.Holder as H\r\nimport lib.pick\r\nfun use(holder: H): Int = /* 界é😀 */ pick(1)";
const TYPED_RECOVERY: &str = "package app\r\nimport lib.Holder\r\nimport lib.pick\r\nfun failed(): Int = pick(true)\r\nfun use(holder: Holder): Int = holder.read() + holder.field + /* 界é😀 */ pick(1)";
const NAME_RECOVERY: &str = "package app\r\nimport lib.pick\r\n// 界é😀\r\n#\r\nfun valid(): Int = pick(1)\r\nval first = absent\r\nval duplicate = 1\r\nval duplicate = 2\r\nfun typeBad(): Int = true\r\nclass Resource()\r\nfun take(own x: Resource): Unit {}\r\nfun moveBad(own x: Resource): Unit { take(x)\r\ntake(x) }\r\nval syntax =\r\n";
const OWNERSHIP: &str = "package app\r\nclass Resource()\r\nfun take(own resource: Resource): Unit {}\r\nfun moved(own resource: Resource): Unit {\r\nval first = take(resource)\r\nval second = take(resource)\r\n}";
const CONSTANT: &str = "package app\r\nconst val answer: Int = 42\r\nfun use(): Int = answer";

fn config(consumer: &str, reverse: bool) -> SourceSetConfig {
    let mut roots = vec!["a-provider", "z-consumer"];
    let mut sources = vec![
        json!({"root": "a-provider", "logicalPath": "lib/api.ko", "uri": PROVIDER_URI, "text": PROVIDER}),
        json!({"root": "z-consumer", "logicalPath": "app/use.ko", "uri": CONSUMER_URI, "text": consumer}),
    ];
    if reverse {
        roots.reverse();
        sources.reverse();
    }
    SourceSetConfig::from_initialization_options(Some(&json!({"koven": {"sourceSet": {
        "schema": "koven.lsp.source-set", "version": 1, "roots": roots, "sources": sources
    }}})))
    .unwrap()
    .unwrap()
}

fn publications(items: Vec<UnitPublication>) -> Vec<PublishDiagnosticsParams> {
    items
        .into_iter()
        .map(|item| PublishDiagnosticsParams::new(item.uri, item.diagnostics, item.version))
        .collect()
}

fn position(text: &str, needle: &str, occurrence: usize) -> Position {
    let offset = text.match_indices(needle).nth(occurrence).unwrap().0;
    let prefix = &text[..offset];
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    Position::new(
        u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count()).unwrap(),
        u32::try_from(text[line_start..offset].encode_utf16().count()).unwrap(),
    )
}

fn location(uri: &str, text: &str, needle: &str, occurrence: usize) -> Location {
    let start = position(text, needle, occurrence);
    Location::new(
        uri.parse().unwrap(),
        Range::new(
            start,
            Position::new(
                start.line,
                start.character + u32::try_from(needle.encode_utf16().count()).unwrap(),
            ),
        ),
    )
}

fn codes(items: &[PublishDiagnosticsParams]) -> Vec<&str> {
    items
        .iter()
        .flat_map(|item| &item.diagnostics)
        .map(|diagnostic| {
            let Some(NumberOrString::String(code)) = &diagnostic.code else {
                panic!("diagnostic code")
            };
            code.as_str()
        })
        .collect()
}

// Compare every UTF-16 position, not only hand-picked successful definitions. This includes
// declarations, imports, aliases, failed candidates, package segments, CRLF/EOF and null targets.
fn assert_old_outputs(
    config: &SourceSetConfig,
    overlays: &BTreeMap<String, Overlay>,
    actual: &UnitSnapshot,
) {
    let expected = manual::ManualSnapshot::analyze(config, overlays).unwrap();
    assert_eq!(
        publications(actual.publications(config, overlays).unwrap()),
        publications(expected.publications(config, overlays).unwrap())
    );
    assert_eq!(actual._typed.is_some(), expected._typed.is_some());
    assert_eq!(actual._owned.is_some(), expected._owned.is_some());
    for source in config.sources() {
        let text = overlays
            .get(source.uri().as_str())
            .map_or(source.text(), |overlay| overlay.text.as_str());
        for (line, content) in text.split('\n').enumerate() {
            for character in 0..=content.encode_utf16().count() + 1 {
                let at = Position::new(
                    u32::try_from(line).unwrap(),
                    u32::try_from(character).unwrap(),
                );
                let actual = actual.definition_locations(config, source.uri(), at);
                let expected = expected.definition_locations(config, source.uri(), at);
                match (actual, expected) {
                    (Ok(actual), Ok(expected)) => {
                        assert_eq!(actual, expected, "{} {at:?}", source.uri().as_str())
                    }
                    (Err(actual), Err(expected)) => {
                        assert_eq!(actual.to_string(), expected.to_string())
                    }
                    (actual, expected) => {
                        panic!("definition differs at {at:?}: {actual:?} vs {expected:?}")
                    }
                }
            }
        }
    }
}

#[test]
fn unit_snapshot_matches_old_pipeline_outputs_for_every_recovery_gate() {
    for (consumer, typed, owned, expected_codes) in [
        (VALID, true, true, vec![]),
        (
            NAME_RECOVERY,
            false,
            false,
            vec!["L0001", "L0080", "L0079", "L0009"],
        ),
        (TYPED_RECOVERY, true, false, vec!["L0123"]),
        (OWNERSHIP, true, true, vec!["L0131"]),
        (CONSTANT, true, false, vec![]),
    ] {
        let mut first = None;
        for reverse in [false, true] {
            let config = config(consumer, reverse);
            let overlays = BTreeMap::new();
            let actual = UnitSnapshot::analyze(&config, &overlays).unwrap();
            assert_old_outputs(&config, &overlays, &actual);
            assert_eq!(actual._typed.is_some(), typed);
            assert_eq!(actual._owned.is_some(), owned);
            let published = publications(actual.publications(&config, &overlays).unwrap());
            assert_eq!(codes(&published), expected_codes, "{consumer}");
            assert_eq!(
                published
                    .iter()
                    .map(|item| item.uri.as_str())
                    .collect::<Vec<_>>(),
                [PROVIDER_URI, CONSUMER_URI]
            );
            if let Some(first) = &first {
                assert_eq!(&published, first);
            } else {
                first = Some(published);
            }
        }
    }
}

#[test]
fn empty_unit_matches_old_pipeline_without_publications_or_targets() {
    let config =
        SourceSetConfig::from_initialization_options(Some(&json!({"koven": {"sourceSet": {
            "schema": "koven.lsp.source-set", "version": 1, "roots": ["unused"], "sources": []
        }}})))
        .unwrap()
        .unwrap();
    let session = UnitSession::new(config).unwrap();
    assert_old_outputs(&session.config, &session.overlays, &session.snapshot);
    assert!(session.publications().unwrap().is_empty());
    assert!(
        session
            .definition_locations(&PROVIDER_URI.parse().unwrap(), Position::new(0, 0))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn typed_recovery_session_keeps_narrowed_calls_members_and_failed_candidates() {
    let session = UnitSession::new(config(TYPED_RECOVERY, true)).unwrap();
    let uri = CONSUMER_URI.parse().unwrap();
    for (needle, occurrence, target, target_occurrence) in [
        ("pick", 2, "pick", 0),
        ("read", 0, "read", 0),
        ("field", 0, "field", 0),
    ] {
        assert_eq!(
            session
                .definition_locations(&uri, position(TYPED_RECOVERY, needle, occurrence))
                .unwrap(),
            [location(PROVIDER_URI, PROVIDER, target, target_occurrence)]
        );
    }
    assert_eq!(
        session
            .definition_locations(&uri, position(TYPED_RECOVERY, "pick", 1))
            .unwrap(),
        [
            location(PROVIDER_URI, PROVIDER, "pick", 0),
            location(PROVIDER_URI, PROVIDER, "pick", 1)
        ]
    );
    assert!(session.snapshot._typed.is_some());
    assert!(session.snapshot._owned.is_none());
}

#[test]
fn name_recovery_commits_complete_diagnostics_and_valid_navigation() {
    let mut session = UnitSession::new(config(VALID, false)).unwrap();
    let uri = CONSUMER_URI.parse().unwrap();
    let update = session
        .prepare_open(&uri, 9, NAME_RECOVERY.to_owned())
        .unwrap();
    assert_old_outputs(&session.config, &update.overlays, &update.snapshot);
    session.commit(update);
    let published = publications(session.publications().unwrap());
    assert_eq!(published[1].version, Some(9));
    assert_eq!(codes(&published), ["L0001", "L0080", "L0079", "L0009"]);
    let diagnostic = &published[1].diagnostics;
    assert_eq!(
        diagnostic[0].range,
        location(CONSUMER_URI, NAME_RECOVERY, "#", 0).range
    );
    assert_eq!(
        diagnostic[1].range,
        location(CONSUMER_URI, NAME_RECOVERY, "absent", 0).range
    );
    assert_eq!(
        diagnostic[2].range,
        location(CONSUMER_URI, NAME_RECOVERY, "duplicate", 1).range
    );
    assert!(
        !diagnostic[2]
            .related_information
            .as_ref()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        session
            .definition_locations(&uri, position(NAME_RECOVERY, "pick", 1))
            .unwrap(),
        [
            location(PROVIDER_URI, PROVIDER, "pick", 0),
            location(PROVIDER_URI, PROVIDER, "pick", 1)
        ]
    );
    assert!(session.snapshot._typed.is_none());
    assert!(session.snapshot._owned.is_none());
}

#[test]
fn dropping_prepared_update_preserves_complete_last_good_state() {
    let mut session = UnitSession::new(config(VALID, false)).unwrap();
    let uri = CONSUMER_URI.parse().unwrap();
    let opened = session.prepare_open(&uri, 5, VALID.to_owned()).unwrap();
    session.commit(opened);
    let previous = publications(session.publications().unwrap());
    let target = session
        .definition_locations(&uri, position(VALID, "pick", 1))
        .unwrap();
    let update = session
        .prepare_change(&uri, 6, NAME_RECOVERY.to_owned())
        .unwrap();
    assert_old_outputs(&session.config, &update.overlays, &update.snapshot);
    assert_ne!(
        publications(
            update
                .snapshot
                .publications(&session.config, &update.overlays)
                .unwrap()
        ),
        previous
    );
    drop(update);
    assert_eq!(publications(session.publications().unwrap()), previous);
    assert_eq!(
        session
            .definition_locations(&uri, position(VALID, "pick", 1))
            .unwrap(),
        target
    );
    let retry = session
        .prepare_change(&uri, 6, TYPED_RECOVERY.to_owned())
        .unwrap();
    session.commit(retry);
    assert_eq!(session.publications().unwrap()[1].version, Some(6));
}

mod owner_contract;
