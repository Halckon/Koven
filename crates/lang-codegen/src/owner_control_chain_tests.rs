//! Three live-root controls remain generic lowering until chain authority is published.
use super::assert_general_counter_baseline;

#[test]
fn three_live_controls_preserve_zero_and_normal_generations() {
    let mut calls = String::new();
    let mut expected = String::new();
    for rounds in [[0, 0, 0], [2, 0, 0], [0, 2, 0], [2, 0, 2], [2, 2, 2]] {
        calls.push_str(&format!(
            "run({}, {}, {});",
            rounds[0], rounds[1], rounds[2]
        ));
        let latest = rounds.iter().rposition(|n| *n > 0);
        let values = match latest {
            Some(0) => ["a0", "b0"],
            Some(1) => ["a1", "b1"],
            Some(2) => ["a2", "b2"],
            None => ["entry-a", "entry-b"],
            _ => unreachable!(),
        };
        expected.push_str(&format!("{}\n{}\n", values[0], values[1]));
    }
    let text = format!(
        r#"
        fun run(first: Int, second: Int, third: Int) {{
            var n0 = first; var g0 = n0 > 0; var a0 = "entry-a"; var b0 = "entry-b";
            while(g0) {{ val x = replace(&a0, "a0"); val y = replace(&b0, "b0");
                n0 = n0 - 1; g0 = n0 > 0 }};
            var n1 = second; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
            while(g1) {{ val x = replace(&a1, "a1"); val y = replace(&b1, "b1");
                n1 = n1 - 1; g1 = n1 > 0 }};
            var n2 = third; var g2 = n2 > 0; var b2 = b1; var a2 = a1;
            while(g2) {{ val y = replace(&b2, "b2"); val x = replace(&a2, "a2");
                n2 = n2 - 1; g2 = n2 > 0 }};
            println(a2); println(b2)
        }}
        fun entry() {{ {calls} }}
    "#
    );
    assert_general_counter_baseline(&text, expected.as_bytes(), false);
}
