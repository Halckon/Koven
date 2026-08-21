//! Parser scalar-replacement matrices shared deterministic alphabet.

#[derive(Clone, Copy)]
pub(crate) struct ScalarReplacement {
    pub(crate) name: &'static str,
    pub(crate) text: &'static str,
}

pub(crate) const SCALAR_REPLACEMENTS: &[ScalarReplacement] = &[
    ScalarReplacement {
        name: "identifier",
        text: "a",
    },
    ScalarReplacement {
        name: "number",
        text: "0",
    },
    ScalarReplacement {
        name: "poison",
        text: "#",
    },
    ScalarReplacement {
        name: "string delimiter",
        text: "\"",
    },
    ScalarReplacement {
        name: "character delimiter",
        text: "'",
    },
    ScalarReplacement {
        name: "escape",
        text: "\\",
    },
    ScalarReplacement {
        name: "interpolation",
        text: "$",
    },
    ScalarReplacement {
        name: "comment slash",
        text: "/",
    },
    ScalarReplacement {
        name: "comment star",
        text: "*",
    },
    ScalarReplacement {
        name: "left brace",
        text: "{",
    },
    ScalarReplacement {
        name: "right brace",
        text: "}",
    },
    ScalarReplacement {
        name: "line break",
        text: "\n",
    },
    ScalarReplacement {
        name: "multibyte unicode",
        text: "界",
    },
];
