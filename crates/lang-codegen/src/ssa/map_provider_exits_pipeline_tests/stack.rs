//! G8 source-only nested-provider contracts, with every real observation collected.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod facts;
mod query;
mod runtime {
    //! Nonempty and independently empty execution paths, without relying on entry destruction order.
    use super::Case;

    pub(super) const OUTER: [(&str, &str); 2] =
        [("outer-a", "outer-first"), ("outer-b", "outer-last")];
    const INNER: [(&str, &str); 2] = [("inner-a", "inner-first"), ("inner-stop", "inner-last")];

    pub(super) fn maker(name: &str, kind: &str, resource: bool, pairs: &[(&str, &str)]) -> String {
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

    pub(super) fn scan_golden(
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

    pub(super) fn result(
        code: &mut String,
        golden: &mut String,
        call: &str,
        suffix: &str,
        returned: bool,
    ) {
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
        resource: bool,
        insert: bool,
        extra: (&str, &str),
    ) {
        size(code, golden, root, pairs.len());
        if insert {
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
        if insert {
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
                                resource,
                                mutable && phase == 0,
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
                                resource,
                                mutable && (phase == 0 || (mode == 1 && phase == 1)),
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
}
mod runtime_owned_resource {
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
}
mod runtime_variants {
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
}

use super::{
    Report,
    fixture::{Failure, clean, failure, verify_failure},
    pipeline,
};
use lang_frontend::{
    lexer::lex,
    parser::{Statement, parse_file},
    source::SourceMap,
};

pub(crate) use runtime::sources as runtime_sources;
pub(crate) use runtime_owned_resource::sources as owned_resource_sources;
pub(crate) use runtime_variants::sources as variant_sources;

pub(crate) struct Case {
    pub label: String,
    pub source: String,
    pub golden: String,
}

fn originals(group: usize) -> Vec<(&'static str, &'static str)> {
    match group {
        0 => vec![
            (
                "different-root",
                include_str!(
                    "../../../tests/fixtures/map-provider-stack-0331/g8-nested-different-root.ko"
                ),
            ),
            (
                "own",
                include_str!("../../../tests/fixtures/map-provider-stack-0331/g8-nested-own.ko"),
            ),
            (
                "resource",
                include_str!(
                    "../../../tests/fixtures/map-provider-stack-0331/g8-nested-resource.ko"
                ),
            ),
        ],
        1 => vec![
            (
                "temporary",
                include_str!(
                    "../../../tests/fixtures/map-provider-stack-0331/g8-nested-temporary.ko"
                ),
            ),
            (
                "nullable-resource",
                include_str!(
                    "../../../tests/fixtures/map-provider-stack-0331/g8-nested-nullable-resource.ko"
                ),
            ),
        ],
        2 => vec![
            (
                "depth-three",
                include_str!("../../../tests/fixtures/map-provider-stack-0331/g8-depth-three.ko"),
            ),
            (
                "inner-conditional",
                include_str!(
                    "../../../tests/fixtures/map-provider-stack-0331/g8-inner-conditional.ko"
                ),
            ),
        ],
        _ => unreachable!("closed test group"),
    }
}

fn observe(label: &str, text: &str, unit: bool) -> Result<(), Failure> {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("provider.ko", text)
        .map_err(|e| failure("source", e))?;
    let lexed = lex(&sources, source).map_err(|e| failure("lex", e))?;
    clean(&sources, "lex", lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
    clean(&sources, "parse", parsed.diagnostics())?;
    let count = parsed
        .ast()
        .statements()
        .iter()
        .filter(|(_, n)| matches!(n.payload(), Statement::For { .. }))
        .count();
    let mut report = Report::default();
    let mut inputs = None;
    let lowered = pipeline::unit_input_observed(
        &sources,
        &parsed,
        "provider.ko",
        &mut report,
        unit,
        |names, typed, owned, report| {
            let observed = facts::unit(&sources, &parsed, names, typed, owned);
            report.query("stack original qualified FE bridge", &observed);
            inputs = Some(observed);
            report.check(
                "stack all original For plans",
                owned.map_iterations().len() == count,
                owned.map_iterations(),
            );
        },
    );
    diagnostic!(
        "G8 STACK LOWER case={label} rooted={unit} report={report:#?} result={:?}",
        lowered.as_ref().map(|(_, entry)| entry)
    );
    let (program, _) = lowered?;
    let queried = inputs
        .as_ref()
        .ok_or_else(|| failure("stack original callback input", "missing"))
        .and_then(|input| {
            input
                .as_ref()
                .map_err(|e| failure("stack original FE bridge", e))
        })
        .and_then(|input| query::check(&sources, &parsed, &program, input, true));
    diagnostic!("G8 STACK QUERY case={label} rooted={unit} result={queried:?}");
    report.query("stack independent complete identities", &queried);
    crate::ssa::verify::verify_program(&program).map_err(|e| verify_failure(&sources, e))?;
    let llvm = crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
    diagnostic!(
        "G8 STACK SSA case={label} rooted={unit}\n{}",
        crate::ssa::render::render_program(&program)
    );
    diagnostic!("G8 STACK LLVM case={label} rooted={unit}\n{llvm}");
    if !report.passed() {
        return Err(failure("stack FE/SSA identities", report));
    }
    Ok(())
}

fn group(index: usize, expected: usize) {
    let mut observations = Vec::new();
    for (label, source) in originals(index) {
        for unit in [false, true] {
            diagnostic!("G8 STACK INPUT case={label} rooted={unit} source={source:?}");
            let result = observe(label, source, unit);
            diagnostic!("G8 STACK CASE case={label} rooted={unit} result={result:?}");
            observations.push(result);
        }
    }
    assert_eq!(observations.len(), expected);
    assert_diagnostic!(observations.iter().all(Result::is_ok), "{observations:#?}");
}

#[test]
fn different_root_owned_and_resource_stack_reach_verified_llvm() {
    group(0, 6);
}

#[test]
fn temporary_and_nullable_stack_reach_verified_llvm() {
    group(1, 4);
}

#[test]
fn depth_three_and_inner_conditional_stack_reach_verified_llvm() {
    group(2, 4);
}

#[test]
fn populated_empty_inner_and_empty_outer_stack_reach_both_pipelines() {
    runtime_group(runtime_sources(), 12);
}

#[test]
fn owned_resource_same_root_preserves_source_and_cleanup_identity() {
    runtime_group(owned_resource_sources(), 4);
}

#[test]
fn temporary_nullable_and_three_layers_preserve_each_exit() {
    runtime_group(variant_sources(), 18);
}

fn runtime_group(cases: Vec<Case>, count: usize) {
    assert_eq!(cases.len(), count);
    let mut observations = Vec::new();
    for case in cases {
        for unit in [false, true] {
            diagnostic!(
                "G8 STACK RUNTIME INPUT case={} rooted={unit} source={:?} golden={:?}",
                case.label,
                case.source,
                case.golden
            );
            let result = observe(&case.label, &case.source, unit);
            diagnostic!(
                "G8 STACK RUNTIME CASE case={} rooted={unit} result={result:?}",
                case.label
            );
            observations.push(result);
        }
    }
    assert_eq!(observations.len(), count * 2);
    assert_diagnostic!(observations.iter().all(Result::is_ok), "{observations:#?}");
}
