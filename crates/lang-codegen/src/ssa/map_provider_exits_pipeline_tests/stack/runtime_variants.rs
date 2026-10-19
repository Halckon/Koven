//! Real nonempty/empty neighbors for temporary, nullable and three-frame providers.
//! Goldens describe source execution and scope cleanup, never production proof objects.
use super::{
    Case,
    runtime::{OUTER, maker},
};

const TEMPORARY: [(&str, &str); 3] = [
    ("inner-a", "inner-first"),
    ("inner-stop", "inner-middle"),
    ("inner-c", "inner-tail"),
];
const NULLABLE: [(&str, &str); 3] = [
    ("outer-a", "some"),
    ("outer-stop", "none"),
    ("outer-c", "some"),
];

#[derive(Clone, Copy)]
enum Form {
    Temporary,
    Nullable,
    Depth3,
}
#[derive(Clone, Copy)]
enum Exit {
    Normal,
    Break,
    Return,
}

impl Form {
    fn label(self) -> &'static str {
        match self {
            Self::Temporary => "temporary",
            Self::Nullable => "nullable",
            Self::Depth3 => "depth3",
        }
    }
    fn payload(self) -> &'static str {
        match self {
            Self::Nullable => "R?",
            Self::Temporary | Self::Depth3 => "String",
        }
    }
    fn pairs(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Nullable => &NULLABLE,
            Self::Temporary | Self::Depth3 => &OUTER,
        }
    }
}
impl Exit {
    fn label(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Break => "innerConditionalBreak",
            Self::Return => "innerConditionalReturn",
        }
    }
    fn transfer(self, form: Form) -> String {
        let (key, value) = if matches!(form, Form::Depth3) {
            ("a", "b")
        } else {
            ("k", "v")
        };
        let stop = match form {
            Form::Temporary => "inner-stop",
            Form::Nullable => "outer-stop",
            Form::Depth3 if matches!(self, Self::Break) => "outer-a",
            Form::Depth3 => "outer-b",
        };
        match self {
            Self::Normal => String::new(),
            Self::Break => format!("if({key}==\"{stop}\"){{break}};"),
            Self::Return => format!("if({key}==\"{stop}\"){{return operand({key},{value})}};"),
        }
    }
}

fn nullable_maker(name: &str, kind: &str) -> String {
    let marker = if name == "makeOuter" {
        "outer-factory"
    } else {
        "inner-factory"
    };
    let finish = if kind == "Map" {
        "val frozen=result.consume();return frozen"
    } else {
        "return result"
    };
    format!(
        "fun {name}(own filled:Boolean):{kind}<String,R?>{{println(\"{marker}\");var result=mutableMapOf<String,R?>();if(filled){{result.put(\"outer-a\".clone(),R(\"first\".clone()));result.put(\"outer-stop\".clone(),null);result.put(\"outer-c\".clone(),R(\"last\".clone()))}};{finish}}}\n"
    )
}

fn scanner(kind: &str, form: Form, exit: Exit, independent: bool) -> String {
    let payload = form.payload();
    let name = if independent {
        "scanEmptyInner"
    } else {
        "scan"
    };
    let parameters = if matches!(form, Form::Temporary) {
        ",own filled:Boolean".to_owned()
    } else if independent {
        format!(",other:{kind}<String,{payload}>")
    } else {
        String::new()
    };
    let inner = if matches!(form, Form::Temporary) {
        "makeInner(filled)"
    } else if independent {
        "other"
    } else {
        "source"
    };
    let transfer = exit.transfer(form);
    let (outer_read, inner_read) = if matches!(form, Form::Temporary) {
        ("readOuter", "readInner")
    } else {
        ("read", "read")
    };
    let body = if matches!(form, Form::Depth3) {
        format!(
            "for((key,value) in source){{val outerGuard=OuterGuard();read(key,value);for((k,v) in source){{val middleGuard=MiddleGuard();read(k,v);for((a,b) in {inner}){{val innerGuard=InnerGuard();read(a,b);{transfer}}};println(\"inner-done\");read(k,v)}};println(\"middle-done\");read(key,value)}}"
        )
    } else {
        format!(
            "for((key,value) in source){{val outerGuard=OuterGuard();{outer_read}(key,value);for((k,v) in {inner}){{val innerGuard=InnerGuard();{inner_read}(k,v);{transfer}}};println(\"inner-done\");{outer_read}(key,value)}}"
        )
    };
    format!("fun {name}(source:{kind}<String,{payload}>{parameters}):Int{{{body};return 0}}\n")
}

fn prefix(kind: &str, form: Form, exit: Exit) -> String {
    let mut source = String::from(
        "class OuterGuard{deinit(){println(\"outer-drop\")}}\nclass MiddleGuard{deinit(){println(\"middle-drop\")}}\nclass InnerGuard{deinit(){println(\"inner-drop\")}}\n",
    );
    if !matches!(form, Form::Depth3) {
        source.push_str("class R(val name:String){deinit(){println(\"resource-drop\")}}\n");
    }
    match form {
        Form::Temporary => {
            source.push_str("fun readOuter(key:String,value:String):Unit{println(key);println(value)}\nfun readInner(key:String,value:R):Unit{println(key);println(value.name)}\nfun operand(key:String,value:R):Int{println(\"operand\");readInner(key,value);return 7}\n");
            source.push_str(&maker("makeOuter", kind, false, &OUTER));
            source.push_str(&maker("makeInner", kind, true, &TEMPORARY));
        }
        Form::Nullable => {
            source.push_str("fun read(key:String,value:R?):Unit{println(key);val absent=value==null;if(absent){println(\"none\")}else{println(\"some\")}}\nfun operand(key:String,value:R?):Int{println(\"operand\");read(key,value);return 7}\n");
            source.push_str(&nullable_maker("makeOuter", kind));
            source.push_str(&nullable_maker("makeInner", kind));
        }
        Form::Depth3 => {
            source.push_str("fun read(key:String,value:String):Unit{println(key);println(value)}\nfun operand(key:String,value:String):Int{println(\"operand\");read(key,value);return 7}\n");
            source.push_str(&maker("makeOuter", kind, false, &OUTER));
            source.push_str(&maker("makeInner", kind, false, &[]));
        }
    }
    source.push_str(&scanner(kind, form, exit, false));
    if !matches!(form, Form::Temporary) {
        source.push_str(&scanner(kind, form, exit, true));
    }
    let payload = form.payload();
    let read = if matches!(form, Form::Temporary) {
        "readOuter"
    } else {
        "read"
    };
    source.push_str(&format!("fun readBack(source:{kind}<String,{payload}>):Unit{{for((key,value) in source){{{read}(key,value)}}}}\n"));
    source
}

fn pair(golden: &mut String, value: (&str, &str)) {
    golden.push_str(value.0);
    golden.push('\n');
    golden.push_str(value.1);
    golden.push('\n');
}

fn innermost(golden: &mut String, form: Form, exit: Exit, inner: &[(&str, &str)]) -> bool {
    for (index, &value) in inner.iter().enumerate() {
        pair(golden, value);
        let stops = index
            == if matches!(form, Form::Depth3) && matches!(exit, Exit::Break) {
                0
            } else {
                1
            };
        if stops && matches!(exit, Exit::Return) {
            golden.push_str("operand\n");
            pair(golden, value);
            golden.push_str("inner-drop\n");
            if matches!(form, Form::Temporary) {
                golden.push_str(&"resource-drop\n".repeat(inner.len()));
            }
            return true;
        }
        golden.push_str("inner-drop\n");
        if stops && matches!(exit, Exit::Break) {
            break;
        }
    }
    // A break destroys the entire temporary owner, including its unread tail.
    if matches!(form, Form::Temporary) {
        golden.push_str(&"resource-drop\n".repeat(inner.len()));
    }
    false
}

fn scan_golden(golden: &mut String, form: Form, exit: Exit, phase: usize) -> bool {
    let outer = if phase == 2 { &[] } else { form.pairs() };
    let inner = if phase == 1 {
        &[]
    } else if matches!(form, Form::Temporary) {
        TEMPORARY.as_slice()
    } else {
        outer
    };
    for &value in outer {
        pair(golden, value);
        if matches!(form, Form::Depth3) {
            for &middle in outer {
                pair(golden, middle);
                if innermost(golden, form, exit, inner) {
                    golden.push_str("middle-drop\nouter-drop\n");
                    return true;
                }
                golden.push_str("inner-done\n");
                pair(golden, middle);
                golden.push_str("middle-drop\n");
            }
            golden.push_str("middle-done\n");
        } else {
            if matches!(form, Form::Temporary) {
                golden.push_str("inner-factory\n");
            }
            if innermost(golden, form, exit, inner) {
                golden.push_str("outer-drop\n");
                return true;
            }
            golden.push_str("inner-done\n");
        }
        pair(golden, value);
        golden.push_str("outer-drop\n");
    }
    false
}

fn phase(kind: &str, form: Form, exit: Exit, phase: usize) -> (String, String) {
    let tag = ["nonempty", "empty-inner", "empty-outer"][phase];
    let filled = phase != 2;
    let binding = if kind == "MutableMap" { "var" } else { "val" };
    let mut code = format!(
        "fun phase{phase}():Unit{{println(\"{tag}\");{binding} source=makeOuter({filled});"
    );
    let mut golden = format!("{tag}\nouter-factory\n");
    let call = if matches!(form, Form::Temporary) {
        format!("scan(source,{})", phase != 1)
    } else if phase == 1 {
        code.push_str("val emptyInner=makeInner(false);");
        golden.push_str("inner-factory\n");
        "scanEmptyInner(source,emptyInner)".to_owned()
    } else {
        "scan(source)".to_owned()
    };
    let returned = scan_golden(&mut golden, form, exit, phase);
    let (expected, marker) = if returned {
        (7, "returned")
    } else {
        (0, "zero")
    };
    code.push_str(&format!("val outcome={call};if(outcome=={expected}){{println(\"{marker}\")}}else{{error(\"wrong-variant-result\")}};println(\"after-scan\");"));
    golden.push_str(&format!("{marker}\nafter-scan\n"));
    let values = if filled { form.pairs() } else { &[] };
    code.push_str(&format!(
        "if(source.size=={}){{println(\"size-ok\")}}else{{error(\"wrong-variant-size\")}};",
        values.len()
    ));
    golden.push_str("size-ok\n");
    if kind == "MutableMap" {
        let payload = if matches!(form, Form::Nullable) {
            "null"
        } else {
            "\"caller-added\".clone()"
        };
        code.push_str(&format!("source.put(\"caller-post\".clone(),{payload});if(source.size=={}){{println(\"mutation-ok\")}}else{{error(\"wrong-variant-mutation\")}};", values.len()+1));
        golden.push_str("mutation-ok\n");
    }
    code.push_str("readBack(source);println(\"post-read\");");
    for &value in values {
        pair(&mut golden, value);
    }
    if kind == "MutableMap" {
        pair(
            &mut golden,
            (
                "caller-post",
                if matches!(form, Form::Nullable) {
                    "none"
                } else {
                    "caller-added"
                },
            ),
        );
    }
    golden.push_str("post-read\n");
    if phase == 1 && !matches!(form, Form::Temporary) {
        code.push_str("if(emptyInner.size==0){println(\"empty-inner-kept\")}else{error(\"wrong-empty-inner\")};");
        golden.push_str("empty-inner-kept\n");
    }
    code.push_str("println(\"phase-end\")}\n");
    golden.push_str("phase-end\n");
    // Borrowed nullable payloads remain caller-owned through all reads and mutation.
    // Identical deinit markers count the two Some owners without imposing their order.
    if matches!(form, Form::Nullable) && filled {
        golden.push_str("resource-drop\nresource-drop\n");
    }
    golden.push_str(&format!("after-{tag}\n"));
    (code, golden)
}

pub(crate) fn sources() -> Vec<Case> {
    let mut cases = Vec::new();
    for kind in ["Map", "MutableMap"] {
        for form in [Form::Temporary, Form::Nullable, Form::Depth3] {
            for exit in [Exit::Normal, Exit::Break, Exit::Return] {
                let mut source = prefix(kind, form, exit);
                let mut golden = String::new();
                let mut entry = String::new();
                for index in 0..3 {
                    let (body, expected) = phase(kind, form, exit, index);
                    source.push_str(&body);
                    golden.push_str(&expected);
                    let tag = ["nonempty", "empty-inner", "empty-outer"][index];
                    entry.push_str(&format!("phase{index}();println(\"after-{tag}\");"));
                }
                source.push_str(&format!(
                    "fun entry():Unit{{{entry}println(\"entry-end\")}}\n"
                ));
                golden.push_str("entry-end\n");
                cases.push(Case {
                    label: format!("g8-{kind}-{}-{}", form.label(), exit.label()),
                    source,
                    golden,
                });
            }
        }
    }
    assert_eq!(cases.len(), 18);
    cases
}
