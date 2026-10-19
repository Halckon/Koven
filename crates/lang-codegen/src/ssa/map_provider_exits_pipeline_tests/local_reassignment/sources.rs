//! Distinct real producers and payloads expose stale roots and replacement cleanup ordering.
pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub mutable: bool,
    pub payload: usize,
    pub history: usize,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        for payload in 0..3 {
            for history in 0..3 {
                cases.push(source(mutable, payload, history));
            }
        }
    }
    assert_eq!(cases.len(), 18);
    assert_eq!(
        cases
            .iter()
            .map(|case| &case.label)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        18
    );
    cases
}

fn source(mutable: bool, payload: usize, history: usize) -> Case {
    let kind = if mutable { "MutableMap" } else { "Map" };
    let value = ["String", "Resource", "Resource?"][payload];
    let conversion = if mutable {
        "return source"
    } else {
        "val result=source.consume();return result"
    };
    let mut factories = String::new();
    for (name, tag, empty) in [
        ("make", "old", false),
        ("other", "new", false),
        ("makeEmpty", "old", true),
        ("otherEmpty", "new", true),
    ] {
        let item = if payload == 0 {
            format!("\"{tag}-value\".clone()")
        } else {
            format!("Resource(\"{tag}-drop\".clone())")
        };
        let entries = if empty {
            String::new()
        } else {
            let mut entries = format!("source.put(\"{tag}-key\".clone(),{item});");
            if payload == 2 {
                entries.push_str(&format!("source.put(\"{tag}-null\".clone(),null);"));
            }
            entries
        };
        factories.push_str(&format!("fun {name}():{kind}<String,{value}>{{println(\"{name}-start\");var source=mutableMapOf<String,{value}>();{entries}println(\"{name}-ready\");{conversion}}}\n"));
    }
    let read_value = match payload {
        0 => "println(value)",
        1 => "println(\"resource-borrow\")",
        _ => "val absent=value==null;if(absent){println(\"none\")}else{println(\"some\")}",
    };
    let mut scanners = String::new();
    for empty in [false, true] {
        let scan = if empty { "scanEmpty" } else { "scan" };
        let make = if empty { "makeEmpty" } else { "make" };
        let other = if empty { "otherEmpty" } else { "other" };
        let first = if history == 1 {
            "for((key,value) in source){val local=Guard();read(key,value)};println(\"between\");"
        } else {
            ""
        };
        let alias = if history == 2 {
            "val alias=source;println(\"moved\");"
        } else {
            ""
        };
        let root = if history == 2 { "alias" } else { "source" };
        scanners.push_str(&format!("fun {scan}():Unit{{var source={make}();println(\"formed\");{first}source={other}();println(\"replaced\");{alias}for((key,value) in {root}){{val local=Guard();read(key,value)}};println(\"tail\")}}\n"));
    }
    let source = format!(
        r#"class Resource(val tag:String){{deinit(){{println(this.tag)}}}}
class Guard{{deinit(){{println("guard-drop")}}}}
fun read(key:String,value:{value}):Unit{{println(key);{read_value}}}
{factories}
{scanners}
fun entry():Unit{{println("nonempty");scan();println("empty");scanEmpty();println("entry-end")}}
"#
    );
    let mut golden = String::new();
    for empty in [false, true] {
        let make = if empty { "makeEmpty" } else { "make" };
        let other = if empty { "otherEmpty" } else { "other" };
        golden.push_str(if empty { "empty\n" } else { "nonempty\n" });
        golden.push_str(&format!("{make}-start\n{make}-ready\nformed\n"));
        if history == 1 {
            if !empty {
                iteration(&mut golden, "old", payload);
            }
            golden.push_str("between\n");
        }
        golden.push_str(&format!("{other}-start\n{other}-ready\n"));
        if !empty && payload != 0 {
            golden.push_str("old-drop\n");
        }
        golden.push_str("replaced\n");
        if history == 2 {
            golden.push_str("moved\n");
        }
        if !empty {
            iteration(&mut golden, "new", payload);
        }
        golden.push_str("tail\n");
        if !empty && payload != 0 {
            golden.push_str("new-drop\n");
        }
    }
    golden.push_str("entry-end\n");
    Case {
        label: format!(
            "local-reassignment-{kind}-{}-{}",
            ["pure", "resource-field", "nullable-field"][payload],
            ["before-for", "between-fors", "then-move"][history]
        ),
        source,
        golden,
        mutable,
        payload,
        history,
    }
}

fn iteration(golden: &mut String, tag: &str, payload: usize) {
    golden.push_str(&format!("{tag}-key\n"));
    golden.push_str(match payload {
        0 => {
            if tag == "old" {
                "old-value\n"
            } else {
                "new-value\n"
            }
        }
        1 => "resource-borrow\n",
        _ => "some\n",
    });
    golden.push_str("guard-drop\n");
    if payload == 2 {
        golden.push_str(&format!("{tag}-null\nnone\nguard-drop\n"));
    }
}
