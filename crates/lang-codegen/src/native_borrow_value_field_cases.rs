//! SPEC-0309: ordinary source inputs shared by SSA and unmodified native tests.
pub(crate) const COPYABLE: &str = include_str!("../tests/fixtures/borrow_value_fields/copyable.ko");
pub(crate) const STRING: &str = include_str!("../tests/fixtures/borrow_value_fields/string.ko");
pub(crate) const CLONE: &str = include_str!("../tests/fixtures/borrow_value_fields/clone.ko");

pub(crate) fn cases() -> [(&'static str, &'static str, &'static [u8]); 3] {
    [
        (
            "copyable",
            COPYABLE,
            b"seven\nseven\nseven\nseven\nseven\nseven\nconsumed\n",
        ),
        (
            "string",
            STRING,
            b"payload\npayload\npayload\npayload\npayload\npayload\nconsumed\n",
        ),
        ("clone", CLONE, b"payload\npayload\npayload\nconsumed\n"),
    ]
}
