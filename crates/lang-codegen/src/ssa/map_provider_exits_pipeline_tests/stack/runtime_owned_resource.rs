//! Owned Resource sources retain the same-root owner across the inner provider.
//! The nonempty and empty-outer paths use one root for both providers. The separate
//! empty-inner neighbor borrows an independent empty root while consuming the outer.
use super::{
    Case,
    runtime::{OUTER, maker, result, scan_golden},
};

fn scanner(kind: &str, returned: bool, independent_empty: bool) -> String {
    let (name, parameter, inner) = if independent_empty {
        (
            "scanEmptyInner",
            format!(",other:{kind}<String,R>"),
            "other",
        )
    } else {
        ("scan", String::new(), "source")
    };
    let transfer = if returned {
        "if(k==\"outer-b\"){return operand(k,v)};"
    } else {
        ""
    };
    format!(
        "fun {name}(own source:{kind}<String,R>{parameter}):Int{{for((key,value) in source){{val outerGuard=OuterGuard();read(key,value);for((k,v) in {inner}){{val innerGuard=InnerGuard();read(k,v);{transfer}}};println(\"inner-done\");read(key,value)}};return 0}}\n"
    )
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for kind in ["Map", "MutableMap"] {
        for returned in [false, true] {
            let mut source = String::from(
                "class OuterGuard{deinit(){println(\"outer-drop\")}}\nclass InnerGuard{deinit(){println(\"inner-drop\")}}\nclass R(val name:String){deinit(){println(\"resource-drop\")}}\nfun read(key:String,value:R):Unit{println(key);println(value.name)}\nfun operand(key:String,value:R):Int{println(\"operand\");read(key,value);return 7}\n",
            );
            source.push_str(&maker("makeOuter", kind, true, &OUTER));
            source.push_str(&maker("makeInner", kind, true, &[]));
            source.push_str(&scanner(kind, returned, false));
            source.push_str(&scanner(kind, returned, true));
            let mut entry = String::new();
            let mut golden = String::new();
            for phase in 0..3 {
                let (tag, filled) = match phase {
                    0 => ("nonempty", true),
                    1 => ("empty-inner", true),
                    _ => ("empty-outer", false),
                };
                entry.push_str(&format!(
                    "println(\"{tag}\");val outer{phase}=makeOuter({filled});"
                ));
                golden.push_str(&format!("{tag}\nouter-factory\n"));
                let call = if phase == 1 {
                    entry.push_str("val emptyInner=makeInner(false);");
                    golden.push_str("inner-factory\n");
                    format!("scanEmptyInner(outer{phase},emptyInner)")
                } else {
                    format!("scan(outer{phase})")
                };
                let outer = if filled { OUTER.as_slice() } else { &[] };
                let inner = if phase == 1 { &[] } else { outer };
                let did_return = scan_golden(&mut golden, outer, inner, returned);
                // The own argument is destroyed by scan after its Guard cleanup and
                // operand evaluation, before the Call result reaches this caller.
                // Identical markers require both payloads exactly once without
                // assigning an order between the two Resource destructors.
                golden.push_str(&"resource-drop\n".repeat(outer.len()));
                result(
                    &mut entry,
                    &mut golden,
                    &call,
                    &phase.to_string(),
                    did_return,
                );
                entry.push_str(&format!(
                    "println(\"after-{tag}\");println(\"owned-delivered\");"
                ));
                golden.push_str(&format!("after-{tag}\nowned-delivered\n"));
                if phase == 1 {
                    entry.push_str("if(emptyInner.size==0){println(\"empty-inner-kept\")}else{error(\"wrong-empty-inner\")};");
                    golden.push_str("empty-inner-kept\n");
                }
            }
            entry.push_str("println(\"entry-end\")");
            golden.push_str("entry-end\n");
            source.push_str(&format!("fun entry():Unit{{{entry}}}\n"));
            cases.push(Case {
                label: format!(
                    "g8-{kind}-ownResourceSameRoot-{}",
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
    assert_eq!(cases.len(), 4);
    cases
}
