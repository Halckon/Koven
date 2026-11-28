namespace F6FileModifiers

inductive FileLexemeKind where
  | eof
  | semicolon
  | kwPackage
  | kwImport
  | other
  deriving DecidableEq, Repr

inductive FileHeaderSeparatorDecision where
  | satisfied
  | consumeSemicolon
  | missing
  deriving DecidableEq, Repr

inductive FileRootSeparatorDiagnostic where
  | none
  | expectedFileHeaderSeparator
  | expectedDeclarationSeparator
  deriving DecidableEq, Repr

inductive ReceiverModifierPolicy where
  | allowed
  | rejected
  | notRecognized
  deriving DecidableEq, Repr

inductive ModifierToken where
  | visibility
  | override
  | receiver
  deriving DecidableEq, Repr

inductive ModifierStepOutcome where
  | accept
  | invalid
  | stop
  deriving DecidableEq, Repr

structure ModifierState where
  hasVisibility : Bool := false
  sawOverride : Bool := false
  sawReceiver : Bool := false
  deriving DecidableEq, Repr

def fileHeaderSeparatorDecision (kind : FileLexemeKind) (hasLineBreak : Bool) :
    FileHeaderSeparatorDecision :=
  if kind == .eof || hasLineBreak then
    .satisfied
  else if kind == .semicolon then
    .consumeSemicolon
  else
    .missing

def fileRootSeparatorDiagnostic
    (startedAsDeclaration atFileBoundary hasLineBreak : Bool)
    (kind : FileLexemeKind) : FileRootSeparatorDiagnostic :=
  if kind == .eof || kind == .semicolon || !startedAsDeclaration || !atFileBoundary || hasLineBreak then
    .none
  else if kind == .kwPackage || kind == .kwImport then
    .expectedFileHeaderSeparator
  else
    .expectedDeclarationSeparator

def stepModifier
    (allowOverride : Bool)
    (policy : ReceiverModifierPolicy)
    (st : ModifierState)
    (tok : ModifierToken) : ModifierState × ModifierStepOutcome :=
  match tok with
  | .visibility =>
      if st.hasVisibility || st.sawOverride || st.sawReceiver then
        (st, .invalid)
      else
        ({ st with hasVisibility := true }, .accept)
  | .override =>
      if !allowOverride || st.sawOverride || st.sawReceiver then
        ({ st with sawOverride := true }, .invalid)
      else
        ({ st with sawOverride := true }, .accept)
  | .receiver =>
      match policy with
      | .notRecognized => (st, .stop)
      | .rejected => ({ st with sawReceiver := true }, .invalid)
      | .allowed =>
          if st.sawReceiver then
            ({ st with sawReceiver := true }, .invalid)
          else
            ({ st with sawReceiver := true }, .accept)

def runModifiers
    (allowOverride : Bool)
    (policy : ReceiverModifierPolicy)
    (st : ModifierState) : List ModifierToken → ModifierState × Nat
  | [] => (st, 0)
  | tok :: rest =>
      let (st', outcome) := stepModifier allowOverride policy st tok
      match outcome with
      | .stop => (st', 0)
      | .accept => runModifiers allowOverride policy st' rest
      | .invalid =>
          let (finalSt, errs) := runModifiers allowOverride policy st' rest
          (finalSt, errs + 1)

def stateRank (st : ModifierState) : Nat :=
  (if st.hasVisibility then 1 else 0) +
  (if st.sawOverride then 1 else 0) +
  (if st.sawReceiver then 1 else 0)

/-- M1: Exact characterization of `fileHeaderSeparatorDecision` across all 3 outcomes. -/
theorem fileHeaderSeparatorDecision_spec (kind : FileLexemeKind) (hasLineBreak : Bool) :
    (fileHeaderSeparatorDecision kind hasLineBreak = .satisfied ↔
      kind = .eof ∨ hasLineBreak = true) ∧
    (fileHeaderSeparatorDecision kind hasLineBreak = .consumeSemicolon ↔
      kind = .semicolon ∧ hasLineBreak = false) ∧
    (fileHeaderSeparatorDecision kind hasLineBreak = .missing ↔
      kind ≠ .eof ∧ kind ≠ .semicolon ∧ hasLineBreak = false) := by
  cases kind <;> cases hasLineBreak <;> simp [fileHeaderSeparatorDecision]

/-- M2: Exact characterization of `fileRootSeparatorDiagnostic` across all 3 outcomes. -/
theorem fileRootSeparatorDiagnostic_spec
    (startedAsDeclaration atFileBoundary hasLineBreak : Bool)
    (kind : FileLexemeKind) :
    (fileRootSeparatorDiagnostic startedAsDeclaration atFileBoundary hasLineBreak kind =
        .expectedFileHeaderSeparator ↔
      startedAsDeclaration = true ∧ atFileBoundary = true ∧ hasLineBreak = false ∧
        (kind = .kwPackage ∨ kind = .kwImport)) ∧
    (fileRootSeparatorDiagnostic startedAsDeclaration atFileBoundary hasLineBreak kind =
        .expectedDeclarationSeparator ↔
      startedAsDeclaration = true ∧ atFileBoundary = true ∧ hasLineBreak = false ∧
        kind = .other) ∧
    (fileRootSeparatorDiagnostic startedAsDeclaration atFileBoundary hasLineBreak kind = .none ↔
      kind = .eof ∨ kind = .semicolon ∨ startedAsDeclaration = false ∨
        atFileBoundary = false ∨ hasLineBreak = true) := by
  cases startedAsDeclaration <;> cases atFileBoundary <;> cases hasLineBreak <;> cases kind <;>
    simp [fileRootSeparatorDiagnostic]

/-- M3: Exact single-step acceptance, rejection, and stop conditions for `stepModifier`. -/
theorem stepModifier_spec
    (allowOverride : Bool)
    (policy : ReceiverModifierPolicy)
    (st : ModifierState)
    (tok : ModifierToken) :
    ((stepModifier allowOverride policy st tok).2 = .stop ↔
      tok = .receiver ∧ policy = .notRecognized) ∧
    ((stepModifier allowOverride policy st tok).2 = .accept ↔
      (tok = .visibility ∧ st.hasVisibility = false ∧ st.sawOverride = false ∧ st.sawReceiver = false) ∨
      (tok = .override ∧ allowOverride = true ∧ st.sawOverride = false ∧ st.sawReceiver = false) ∨
      (tok = .receiver ∧ policy = .allowed ∧ st.sawReceiver = false)) := by
  cases st with
  | mk hv so sr =>
      cases allowOverride <;> cases policy <;> cases hv <;> cases so <;> cases sr <;> cases tok <;>
        simp [stepModifier]

/-- M4: Monotonicity of `stepModifier`: no flag ever reverts from `true` to `false`. -/
theorem stepModifier_monotone
    (allowOverride : Bool)
    (policy : ReceiverModifierPolicy)
    (st : ModifierState)
    (tok : ModifierToken) :
    let st' := (stepModifier allowOverride policy st tok).1
    (st.hasVisibility = true → st'.hasVisibility = true) ∧
    (st.sawOverride = true → st'.sawOverride = true) ∧
    (st.sawReceiver = true → st'.sawReceiver = true) := by
  cases st with
  | mk hv so sr =>
      cases allowOverride <;> cases policy <;> cases hv <;> cases so <;> cases sr <;> cases tok <;>
        simp [stepModifier]

/-- M5: Unbounded monotonicity of `runModifiers` over arbitrary-length modifier sequences. -/
theorem runModifiers_monotone
    (allowOverride : Bool)
    (policy : ReceiverModifierPolicy)
    (toks : List ModifierToken)
    (st : ModifierState) :
    let st' := (runModifiers allowOverride policy st toks).1
    (st.hasVisibility = true → st'.hasVisibility = true) ∧
    (st.sawOverride = true → st'.sawOverride = true) ∧
    (st.sawReceiver = true → st'.sawReceiver = true) := by
  induction toks generalizing st with
  | nil =>
      simp [runModifiers]
  | cons tok rest ih =>
      simp only [runModifiers]
      have hStep := stepModifier_monotone allowOverride policy st tok
      cases hOut : (stepModifier allowOverride policy st tok).2 with
      | stop =>
          exact hStep
      | accept =>
          have hRest := ih (stepModifier allowOverride policy st tok).1
          exact ⟨fun h => hRest.1 (hStep.1 h), fun h => hRest.2.1 (hStep.2.1 h),
            fun h => hRest.2.2 (hStep.2.2 h)⟩
      | invalid =>
          have hRest := ih (stepModifier allowOverride policy st tok).1
          exact ⟨fun h => hRest.1 (hStep.1 h), fun h => hRest.2.1 (hStep.2.1 h),
            fun h => hRest.2.2 (hStep.2.2 h)⟩

/-- M6: Every accepted step strictly increases `stateRank`, which is bounded by 3. -/
theorem stepModifier_accept_increases_rank
    (allowOverride : Bool)
    (policy : ReceiverModifierPolicy)
    (st : ModifierState)
    (tok : ModifierToken)
    (hAcc : (stepModifier allowOverride policy st tok).2 = .accept) :
    stateRank (stepModifier allowOverride policy st tok).1 = stateRank st + 1 ∧
    stateRank (stepModifier allowOverride policy st tok).1 ≤ 3 := by
  cases st with
  | mk hv so sr =>
      cases allowOverride <;> cases policy <;> cases hv <;> cases so <;> cases sr <;> cases tok <;>
        simp [stepModifier, stateRank] at hAcc ⊢

/-- M7: Unbounded error-free bound: any sequence accepted with zero errors and no `.notRecognized` stop has length at most 3 from the empty state. -/
theorem runModifiers_zero_errors_len_le_three
    (allowOverride : Bool)
    (policy : ReceiverModifierPolicy)
    (toks : List ModifierToken)
    (st : ModifierState)
    (hNoStop : ∀ t ∈ toks, ¬ (t = .receiver ∧ policy = .notRecognized))
    (hErr : (runModifiers allowOverride policy st toks).2 = 0) :
    stateRank st + toks.length ≤ 3 := by
  induction toks generalizing st with
  | nil =>
      cases st with
      | mk hv so sr =>
          cases hv <;> cases so <;> cases sr <;> simp [stateRank]
  | cons tok rest ih =>
      simp only [runModifiers] at hErr
      have hNotStop : (stepModifier allowOverride policy st tok).2 ≠ .stop := by
        intro hStop
        have := (stepModifier_spec allowOverride policy st tok).1.mp hStop
        exact hNoStop tok (by simp) this
      cases hOut : (stepModifier allowOverride policy st tok).2 with
      | stop =>
          exact absurd hOut hNotStop
      | invalid =>
          simp [hOut] at hErr
      | accept =>
          simp [hOut] at hErr
          have hRank := stepModifier_accept_increases_rank allowOverride policy st tok hOut
          have hRest := ih (stepModifier allowOverride policy st tok).1
            (fun t ht => hNoStop t (by simp [ht])) hErr
          simp only [List.length_cons]
          omega

end F6FileModifiers
