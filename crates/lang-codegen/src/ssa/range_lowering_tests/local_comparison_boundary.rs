//! Ordinary source keeps the non-Int, short-circuit and alias gates closed.
use super::*;
use crate::ssa::LoweringErrorKind;

fn reject(statement: &str, fragment: &str) {
    let source = format!(
        r#"
fun check(flag:Boolean):Unit{{}}
fun read(view:View<String>):Unit{{}}
fun consume(own root:List<String>):Unit{{}}
fun scan(n:Int,flag:Boolean):Unit{{
    val root=listOf("kept")
    {{borrow val part=take(root,1);{statement};read(part)}}
    consume(root)
}}
fun main():Unit{{scan(0,true)}}
"#
    );
    for pipeline in [
        Pipeline::Single,
        Pipeline::Unit,
        Pipeline::UnitReversed,
        Pipeline::SingleConstant,
        Pipeline::UnitConstant,
        Pipeline::UnitConstantReversed,
    ] {
        let lowered = lower(&source, fragment, pipeline);
        let error = lowered
            .result
            .err()
            .expect("this ordinary source is still outside the capability gate");
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
        assert_eq!(
            lowered.sources.slice(error.span.unwrap()).unwrap(),
            fragment
        );
    }
}

#[test]
fn range_comparison_boundary_non_int() {
    for expression in [
        "2L!=4L",
        "2L<=4L",
        "2L>4L",
        "2L>=4L",
        "flag!=true",
        "\"a\"!=\"b\"",
    ] {
        reject(&format!("check({expression})"), expression);
    }
}

#[test]
fn range_comparison_boundary_short_circuit_and_concat() {
    for expression in ["flag && flag", "flag || flag"] {
        reject(&format!("check({expression})"), expression);
    }
    reject("println(\"a\"+\"b\")", "\"a\"+\"b\"");
}

#[test]
fn range_comparison_boundary_ordinary_and_metadata_alias() {
    for binding in [
        "val text=\"kept\";borrow val alias=text;println(alias)",
        "borrow val alias=part;read(alias)",
    ] {
        for expression in ["n!=0", "n<=0", "n>0", "n>=0"] {
            reject(&format!("{binding};check({expression})"), expression);
        }
    }
}
