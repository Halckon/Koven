use super::run;

#[test]
fn subjectless_when_known_match_has_no_implicit_return_edge() {
    for fallback in ["", "; else -> {}"] {
        let source = format!(
            r#"fun choose(): Int {{
                when {{ false -> {{ error("unexpected arm") }}; !false -> {{ return 1 }}{fallback} }}
            }}
            fun main(): Unit {{
                if (choose() != 1) {{ error("wrong return") }}
                println("ok")
            }}"#
        );
        assert_eq!(run(&source), ("ok\n".to_owned(), Some(0)));
    }
}
