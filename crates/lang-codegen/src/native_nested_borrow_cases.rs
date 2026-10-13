//! SPEC-0307 ordinary source cases shared by SSA and unmodified native tests.
pub(crate) struct Case {
    pub name: &'static str,
    pub source: &'static str,
    pub stdout: &'static [u8],
    pub nested: bool,
}

pub(crate) fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "single_direct",
            source: include_str!("../tests/fixtures/nested_borrow/single_direct.ko"),
            stdout: b"kept\nkept\n",
            nested: true,
        },
        Case {
            name: "single_explicit_control",
            source: include_str!("../tests/fixtures/nested_borrow/single_explicit_control.ko"),
            stdout: b"kept\nkept\n",
            nested: false,
        },
        Case {
            name: "single_grouped",
            source: include_str!("../tests/fixtures/nested_borrow/single_grouped.ko"),
            stdout: b"kept\nkept\n",
            nested: true,
        },
        Case {
            name: "multi_source_first",
            source: include_str!("../tests/fixtures/nested_borrow/multi_source_first.ko"),
            stdout: b"kept\nkept\n",
            nested: true,
        },
        Case {
            name: "multi_source_middle",
            source: include_str!("../tests/fixtures/nested_borrow/multi_source_middle.ko"),
            stdout: b"kept\nkept\n",
            nested: true,
        },
        Case {
            name: "multi_source_last",
            source: include_str!("../tests/fixtures/nested_borrow/multi_source_last.ko"),
            stdout: b"kept\nkept\n",
            nested: true,
        },
        Case {
            name: "mixed_copyable_aux",
            source: include_str!("../tests/fixtures/nested_borrow/mixed_copyable_aux.ko"),
            stdout: b"kept\nkept\n",
            nested: true,
        },
        Case {
            name: "mixed_resource_aux",
            source: include_str!("../tests/fixtures/nested_borrow/mixed_resource_aux.ko"),
            stdout: b"drop-aux\nkept\nkept\n",
            nested: true,
        },
        Case {
            name: "mixed_resource_explicit_control",
            source: include_str!(
                "../tests/fixtures/nested_borrow/mixed_resource_explicit_control.ko"
            ),
            stdout: b"drop-aux\nkept\nkept\n",
            nested: false,
        },
        Case {
            name: "inner_named_reverse",
            source: include_str!("../tests/fixtures/nested_borrow/inner_named_reverse.ko"),
            stdout: b"eval-right\neval-left\nkept\nkept\n",
            nested: true,
        },
        Case {
            name: "outer_named_reverse",
            source: include_str!("../tests/fixtures/nested_borrow/outer_named_reverse.ko"),
            stdout: b"eval-tail\nkept\ntail\nkept\n",
            nested: true,
        },
        Case {
            name: "same_root_inner",
            source: include_str!("../tests/fixtures/nested_borrow/same_root_inner.ko"),
            stdout: b"kept\nkept\n",
            nested: true,
        },
        Case {
            name: "same_root_outer",
            source: include_str!("../tests/fixtures/nested_borrow/same_root_outer.ko"),
            stdout: b"kept\nkept\nkept\n",
            nested: true,
        },
        Case {
            name: "same_root_outer_explicit_control",
            source: include_str!(
                "../tests/fixtures/nested_borrow/same_root_outer_explicit_control.ko"
            ),
            stdout: b"kept\nkept\nkept\n",
            nested: false,
        },
        Case {
            name: "different_roots_outer",
            source: include_str!("../tests/fixtures/nested_borrow/different_roots_outer.ko"),
            stdout: b"left\nright\nleft\nright\n",
            nested: true,
        },
        Case {
            name: "inner_borrow_and_owned_resource",
            source: include_str!(
                "../tests/fixtures/nested_borrow/inner_borrow_and_owned_resource.ko"
            ),
            stdout: b"drop-aux\ndrop-temp\nkept\nkept\n",
            nested: true,
        },
        Case {
            name: "outer_borrow_temp_after",
            source: include_str!("../tests/fixtures/nested_borrow/outer_borrow_temp_after.ko"),
            stdout: b"kept\ndrop-temp\nafter\nkept\n",
            nested: true,
        },
        Case {
            name: "outer_borrow_temp_before",
            source: include_str!("../tests/fixtures/nested_borrow/outer_borrow_temp_before.ko"),
            stdout: b"kept\ndrop-temp\nafter\nkept\n",
            nested: true,
        },
        Case {
            name: "outer_owned_temp",
            source: include_str!("../tests/fixtures/nested_borrow/outer_owned_temp.ko"),
            stdout: b"kept\ndrop-aux\nafter\nkept\n",
            nested: true,
        },
        Case {
            name: "outer_temp_explicit_control",
            source: include_str!("../tests/fixtures/nested_borrow/outer_temp_explicit_control.ko"),
            stdout: b"kept\ndrop-aux\ndrop-temp\nafter\nkept\n",
            nested: false,
        },
        Case {
            name: "wrapper_direct",
            source: include_str!("../tests/fixtures/nested_borrow/wrapper_direct.ko"),
            stdout: b"kept\nkept\n",
            nested: true,
        },
        Case {
            name: "wrapper_explicit_control",
            source: include_str!("../tests/fixtures/nested_borrow/wrapper_explicit_control.ko"),
            stdout: b"kept\nkept\n",
            nested: false,
        },
        Case {
            name: "generic_copyable",
            source: include_str!("../tests/fixtures/nested_borrow/generic_copyable.ko"),
            stdout: b"seven\nseven\n",
            nested: true,
        },
        Case {
            name: "generic_moveonly",
            source: include_str!("../tests/fixtures/nested_borrow/generic_moveonly.ko"),
            stdout: b"kept\nkept\n",
            nested: true,
        },
        Case {
            name: "field_projection",
            source: include_str!("../tests/fixtures/nested_borrow/field_projection.ko"),
            stdout: b"field\nconsume-source\ndrop-source\n",
            nested: true,
        },
        Case {
            name: "reuse_existing_borrow",
            source: include_str!("../tests/fixtures/nested_borrow/reuse_existing_borrow.ko"),
            stdout: b"kept\nkept\nkept\n",
            nested: true,
        },
        Case {
            name: "outer_owned_copyable_result",
            source: include_str!("../tests/fixtures/nested_borrow/outer_owned_copyable_result.ko"),
            stdout: b"kept\nkept\n",
            nested: true,
        },
        Case {
            name: "copyable_explicit_owned_control",
            source: include_str!(
                "../tests/fixtures/nested_borrow/copyable_explicit_owned_control.ko"
            ),
            stdout: b"seven\nseven\n",
            nested: false,
        },
        Case {
            name: "outer_later_operand_reads_root",
            source: include_str!(
                "../tests/fixtures/nested_borrow/outer_later_operand_reads_root.ko"
            ),
            stdout: b"eval-read\nkept\nkept\n",
            nested: true,
        },
        Case {
            name: "field_projection_explicit_control",
            source: include_str!(
                "../tests/fixtures/nested_borrow/field_projection_explicit_control.ko"
            ),
            stdout: b"field\nconsume-source\ndrop-source\n",
            nested: false,
        },
        Case {
            name: "same_root_existing_parent_control",
            source: include_str!(
                "../tests/fixtures/nested_borrow/same_root_existing_parent_control.ko"
            ),
            stdout: b"kept\nkept\nkept\n",
            nested: false,
        },
        Case {
            name: "asap_same_root_twice",
            source: include_str!("../tests/fixtures/nested_borrow/asap_same_root_twice.ko"),
            stdout: b"kept\nkept\nafter\n",
            nested: true,
        },
        Case {
            name: "resource_lexical_source",
            source: include_str!("../tests/fixtures/nested_borrow/resource_lexical_source.ko"),
            stdout: b"field\nafter\ndrop-source\n",
            nested: true,
        },
        Case {
            name: "outer_owned_moveonly_result",
            source: include_str!("../tests/fixtures/nested_borrow/outer_owned_moveonly_result.ko"),
            stdout: b"kept\nkept\nowned\n",
            nested: true,
        },
        Case {
            name: "borrow_entry_source",
            source: include_str!("../tests/fixtures/nested_borrow/borrow_entry_source.ko"),
            stdout: b"kept\nkept\nkept\n",
            nested: true,
        },
        Case {
            name: "inner_named_resource_slots",
            source: include_str!("../tests/fixtures/nested_borrow/inner_named_resource_slots.ko"),
            stdout: b"eval-right\neval-left\ndrop-right\ndrop-left\nkept\nkept\n",
            nested: true,
        },
        Case {
            name: "existing_auxiliary_parent",
            source: include_str!("../tests/fixtures/nested_borrow/existing_auxiliary_parent.ko"),
            stdout: b"kept\naux\nkept\n",
            nested: true,
        },
        Case {
            name: "wrapper_field_payload",
            source: include_str!("../tests/fixtures/nested_borrow/wrapper_field_payload.ko"),
            stdout: b"field\nconsume-source\ndrop-source\n",
            nested: true,
        },
        Case {
            name: "different_outer_lifetimes",
            source: include_str!("../tests/fixtures/nested_borrow/different_outer_lifetimes.ko"),
            stdout: b"kept\nkept\nkept\n",
            nested: true,
        },
        Case {
            name: "different_outer_lifetimes_asap",
            source: include_str!(
                "../tests/fixtures/nested_borrow/different_outer_lifetimes_asap.ko"
            ),
            stdout: b"kept\nkept\n",
            nested: true,
        },
    ]
}
