//! Frozen actual attribution inputs; Single FunctionValue stays a separate 0336 goal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    New,
    Consume,
    FunctionValue,
    Updated,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    Control,
    Named,
    Direct,
}
pub(crate) struct Case {
    pub label: &'static str,
    pub source: &'static str,
    pub golden: &'static str,
    pub kind: Kind,
    pub shape: Shape,
    pub mutable: bool,
}
pub(crate) fn sources() -> Vec<Case> {
    vec![
        Case {
            label: "function-value-Map-no-for",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/function-value-Map-no-for.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/function-value-Map-no-for.stdout"
            ),
            kind: Kind::FunctionValue,
            shape: Shape::Control,
            mutable: false,
        },
        Case {
            label: "function-value-Map-named",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/function-value-Map-named.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/function-value-Map-named.stdout"
            ),
            kind: Kind::FunctionValue,
            shape: Shape::Named,
            mutable: false,
        },
        Case {
            label: "function-value-Map-direct",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/function-value-Map-direct.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/function-value-Map-direct.stdout"
            ),
            kind: Kind::FunctionValue,
            shape: Shape::Direct,
            mutable: false,
        },
        Case {
            label: "function-value-MutableMap-no-for",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/function-value-MutableMap-no-for.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/function-value-MutableMap-no-for.stdout"
            ),
            kind: Kind::FunctionValue,
            shape: Shape::Control,
            mutable: true,
        },
        Case {
            label: "function-value-MutableMap-named",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/function-value-MutableMap-named.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/function-value-MutableMap-named.stdout"
            ),
            kind: Kind::FunctionValue,
            shape: Shape::Named,
            mutable: true,
        },
        Case {
            label: "function-value-MutableMap-direct",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/function-value-MutableMap-direct.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/function-value-MutableMap-direct.stdout"
            ),
            kind: Kind::FunctionValue,
            shape: Shape::Direct,
            mutable: true,
        },
        Case {
            label: "intrinsic-empty-map-no-for",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-empty-map-no-for.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-empty-map-no-for.stdout"
            ),
            kind: Kind::New,
            shape: Shape::Control,
            mutable: false,
        },
        Case {
            label: "intrinsic-empty-map-named",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-empty-map-named.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-empty-map-named.stdout"
            ),
            kind: Kind::New,
            shape: Shape::Named,
            mutable: false,
        },
        Case {
            label: "intrinsic-empty-map-direct",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-empty-map-direct.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-empty-map-direct.stdout"
            ),
            kind: Kind::New,
            shape: Shape::Direct,
            mutable: false,
        },
        Case {
            label: "intrinsic-mutable-map-no-for",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-no-for.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-no-for.stdout"
            ),
            kind: Kind::New,
            shape: Shape::Control,
            mutable: true,
        },
        Case {
            label: "intrinsic-mutable-map-named",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-named.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-named.stdout"
            ),
            kind: Kind::New,
            shape: Shape::Named,
            mutable: true,
        },
        Case {
            label: "intrinsic-mutable-map-direct",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-direct.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-direct.stdout"
            ),
            kind: Kind::New,
            shape: Shape::Direct,
            mutable: true,
        },
        Case {
            label: "intrinsic-consumed-map-no-for",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-consumed-map-no-for.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-consumed-map-no-for.stdout"
            ),
            kind: Kind::Consume,
            shape: Shape::Control,
            mutable: false,
        },
        Case {
            label: "intrinsic-consumed-map-named",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-consumed-map-named.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-consumed-map-named.stdout"
            ),
            kind: Kind::Consume,
            shape: Shape::Named,
            mutable: false,
        },
        Case {
            label: "intrinsic-consumed-map-direct",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-consumed-map-direct.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-consumed-map-direct.stdout"
            ),
            kind: Kind::Consume,
            shape: Shape::Direct,
            mutable: false,
        },
        Case {
            label: "intrinsic-mutable-map-put-current-named",
            source: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-put-current-named.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-births-0331/intrinsic-mutable-map-put-current-named.stdout"
            ),
            kind: Kind::Updated,
            shape: Shape::Named,
            mutable: true,
        },
    ]
}
