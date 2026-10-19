//! New source derivations leave every already accepted temporary/ordinary input untouched.
use super::super::{temporary_argument_cfg_runtime, temporary_delivery};

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub mutable: bool,
    pub resource: bool,
    pub unit_only: bool,
    pub direct: bool,
    pub route: bool,
    pub delegated: bool,
}
fn named(text: &str) -> String {
    let prefix = "for((key,value) in ";
    let start = text.find(prefix).unwrap() + prefix.len();
    let end = text[start..]
        .find("){read(key,value)}")
        .map(|i| start + i)
        .unwrap();
    let call = &text[start..end];
    text.replacen(
        &format!("{prefix}{call})"),
        &format!("val source={call};for((key,value) in source)"),
        1,
    )
}

pub(crate) fn sources() -> Vec<Case> {
    let mut out = super::temporary_arguments::sources()
        .into_iter()
        .map(|case| {
            // These are the exact two first-red source/golden bytes.
            let old = "for((key,value) in produce(argument()))";
            assert_eq!(case.source.matches(old).count(), 1);
            Case {
                label: case
                    .label
                    .replacen("temporary-argument", "local-argument", 1),
                source: case.source.replace(
                    old,
                    "val source=produce(argument());for((key,value) in source)",
                ),
                golden: case.golden,
                mutable: case.mutable,
                resource: false,
                unit_only: false,
                direct: false,
                route: false,
                delegated: false,
            }
        })
        .collect::<Vec<_>>();
    for case in temporary_delivery::sources() {
        let resource = !matches!(case.delivery, temporary_delivery::Delivery::SameMapBorrow);
        let golden = if resource {
            let old = "deinit\nowned-drop\nafter\n";
            assert!(case.golden.ends_with(old));
            case.golden.strip_suffix(old).unwrap().to_owned() + "after\ndeinit\nowned-drop\n"
        } else {
            case.golden
        };
        out.push(Case {
            label: case
                .label
                .replacen("temporary-delivery", "local-delivery", 1),
            source: named(&case.source),
            golden,
            mutable: case.mutable,
            resource,
            unit_only: false,
            direct: false,
            route: false,
            delegated: false,
        });
    }
    for case in temporary_argument_cfg_runtime::sources()
        .into_iter()
        .filter(|c| c.label.ends_with("control-false"))
    {
        let mutable = case.label.contains("MutableMap");
        out.push(Case {
            label: case
                .label
                .replacen("temporary-argument", "local-argument", 1),
            source: named(&case.source),
            golden: case.golden,
            mutable,
            resource: false,
            unit_only: false,
            direct: false,
            route: false,
            delegated: false,
        });
    }
    // Use the original ordinary-route declarations; the selected nonzero-arg
    // callable itself supplies the current owner, without an intervening make().
    for (label, source, _) in super::super::local_ordinary_route_sources()
        .into_iter()
        .filter(|(l, _, _)| l.ends_with("previous-epoch"))
    {
        let prefix = source.split("fun entry():Unit").next().unwrap();
        let route = label
            .strip_prefix("ordinary-")
            .unwrap()
            .strip_suffix("-previous-epoch")
            .unwrap();
        let callee = if route == "delegation" {
            "Host(Factory()).other"
        } else {
            "Factory().other"
        };
        for direct in [false, true] {
            let provider = if direct {
                format!("for((key,value) in {callee}(1))")
            } else {
                format!("val source={callee}(1);for((key,value) in source)")
            };
            let drop = "other-drop\n";
            let (before, after) = if direct { (drop, "") } else { ("", drop) };
            out.push(Case {
                label: format!("local-argument-route-{route}-{}", if direct { "direct-temporary" } else { "current-owner" }),
                source: format!("{prefix}fun entry():Unit{{println(\"begin\");{provider}{{read(key,value)}};println(\"after\")}}\n"),
                golden: format!("begin\n{route}-call\nother-start\nother-ready\nother-key\nresource-borrow\n{before}after\n{after}"),
                mutable: false, resource: true, unit_only: true, direct, route: true, delegated: route == "delegation",
            });
        }
    }
    assert_eq!(out.len(), 18);
    out
}
