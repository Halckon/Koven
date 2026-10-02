//! Shared source fixture: dynamic operands and independent decimal boundary oracles.

pub(crate) fn binary_fixture(qualifier: &str) -> (String, String, String) {
    let mut functions = String::new();
    let mut checks = String::new();
    let mut stdout = String::new();
    let mut case_index = 0;
    for (ty, width, signed) in [
        ("Byte", 8, true),
        ("Short", 16, true),
        ("Int", 32, true),
        ("Long", 64, true),
        ("UByte", 8, false),
        ("UShort", 16, false),
        ("UInt", 32, false),
        ("ULong", 64, false),
    ] {
        let high = 1_i128 << (width - 1);
        let max = if signed { high - 1 } else { high * 2 - 1 };
        let top = if signed { -high } else { high };
        let ones = if signed { -1 } else { max };
        for op in ["and", "or", "xor", "shl", "shr", "ushr"] {
            functions.push_str(&format!(
                "fun {op}{ty}(left: {ty}, right: {ty}): {ty} = left {op} right\n"
            ));
        }
        let mut cases = vec![
            ("and", top, high - 1, 0),
            ("and", top, ones, top),
            ("and", max, 0, 0),
            ("or", top, high - 1, ones),
            ("or", 0, top, top),
            ("or", max, 0, max),
            ("xor", top, ones, high - 1),
            ("xor", max, max, 0),
            ("xor", 0, ones, ones),
            ("shl", top, 1, 0),
            ("shl", max, 1, if signed { -2 } else { max - 1 }),
        ];
        // Expected effective counts are stated independently of the implementation's bit mask.
        let mut counts = vec![
            (0, 0),
            (1, 1),
            (width - 1, width - 1),
            (width, 0),
            (width + 1, 1),
            (width * 2, 0),
            (max, width - 1),
        ];
        if signed {
            counts.extend([
                (-1, width - 1),
                (-width, 0),
                (-width - 1, width - 1),
                (-high, 0),
            ]);
        }
        for (count, effective) in counts {
            let left_expected = if effective == width - 1 {
                top
            } else {
                1_i128 << effective
            };
            cases.push(("shl", 1, count, left_expected));
            cases.push((
                "shr",
                top,
                count,
                if signed {
                    -high >> effective
                } else {
                    high >> effective
                },
            ));
            cases.push((
                "ushr",
                top,
                count,
                if effective == 0 {
                    top
                } else {
                    high >> effective
                },
            ));
        }
        for (op, left, right, expected) in cases {
            checks.push_str(&format!(
                "val expected{case_index}: {ty} = {}\nif ({qualifier}{op}{ty}({}, {}) == expected{case_index}) {{ println(\"case-{case_index}\") }}\n",
                literal(expected, ty), literal(left, ty), literal(right, ty)
            ));
            stdout.push_str(&format!("case-{case_index}\n"));
            case_index += 1;
        }
    }
    for (index, op) in ["and", "or", "xor", "shl", "shr", "ushr"]
        .iter()
        .enumerate()
    {
        let left = if *op == "or" { -1 } else { 0 };
        functions.push_str(&format!(
            "fun left{index}(): Int {{ println(\"left-{index}\"); return {left} }}\nfun right{index}(): Int {{ println(\"right-{index}\"); return 1 }}\nfun eager{index}(): Int = left{index}() {op} right{index}()\n"
        ));
        checks.push_str(&format!(
            "val eager{index}: Int = {qualifier}eager{index}()\n"
        ));
        stdout.push_str(&format!("left-{index}\nright-{index}\n"));
    }
    (functions, checks, stdout)
}

fn literal(value: i128, ty: &str) -> String {
    let suffix = match ty {
        "Long" => "L",
        "ULong" => "uL",
        ty if ty.starts_with('U') => "u",
        _ => "",
    };
    format!("{value}{suffix}")
}

pub(crate) fn inv_fixture(qualifier: &str) -> (String, String, String) {
    let mut functions = String::new();
    let mut checks = String::new();
    let mut stdout = String::new();
    let mut case_index = 0;
    for (ty, width, signed) in [
        ("Byte", 8, true),
        ("Short", 16, true),
        ("Int", 32, true),
        ("Long", 64, true),
        ("UByte", 8, false),
        ("UShort", 16, false),
        ("UInt", 32, false),
        ("ULong", 64, false),
    ] {
        let high = 1_i128 << (width - 1);
        let max = if signed { high - 1 } else { high * 2 - 1 };
        let top = if signed { -high } else { high };
        let ones = if signed { -1 } else { max };
        functions.push_str(&format!(
            "fun invert{ty}(value: {ty}): {ty} = value.inv()\n"
        ));
        let cases = [
            (0, ones),
            (ones, 0),
            (top, high - 1),
            (high - 1, top),
            (1, if signed { -2 } else { max - 1 }),
        ];
        for (input, expected) in cases {
            checks.push_str(&format!("val expected{case_index}: {ty} = {}\nif ({qualifier}invert{ty}({}) == expected{case_index}) {{ println(\"inv-{case_index}\") }}\n", literal(expected, ty), literal(input, ty)));
            stdout.push_str(&format!("inv-{case_index}\n"));
            case_index += 1;
        }
    }
    functions.push_str("fun operand(): Int { println(\"operand\"); return 42 }\nfun eager(): Int = operand().inv()\n");
    checks.push_str(&format!(
        "if ({qualifier}eager() == -43) {{ println(\"inv-eager\") }}\n"
    ));
    stdout.push_str("operand\ninv-eager\n");
    (functions, checks, stdout)
}

pub(crate) fn inv_receiver_fixture() -> &'static str {
    r#"
class Number(val number: Int)
fun lastField(): Int { val value = Number(1); return value.number.inv() }
fun exercise(): Unit {
    if (1.inv().inv() == 1) { println("literal-chain") }
    if (lastField() == -2) { println("last-field") }
}
"#
}

pub(crate) const INV_RECEIVER_STDOUT: &str = "literal-chain\nlast-field\n";

pub(crate) fn guide_litmus_12() -> &'static str {
    let guide = include_str!("../../../docs/guide/15-conformance-and-staging.md");
    guide
        .split_once("### Litmus 12:")
        .expect("Guide Litmus 12")
        .1
        .split_once("```kotlin\n")
        .expect("Koven source fence")
        .1
        .split_once("\n```")
        .expect("closed source fence")
        .0
}

pub(crate) fn control_fixture() -> (&'static str, &'static str, &'static str) {
    (
        r#"
fun left(): Int { println("left"); return 3 }
fun inverted(flag: Boolean): Int = (if (flag) { return 7 } else { left() }).inv()
fun binary(flag: Boolean): Int = left() and (if (flag) { return 9 } else { left() })
fun exerciseControl(): Unit {
    if (inverted(true) == 7) { println("inv-return") }
    if (inverted(false) == -4) { println("inv-normal") }
    if (binary(true) == 9) { println("binary-return") }
    if (binary(false) == 3) { println("binary-normal") }
}
"#,
        "exerciseControl()",
        "inv-return\nleft\ninv-normal\nleft\nbinary-return\nleft\nleft\nbinary-normal\n",
    )
}

pub(crate) fn constant_runtime_fixture(qualifier: &str) -> (String, String, String) {
    let mut functions = String::new();
    let mut checks = String::new();
    let mut stdout = String::new();
    let mut index = 0;
    for (ty, width, signed) in [
        ("Byte", 8, true),
        ("Short", 16, true),
        ("Int", 32, true),
        ("Long", 64, true),
        ("UByte", 8, false),
        ("UShort", 16, false),
        ("UInt", 32, false),
        ("ULong", 64, false),
    ] {
        let high = 1_i128 << (width - 1);
        let top = if signed { -high } else { high };
        let ones = if signed { -1 } else { high * 2 - 1 };
        for (op, left, right, expected) in [
            ("and", top, high - 1, 0),
            ("or", top, high - 1, ones),
            ("xor", top, ones, high - 1),
            ("shl", 1, if signed { -1 } else { width - 1 }, top),
            (
                "shr",
                top,
                width + 1,
                if signed { -high / 2 } else { high / 2 },
            ),
            ("ushr", top, if signed { -1 } else { width - 1 }, 1),
        ] {
            functions.push_str(&format!("fun dynamic{index}(left: {ty}, right: {ty}): {ty} = left {op} right\nconst val LEFT{index}: {ty} = {}\nconst val RIGHT{index}: {ty} = {}\nconst val CONST{index}: {ty} = LEFT{index} {op} RIGHT{index}\n", literal(left, ty), literal(right, ty)));
            checks.push_str(&format!("val oracle{index}: {ty} = {}\nval actual{index}: {ty} = {qualifier}dynamic{index}({}, {})\nif (actual{index} == oracle{index} && actual{index} == {qualifier}CONST{index}) {{ println(\"const-runtime-{index}\") }}\n", literal(expected, ty), literal(left, ty), literal(right, ty)));
            stdout.push_str(&format!("const-runtime-{index}\n"));
            index += 1;
        }
    }
    (functions, checks, stdout)
}
