//! Twenty-four cross-axis corners; formation is assigned, never advertised as a product axis.
pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub mutable: bool,
    pub nullable: bool,
    pub binding: usize,
    pub exit: usize,
    pub form: usize,
}
pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        for nullable in [false, true] {
            for exit in 0..3 {
                for continuation in 0..2 {
                    cases.push(source(mutable, nullable, exit, continuation));
                }
            }
        }
    }
    assert_eq!(cases.len(), 24);
    assert_eq!(
        cases
            .iter()
            .map(|c| &c.label)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        24
    );
    for binding in 0..3 {
        assert_eq!(cases.iter().filter(|c| c.binding == binding).count(), 8);
    }
    for mutable in [false, true] {
        for nullable in [false, true] {
            for exit in 0..3 {
                for form in [0, 2] {
                    assert_eq!(
                        cases
                            .iter()
                            .filter(|c| c.mutable == mutable
                                && c.nullable == nullable
                                && c.exit == exit
                                && c.form == form)
                            .count(),
                        1
                    );
                }
            }
        }
    }
    cases
}
fn source(mutable: bool, nullable: bool, exit: usize, continuation: usize) -> Case {
    let binding = (usize::from(mutable) + usize::from(nullable) + exit + continuation) % 3;
    let form = if continuation == 0 { 0 } else { 2 };
    let kind = if mutable { "MutableMap" } else { "Map" };
    let payload = if nullable { "Resource?" } else { "Resource" };
    let factory = if mutable { "mutableMapOf" } else { "mapOf" };
    let conversion = if mutable {
        "return source"
    } else {
        "val result=source.consume();return result"
    };
    let middle = if nullable {
        "source.put(\"n\".clone(),null);"
    } else {
        ""
    };
    let read_value = if nullable {
        "val absent=value==null;if(absent){println(\"none\")}else{println(\"some\")}"
    } else {
        "println(\"value\")"
    };
    let transfer = ["", "continue", "break"][exit];
    let after = if form == 0 {
        "readMap(source);println(\"between\");readMap(source);val count=source.size;println(\"after-last-use\");if(count==0){println(\"zero\")}else{println(\"nonzero\")};println(\"tail\");return 0"
    } else {
        "println(\"second\");for((key,value) in source){val local=Guard();read(key,value)};readMap(source);println(\"after-last-use\");println(\"tail\");return 0"
    };
    let mut scans = String::new();
    for empty in [false, true] {
        let scanner = if empty { "scanEmpty" } else { "scan" };
        let maker = if empty { "makeEmpty" } else { "make" };
        let declaration = match binding {
            0 => format!("val source={maker}()"),
            1 => format!("var source={maker}()"),
            _ => format!("val original={maker}();val source=original"),
        };
        scans.push_str(&format!("fun {scanner}():Int{{{declaration};println(\"formed\");for((key,value) in source){{val local=Guard();read(key,value);{transfer}}};{after}}}\n"));
    }
    let source = format!(
        r#"class Resource(val tag:String){{deinit(){{println(this.tag)}}}}
class Guard{{deinit(){{println("guard-drop")}}}}
fun read(key:String,value:{payload}):Unit{{println(key);{read_value}}}
fun readMap(source:{kind}<String,{payload}>):Unit{{if(source.size==0){{println("borrow-zero")}}else{{println("borrow-nonzero")}}}}
fun make():{kind}<String,{payload}>{{var source=mutableMapOf<String,{payload}>();source.put("z".clone(),Resource("resource-drop".clone()));{middle}source.put("a".clone(),Resource("resource-drop".clone()));{conversion}}}
fun makeEmpty():{kind}<String,{payload}>{{return {factory}<String,{payload}>()}}
{scans}
fun entry():Unit{{println("nonempty");val first=scan();if(first==0){{println("done")}}else{{error("bad return")}};println("empty");val second=scanEmpty();if(second==0){{println("done")}}else{{error("bad empty return")}};println("entry-end")}}
"#
    );
    let entries: &[(&str, &str)] = if nullable {
        &[("z", "some"), ("n", "none"), ("a", "some")]
    } else {
        &[("z", "value"), ("a", "value")]
    };
    let mut golden = String::new();
    for empty in [false, true] {
        golden.push_str(if empty {
            "empty\nformed\n"
        } else {
            "nonempty\nformed\n"
        });
        if !empty {
            for (key, value) in entries
                .iter()
                .take(if exit == 2 { 1 } else { entries.len() })
            {
                golden.push_str(&format!("{key}\n{value}\nguard-drop\n"));
            }
        }
        let borrowed = if empty {
            "borrow-zero\n"
        } else {
            "borrow-nonzero\n"
        };
        if form == 0 {
            golden.push_str(borrowed);
            golden.push_str("between\n");
            golden.push_str(borrowed);
            golden.push_str("after-last-use\n");
            golden.push_str(if empty { "zero\n" } else { "nonzero\n" });
        } else {
            golden.push_str("second\n");
            if !empty {
                for (key, value) in entries {
                    golden.push_str(&format!("{key}\n{value}\nguard-drop\n"));
                }
            }
            golden.push_str(borrowed);
            golden.push_str("after-last-use\n");
        }
        golden.push_str("tail\n");
        if !empty {
            golden.push_str("resource-drop\nresource-drop\n");
        }
        golden.push_str("done\n");
    }
    golden.push_str("entry-end\n");
    Case {
        label: format!(
            "local-resource-future-{kind}-{}-{}-{}-{}",
            if nullable { "nullable" } else { "direct" },
            ["normal", "continue", "break"][exit],
            if form == 0 {
                "calls-size"
            } else {
                "repeat-borrow"
            },
            ["val", "var", "alias"][binding]
        ),
        source,
        golden,
        mutable,
        nullable,
        binding,
        exit,
        form,
    }
}
