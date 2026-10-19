//! Resource continue retains provider/source and lexical owner across every backedge.
use super::continuation::{Case, sources as continuations};
pub(crate) fn sources() -> Vec<Case> {
    let originals = continuations();
    let mut cases = vec![Case {
        label: "original-continue",
        owned: true,
        source: include_str!(
            "../../../../tests/fixtures/map-provider-resource-0331/boundary-continue.ko"
        )
        .to_owned(),
        golden: include_str!(
            "../../../../tests/fixtures/map-provider-resource-0331/map-resource.stdout"
        )
        .to_owned(),
    }];
    for (index, guard, label) in [
        (1, false, "continue-last-use"),
        (4, false, "continue-later-use"),
        (1, true, "guard-continue-last-use"),
        (4, true, "guard-continue-later-use"),
    ] {
        let original = &originals[index];
        let body = if guard {
            "val local = Guard(); read(key, value); continue"
        } else {
            "read(key, value); continue"
        };
        let mut source = original.source.replace("read(key, value)", body);
        let golden = if guard {
            source = "class Guard { deinit() { println(\"local-drop\") } }\n".to_owned() + &source;
            original
                .golden
                .replace("value-borrow\n", "value-borrow\nlocal-drop\n")
        } else {
            original.golden.clone()
        };
        cases.push(Case {
            label,
            owned: true,
            source,
            golden,
        });
    }
    for mutable in [false, true] {
        let container = if mutable { "MutableMap" } else { "Map" };
        let conversion = if mutable {
            "return source"
        } else {
            "val result=source.consume(); return result"
        };
        let mutation = if mutable {
            "populated.put(\"later\".clone(),Resource());"
        } else {
            ""
        };
        let size = if mutable { 3 } else { 2 };
        let source = format!(
            r#"
class Resource {{ deinit() {{ println("resource-drop") }} }}
fun read(key:String,value:Resource):Unit {{ println(key); println("value-borrow") }}
fun makeMap():{container}<String,Resource> {{ var source=mutableMapOf<String,Resource>(); source.put("z".clone(),Resource()); source.put("a".clone(),Resource()); {conversion} }}
fun scan(source:{container}<String,Resource>):Unit {{ for((key,value) in source){{read(key,value);continue}};println("exhausted") }}
fun entry():Unit {{ println("nonempty"); var populated=makeMap(); scan(populated); println("after-scan"); {mutation}
 if(populated.size=={size}){{println("post-size")}}else{{error("bad size")}};
 println("empty");val empty={factory}<String,Resource>();scan(empty);println("entry-end") }}
"#,
            factory = if mutable { "mutableMapOf" } else { "mapOf" }
        );
        let drops = "resource-drop\n".repeat(size as usize);
        cases.push(Case{label:if mutable {"borrow-mutable-continue"}else{"borrow-map-continue"},owned:false,source,golden:format!("nonempty\nz\nvalue-borrow\na\nvalue-borrow\nexhausted\nafter-scan\npost-size\nempty\nexhausted\nentry-end\n{drops}")});
    }
    cases
}
#[test]
fn resource_continue_reaches_both_verified_pipelines() {
    let results = sources()
        .iter()
        .flat_map(|case| {
            [false, true].map(|unit| (case.label, unit, super::continuation::observe(case, unit)))
        })
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 14);
    assert!(results.iter().all(|(_, _, r)| r.is_ok()), "{results:#?}");
}
