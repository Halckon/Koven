//! Public API oracles use Rust's UTF-8 boundaries, independently of the Koven scanner.
use super::check_build_run_both_modes;
use std::fmt::Write;

#[test]
fn public_text_roundtrips_every_byte_and_classifies_utf8_and_slice_boundaries() {
    let mut program = String::from(
        r#"fun slice(text: String, start: Int, end: Int, expected: Int): Unit {
    val result = text.sliceBytes(start, end)
    when (result) {
        is Ok -> {
            if (expected != 0) { error("invalid slice accepted") }
            println(result.success)
        }
        is Err -> {
            val problem = result.error
            when (problem) {
                is TextError.InvalidRange -> {
                    if (expected != 1) { error("wrong range precedence") }
                }
                is TextError.NotScalarBoundary -> {
                    if (expected != 2) { error("wrong scalar boundary") }
                }
                else -> error("unexpected slice error")
            }
        }
    }
}
"#,
    );
    let mut calls = String::from("fun main(): Unit {\n");
    let mut expected = Vec::new();
    // Include each UTF-8 width transition, both sides of the surrogate gap and
    // the highest scalar; byte equality catches lossy decoding and NUL truncation.
    for (case, text) in [
        "",
        "ASCII",
        "中文🙂",
        "A\0é",
        "\u{7f}\u{80}\u{7ff}\u{800}\u{d7ff}\u{e000}\u{ffff}\u{10000}\u{10ffff}",
    ]
    .iter()
    .enumerate()
    {
        writeln!(program, "fun roundtrip{case}(): Unit {{").unwrap();
        writeln!(program, "val source = {}.clone()", literal(text)).unwrap();
        writeln!(
            program,
            "if (source.byteLength() != {}) {{ error(\"length\") }}",
            text.len()
        )
        .unwrap();
        program.push_str("val bytes = source.toBytes()\n");
        writeln!(
            program,
            "if (bytes.size != {}) {{ error(\"array size\") }}",
            text.len()
        )
        .unwrap();
        for (index, byte) in text.bytes().enumerate() {
            writeln!(program, "val byte{index}: UByte = {byte}u\nif (bytes[{index}] != byte{index}) {{ error(\"byte {index}\") }}").unwrap();
        }
        program.push_str("val decoded = decodeUtf8(bytes)\nwhen (decoded) {\nis Ok -> println(decoded.success)\nis Err -> error(\"valid bytes rejected\")\n}\nprintln(source)\n}\n");
        writeln!(calls, "roundtrip{case}()").unwrap();
        for _ in 0..2 {
            expected.extend_from_slice(text.as_bytes());
            expected.push(b'\n');
        }
    }
    let invalid: &[&[u8]] = &[
        &[0x80],
        &[0xbf],
        &[0xc0, 0x80],
        &[0xc1, 0xbf],
        &[0xe0, 0x9f, 0xbf],
        &[0xed, 0xa0, 0x80],
        &[0xf0, 0x8f, 0xbf, 0xbf],
        &[0xf4, 0x90, 0x80, 0x80],
        &[0xf5, 0x80, 0x80, 0x80],
        &[0xff],
        &[0x41, 0xe2, 0x28, 0xa1],
        &[0x41, 0xe2, 0x82],
        &[0x41, 0xc2],
        &[0x41, 0xf0, 0x90, 0x80],
        &[0xc2, 0xa2, 0x80],
        &[0xc2, 0xa2, 0xe2, 0x28, 0xa1, 0xff],
        &[0xe2, 0x82, 0xac, 0xf0, 0x90, 0x80, 0x41],
    ];
    for bytes in invalid {
        let offset = std::str::from_utf8(bytes).unwrap_err().valid_up_to();
        let mut initializer = String::new();
        for (index, byte) in bytes.iter().enumerate() {
            if index + 1 < bytes.len() {
                write!(initializer, "if (index == {index}) {byte}u else ").unwrap();
            } else {
                write!(initializer, "{byte}u").unwrap();
            }
        }
        writeln!(
            calls,
            "invalidUtf8(Array<UByte>({}, {{ index -> {initializer} }}), {offset})",
            bytes.len()
        )
        .unwrap();
    }
    // All endpoint pairs include valid empty slices, every continuation byte,
    // negative/reversed/out-of-range intervals and range-vs-boundary precedence.
    for text in ["", "Aé中𐀀\0"] {
        for start in -1..=text.len() as i32 + 1 {
            for end in -1..=text.len() as i32 + 1 {
                let kind = if start < 0 || end < start || end > text.len() as i32 {
                    1
                } else if !text.is_char_boundary(start as usize)
                    || !text.is_char_boundary(end as usize)
                {
                    2
                } else {
                    expected.extend_from_slice(&text.as_bytes()[start as usize..end as usize]);
                    expected.push(b'\n');
                    0
                };
                writeln!(calls, "slice({}, {start}, {end}, {kind})", literal(text)).unwrap();
            }
        }
    }
    calls.push_str("println(\"matrix passed\")\n}\n");
    expected.extend_from_slice(b"matrix passed\n");
    program.push_str(&calls);
    check_build_run_both_modes(&program, &expected);
}

fn literal(text: &str) -> String {
    format!(
        "\"{}\"",
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\0', "\\0")
    )
}

#[test]
fn public_text_accepts_field_and_temporary_borrows_and_owned_result_transfers() {
    let program = r#"class Holder(val text: String)
fun transfer(flag: Boolean): Result<String, TextError> {
    val result = "early".sliceBytes(0, 5)
    if (flag) { return result }
    return result
}
fun transferred(flag: Boolean): Unit {
    val result = transfer(flag)
    when (result) {
        is Ok -> println(result.success)
        is Err -> error("transferred result failed")
    }
}
fun main(): Unit {
    val holder = Holder("field".clone())
    val bytes = holder.text.toBytes()
    val decoded = decodeUtf8(bytes)
    when (decoded) {
        is Ok -> println(decoded.success)
        is Err -> error("field decode failed")
    }
    println(holder.text)
    val temporary = decodeUtf8("temporary".clone().toBytes())
    when (temporary) {
        is Ok -> println(temporary.success)
        is Err -> error("temporary decode failed")
    }
    transferred(true)
    transferred(false)
    val results = Array<Result<String, TextError>>(2, { index -> "stored".sliceBytes(0, 6) })
    if (results.size != 2) { error("result container size") }
    println("container")
}
"#;
    // This is public reachability and output evidence. Allocation/drop counters
    // separately establish lifetime correctness; successful exit is not a leak oracle.
    check_build_run_both_modes(
        program,
        b"field\nfield\ntemporary\nearly\nearly\ncontainer\n",
    );
}

#[test]
fn public_text_independent_copies_and_exhaustive_result_returns() {
    let program = r#"fun bytes(): Array<UByte> {
    val source = "Aé\0".clone()
    val data = source.toBytes()
    data[0] = 88u
    println(source)
    return data
}
fun decodedLength(data: Array<UByte>): Int {
    val result = decodeUtf8(data)
    when (result) {
        is Ok -> return result.success.byteLength()
        is Err -> return -1
    }
}
fun tagOnly(valid: Boolean): Int {
    val result = decodeUtf8(Array<UByte>(1, { index -> if (valid) { 65u } else { 255u } }))
    when ((result)) {
        is Ok -> return 1
        is Err -> return 0
    }
}
fun partialTag(valid: Boolean): Int {
    val result = decodeUtf8(Array<UByte>(1, { index -> if (valid) { 65u } else { 255u } }))
    when ((result)) { !is Err -> return 1 }
    return 0
}
fun main(): Unit {
    if (tagOnly(true) != 1 || tagOnly(false) != 0) { error("tag-only return") }
    if (partialTag(true) != 1 || partialTag(false) != 0) { error("tag fallthrough") }
    val data = bytes()
    val decoded = decodeUtf8(data)
    data[0] = 89u
    when (decoded) {
        is Ok -> println(decoded.success)
        is Err -> error("copy failed")
    }
    val after = decodeUtf8(data)
    when (after) {
        is Ok -> println(after.success)
        is Err -> error("mutated input failed")
    }
    if (decodedLength(data) != 4) { error("success return") }
    val bad = Array<UByte>(1, { index -> 128u })
    if (decodedLength(bad) != -1) { error("error return") }
    println("returns")
}
"#;
    check_build_run_both_modes(program, b"A\xc3\xa9\0\nX\xc3\xa9\0\nY\xc3\xa9\0\nreturns\n");
}

#[test]
fn public_text_receiver_argument_transfers_work_in_both_modes() {
    let program = r#"class Holder(val text: String)
fun join(stop: Boolean): Unit {
    val holder = Holder("field".clone())
    val result = holder.text.checkedPlus(if (stop) { println("join-stop"); return } else { "!" })
    if (result != null) { println(result) }
    println(holder.text)
}
fun first(): Int { println("first"); return 0 }
fun slice(stop: Boolean): Unit {
    val holder = Holder("slice".clone())
    val part = holder.text.sliceBytes(first(), if (stop) { println("slice-stop"); return } else { 5 })
    when (part) {
        is Ok -> println(part.success)
        is Err -> error("slice failed")
    }
    println(holder.text)
}
fun cycle(stop: Boolean, again: Boolean): Unit {
    var once = true
    while (once) {
        once = false
        val holder = Holder("loop".clone())
        val joined = holder.text.checkedPlus(if (stop) {
            if (again) { continue } else { break }
        } else { "!" })
        println("completed")
        println(holder.text)
    }
    println("tail")
}
fun main(): Unit {
    join(true)
    join(false)
    slice(true)
    slice(false)
    cycle(true, true)
    cycle(true, false)
    cycle(false, true)
}
"#;
    check_build_run_both_modes(program,
        b"join-stop\nfield!\nfield\nfirst\nslice-stop\nfirst\nslice\nslice\ntail\ntail\ncompleted\nloop\ntail\n");
}

#[test]
fn public_text_nullable_results_preserve_borrowed_inputs_in_both_modes() {
    let program = r#"fun read(value: String): Unit { println(value) }
fun join(input: String): Unit {
    val joined = input.checkedPlus("!")
    if (joined != null) { read(joined); println((joined)) }
    println(input)
}
fun inspect(input: String, own value: String?, flag: Boolean): Unit {
    if (flag) { println("flag") }
    if (value != null) { read(value) } else { println("null") }
    if (value == null) { println("absent") } else { println(value) }
    println(input)
}
fun main(): Unit {
    val input = "input".clone()
    join(input)
    inspect(input, "value".clone(), true)
    inspect(input, null, false)
    println(input)
}
"#;
    check_build_run_both_modes(
        program,
        b"input!\ninput!\ninput\nflag\nvalue\nvalue\ninput\nnull\nabsent\ninput\ninput\n",
    );
}

#[test]
fn public_text_nullable_control_results_work_in_both_modes() {
    let program = r#"fun observe(input: String, present: Boolean, stop: Boolean): Unit {
    val joined = if (present) { input.checkedPlus("!") } else { ((null)) }
    if (joined != null) { println(joined) } else { println("null") }
    val text = when {
        present -> input.clone()
        else -> {
            val discarded = "discarded".clone()
            println(discarded)
            if (stop) { return }
            ((null))
        }
    }
    if (text != null) { println(text) } else { println("null") }
    println(input)
}
fun main(): Unit {
    val input = "input".clone()
    observe(input, true, false)
    observe(input, false, false)
    observe(input, false, true)
    println(input)
}
"#;
    check_build_run_both_modes(
        program,
        b"input!\ninput\ninput\nnull\ndiscarded\nnull\ninput\nnull\ndiscarded\ninput\n",
    );
}
