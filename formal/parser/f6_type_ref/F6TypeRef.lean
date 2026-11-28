namespace F6TypeRef

inductive TypeRefLexemeKind where
  | eof
  | ident
  | leftParen
  | rightParen
  | other
  deriving DecidableEq, Repr

def isFunctionTypeStart
    (currentKind : TypeRefLexemeKind)
    (isMoveIdent : Bool)
    (nextKind : Option TypeRefLexemeKind) : Bool :=
  (currentKind == .ident && isMoveIdent && nextKind == some .leftParen) ||
    currentKind == .leftParen

def isExtensionReceiverDotStop
    (endsAtExtensionName : Bool)
    (nextKind next2Kind : Option TypeRefLexemeKind) : Bool :=
  endsAtExtensionName && nextKind == some .ident && next2Kind == some .leftParen

def isFunctionTypeRightParenCloser
    (currentKind : TypeRefLexemeKind)
    (currentRaw : Nat)
    (provenTypeCloser : Option Nat) : Bool :=
  currentKind == .rightParen &&
    match provenTypeCloser with
    | none => true
    | some limit => currentRaw != limit

def boundedProvenParameterCloser (closer provenTypeCloser : Option Nat) : Option Nat :=
  match closer with
  | none => none
  | some c =>
      match provenTypeCloser with
      | none => some c
      | some limit => if c < limit then some c else none

def stepStringTypePoisonEnd
    (currentEnd recoveryEnd : Nat)
    (kind : TypeRefLexemeKind)
    (lexemeEnd : Nat) : Nat × Bool :=
  if currentEnd ≥ recoveryEnd || kind == .eof then
    (currentEnd, true)
  else
    (lexemeEnd, false)

def runStringTypePoisonEnd
    (currentEnd recoveryEnd : Nat) : List (TypeRefLexemeKind × Nat) → Nat
  | [] => currentEnd
  | (kind, lexEnd) :: rest =>
      let (nextEnd, stop) := stepStringTypePoisonEnd currentEnd recoveryEnd kind lexEnd
      if stop then nextEnd else runStringTypePoisonEnd nextEnd recoveryEnd rest

/-- R1: Exact characterization of `isFunctionTypeStart`. -/
theorem isFunctionTypeStart_spec
    (currentKind : TypeRefLexemeKind)
    (isMoveIdent : Bool)
    (nextKind : Option TypeRefLexemeKind) :
    isFunctionTypeStart currentKind isMoveIdent nextKind = true ↔
      currentKind = .leftParen ∨
        (currentKind = .ident ∧ isMoveIdent = true ∧ nextKind = some .leftParen) := by
  cases currentKind <;> cases isMoveIdent <;> cases nextKind with
  | none => simp [isFunctionTypeStart]
  | some nk => cases nk <;> simp [isFunctionTypeStart]

/-- R2: Exact characterization of `isExtensionReceiverDotStop`. -/
theorem isExtensionReceiverDotStop_spec
    (endsAtExtensionName : Bool)
    (nextKind next2Kind : Option TypeRefLexemeKind) :
    isExtensionReceiverDotStop endsAtExtensionName nextKind next2Kind = true ↔
      endsAtExtensionName = true ∧ nextKind = some .ident ∧ next2Kind = some .leftParen := by
  cases endsAtExtensionName <;> cases nextKind with
  | none => simp [isExtensionReceiverDotStop]
  | some n1 =>
      cases n1 <;> cases next2Kind with
      | none => simp [isExtensionReceiverDotStop]
      | some n2 => cases n2 <;> simp [isExtensionReceiverDotStop]

/-- R3: Exact characterization of `isFunctionTypeRightParenCloser`: an inner function type never steals an outer `provenTypeCloser`. -/
theorem isFunctionTypeRightParenCloser_spec
    (currentKind : TypeRefLexemeKind)
    (currentRaw : Nat)
    (provenTypeCloser : Option Nat) :
    isFunctionTypeRightParenCloser currentKind currentRaw provenTypeCloser = true ↔
      currentKind = .rightParen ∧ ∀ limit, provenTypeCloser = some limit → currentRaw ≠ limit := by
  cases currentKind <;> cases provenTypeCloser with
  | none => simp [isFunctionTypeRightParenCloser]
  | some limit => simp [isFunctionTypeRightParenCloser]

/-- R4: Exact characterization of `boundedProvenParameterCloser`. -/
theorem boundedProvenParameterCloser_spec
    (closer provenTypeCloser : Option Nat) (c : Nat) :
    boundedProvenParameterCloser closer provenTypeCloser = some c ↔
      closer = some c ∧ ∀ limit, provenTypeCloser = some limit → c < limit := by
  cases closer with
  | none => simp [boundedProvenParameterCloser]
  | some c0 =>
      cases provenTypeCloser with
      | none => simp [boundedProvenParameterCloser]
      | some limit =>
          by_cases hLt : c0 < limit
          · simp [boundedProvenParameterCloser, hLt]
            rintro rfl
            exact hLt
          · simp [boundedProvenParameterCloser, hLt]
            rintro rfl
            omega

/-- R5: Unbounded stop property: once `currentEnd ≥ recoveryEnd` or on `.eof`, `runStringTypePoisonEnd` halts immediately and ignores all trailing lexemes. -/
theorem runStringTypePoisonEnd_stops_at_eof_and_recovery_end
    (currentEnd recoveryEnd eofEnd : Nat)
    (pre suf : List (TypeRefLexemeKind × Nat)) :
    (currentEnd ≥ recoveryEnd → runStringTypePoisonEnd currentEnd recoveryEnd suf = currentEnd) ∧
    runStringTypePoisonEnd currentEnd recoveryEnd (pre ++ (.eof, eofEnd) :: suf) =
      runStringTypePoisonEnd currentEnd recoveryEnd pre := by
  constructor
  · intro hGe
    cases suf with
    | nil => rfl
    | cons hd tl =>
        rcases hd with ⟨k, e⟩
        have hDec : decide (currentEnd ≥ recoveryEnd) = true := decide_eq_true hGe
        simp [runStringTypePoisonEnd, stepStringTypePoisonEnd, hDec]
  · induction pre generalizing currentEnd with
    | nil =>
        simp [runStringTypePoisonEnd, stepStringTypePoisonEnd]
    | cons hd tl ih =>
        rcases hd with ⟨k, e⟩
        simp only [List.cons_append, runStringTypePoisonEnd]
        cases hStop : (stepStringTypePoisonEnd currentEnd recoveryEnd k e).2
        · simp [hStop]
          exact ih (stepStringTypePoisonEnd currentEnd recoveryEnd k e).1
        · simp [hStop]

/-- R6: Unbounded monotonicity: if every lexeme end in the stream is at least the previous end, `runStringTypePoisonEnd` never decreases `currentEnd`. -/
theorem runStringTypePoisonEnd_monotone
    (lexemes : List (TypeRefLexemeKind × Nat))
    (currentEnd recoveryEnd : Nat)
    (hMono : ∀ p ∈ lexemes, currentEnd ≤ p.2)
    (hChain : lexemes.Pairwise (fun a b => a.2 ≤ b.2)) :
    currentEnd ≤ runStringTypePoisonEnd currentEnd recoveryEnd lexemes := by
  induction lexemes generalizing currentEnd with
  | nil =>
      exact Nat.le_refl _
  | cons hd tl ih =>
      rcases hd with ⟨k, e⟩
      simp only [runStringTypePoisonEnd]
      by_cases hStop : currentEnd ≥ recoveryEnd ∨ k = .eof
      · have hCond : (decide (currentEnd ≥ recoveryEnd) || (k == .eof)) = true := by
          rcases hStop with hGe | rfl
          · simp [decide_eq_true hGe]
          · simp
        simp [stepStringTypePoisonEnd, hCond]
      · have hNotGeProp : ¬ (currentEnd ≥ recoveryEnd) := fun h => hStop (Or.inl h)
        have hNotEofProp : k ≠ .eof := fun h => hStop (Or.inr h)
        have hNotGe : decide (currentEnd ≥ recoveryEnd) = false := decide_eq_false hNotGeProp
        have hNotEof : (k == .eof) = false := beq_false_of_ne hNotEofProp
        simp [stepStringTypePoisonEnd, hNotGe, hNotEof]
        have hLeHead : currentEnd ≤ e := hMono (k, e) (by simp)
        cases hChain with
        | cons hRel hTail =>
            have hSub := ih e (fun p hp => hRel p hp) hTail
            exact Nat.le_trans hLeHead hSub

end F6TypeRef
