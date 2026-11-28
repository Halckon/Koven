/-!
# F6 Slice 24: `class.rs` / `class/kernel.rs` 分类器与成员策略纯内核机器证明模型

本文件对应 `crates/lang-frontend/src/parser/engine/class/kernel.rs` 与 `boundary.rs` 中的纯栈值策略内核：
1. `ClassifierKindTag`（构造器支持/必需、类体必需、类型参数禁止）
2. `ClassMemberContext`（`allows_override`/`allows_companion`/`allows_constant`/`allows_deinit` 与 `member_receiver_policy`）
3. `member_visibility_invalid_for_fun` 与 `member_companion_is_unsupported`
4. `is_classifier_name_boundary_kind` 与 `is_enum_body_stop_kind`
5. `is_class_field_invalid_prefix_kind` 与 `is_enum_variant_param_invalid_prefix_kind`

证据等级：`proved (model, complete finite domain)`（5 条有限枚举/布尔全空间定理，零 Mathlib，仅核心公理）。
-/

namespace F6Class

inductive ClassifierKindTag where
  | valueClass
  | plainClass
  | iface
  | enumClass
  | obj
  deriving DecidableEq, Repr

inductive ClassMemberContext where
  | valueClass
  | plainClass
  | iface
  | enumClass
  | obj
  | companion
  deriving DecidableEq, Repr

inductive ReceiverModifierPolicy where
  | notRecognized
  | rejected
  | allowed
  deriving DecidableEq, Repr

inductive ClassToken where
  | eof
  | ident
  | kwVal
  | kwVar
  | kwVararg
  | symLess
  | symLeftParen
  | symColon
  | symLeftBrace
  | symRightBrace
  | symSemicolon
  | other
  deriving DecidableEq, Repr

def classifierSupportsConstructor (tag : ClassifierKindTag) : Bool :=
  match tag with
  | .valueClass | .plainClass => true
  | _ => false

def classifierRequiresConstructor (tag : ClassifierKindTag) : Bool :=
  tag == .valueClass

def classifierRequiresBody (tag : ClassifierKindTag) : Bool :=
  tag == .enumClass

def classifierForbidsTypeParameters (tag : ClassifierKindTag) : Bool :=
  tag == .obj

def allowsOverride (ctx : ClassMemberContext) : Bool :=
  match ctx with
  | .valueClass | .plainClass | .enumClass | .obj => true
  | .iface | .companion => false

def allowsCompanion (ctx : ClassMemberContext) : Bool :=
  match ctx with
  | .obj | .companion => false
  | _ => true

def allowsConstant (ctx : ClassMemberContext) : Bool :=
  match ctx with
  | .obj | .companion => true
  | _ => false

def allowsDeinit (ctx : ClassMemberContext) : Bool :=
  ctx == .plainClass

def memberReceiverPolicy (ctx : ClassMemberContext) : ReceiverModifierPolicy :=
  match ctx with
  | .companion => .rejected
  | _ => .allowed

def memberVisibilityInvalidForFun
    (ctx : ClassMemberContext) (isFun isInternalOrPrivate : Bool) : Bool :=
  ctx == .iface && isFun && isInternalOrPrivate

def memberCompanionIsUnsupported
    (ctx : ClassMemberContext) (companionSeen : Bool) : Bool :=
  !allowsCompanion ctx || companionSeen

def isClassifierNameBoundary (tok : ClassToken) : Bool :=
  match tok with
  | .eof | .symLess | .symLeftParen | .symColon | .symLeftBrace | .symRightBrace => true
  | _ => false

def isEnumBodyStop (tok : ClassToken) : Bool :=
  match tok with
  | .symRightBrace | .symSemicolon | .eof => true
  | _ => false

def isClassFieldInvalidPrefix
    (tok : ClassToken) (isModeIdent : Bool) (next1 : Option ClassToken) : Bool :=
  (isModeIdent && (next1 == some .kwVal || next1 == some .kwVar)) || tok == .kwVararg

def isEnumVariantParamInvalidPrefix
    (tok : ClassToken) (isModeIdent : Bool) (next1 : Option ClassToken) : Bool :=
  (isModeIdent && next1 == some .ident) ||
    tok == .kwVal || tok == .kwVar || tok == .kwVararg

theorem classifierKindPolicy_spec (tag : ClassifierKindTag) :
    (classifierSupportsConstructor tag = true ↔ (tag = .valueClass ∨ tag = .plainClass)) ∧
    (classifierRequiresConstructor tag = true ↔ tag = .valueClass) ∧
    (classifierRequiresConstructor tag = true → classifierSupportsConstructor tag = true) ∧
    (classifierRequiresBody tag = true ↔ tag = .enumClass) ∧
    (classifierForbidsTypeParameters tag = true ↔ tag = .obj) := by
  cases tag <;> decide

theorem classMemberContext_policy_spec (ctx : ClassMemberContext) :
    (allowsCompanion ctx = !allowsConstant ctx) ∧
    (allowsDeinit ctx = true ↔ ctx = .plainClass) ∧
    (allowsDeinit ctx = true → allowsOverride ctx = true ∧ allowsCompanion ctx = true) ∧
    (memberReceiverPolicy ctx = .rejected ↔ ctx = .companion) ∧
    (memberReceiverPolicy ctx = .allowed ↔ ctx ≠ .companion) := by
  cases ctx <;> decide

theorem memberModifierAndCompanionGuards_spec
    (ctx : ClassMemberContext) (isFun isInternalOrPrivate companionSeen : Bool) :
    (memberVisibilityInvalidForFun ctx isFun isInternalOrPrivate = true ↔
      ctx = .iface ∧ isFun = true ∧ isInternalOrPrivate = true) ∧
    (memberCompanionIsUnsupported ctx companionSeen = true ↔
      allowsCompanion ctx = false ∨ companionSeen = true) := by
  cases ctx <;> cases isFun <;> cases isInternalOrPrivate <;> cases companionSeen <;> decide

theorem classifierNameBoundary_and_enumBodyStop_spec (tok : ClassToken) :
    (isClassifierNameBoundary tok = true ↔
      tok = .eof ∨ tok = .symLess ∨ tok = .symLeftParen ∨
      tok = .symColon ∨ tok = .symLeftBrace ∨ tok = .symRightBrace) ∧
    (isEnumBodyStop tok = true ↔
      tok = .symRightBrace ∨ tok = .symSemicolon ∨ tok = .eof) ∧
    (isEnumBodyStop tok = true → tok ≠ .symSemicolon → isClassifierNameBoundary tok = true) := by
  cases tok <;> decide

theorem fieldAndEnumParamInvalidPrefix_spec
    (tok : ClassToken) (isModeIdent : Bool) (next1 : Option ClassToken) :
    (isClassFieldInvalidPrefix tok isModeIdent next1 = true ↔
      (isModeIdent = true ∧ (next1 = some .kwVal ∨ next1 = some .kwVar)) ∨
        tok = .kwVararg) ∧
    (isEnumVariantParamInvalidPrefix tok isModeIdent next1 = true ↔
      (isModeIdent = true ∧ next1 = some .ident) ∨
        tok = .kwVal ∨ tok = .kwVar ∨ tok = .kwVararg) := by
  cases tok <;> cases isModeIdent <;> cases next1 with
  | none => decide
  | some n => cases n <;> decide

end F6Class
