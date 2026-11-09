//! Ordinary, frontend-valid source probes for the frozen capability boundary.
use super::support::{UnitExpectation, observe_original_unit};
use super::*;
use crate::ssa::LoweringErrorKind;
use crate::test_support::diagnostics::{assert_eq_diagnostic, diagnostic};

fn reject(label: &str, consumer: &str, fragment: &str) {
    for pipeline in [Pipeline::Single, Pipeline::Unit, Pipeline::UnitReversed] {
        // A frontend diagnostic is a fixture failure, not a backend rejection.
        // Print the current source case before the shared frontend setup asserts.
        diagnostic!("boundary={label}; pipeline={pipeline:?}; expected={fragment:?}");
        if matches!(pipeline, Pipeline::Single) {
            observe_original_unit(
                consumer,
                fragment,
                pipeline,
                UnitExpectation::Reject(fragment),
                label,
            );
            continue;
        }
        let lowered = lower(consumer, fragment, pipeline);
        let error = lowered
            .result
            .err()
            .expect("this source remains outside the bounded capability");
        let actual = error.span.and_then(|span| lowered.sources.slice(span).ok());
        assert_eq_diagnostic!(
            error.kind,
            LoweringErrorKind::UnsupportedNode,
            "{label}/{pipeline:?}: {error:?}; source={actual:?}"
        );
        assert_eq_diagnostic!(
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
    let consumer = scoped(&format!("sink({expression})"));
    reject(
        "unit-argument",
        &format!("fun sink(value:Unit):Unit{{}}\n{consumer}"),
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
fn range_local_control_rejects_abort_arm() {
    let expression = "if(flag){error(\"stop\")}else{read(part)}";
    reject("abort-arm", &scoped(expression), expression);
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
fn range_local_control_keeps_unproved_alias_control_gates() {
    let branch = "if(flag){read(part)}else{read(part)}";
    let ordinary = "val text=\"kept\";borrow val alias=text;println(alias)";
    reject(
        "ordinary-if",
        &scoped(&format!("{ordinary};{branch}")),
        branch,
    );

    let metadata = "borrow val alias=part;read(alias)";
    for (kind, statement, fragment) in [
        ("eq", "check(n==2)", "n==2"),
        ("lt", "check(n<3)", "n<3"),
        ("if", branch, branch),
    ] {
        reject(
            &format!("metadata-{kind}"),
            &scoped(&format!("{metadata};{statement}")),
            fragment,
        );
    }
}

#[test]
fn range_local_control_accepts_int_comparisons_with_ordinary_root_alias() {
    let binding = "val text=\"kept\";borrow val alias=text;println(alias)";
    for (label, expression, expected) in [
        ("eq", "n==2", "true"),
        ("lt", "n<3", "true"),
        ("ne", "n!=0", "true"),
        ("le", "n<=0", "false"),
        ("gt", "n>0", "true"),
        ("ge", "n>=0", "true"),
    ] {
        let source = scoped(&format!("{binding};check({expression})"));
        let expected = format!("kept\n{expected}\n");
        for pipeline in [
            Pipeline::Single,
            Pipeline::Unit,
            Pipeline::UnitReversed,
            Pipeline::SingleConstant,
            Pipeline::UnitConstant,
            Pipeline::UnitConstantReversed,
        ] {
            let label = format!("ordinary-alias-{label}-{pipeline:?}");
            let lowered = lower_positive(&source, expression, pipeline);
            let (program, entry) = lowered.require_success(&label);
            run_unmodified(&program, entry, &label, expected.as_bytes());
        }
    }
}

#[test]
fn range_local_control_keeps_unselected_binary_gates() {
    for (label, statement, fragment) in [
        ("string-concat", "println(\"a\"+\"b\")", "\"a\"+\"b\""),
        ("long-equality", "check(2L==4L)", "2L==4L"),
    ] {
        reject(label, &scoped(statement), fragment);
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
