/-!
# F6 Slice 26: `boundary.rs` 起始集层级、`NameContext` 停止一致性、修饰符最小偏移与恢复映射机器证明模型

本文件对应 `crates/lang-frontend/src/parser/engine/boundary.rs` 中的 5 组纯判定与计算内核：
1. 词法起始集层级（`control_expression_start_kind`、`classifier_declaration_start_kind`、`class_field_start_kind`、`simple_declaration_start_kind`、`class_member_start_kind`、`file_construct_start_kind`）
2. `NameContext::is_stop_kind` 与 `NameContext::recovery_stops` 的双向一致性
3. `declaration_modifier_start`（三路可选修饰符起点的无界自然数最小值）
4. `DeclarationStops::keeps_unproved_angle_comma`（委托恢复与前导尖括号逗号保留守卫）
5. `TypeStops::supertype_element_recovery`（基类型列表元素恢复掩码与 `ClassifierHeaderOwner` 归属映射）

证据等级：
- 1 条 `proved (model, unbounded)`（三路 `Option Nat` 最小值定理）
- 4 条 `proved (model, complete finite domain)`（有限枚举/布尔全空间）
零 Mathlib，仅核心公理。
-/

namespace F6Boundary

inductive BoundaryTok where
  | kwIf | kwWhen | kwReturn | kwBreak | kwContinue | kwSuper
  | kwVal | kwVar | kwConst | kwFun
  | kwClass | kwInterface | kwEnum | kwObject
  | kwCompanion
  | kwPublic | kwInternal | kwPrivate | kwOverride
  | kwPackage | kwImport
  | kwWhile | kwFor | kwTypealias | kwExtern
  | symColon | symEqual | symLeftParen | symRightParen
  | symLeftBrace | symRightBrace | symComma | symGreater
  | other
  deriving DecidableEq, Repr

def isControlExprStart : BoundaryTok → Bool
  | .kwIf | .kwWhen | .kwReturn | .kwBreak | .kwContinue | .kwSuper => true
  | _ => false

def isClassifierDeclStart : BoundaryTok → Bool
  | .kwClass | .kwInterface | .kwEnum | .kwObject => true
  | _ => false

def isClassFieldStart : BoundaryTok → Bool
  | .kwVal | .kwVar | .kwPublic | .kwInternal | .kwPrivate | .kwOverride => true
  | _ => false

def isSimpleDeclStart : BoundaryTok → Bool
  | .kwVal | .kwVar | .kwConst | .kwFun
  | .kwClass | .kwInterface | .kwEnum | .kwObject
  | .kwPublic | .kwInternal | .kwPrivate | .kwOverride => true
  | _ => false

def isClassMemberStart (tok : BoundaryTok) : Bool :=
  isClassifierDeclStart tok ||
    match tok with
    | .kwFun | .kwConst | .kwVal | .kwVar | .kwCompanion
    | .kwPublic | .kwInternal | .kwPrivate | .kwOverride => true
    | _ => false

def isFileConstructStart (tok : BoundaryTok) : Bool :=
  isSimpleDeclStart tok ||
    match tok with
    | .kwPackage | .kwImport => true
    | _ => false

def isUnsupportedBlockElem : BoundaryTok → Bool
  | .kwWhile | .kwFor | .kwFun | .kwConst | .kwClass | .kwInterface
  | .kwEnum | .kwObject | .kwTypealias | .kwExtern
  | .kwPublic | .kwInternal | .kwPrivate | .kwOverride => true
  | _ => false

/-- 有限域定理（B1）：词法起始集满足严格包含链与控制表达式互斥性。 -/
theorem starterHierarchy_and_disjointness_spec (tok : BoundaryTok) :
    (isClassifierDeclStart tok = true → isSimpleDeclStart tok = true) ∧
    (isSimpleDeclStart tok = true → isFileConstructStart tok = true) ∧
    (isClassifierDeclStart tok = true → isClassMemberStart tok = true) ∧
    (isClassFieldStart tok = true → isSimpleDeclStart tok = true) ∧
    (isClassFieldStart tok = true → isClassMemberStart tok = true) ∧
    (isControlExprStart tok = true → isFileConstructStart tok = false) ∧
    (isControlExprStart tok = true → isClassMemberStart tok = false) := by
  cases tok <;> decide

inductive NameContext where
  | declaration
  | localDeclaration
  | typeParameter
  | valueParameter
  deriving DecidableEq, Repr

def nameContextIsStop (ctx : NameContext) (tok : BoundaryTok) : Bool :=
  match ctx with
  | .declaration =>
      match tok with
      | .symColon | .symEqual | .symLeftParen => true
      | _ => false
  | .localDeclaration =>
      isUnsupportedBlockElem tok ||
        match tok with
        | .symColon | .symEqual | .symLeftParen | .symLeftBrace | .symRightBrace
        | .kwVal | .kwVar => true
        | _ => false
  | .typeParameter =>
      match tok with
      | .symColon | .symComma | .symGreater => true
      | _ => false
  | .valueParameter =>
      match tok with
      | .symColon | .symComma | .symRightParen => true
      | _ => false

def nameContextRecoveryContains (ctx : NameContext) (tok : BoundaryTok) : Bool :=
  let hasColon := true
  let hasEqual := ctx == .declaration || ctx == .localDeclaration
  let hasLeftParen := ctx == .declaration || ctx == .localDeclaration
  let hasLeftBrace := ctx == .localDeclaration
  let hasRightBrace := ctx == .localDeclaration
  let hasBlockElem := ctx == .localDeclaration
  let hasComma := ctx == .typeParameter || ctx == .valueParameter
  let hasGreater := ctx == .typeParameter
  let hasRightParen := ctx == .valueParameter
  match tok with
  | .symColon => hasColon
  | .symEqual => hasEqual
  | .symLeftParen => hasLeftParen
  | .symLeftBrace => hasLeftBrace
  | .symRightBrace => hasRightBrace
  | .symComma => hasComma
  | .symGreater => hasGreater
  | .symRightParen => hasRightParen
  | .kwVal | .kwVar => hasBlockElem
  | t => hasBlockElem && isUnsupportedBlockElem t

/-- 有限域定理（B2）：全部 4 种 `NameContext` 下，`is_stop` 与 `recovery_stops` 的 hard/soft 命中完全等价。 -/
theorem nameContext_isStop_iff_recoveryStops_spec (ctx : NameContext) (tok : BoundaryTok) :
    nameContextIsStop ctx tok = nameContextRecoveryContains ctx tok := by
  cases ctx <;> cases tok <;> decide

def optMin (a b : Option Nat) : Option Nat :=
  match a, b with
  | none, y => y
  | x, none => x
  | some x, some y => some (if x ≤ y then x else y)

def declarationModifierStart (vis ovr recv : Option Nat) : Option Nat :=
  optMin (optMin vis ovr) recv

/-- 无界自然数定理（B3）：`declarationModifierStart` 为 `none` 当且仅当三路修饰符均缺失；否则精确返回所有非空偏移的最小值且必取自其中之一。 -/
theorem declarationModifierStart_spec (vis ovr recv : Option Nat) :
    (declarationModifierStart vis ovr recv = none ↔ vis = none ∧ ovr = none ∧ recv = none) ∧
    (∀ m, declarationModifierStart vis ovr recv = some m →
      (∀ v, vis = some v → m ≤ v) ∧
      (∀ o, ovr = some o → m ≤ o) ∧
      (∀ r, recv = some r → m ≤ r) ∧
      (vis = some m ∨ ovr = some m ∨ recv = some m)) := by
  constructor
  · cases vis <;> cases ovr <;> cases recv <;> simp [declarationModifierStart, optMin]
  · intro m hm
    cases vis <;> cases ovr <;> cases recv <;>
      simp [declarationModifierStart, optMin] at hm <;>
      subst hm <;>
      simp <;>
      repeat (first | split | omega)

def keepsUnprovedAngleComma (errTarget unsupSuffix leadingFlag leading : Bool) : Bool :=
  (errTarget || unsupSuffix) || (leading && leadingFlag)

/-- 有限域定理（B4）：未证明尖括号逗号保留守卫精确充要条件，委托恢复下与 `leading` 无关。 -/
theorem keepsUnprovedAngleComma_spec
    (errTarget unsupSuffix leadingFlag leading : Bool) :
    (keepsUnprovedAngleComma errTarget unsupSuffix leadingFlag leading = true ↔
      errTarget = true ∨ unsupSuffix = true ∨ (leading = true ∧ leadingFlag = true)) ∧
    ((errTarget = true ∨ unsupSuffix = true) →
      keepsUnprovedAngleComma errTarget unsupSuffix leadingFlag leading = true) := by
  cases errTarget <;> cases unsupSuffix <;> cases leadingFlag <;> cases leading <;> decide

inductive ClassifierHeaderOwner where
  | file
  | declaration
  | member
  deriving DecidableEq, Repr

structure SupertypeRecoveryFlags where
  supertypeSuffix : Bool
  typeElement : Bool
  semicolon : Bool
  leftBrace : Bool
  classFile : Bool
  classMember : Bool
  deriving DecidableEq, Repr

def supertypeRecoveryFlags
    (hasLeftBrace hasClassBody : Bool)
    (owner : ClassifierHeaderOwner) : SupertypeRecoveryFlags :=
  { supertypeSuffix := true
    typeElement := true
    semicolon := true
    leftBrace := hasLeftBrace || hasClassBody
    classFile := owner == .file
    classMember := owner == .member }

/-- 有限域定理（B5）：`supertype_element_recovery` 恒保留基类型基础停止集，合并 `LEFT_BRACE | CLASS_BODY`，并按归属精确互斥设置 `CLASS_FILE` / `CLASS_MEMBER`。 -/
theorem supertypeElementRecovery_owner_and_flags_spec
    (hasLeftBrace hasClassBody : Bool)
    (owner : ClassifierHeaderOwner) :
    let out := supertypeRecoveryFlags hasLeftBrace hasClassBody owner
    out.supertypeSuffix = true ∧
    out.typeElement = true ∧
    out.semicolon = true ∧
    (out.leftBrace = true ↔ hasLeftBrace = true ∨ hasClassBody = true) ∧
    (out.classFile = true ↔ owner = .file) ∧
    (out.classMember = true ↔ owner = .member) ∧
    !(out.classFile && out.classMember) = true := by
  cases hasLeftBrace <;> cases hasClassBody <;> cases owner <;> decide

end F6Boundary
