//! SPEC-0229: exact extended integer values survive object emission, linking and execution.
use super::emit_link_and_run;

// Each expected value deliberately uses the pre-existing decimal spelling.
const VALUES: &[(&str, &str, &str)] = &[
    ("Int", "0x2a", "42"),
    ("Int", "0X2A", "42"),
    ("Int", "0b10_1010", "42"),
    ("Int", "0B101010", "42"),
    ("Int", "1_234_567", "1234567"),
    ("Int", "0x1f", "31"),
    ("Byte", "0x7_f", "127"),
    ("Short", "-0X8_000", "-32768"),
    ("UByte", "0b1111_1111u", "255u"),
    ("UShort", "0B1111_1111_1111_1111U", "65535u"),
    ("Long", "0x1_0000_0000L", "4294967296L"),
    (
        "Long",
        "0B1_00000000_00000000_00000000_00000000L",
        "4294967296L",
    ),
    ("UInt", "0xFFFF_FFFFu", "4294967295u"),
    ("UInt", "0XFFFF_FFFFU", "4294967295u"),
    ("UInt", "4_294_967_295U", "4294967295u"),
    ("ULong", "0xFFFF_FFFF_FFFF_FFFFuL", "18446744073709551615uL"),
    ("ULong", "0XFFFF_FFFF_FFFF_FFFFUL", "18446744073709551615uL"),
    (
        "ULong",
        "18_446_744_073_709_551_615uL",
        "18446744073709551615uL",
    ),
    (
        "ULong",
        "0b11111111_11111111_11111111_11111111_11111111_11111111_11111111_11111111UL",
        "18446744073709551615uL",
    ),
    ("Int", "-0x8000_0000", "-2147483648"),
    (
        "Int",
        "-0B10000000_00000000_00000000_00000000",
        "-2147483648",
    ),
    ("Int", "-2_147_483_648", "-2147483648"),
    ("Long", "-0X8000_0000_0000_0000L", "-9223372036854775808L"),
    (
        "Long",
        "-0b10000000_00000000_00000000_00000000_00000000_00000000_00000000_00000000L",
        "-9223372036854775808L",
    ),
    (
        "Long",
        "-9_223_372_036_854_775_808L",
        "-9223372036854775808L",
    ),
];

#[test]
fn numeric_literals_run_with_decimal_value_oracles() {
    let functions = VALUES
        .iter()
        .enumerate()
        .map(|(index, (ty, literal, _))| format!("fun value{index}(): {ty} = {literal}\n"))
        .collect::<String>();
    let checks = VALUES
        .iter()
        .enumerate()
        .map(|(index, (ty, _, expected))| {
            format!(
                "val expected{index}: {ty} = {expected}\nif (value{index}() == expected{index}) {{ println(\"success-{index}\") }}\n"
            )
        })
        .collect::<String>();
    let source = format!("{functions}fun entry(): Unit {{\n{checks}}}\n");
    assert_success(&source, expected_stdout());
}

#[test]
fn numeric_literal_constants_run_with_decimal_value_oracles() {
    let declarations = VALUES
        .iter()
        .enumerate()
        .map(|(index, (ty, literal, _))| format!("const val VALUE{index}: {ty} = {literal}\n"))
        .collect::<String>();
    let checks = VALUES
        .iter()
        .enumerate()
        .map(|(index, (ty, _, expected))| {
            format!(
                "val expected{index}: {ty} = {expected}\nif (VALUE{index} == expected{index}) {{ println(\"success-{index}\") }}\n"
            )
        })
        .collect::<String>();
    let source = format!(
        "{declarations}const val DERIVED: Int = VALUE5 + 1_1\nfun entry(): Unit {{\n{checks}if (DERIVED == 42) {{ println(\"success-derived\") }}\n}}\n"
    );
    assert_success(&source, expected_stdout() + "success-derived\n");
}

fn expected_stdout() -> String {
    (0..VALUES.len())
        .map(|index| format!("success-{index}\n"))
        .collect()
}

fn assert_success(source: &str, expected: String) {
    let run = emit_link_and_run("numeric-literals.ko", source, "entry");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, expected.as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");
}
