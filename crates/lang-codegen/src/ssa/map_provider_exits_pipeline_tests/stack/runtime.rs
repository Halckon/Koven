//! Nonempty and independently empty execution paths, without relying on entry destruction order.
use super::Case;

const OUTER: [(&str, &str); 2] = [("outer-a", "outer-first"), ("outer-b", "outer-last")];
const INNER: [(&str, &str); 2] = [("inner-a", "inner-first"), ("inner-stop", "inner-last")];

fn maker(name: &str, kind: &str, resource: bool, pairs: &[(&str, &str)]) -> String {
    let value = if resource { "R" } else { "String" };
    let mut puts = String::new();
    for &(key, text) in pairs {
        let payload = if resource {
            format!("R(\"{text}\".clone())")
        } else {
            format!("\"{text}\".clone()")
        };
        puts.push_str(&format!("result.put(\"{key}\".clone(),{payload});"));
    }
    let finish = if kind == "Map" {
        "val frozen=result.consume();return frozen"
    } else {
        "return result"
    };
    let marker = if name == "makeOuter" {
        "outer-factory"
    } else {
        "inner-factory"
    };
    format!(
        "fun {name}(own filled:Boolean):{kind}<String,{value}>{{println(\"{marker}\");var result=mutableMapOf<String,{value}>();if(filled){{{puts}}};{finish}}}\n"
    )
}

fn scanner(name: &str, kind: &str, mode: usize, returned: bool, independent: bool) -> String {
    let value = if mode == 2 { "R" } else { "String" };
    let ownership = if mode == 1 { "own " } else { "" };
    let other = if mode == 0 || independent {
        format!(",other:{kind}<String,{value}>")
    } else {
        String::new()
    };
    let inner = if mode == 0 || independent {
        "other"
    } else {
        "source"
    };
    let stop = if independent || mode == 0 {
        "inner-stop"
    } else {
        "outer-b"
    };
    let transfer = if returned {
        format!("if(k==\"{stop}\"){{return operand(k,v)}};")
    } else {
        String::new()
    };
    format!(
        "fun {name}({ownership}source:{kind}<String,{value}>{other}):Int{{for((key,value) in source){{val outerGuard=OuterGuard();read(key,value);for((k,v) in {inner}){{val innerGuard=InnerGuard();read(k,v);{transfer}}};println(\"inner-done\");read(key,value)}};return 0}}\n"
    )
}

fn read_pair(golden: &mut String, pair: (&str, &str)) {
    golden.push_str(pair.0);
    golden.push('\n');
    golden.push_str(pair.1);
    golden.push('\n');
}

fn scan_golden(
    golden: &mut String,
    outer: &[(&str, &str)],
    inner: &[(&str, &str)],
    returned: bool,
) -> bool {
    for &pair in outer {
        read_pair(golden, pair);
        for (index, &inside) in inner.iter().enumerate() {
            read_pair(golden, inside);
            if returned && index == 1 {
                golden.push_str("operand\n");
                read_pair(golden, inside);
                golden.push_str("inner-drop\nouter-drop\n");
                return true;
            }
            golden.push_str("inner-drop\n");
        }
        golden.push_str("inner-done\n");
        read_pair(golden, pair);
        golden.push_str("outer-drop\n");
    }
    false
}

fn result(code: &mut String, golden: &mut String, call: &str, suffix: &str, returned: bool) {
    let (expected, marker) = if returned {
        (7, "returned")
    } else {
        (0, "zero")
    };
    code.push_str(&format!("val result{suffix}={call};if(result{suffix}=={expected}){{println(\"{marker}\")}}else{{error(\"wrong-stack-result\")}};"));
    golden.push_str(marker);
    golden.push('\n');
}

fn size(code: &mut String, golden: &mut String, root: &str, expected: usize) {
    code.push_str(&format!(
        "if({root}.size=={expected}){{println(\"size-ok\")}}else{{error(\"wrong-caller-size\")}};"
    ));
    golden.push_str("size-ok\n");
}

fn after(
    code: &mut String,
    golden: &mut String,
    root: &str,
    pairs: &[(&str, &str)],
    mutable: bool,
    resource: bool,
    insert: bool,
    extra: (&str, &str),
) {
    size(code, golden, root, pairs.len());
    if mutable && insert {
        let value = if resource {
            format!("R(\"{}\".clone())", extra.1)
        } else {
            format!("\"{}\".clone()", extra.1)
        };
        code.push_str(&format!("{root}.put(\"{}\".clone(),{value});", extra.0));
        size(code, golden, root, pairs.len() + 1);
    }
    code.push_str(&format!("readBack({root});println(\"post-read\");"));
    for &pair in pairs {
        read_pair(golden, pair);
    }
    if mutable && insert {
        read_pair(golden, extra);
    }
    golden.push_str("post-read\n");
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        let kind = if mutable { "MutableMap" } else { "Map" };
        let binding = if mutable { "var" } else { "val" };
        for mode in 0..3 {
            let resource = mode == 2;
            let value = if resource { "R" } else { "String" };
            let read = if resource {
                "println(value.name)"
            } else {
                "println(value)"
            };
            for returned in [false, true] {
                let mut source = String::from(
                    "class OuterGuard{deinit(){println(\"outer-drop\")}}\nclass InnerGuard{deinit(){println(\"inner-drop\")}}\n",
                );
                if resource {
                    source.push_str(
                        "class R(val name:String){deinit(){println(\"resource-drop\")}}\n",
                    );
                }
                source.push_str(&format!("fun read(key:String,value:{value}):Unit{{println(key);{read}}}\nfun operand(key:String,value:{value}):Int{{println(\"operand\");read(key,value);return 7}}\n"));
                source.push_str(&maker("makeOuter", kind, resource, &OUTER));
                source.push_str(&maker("makeInner", kind, resource, &INNER));
                source.push_str(&scanner("scan", kind, mode, returned, false));
                source.push_str(&scanner("scanEmptyInner", kind, mode, returned, true));
                source.push_str(&format!("fun readBack(input:{kind}<String,{value}>):Unit{{for((backKey,backValue) in input){{read(backKey,backValue)}}}}\n"));
                let mut entry = String::new();
                let mut golden = String::new();
                for phase in 0..3 {
                    let (tag, filled) = match phase {
                        0 => ("nonempty", true),
                        1 => ("empty-inner", true),
                        _ => ("empty-outer", false),
                    };
                    entry.push_str(&format!(
                        "println(\"{tag}\");{binding} outer{phase}=makeOuter({filled});"
                    ));
                    golden.push_str(&format!("{tag}\nouter-factory\n"));
                    if mode == 0 || phase == 1 {
                        entry.push_str(&format!(
                            "{binding} inner{phase}=makeInner({});",
                            phase != 1
                        ));
                        golden.push_str("inner-factory\n");
                    }
                    let call = if phase == 1 {
                        format!("scanEmptyInner(outer{phase},inner{phase})")
                    } else if mode == 0 {
                        format!("scan(outer{phase},inner{phase})")
                    } else {
                        format!("scan(outer{phase})")
                    };
                    let outer = if filled { OUTER.as_slice() } else { &[] };
                    let inner = if phase == 1 {
                        &[]
                    } else if mode == 0 {
                        INNER.as_slice()
                    } else {
                        OUTER.as_slice()
                    };
                    let did_return = scan_golden(&mut golden, outer, inner, returned);
                    result(
                        &mut entry,
                        &mut golden,
                        &call,
                        &phase.to_string(),
                        did_return,
                    );
                    entry.push_str(&format!("println(\"after-{tag}\");"));
                    golden.push_str(&format!("after-{tag}\n"));
                    if mode == 1 {
                        entry.push_str("println(\"owned-delivered\");");
                        golden.push_str("owned-delivered\n");
                    } else {
                        after(
                            &mut entry,
                            &mut golden,
                            &format!("outer{phase}"),
                            outer,
                            mutable,
                            resource,
                            phase == 0,
                            ("outer-post", "outer-added"),
                        );
                    }
                    if mode == 0 || phase == 1 {
                        let pairs = if phase == 1 { &[] } else { INNER.as_slice() };
                        after(
                            &mut entry,
                            &mut golden,
                            &format!("inner{phase}"),
                            pairs,
                            mutable,
                            resource,
                            phase == 0 || (mode == 1 && phase == 1),
                            ("inner-post", "inner-added"),
                        );
                    }
                }
                entry.push_str("println(\"entry-end\")");
                golden.push_str("entry-end\n");
                if resource {
                    golden.push_str(&"resource-drop\n".repeat(if mutable { 5 } else { 4 }));
                }
                source.push_str(&format!("fun entry():Unit{{{entry}}}\n"));
                cases.push(Case {
                    label: format!(
                        "g8-{kind}-{}-{}",
                        [
                            "differentBorrowRoots",
                            "ownSameRoot",
                            "BorrowResourceSameRoot"
                        ][mode],
                        if returned {
                            "conditionalReturn"
                        } else {
                            "normal"
                        }
                    ),
                    source,
                    golden,
                });
            }
        }
    }
    assert_eq!(cases.len(), 12);
    cases
}
