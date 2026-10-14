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
        (
            "flag && (if(flag){true}else{false})",
            "if(flag){true}else{false}",
        ),
    ] {
        reject(
            &scoped(
                "",
                "val values=listOf(true);val nullable:Boolean?=true",
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
    use lang_frontend::{
        name_resolution::resolve_names, ownership_checking::check_ownership,
        type_checking::check_types,
    };
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
        let mut sources = SourceMap::new();
        let (source, parsed) =
            crate::ssa::unit_lower_test_support::parsed(&mut sources, "0317-frontier.ko", &text);
        let (environment, mut types) = standard_environments();
        types.authorize_range_source(&sources, source).unwrap();
        types
            .authorize_range_extension_source(&sources, source)
            .unwrap();
        let names = resolve_names(&sources, &parsed, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let typed = check_types(&sources, &parsed, &names, &types).unwrap();
        let diagnostics = if code == "L0085" {
            typed.diagnostics().to_vec()
        } else {
            assert!(typed.diagnostics().is_empty());
            check_ownership(&sources, &parsed, &names, &typed)
                .unwrap()
                .diagnostics()
                .to_vec()
        };
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code().to_string(), code);
        assert_eq!(
            sources.slice(diagnostics[0].primary_span()).unwrap(),
            fragment
        );
    }
}
