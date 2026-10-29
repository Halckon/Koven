//! The shared scanner must report the start of the first invalid sequence, not the bad continuation.
use inkwell::{context::Context, module::Linkage};
use std::{fs, process::Command};

#[test]
fn utf8_error_offset_native_matches_strict_sequence_boundaries() {
    let cases: &[&[u8]] = &[
        b"",
        b"ASCII\0bytes",
        "中文🙂".as_bytes(),
        &[0xc2, 0x80],
        &[0xdf, 0xbf],
        &[0xe0, 0xa0, 0x80],
        &[0xed, 0x9f, 0xbf],
        &[0xee, 0x80, 0x80],
        &[0xef, 0xbf, 0xbf],
        &[0xf0, 0x90, 0x80, 0x80],
        &[0xf4, 0x8f, 0xbf, 0xbf],
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
        &[0xc2, 0xa2, 0xe2, 0x82],
        &[0xc2, 0xa2, 0x80],
        &[0xc2, 0xa2, 0xe2, 0x28, 0xa1, 0xff],
        &[0xe2, 0x82, 0xac, 0xf0, 0x90, 0x80, 0x41],
        &[0xe2, 0x82, 0xac, 0xc2, 0x41],
    ];
    let context = Context::create();
    let module = context.create_module("utf8-offset-probe");
    let scanner = super::utf8::define_error_offset(&context, &module, context.i64_type()).unwrap();
    scanner.set_name("test_utf8_error_offset");
    scanner.set_linkage(Linkage::External);
    module.verify().unwrap();
    let mut harness = String::from(
        "#include <stdint.h>\n#include <stdio.h>\nextern uint64_t test_utf8_error_offset(const unsigned char *, uint64_t);\nint main(void) {\nif(test_utf8_error_offset((const unsigned char*)0,0)!=UINT64_MAX)return 90;\n",
    );
    for (index, bytes) in cases.iter().enumerate() {
        let expected = std::str::from_utf8(bytes)
            .err()
            .map_or(u64::MAX, |error| error.valid_up_to() as u64);
        let data = if bytes.is_empty() {
            "0".to_owned()
        } else {
            bytes
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        harness.push_str(&format!("{{const unsigned char data[]={{ {data} }}; uint64_t actual=test_utf8_error_offset(data,{}); if(actual!=UINT64_C({expected})){{fprintf(stderr,\"case {index}: %llu\\n\",(unsigned long long)actual);return 1;}}}}\n", bytes.len()));
    }
    harness.push_str("for(unsigned value=0;value<256;value++){unsigned char byte=(unsigned char)value;uint64_t expected=value<128?UINT64_MAX:0;if(test_utf8_error_offset(&byte,1)!=expected)return 2;}\nreturn 0;}\n");
    let directory = crate::native_tests::TestDirectory::create();
    let ir = directory.join("scanner.ll");
    let c = directory.join("probe.c");
    let binary = directory.join("probe");
    fs::write(&ir, module.print_to_string().to_bytes()).unwrap();
    fs::write(&c, harness).unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .arg("-Wno-override-module")
        .arg(&ir)
        .arg(&c)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(binary).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert!(run.stdout.is_empty() && run.stderr.is_empty(), "{run:?}");
}
