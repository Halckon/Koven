//! Parser lexical-poison mutation matrices shared definitions.

#[derive(Clone, Copy)]
pub(crate) struct Poison {
    pub(crate) name: &'static str,
    pub(crate) text: &'static str,
    pub(crate) code: &'static str,
}

pub(crate) const LEXICAL_POISONS: &[Poison] = &[
    Poison {
        name: "invalid character",
        text: "#",
        code: "L0001",
    },
    Poison {
        name: "future reserved word",
        text: "async",
        code: "L0002",
    },
    Poison {
        name: "invalid character literal",
        text: "'ab'",
        code: "L0007",
    },
    Poison {
        name: "invalid numeric literal",
        text: "1e3",
        code: "L0008",
    },
];
