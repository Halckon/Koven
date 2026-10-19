//! 已审的八份普通源码；每份分别经 single/unit 真实阶段分析。
use lang_frontend::type_checking::{BuiltinType, IntrinsicTypeConstructor};

#[derive(Clone, Copy, Debug)]
pub(super) enum SourceKind {
    Owned,
    Borrowed,
    Temporary,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Expected {
    Accepted(SourceKind),
    Rejected(&'static str),
}

impl Expected {
    pub(super) fn is_success(self) -> bool {
        matches!(self, Self::Accepted(_))
    }
}

pub(super) struct Case {
    pub label: &'static str,
    pub text: &'static str,
    pub constructor: IntrinsicTypeConstructor,
    pub component: BuiltinType,
    pub expected: Expected,
    pub reads: [usize; 2],
    pub call: Option<&'static str>,
    pub source: &'static str,
}

const SOURCE_1: &str = r#"class Guard { deinit() {} }
fun scan(own source: Map<Int, Int>): Unit {
  for ((key, value) in source) {
    val local = Guard()
    val keyCopy: Int = key
    val valueCopy: Int = value
  }
}
"#;

const SOURCE_2: &str = r#"class Guard { deinit() {} }
fun scan(source: MutableMap<Int, Int>): Unit {
  for ((key, value) in source) {
    val local = Guard()
    val keyCopy: Int = key
    val valueCopy: Int = value
    continue
  }
}
"#;

const SOURCE_3: &str = r#"class Guard { deinit() {} }
fun read(key: String, value: String): Unit {}
fun makeMap(): Map<String, String> {
  var source = mutableMapOf<String, String>()
  source.put("key", "value")
  val result = source.consume()
  return result
}
fun scan(): Unit {
  for ((key, value) in makeMap()) {
    val local = Guard()
    read(key, value)
    break
  }
}
"#;

const SOURCE_4: &str = r#"class Guard { deinit() {} }
fun read(key: String, value: String): Unit {}
fun makeMutable(): MutableMap<String, String> {
  var source = mutableMapOf<String, String>()
  source.put("key", "value")
  return source
}
fun scan(): Unit {
  for ((key, value) in makeMutable()) {
    val local = Guard()
    read(key, value)
    return
  }
}
"#;

const SOURCE_5: &str = r#"fun take(own text: String): Unit {}
fun scan(source: Map<String, String>): Unit {
  for ((key, value) in source) {
    take(key)
    break
  }
}
"#;

const SOURCE_6: &str = r#"fun take(own text: String): Unit {}
fun scan(source: Map<String, String>): Unit {
  for ((key, value) in source) {
    take(value)
    break
  }
}
"#;

const SOURCE_7: &str = r#"fun scan(own input: MutableMap<String, String>): Unit {
  val source = input
  for ((key, value) in source) {
    val result = source.consume()
    break
  }
}
"#;

const SOURCE_8: &str = r#"fun scan(own source: Map<Int, Int>): Map<Int, Int> {
  for ((key, value) in source) {
    return source
  }
  return source
}
"#;

pub(super) const CASES: [Case; 8] = [
    Case {
        label: "P1-owned-map-normal",
        text: SOURCE_1,
        constructor: IntrinsicTypeConstructor::Map,
        component: BuiltinType::Int,
        expected: Expected::Accepted(SourceKind::Owned),
        reads: [1, 1],
        call: None,
        source: "source",
    },
    Case {
        label: "P2-borrowed-mutable-continue",
        text: SOURCE_2,
        constructor: IntrinsicTypeConstructor::MutableMap,
        component: BuiltinType::Int,
        expected: Expected::Accepted(SourceKind::Borrowed),
        reads: [1, 1],
        call: None,
        source: "source",
    },
    Case {
        label: "P3-temporary-map-break",
        text: SOURCE_3,
        constructor: IntrinsicTypeConstructor::Map,
        component: BuiltinType::String,
        expected: Expected::Accepted(SourceKind::Temporary),
        reads: [1, 1],
        call: Some("read"),
        source: "makeMap()",
    },
    Case {
        label: "P4-temporary-mutable-return",
        text: SOURCE_4,
        constructor: IntrinsicTypeConstructor::MutableMap,
        component: BuiltinType::String,
        expected: Expected::Accepted(SourceKind::Temporary),
        reads: [1, 1],
        call: Some("read"),
        source: "makeMutable()",
    },
    Case {
        label: "N1-key-owned-move",
        text: SOURCE_5,
        constructor: IntrinsicTypeConstructor::Map,
        component: BuiltinType::String,
        expected: Expected::Rejected("L0133"),
        reads: [1, 0],
        call: Some("take"),
        source: "source",
    },
    Case {
        label: "N2-value-owned-move",
        text: SOURCE_6,
        constructor: IntrinsicTypeConstructor::Map,
        component: BuiltinType::String,
        expected: Expected::Rejected("L0133"),
        reads: [0, 1],
        call: Some("take"),
        source: "source",
    },
    Case {
        label: "N3-active-local-consume",
        text: SOURCE_7,
        constructor: IntrinsicTypeConstructor::MutableMap,
        component: BuiltinType::String,
        expected: Expected::Rejected("L0135"),
        reads: [0, 0],
        call: None,
        source: "source",
    },
    Case {
        label: "N4-active-owned-return",
        text: SOURCE_8,
        constructor: IntrinsicTypeConstructor::Map,
        component: BuiltinType::Int,
        expected: Expected::Rejected("L0135"),
        reads: [0, 0],
        call: None,
        source: "source",
    },
];
