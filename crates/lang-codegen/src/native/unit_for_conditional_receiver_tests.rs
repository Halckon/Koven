//! StaticSelf cleanup must be on its lexical side of each provider, including nested providers.
use super::resource_exchange_tests::run_unit;
use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};

#[test]
fn unit_for_conditional_receiver_cleanup_tracks_inside_outside_and_nested_positions() {
    for receiver in ["consume", "this.consume"] {
        for copyable in [false, true] {
            for position in ["inside", "outside", "between"] {
                let body = match position {
                    "inside" => format!(
                        "for (_ in listOf(Leaf(\"outer\"))) {{ {receiver}(if(flag) {{ return }} else {{ return }}) }}"
                    ),
                    "outside" => format!(
                        "{receiver}(if(flag) {{ for (_ in listOf(Leaf(\"outer\"))) {{ return }}; return }} else {{ return }})"
                    ),
                    _ => format!(
                        "for (_ in listOf(Leaf(\"outer\"))) {{ {receiver}(if(flag) {{ for (_ in listOf(Leaf(\"inner\"))) {{ return }}; return }} else {{ return }}) }}"
                    ),
                };
                let field = if copyable { "Int" } else { "String" };
                let constructor = if copyable {
                    "0"
                } else {
                    "\"receiver\" + \"owner\""
                };
                let provider = format!(
                    "package p\nclass Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}\ninterface Relay {{ own fun consume(own flag: Boolean): Unit {{ println(\"unexpected call\") }}\nown fun run(own flag: Boolean): Unit {{ {body}; println(\"unexpected after\") }} }}\nvalue class Host(val item: {field}): Relay {{}}"
                );
                let consumer = format!(
                    "package q\nimport p.Host\nfun entry(): Unit {{ Host({constructor}).run(true); println(\"caller\") }}"
                );
                let order: &[usize] = match (copyable, position) {
                    (false, "inside") => &[0, 1, 2],
                    (false, "outside") => &[1, 2, 0],
                    (false, _) => &[3, 4, 0, 1, 2],
                    (true, "inside" | "outside") => &[0, 1],
                    (true, _) => &[2, 3, 0, 1],
                };
                let expected: &[u8] = if position == "between" {
                    b"inner\nouter\ncaller\n"
                } else {
                    b"outer\ncaller\n"
                };
                for constants in [false, true] {
                    let (run, ir) = run_unit(&provider, &consumer, constants);
                    assert_success(&run, expected);
                    assert_success(&run_counted_allocations_in_order(&ir, order), expected);
                }
            }
        }
    }
}
