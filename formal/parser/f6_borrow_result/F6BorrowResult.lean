namespace F6BorrowResult

/-- Explicit return result dispatch in `borrow_result.rs`. -/
inductive ExplicitResultAction where
  | plainTypeOnly
  | missingBorrowFrom
  | parseBorrowSource
  | parseCarrierSource
  deriving DecidableEq, Repr

/-- Abstract token classes relevant to `borrow_result.rs` and `declaration_head.rs`. -/
inductive HeadToken where
  | kwVal
  | kwVar
  | symQuestion
  | symDot
  | symLess
  | other
  deriving DecidableEq, Repr

def explicitResultAction (hasBorrow hasFrom : Bool) : ExplicitResultAction :=
  match hasBorrow, hasFrom with
  | false, false => .plainTypeOnly
  | true, false => .missingBorrowFrom
  | true, true => .parseBorrowSource
  | false, true => .parseCarrierSource

def isBorrowLocalBindingKeyword (tok : HeadToken) : Bool :=
  match tok with
  | .kwVal | .kwVar => true
  | _ => false

def isBorrowLocalStartTail (next1 : HeadToken) (next2 : Option HeadToken) : Bool :=
  isBorrowLocalBindingKeyword next1 ||
    (next1 == .symQuestion &&
      match next2 with
      | some k => isBorrowLocalBindingKeyword k
      | none => false)

def borrowLocalContractIsInvalid (isBorrow immutable conditional : Bool) : Bool :=
  !isBorrow || !immutable || conditional

def isExtensionReceiverFollow (tok : HeadToken) : Bool :=
  match tok with
  | .symDot | .symLess | .symQuestion => true
  | _ => false

def isExtensionReceiverHead (currentIsIdent : Bool) (next1 : HeadToken) : Bool :=
  currentIsIdent && isExtensionReceiverFollow next1

/-- B1: Complete 4-way specification of explicit return result dispatch. -/
theorem explicitResultAction_spec (hasBorrow hasFrom : Bool) :
    (explicitResultAction hasBorrow hasFrom = .plainTypeOnly ↔
      hasBorrow = false ∧ hasFrom = false) ∧
    (explicitResultAction hasBorrow hasFrom = .missingBorrowFrom ↔
      hasBorrow = true ∧ hasFrom = false) ∧
    (explicitResultAction hasBorrow hasFrom = .parseBorrowSource ↔
      hasBorrow = true ∧ hasFrom = true) ∧
    (explicitResultAction hasBorrow hasFrom = .parseCarrierSource ↔
      hasBorrow = false ∧ hasFrom = true) := by
  cases hasBorrow <;> cases hasFrom <;> simp [explicitResultAction]

/-- B2: Exact iff characterization of `isBorrowLocalStartTail`. -/
theorem borrowLocalStartTail_spec (next1 : HeadToken) (next2 : Option HeadToken) :
    isBorrowLocalStartTail next1 next2 = true ↔
      (next1 = .kwVal ∨ next1 = .kwVar) ∨
      (next1 = .symQuestion ∧ (next2 = some .kwVal ∨ next2 = some .kwVar)) := by
  cases next1 <;> cases next2 with
  | none => simp [isBorrowLocalStartTail, isBorrowLocalBindingKeyword]
  | some k => cases k <;> simp [isBorrowLocalStartTail, isBorrowLocalBindingKeyword]

/-- B3: Ordinary `borrow val` is the unique valid local borrow binding combination. -/
theorem borrowLocalContractInvalid_spec (isBorrow immutable conditional : Bool) :
    borrowLocalContractIsInvalid isBorrow immutable conditional = false ↔
      (isBorrow = true ∧ immutable = true ∧ conditional = false) := by
  cases isBorrow <;> cases immutable <;> cases conditional <;>
    simp [borrowLocalContractIsInvalid]

/-- B4: Exact iff characterization of `isExtensionReceiverHead`. -/
theorem extensionReceiverHead_spec (currentIsIdent : Bool) (next1 : HeadToken) :
    isExtensionReceiverHead currentIsIdent next1 = true ↔
      currentIsIdent = true ∧ (next1 = .symDot ∨ next1 = .symLess ∨ next1 = .symQuestion) := by
  cases currentIsIdent <;> cases next1 <;>
    simp [isExtensionReceiverHead, isExtensionReceiverFollow]

/-- B5: Binding keywords never trigger extension-receiver follow, and `?` alone without a binding keyword never triggers local borrow start. -/
theorem borrowLocal_and_extensionReceiver_disjoint (tok : HeadToken) :
    (isBorrowLocalBindingKeyword tok = true → isExtensionReceiverFollow tok = false) ∧
    (isBorrowLocalStartTail .symQuestion (some tok) = true ↔ isBorrowLocalBindingKeyword tok = true) := by
  cases tok <;> simp [isBorrowLocalBindingKeyword, isExtensionReceiverFollow, isBorrowLocalStartTail]

end F6BorrowResult
