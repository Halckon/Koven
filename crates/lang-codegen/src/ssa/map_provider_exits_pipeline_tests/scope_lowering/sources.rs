//! A1 references fourteen frozen originals; it does not register later scope groups.
pub(crate) struct Case {
    pub label: &'static str,
    pub gate: u8,
    pub provider: bool,
    pub source: &'static str,
    pub golden: &'static str,
}
macro_rules! pair {
    ($gate:literal,$id:literal) => {
        [
            Case {
                label: concat!($id, "-for"),
                gate: $gate,
                provider: true,
                source: include_str!(concat!(
                    "../../../../tests/fixtures/map-provider-scope-0331/",
                    $id,
                    "-for.ko"
                )),
                golden: include_str!(concat!(
                    "../../../../tests/fixtures/map-provider-scope-0331/",
                    $id,
                    "-for.stdout"
                )),
            },
            Case {
                label: concat!($id, "-control"),
                gate: $gate,
                provider: false,
                source: include_str!(concat!(
                    "../../../../tests/fixtures/map-provider-scope-0331/",
                    $id,
                    "-control.ko"
                )),
                golden: include_str!(concat!(
                    "../../../../tests/fixtures/map-provider-scope-0331/",
                    $id,
                    "-control.stdout"
                )),
            },
        ]
    };
}
pub(crate) fn sources() -> Vec<Case> {
    [
        pair!(2, "P02"),
        pair!(12, "P12"),
        pair!(13, "P13"),
        pair!(14, "P14"),
        pair!(16, "P16"),
        pair!(17, "P17"),
        pair!(18, "P18"),
    ]
    .into_iter()
    .flatten()
    .collect()
}
