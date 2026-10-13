//! Normal source shared by SSA and unmodified native tests; no IR/allocator rewriting.
pub(crate) struct Case {
    pub name: &'static str,
    pub source: &'static str,
    pub stdout: &'static [u8],
    pub source_index: usize,
}
pub(crate) fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "second_parameter",
            source: include_str!("../tests/fixtures/multi_borrow/second_parameter.ko"),
            stdout: b"kept\nkept\n",
            source_index: 1,
        },
        Case {
            name: "named_reverse_middle",
            source: include_str!("../tests/fixtures/multi_borrow/named_reverse_middle.ko"),
            stdout: b"eval-right\neval-left\nkept\nkept\n",
            source_index: 1,
        },
        Case {
            name: "same_type_different_roots",
            source: include_str!("../tests/fixtures/multi_borrow/same_type_different_roots.ko"),
            stdout: b"other\nkept\nkept\n",
            source_index: 1,
        },
        Case {
            name: "same_owner_twice",
            source: include_str!("../tests/fixtures/multi_borrow/same_owner_twice.ko"),
            stdout: b"kept\nkept\n",
            source_index: 1,
        },
        Case {
            name: "wrapper_changes_index",
            source: include_str!("../tests/fixtures/multi_borrow/wrapper_changes_index.ko"),
            stdout: b"other\nkept\nkept\n",
            source_index: 1,
        },
        Case {
            name: "projected_field",
            source: include_str!("../tests/fixtures/multi_borrow/projected_field.ko"),
            stdout: b"other\nfield\nresource-owned\ndrop-resource\n",
            source_index: 1,
        },
        Case {
            name: "first_parameter",
            source: include_str!("../tests/fixtures/multi_borrow/first_parameter.ko"),
            stdout: b"other\nkept\nkept\n",
            source_index: 0,
        },
        Case {
            name: "last_parameter_three",
            source: include_str!("../tests/fixtures/multi_borrow/last_parameter_three.ko"),
            stdout: b"other\nkept\nkept\n",
            source_index: 2,
        },
        Case {
            name: "copyable_int",
            source: include_str!("../tests/fixtures/multi_borrow/copyable_int.ko"),
            stdout: b"seven\nseven\n",
            source_index: 1,
        },
        Case {
            name: "generic_move_only",
            source: include_str!("../tests/fixtures/multi_borrow/generic_move_only.ko"),
            stdout: b"packet\nowned\n",
            source_index: 1,
        },
        Case {
            name: "same_entry_loan",
            source: include_str!("../tests/fixtures/multi_borrow/same_entry_loan.ko"),
            stdout: b"kept\nkept\n",
            source_index: 1,
        },
        Case {
            name: "unrelated_resource_temporary",
            source: include_str!("../tests/fixtures/multi_borrow/unrelated_resource_temporary.ko"),
            stdout: b"other-drop\nkept\n",
            source_index: 1,
        },
        Case {
            name: "wrapper_field_arguments",
            source: include_str!("../tests/fixtures/multi_borrow/wrapper_field_arguments.ko"),
            stdout: b"owned\ndrop\nfield\nowned\ndrop\n",
            source_index: 1,
        },
    ]
}
