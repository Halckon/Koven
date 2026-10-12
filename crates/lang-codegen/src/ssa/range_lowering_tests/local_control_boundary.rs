//! Ordinary, frontend-valid source probes for the frozen capability boundary.
use super::*;
use crate::ssa::LoweringErrorKind;

fn reject(label: &str, consumer: &str, fragment: &str) {
    for pipeline in [Pipeline::Single, Pipeline::Unit, Pipeline::UnitReversed] {
        // A frontend diagnostic is a fixture failure, not a backend rejection.
        // Print the current source case before the shared frontend setup asserts.
        eprintln!("boundary={label}; pipeline={pipeline:?}; expected={fragment:?}");
        let lowered = lower(consumer, fragment, pipeline);
        let error = lowered
            .result
            .err()
            .expect("this source remains outside the bounded capability");
        let actual = error.span.and_then(|span| lowered.sources.slice(span).ok());
        assert_eq!(
            error.kind,
            LoweringErrorKind::UnsupportedNode,
            "{label}/{pipeline:?}: {error:?}; source={actual:?}"
        );
        assert_eq!(
            error.span,
            Some(lowered.expected_span),
            "{label}/{pipeline:?}: {error:?}; source={actual:?}"
        );
    }
}

fn scoped(statement: &str) -> String {
    format!(
        r#"
fun check(flag:Boolean):Unit{{if(flag){{println("true")}}else{{println("false")}}}}
fun sink(value:Unit):Unit{{}}
fun read(view:View<String>):Unit{{}}
fun consume(own root:List<String>):Unit{{}}
fun scan(flag:Boolean,n:Int):Unit{{
    val root=listOf("kept")
    {{
        borrow val part=take(root,1)
        {statement}
        read(part)
    }}
    consume(root)
}}
fun main():Unit{{scan(true,2)}}
"#
    )
}

#[test]
fn range_local_control_rejects_if_without_else() {
    let expression = "if(flag){read(part)}";
    reject("missing-else", &scoped(expression), expression);
}

#[test]
fn range_local_control_rejects_value_context_if() {
    let expression = "if(flag){read(part)}else{read(part)}";
    reject(
        "unit-argument",
        &scoped(&format!("sink({expression})")),
        expression,
    );
}

#[test]
fn range_local_control_rejects_nested_if_at_inner_expression() {
    let inner = "if(flag){read(part)}else{read(part)}";
    let outer = format!("if(flag){{{inner}}}else{{read(part)}}");
    reject("nested-if", &scoped(&outer), inner);
}

#[test]
fn range_local_control_rejects_non_normal_arms() {
    for (label, statement, fragment) in [
        (
            "return-arm",
            "if(flag){read(part);return}else{read(part)}",
            "return",
        ),
        (
            "abort-arm",
            "if(flag){error(\"stop\")}else{read(part)}",
            "if(flag){error(\"stop\")}else{read(part)}",
        ),
    ] {
        reject(label, &scoped(statement), fragment);
    }
}

#[test]
fn range_local_control_rejects_new_binding_inside_an_arm() {
    let expression = "if(flag){borrow val child=take(part,1);read(child)}else{read(part)}";
    reject("arm-binding", &scoped(expression), expression);
}

#[test]
fn range_local_control_rejects_if_nested_in_a_loop() {
    let expression = "if(flag){read(part)}else{read(part)}";
    for (label, statement) in [
        ("while-if", format!("while(flag){{{expression}}}")),
        ("for-if", format!("for(item in part){{{expression}}}")),
    ] {
        reject(label, &scoped(&statement), expression);
    }
}

#[test]
fn range_local_control_rejects_different_roots_in_the_same_scope() {
    let expression = "if(flag){read(part)}else{read(part)}";
    let consumer = format!(
        r#"
fun read(view:View<String>):Unit{{}}
fun consume(own root:List<String>):Unit{{}}
fun scan(flag:Boolean):Unit{{
    val root=listOf("first")
    val other=listOf("second")
    {{
        borrow val part=take(root,1)
        borrow val sibling=take(other,1)
        {expression}
        read(part)
        read(sibling)
    }}
    consume(root)
    consume(other)
}}
fun main():Unit{{scan(true)}}
"#
    );
    reject("different-roots", &consumer, expression);
}

#[test]
fn range_local_control_keeps_ordinary_and_metadata_alias_whole_function_gates() {
    for (binding_kind, binding) in [
        (
            "ordinary",
            "val text=\"kept\";borrow val alias=text;println(alias)",
        ),
        ("metadata", "borrow val alias=part;read(alias)"),
    ] {
        for (expression_kind, statement, fragment) in [
            ("eq", "check(n==2)", "n==2"),
            ("lt", "check(n<3)", "n<3"),
            (
                "if",
                "if(flag){read(part)}else{read(part)}",
                "if(flag){read(part)}else{read(part)}",
            ),
        ] {
            reject(
                &format!("{binding_kind}-{expression_kind}"),
                &scoped(&format!("{binding};{statement}")),
                fragment,
            );
        }
    }
}

#[test]
fn range_local_control_keeps_unselected_binary_gates() {
    for (label, statement, fragment) in [
        ("not-equal", "check(n!=2)", "n!=2"),
        ("less-equal", "check(n<=3)", "n<=3"),
        ("greater", "check(n>3)", "n>3"),
        ("greater-equal", "check(n>=3)", "n>=3"),
        ("and", "check(flag && flag)", "flag && flag"),
        ("or", "check(flag || flag)", "flag || flag"),
        ("string-concat", "println(\"a\"+\"b\")", "\"a\"+\"b\""),
        ("long-equality", "check(2L==4L)", "2L==4L"),
    ] {
        reject(label, &scoped(statement), fragment);
    }
}

#[test]
fn range_local_control_rejects_if_outside_the_live_lexical_scope() {
    let expression = "if(flag){println(\"left\")}else{println(\"right\")}";
    let binding = "{borrow val part=take(root,1);read(part)}";
    for (label, body) in [
        ("before-binding", format!("{expression}\n{binding}")),
        ("after-scope", format!("{binding}\n{expression}")),
    ] {
        let consumer = format!(
            r#"
fun read(view:View<String>):Unit{{}}
fun consume(own root:List<String>):Unit{{}}
fun scan(flag:Boolean):Unit{{
    val root=listOf("kept")
    {body}
    consume(root)
}}
fun main():Unit{{scan(true)}}
"#
        );
        reject(label, &consumer, expression);
    }
}

#[test]
fn range_local_control_rejects_hidden_cfg_in_condition_or_argument() {
    let nullable_if = "if(n!!==2){read(part)}else{read(part)}";
    let nullable = format!(
        r#"
fun read(view:View<String>):Unit{{}}
fun consume(own root:List<String>):Unit{{}}
fun scan(n:Int?):Unit{{
    val root=listOf("kept")
    {{
        borrow val part=take(root,1)
        {nullable_if}
        read(part)
    }}
    consume(root)
}}
fun main():Unit{{scan(2)}}
"#
    );
    reject("nullable-condition", &nullable, nullable_if);

    let indexed_if = "if(values[0]==2){read(part)}else{read(part)}";
    reject(
        "indexed-condition",
        &scoped(&format!("val values=listOf(2);{indexed_if}")),
        indexed_if,
    );

    let interpolation_if = "if(flag){println(\"${n}\")}else{read(part)}";
    reject(
        "interpolated-argument",
        &scoped(interpolation_if),
        interpolation_if,
    );
}

#[test]
fn range_local_control_rejects_old_call_prefix_exit_inside_a_new_if() {
    let expression = "if(flag){readCount(take(part,1),if(flag){return}else{0})}else{read(part)}";
    let consumer = format!(
        "fun readCount(view:View<String>,n:Int):Unit{{}}\n{}",
        scoped(expression)
    );
    reject("call-prefix-return-in-arm", &consumer, expression);
}

#[test]
fn range_local_control_rejects_carrier_call_results() {
    let carrier = "if(flag){read(take(part,1))}else{read(part)}";
    reject("new-carrier-in-arm", &scoped(carrier), carrier);
}
