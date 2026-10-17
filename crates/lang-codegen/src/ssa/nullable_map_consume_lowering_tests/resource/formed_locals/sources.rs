//! Frozen reviewed nine-program source matrix; all strings are stored verbatim.

#[derive(Clone, Copy, Debug)]
pub(super) enum Shape {
    Map,
    NullableMap,
    NullableMapOfNullableInt,
    Function,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Initializer {
    EmptyMap,
    Null,
    Return,
    Name(usize),
    Lambda,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Owner {
    Function(&'static str),
    Lambda,
}

#[derive(Debug)]
pub(super) struct Local {
    pub name: &'static str,
    pub shape: Shape,
    pub initializer: Initializer,
    pub owner: Owner,
}

pub(super) struct Case {
    pub id: &'static str,
    pub path: &'static str,
    pub single: &'static str,
    pub provider: &'static str,
    pub consumer: &'static str,
    pub constructions: usize,
    pub locals: &'static [Local],
}

pub(super) const CASES: &[Case] = &[
    Case {
        id: "P1",
        path: "formed_p1_prefix_present.ko",
        single: r#"fun main(): Unit {
  println("prefix")
  val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
}
"#,
        provider: r#"package p
fun main(): Unit {
  println("prefix")
  val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        constructions: 1,
        locals: &[Local {
            name: "source",
            shape: Shape::NullableMap,
            initializer: Initializer::EmptyMap,
            owner: Owner::Function("main"),
        }],
    },
    Case {
        id: "P2",
        path: "formed_p2_seed_then_move.ko",
        single: r#"fun main(): Unit {
  val seed = mutableMapOf<Int, Int>()
  val source: MutableMap<Int, Int>? = seed
  val delivered = source
}
"#,
        provider: r#"package p
fun main(): Unit {
  val seed = mutableMapOf<Int, Int>()
  val source: MutableMap<Int, Int>? = seed
  val delivered = source
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        constructions: 1,
        locals: &[
            Local {
                name: "seed",
                shape: Shape::Map,
                initializer: Initializer::EmptyMap,
                owner: Owner::Function("main"),
            },
            Local {
                name: "source",
                shape: Shape::NullableMap,
                initializer: Initializer::Name(0),
                owner: Owner::Function("main"),
            },
            Local {
                name: "delivered",
                shape: Shape::NullableMap,
                initializer: Initializer::Name(1),
                owner: Owner::Function("main"),
            },
        ],
    },
    Case {
        id: "P3",
        path: "formed_p3_branch_present.ko",
        single: r#"fun run(flag: Boolean): Unit {
  if (flag) {
    val branch: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  }
}
fun main(): Unit { run(true) }
"#,
        provider: r#"package p
fun run(flag: Boolean): Unit {
  if (flag) {
    val branch: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
  }
}
fun main(): Unit { run(true) }
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        constructions: 1,
        locals: &[Local {
            name: "branch",
            shape: Shape::NullableMap,
            initializer: Initializer::EmptyMap,
            owner: Owner::Function("run"),
        }],
    },
    Case {
        id: "K1",
        path: "unreachable_nullable_map.ko",
        single: r#"fun main(): Unit { return; val dead: MutableMap<Int, Int?>? = null }
"#,
        provider: r#"package p
fun main(): Unit { return; val dead: MutableMap<Int, Int?>? = null }
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        constructions: 0,
        locals: &[Local {
            name: "dead",
            shape: Shape::NullableMapOfNullableInt,
            initializer: Initializer::Null,
            owner: Owner::Function("main"),
        }],
    },
    Case {
        id: "K2",
        path: "formed_k2_initializer_return.ko",
        single: r#"fun main(): Unit {
  val dead: MutableMap<Int, Int?>? = return
}
"#,
        provider: r#"package p
fun main(): Unit {
  val dead: MutableMap<Int, Int?>? = return
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        constructions: 0,
        locals: &[Local {
            name: "dead",
            shape: Shape::NullableMapOfNullableInt,
            initializer: Initializer::Return,
            owner: Owner::Function("main"),
        }],
    },
    Case {
        id: "K3",
        path: "formed_k3_constant_false_unsupported.ko",
        single: r#"fun main(): Unit {
  if (false) {
    val branch: MutableMap<Int, Int?>? = null
  }
}
"#,
        provider: r#"package p
fun main(): Unit {
  if (false) {
    val branch: MutableMap<Int, Int?>? = null
  }
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        constructions: 0,
        locals: &[Local {
            name: "branch",
            shape: Shape::NullableMapOfNullableInt,
            initializer: Initializer::Null,
            owner: Owner::Function("main"),
        }],
    },
    Case {
        id: "K4",
        path: "formed_k4_lambda_scope.ko",
        single: r#"fun main(): Unit {
  val action: () -> Unit = {
    val inner: MutableMap<Int, Int?>? = null
  }
}
"#,
        provider: r#"package p
fun main(): Unit {
  val action: () -> Unit = {
    val inner: MutableMap<Int, Int?>? = null
  }
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        constructions: 0,
        locals: &[
            Local {
                name: "action",
                shape: Shape::Function,
                initializer: Initializer::Lambda,
                owner: Owner::Function("main"),
            },
            Local {
                name: "inner",
                shape: Shape::NullableMapOfNullableInt,
                initializer: Initializer::Null,
                owner: Owner::Lambda,
            },
        ],
    },
    Case {
        id: "K5",
        path: "formed_k5_shadow_dead_tail.ko",
        single: r#"fun main(): Unit {
  val source: MutableMap<Int, Int>? = null;
  {
    return
    val source: MutableMap<Int, Int?>? = null
  }
}
"#,
        provider: r#"package p
fun main(): Unit {
  val source: MutableMap<Int, Int>? = null;
  {
    return
    val source: MutableMap<Int, Int?>? = null
  }
}
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        constructions: 0,
        locals: &[
            Local {
                name: "source",
                shape: Shape::NullableMap,
                initializer: Initializer::Null,
                owner: Owner::Function("main"),
            },
            Local {
                name: "source",
                shape: Shape::NullableMapOfNullableInt,
                initializer: Initializer::Null,
                owner: Owner::Function("main"),
            },
        ],
    },
    Case {
        id: "K6",
        path: "formed_k6_loop_existing_header.ko",
        single: r#"fun run(flag: Boolean): Unit {
  val warm: MutableMap<Int, Int>? = null
  while (flag) {
    val local: MutableMap<Int, Int>? = null
    break
  }
}
fun main(): Unit { run(false) }
"#,
        provider: r#"package p
fun run(flag: Boolean): Unit {
  val warm: MutableMap<Int, Int>? = null
  while (flag) {
    val local: MutableMap<Int, Int>? = null
    break
  }
}
fun main(): Unit { run(false) }
"#,
        consumer: r#"package q
fun entry(): Unit { p.main() }
"#,
        constructions: 0,
        locals: &[
            Local {
                name: "warm",
                shape: Shape::NullableMap,
                initializer: Initializer::Null,
                owner: Owner::Function("run"),
            },
            Local {
                name: "local",
                shape: Shape::NullableMap,
                initializer: Initializer::Null,
                owner: Owner::Function("run"),
            },
        ],
    },
];
