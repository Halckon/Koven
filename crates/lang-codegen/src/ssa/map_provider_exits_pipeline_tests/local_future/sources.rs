//! Fixed source matrix; each case executes a populated and a truly empty local producer.
pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
    pub mutable: bool,
    pub binding: usize,
    pub exit: usize,
    pub form: usize,
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for mutable in [false, true] {
        for binding in 0..3 {
            for exit in 0..3 {
                for form in 0..3 {
                    cases.push(source(mutable, binding, exit, form));
                }
            }
        }
    }
    assert_eq!(cases.len(), 54);
    assert_eq!(
        cases
            .iter()
            .map(|c| &c.label)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        54
    );
    cases
}

fn source(mutable: bool, binding: usize, exit: usize, form: usize) -> Case {
    let kind = if mutable { "MutableMap" } else { "Map" };
    let factory = if mutable { "mutableMapOf" } else { "mapOf" };
    let conversion = if mutable {
        "return source"
    } else {
        "val result=source.consume();return result"
    };
    let transfer = ["", "continue", "break"][exit];
    let continuation = match form {
        0 => {
            "readMap(source);println(\"between\");readMap(source);val count=source.size;println(\"after-last-use\");if(count==0){println(\"zero\")}else{println(\"nonzero\")}"
        }
        1 => {
            "if(source.size==0){println(\"zero\")}else{println(\"nonzero\")};println(\"between\");readMap(source);println(\"after-last-use\")"
        }
        _ => {
            "println(\"second\");for((key,value) in source){val local=Guard();read(key,value)};readMap(source);println(\"after-last-use\")"
        }
    };
    let mut scanners = String::new();
    for empty in [false, true] {
        let scan = if empty { "scanEmpty" } else { "scan" };
        let producer = if empty { "makeEmpty" } else { "make" };
        let declaration = match binding {
            0 => format!("val source={producer}()"),
            1 => format!("var source={producer}()"),
            _ => format!("val original={producer}();val source=original"),
        };
        scanners.push_str(&format!("fun {scan}():Unit{{{declaration};println(\"formed\");for((key,value) in source){{val local=Guard();read(key,value);{transfer}}};{continuation};println(\"tail\")}}\n"));
    }
    let source = format!(
        r#"class Guard{{deinit(){{println("guard-drop")}}}}
fun read(key:String,value:String):Unit{{println(key);println(value)}}
fun readMap(source:{kind}<String,String>):Unit{{if(source.size==0){{println("borrow-zero")}}else{{println("borrow-nonzero")}}}}
fun make():{kind}<String,String>{{var source=mutableMapOf<String,String>();source.put("z".clone(),"first".clone());source.put("a".clone(),"second".clone());{conversion}}}
fun makeEmpty():{kind}<String,String>{{return {factory}<String,String>()}}
{scanners}
fun entry():Unit{{println("nonempty");scan();println("empty");scanEmpty();println("entry-end")}}
"#
    );
    let mut golden = String::new();
    for empty in [false, true] {
        golden.push_str(if empty {
            "empty\nformed\n"
        } else {
            "nonempty\nformed\n"
        });
        if !empty {
            golden.push_str("z\nfirst\nguard-drop\n");
            if exit != 2 {
                golden.push_str("a\nsecond\nguard-drop\n");
            }
        }
        let borrowed = if empty {
            "borrow-zero\n"
        } else {
            "borrow-nonzero\n"
        };
        let size = if empty { "zero\n" } else { "nonzero\n" };
        match form {
            0 => {
                golden.push_str(borrowed);
                golden.push_str("between\n");
                golden.push_str(borrowed);
                golden.push_str("after-last-use\n");
                golden.push_str(size);
            }
            1 => {
                golden.push_str(size);
                golden.push_str("between\n");
                golden.push_str(borrowed);
                golden.push_str("after-last-use\n");
            }
            _ => {
                golden.push_str("second\n");
                if !empty {
                    golden.push_str("z\nfirst\nguard-drop\na\nsecond\nguard-drop\n");
                }
                golden.push_str(borrowed);
                golden.push_str("after-last-use\n");
            }
        }
        golden.push_str("tail\n");
    }
    golden.push_str("entry-end\n");
    Case {
        label: format!(
            "local-future-{kind}-{}-{}-{}",
            ["val", "var", "moved-alias"][binding],
            ["normal", "continue", "break"][exit],
            ["calls-size", "cfg-borrow", "repeat-borrow"][form]
        ),
        source,
        golden,
        mutable,
        binding,
        exit,
        form,
    }
}
