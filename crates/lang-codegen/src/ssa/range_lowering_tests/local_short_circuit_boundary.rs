//! Legal source at the still-unsupported boundaries, never mutated facts or IR.
use super::*;
use crate::ssa::LoweringErrorKind;

fn scoped(declaration: &str, setup: &str, statement: &str) -> String {
    format!(
        r#"
{declaration}
fun read(view:View<String>):Unit{{}}
fun show(flag:Boolean):Unit{{}}
fun scan(flag:Boolean):Unit{{val root=listOf("a");{setup};{{borrow val part=take(root,1);{statement};read(part)}}}}
fun main():Unit{{scan(true)}}
"#
    )
}
fn reject(source: &str, fragment: &str) {
    for pipeline in [
        Pipeline::Single,
        Pipeline::Unit,
        Pipeline::UnitReversed,
        Pipeline::UnitConstant,
        Pipeline::UnitConstantReversed,
    ] {
        let lowered = lower(source, fragment, pipeline);
        let error = lowered
            .result
            .err()
            .expect("this unselected combination must retain the gate");
        assert_eq!(
            error.kind,
            LoweringErrorKind::UnsupportedNode,
            "{pipeline:?}: {error:?}"
        );
        assert_eq!(
            error.span,
            Some(lowered.expected_span),
            "{pipeline:?}: {error:?}"
        );
    }
}

#[test]
fn range_short_circuit_boundary_nested_and_hidden_cfg() {
    for (expression, fragment) in [
        ("flag && (flag || flag)", "flag || flag"),
        ("(flag && flag) || flag", "flag && flag"),
        ("flag && values[0]", "flag && values[0]"),
        ("flag || nullable!!", "flag || nullable!!"),
        ("flag && (n+1>0)", "flag && (n+1>0)"),
        ("flag && (n>-1)", "flag && (n>-1)"),
        (
            "flag && (if(flag){true}else{false})",
            "if(flag){true}else{false}",
        ),
    ] {
        reject(
            &scoped(
                "",
                "val n=1;val values=listOf(true);val nullable:Boolean?=true",
                &format!("show({expression})"),
            ),
            fragment,
        );
    }
}

#[test]
fn range_short_circuit_boundary_owning_and_inout_calls() {
    reject(
        &scoped(
            "fun takeText(own text:String):Boolean{return true}",
            "val text=\"owned\"",
            "show(flag && takeText(text))",
        ),
        "flag && takeText(text)",
    );
    reject(
        &scoped(
            "fun change(inout n:Int):Boolean{n=2;return true}",
            "var n=1",
            "show(flag || change(&n))",
        ),
        "flag || change(&n)",
    );
}

#[test]
fn range_short_circuit_boundary_range_creation_and_aliases() {
    reject(
        &scoped(
            "fun test(view:View<String>):Boolean{return true}",
            "",
            "show(flag && test(take(part,1)))",
        ),
        "flag && test(take(part,1))",
    );
    for binding in [
        "borrow val alias=part;read(alias)",
        "val text=\"kept\";borrow val alias=text;println(alias)",
    ] {
        reject(
            &scoped("", "", &format!("{binding};show(flag && flag)")),
            "flag && flag",
        );
    }
}

#[test]
fn range_short_circuit_boundary_scope_and_roots() {
    for body in [
        "show(flag && flag);{borrow val part=take(root,1);read(part)}",
        "{borrow val part=take(root,1);read(part)};show(flag && flag)",
        "{borrow val part=take(root,1);show(flag && flag);borrow val child=take(part,1);read(child);read(part)}",
        "{borrow val part=take(root,1);borrow val second=take(other,1);show(flag && flag);read(part);read(second)}",
    ] {
        let source = format!(
            "fun show(flag:Boolean):Unit{{}}\nfun read(view:View<String>):Unit{{}}\nfun scan(flag:Boolean):Unit{{val root=listOf(\"a\");val other=listOf(\"b\");{body}}}\nfun main():Unit{{scan(true)}}"
        );
        reject(&source, "flag && flag");
    }
}

#[test]
fn range_short_circuit_boundary_loop_lambda_and_control_exit() {
    for (statement, fragment) in [
        ("for(item in part){show(flag && flag)}", "flag && flag"),
        ("val callback:()->Boolean={->flag && flag}", "flag && flag"),
        ("show(flag && (if(flag){return}else{true}))", "return"),
    ] {
        reject(&scoped("", "", statement), fragment);
    }
}

#[test]
fn range_short_circuit_boundary_receiver_effects() {
    for (declaration, setup, expression) in [
        (
            "class Gate(var n:Int){inout fun test():Boolean{n=2;return true}}",
            "var gate=Gate(1)",
            "flag && gate.test()",
        ),
        (
            "value class Gate(var n:Int){inout fun test():Boolean{n=2;return true}}",
            "var gate=Gate(1)",
            "flag && gate.test()",
        ),
        (
            "value class Gate(val text:String){own fun test():Boolean{return true}}",
            "val gate=Gate(\"owned\")",
            "flag || gate.test()",
        ),
        (
            "class Gate(val text:String){own fun test():Boolean{return true}}",
            "val gate=Gate(\"owned\")",
            "flag || gate.test()",
        ),
    ] {
        reject(
            &scoped(declaration, setup, &format!("show({expression})")),
            expression,
        );
    }
}

// These invalid forms stop in frontend, and must not be counted as backend gates.
#[test]
fn range_short_circuit_frontend_nothing_and_borrow_result_diagnostics() {
    use crate::native_tests::boxed_enum_tests::unit_source::{SourceRejectionKind, UnitSource};
    for (declaration, expression, fragment, code) in [
        ("", "flag || error(\"stop\")", "||", "L0085"),
        (
            "fun view(source:Boolean):borrow Boolean from source=source",
            "flag && view(flag)",
            "view(flag)",
            "L0164",
        ),
    ] {
        let consumer = scoped(declaration, "", &format!("show({expression})"));
        let text = format!(
            "{}\n{consumer}",
            include_str!("../../../../lang-std/koven/algorithms/ranges.ko")
                .strip_prefix("package koven.algorithms\n")
                .unwrap()
        );
        let mut authorized_source = None;
        let rejection = UnitSource::analyze_with_type_environment(
            "0317-frontier.ko",
            &text,
            |sources, source, types| {
                types.authorize_range_source(sources, source).unwrap();
                types
                    .authorize_range_extension_source(sources, source)
                    .unwrap();
                authorized_source = Some(source);
            },
        )
        .err()
        .expect("the original source must retain its frontend diagnostic");
        assert!(matches!(rejection.kind, SourceRejectionKind::Diagnostics));
        let expected_stage = if code == "L0085" {
            assert_eq!(
                rejection.report.stages,
                [("parse", 0), ("names", 0), ("types", 1)]
            );
            "types"
        } else {
            assert_eq!(
                rejection.report.stages,
                [("parse", 0), ("names", 0), ("types", 0), ("ownership", 1)]
            );
            "ownership"
        };
        let [(stage, diagnostic)] = rejection.report.diagnostics.as_slice() else {
            panic!("one exact original diagnostic: {:?}", rejection.report);
        };
        assert_eq!(*stage, expected_stage);
        assert_eq!(diagnostic.code().to_string(), code);
        let span = diagnostic.primary_span();
        assert_eq!(Some(span.source_id()), authorized_source);
        assert_eq!(
            rejection.sources.source_name(span.source_id()).unwrap(),
            "0317-frontier.ko"
        );
        assert_eq!(
            rejection.sources.source_text(span.source_id()).unwrap(),
            text
        );
        let start = text.rfind(fragment).unwrap();
        assert_eq!((span.start(), span.end()), (start, start + fragment.len()));
        assert_eq!(rejection.sources.slice(span).unwrap(), fragment);
    }
}

#[test]
fn range_short_circuit_boundary_outer_call_effects_and_new_metadata_prefix() {
    for (declaration, setup, statement) in [
        (
            "fun deliver(own text:String,result:Boolean):Unit{}",
            "val text=\"owned\"",
            "deliver(text,flag && flag)",
        ),
        (
            "fun deliver(inout n:Int,result:Boolean):Unit{}",
            "var n=1",
            "deliver(&n,flag && flag)",
        ),
        (
            "fun deliver(view:View<String>,result:Boolean):Unit{}",
            "",
            "deliver(take(part,1),flag && flag)",
        ),
    ] {
        reject(&scoped(declaration, setup, statement), "flag && flag");
    }
}
