namespace F6Supertypes

inductive ClassifierHeaderOwner where
  | file
  | declaration
  | member
  deriving DecidableEq, Repr

inductive SupertypeTokenKind where
  | eof
  | trivia
  | leftParen
  | comma
  | leftBrace
  | rightBrace
  | other
  deriving DecidableEq, Repr

inductive DelegationSuffixStop where
  | delegationErrorTarget
  | leadingUnprovedAngleComma
  | delegationUnsupportedSuffix
  deriving DecidableEq, Repr

inductive DelegationLambdaTailEvent where
  | trivia
  | sameLineClosedCall
  | lineBreakLeftParen
  | unclosedLeftParen
  | outerHeaderDelimiter
  | other
  deriving DecidableEq, Repr

inductive DelegationLambdaTailStep where
  | cont
  | accept
  | reject
  deriving DecidableEq, Repr

def isDirectDelegationBoundary (kind : SupertypeTokenKind) : Bool :=
  match kind with
  | .eof | .comma | .leftBrace | .rightBrace => true
  | .trivia | .leftParen | .other => false

def delegationSuffixExtraStop (invalidTarget poison : Bool) : DelegationSuffixStop :=
  if invalidTarget then
    .delegationErrorTarget
  else if poison then
    .leadingUnprovedAngleComma
  else
    .delegationUnsupportedSuffix

def supertypeDeinitDecision
    (owner : ClassifierHeaderOwner)
    (inDelegationRecovery inUnsupportedSuffix nextLine callBodyFollows : Bool) : Option Bool :=
  if owner != .member || !inDelegationRecovery then
    none
  else if inUnsupportedSuffix || !nextLine then
    some ((!inUnsupportedSuffix || nextLine) && callBodyFollows)
  else
    none

def classifyDelegationLambdaToken
    (kind : SupertypeTokenKind)
    (hasLineBreak hasCallCloser : Bool) : DelegationLambdaTailEvent :=
  match kind with
  | .trivia => .trivia
  | .leftParen =>
      if hasLineBreak then
        .lineBreakLeftParen
      else if hasCallCloser then
        .sameLineClosedCall
      else
        .unclosedLeftParen
  | .comma | .leftBrace => .outerHeaderDelimiter
  | .eof | .rightBrace | .other => .other

def stepDelegationLambdaTail (ev : DelegationLambdaTailEvent) : DelegationLambdaTailStep :=
  match ev with
  | .trivia | .sameLineClosedCall => .cont
  | .outerHeaderDelimiter => .accept
  | .lineBreakLeftParen | .unclosedLeftParen | .other => .reject

def runDelegationLambdaTail : List DelegationLambdaTailEvent → Bool
  | [] => false
  | ev :: rest =>
      match stepDelegationLambdaTail ev with
      | .cont => runDelegationLambdaTail rest
      | .accept => true
      | .reject => false

/-- T1: Exact characterization of direct delegation boundary tokens. -/
theorem isDirectDelegationBoundary_spec (kind : SupertypeTokenKind) :
    isDirectDelegationBoundary kind = true ↔
      kind = .eof ∨ kind = .comma ∨ kind = .leftBrace ∨ kind = .rightBrace := by
  cases kind <;> simp [isDirectDelegationBoundary]

/-- T2: `invalidTarget` strictly dominates `poison` in delegation suffix recovery stop selection. -/
theorem delegationSuffixExtraStop_spec (invalidTarget poison : Bool) :
    (invalidTarget = true →
      delegationSuffixExtraStop invalidTarget poison = .delegationErrorTarget) ∧
    (invalidTarget = false → poison = true →
      delegationSuffixExtraStop invalidTarget poison = .leadingUnprovedAngleComma) ∧
    (invalidTarget = false → poison = false →
      delegationSuffixExtraStop invalidTarget poison = .delegationUnsupportedSuffix) := by
  cases invalidTarget <;> cases poison <;> simp [delegationSuffixExtraStop]

/-- T3: Complete characterization of `supertypeDeinitDecision` across all owners and line-break/suffix flags. -/
theorem supertypeDeinitDecision_spec
    (owner : ClassifierHeaderOwner)
    (inDelegationRecovery inUnsupportedSuffix nextLine callBodyFollows : Bool) :
    (supertypeDeinitDecision owner inDelegationRecovery inUnsupportedSuffix nextLine callBodyFollows = some true ↔
      owner = .member ∧ inDelegationRecovery = true ∧ inUnsupportedSuffix = nextLine ∧ callBodyFollows = true) ∧
    (supertypeDeinitDecision owner inDelegationRecovery inUnsupportedSuffix nextLine callBodyFollows = none ↔
      owner ≠ .member ∨ inDelegationRecovery = false ∨ (inUnsupportedSuffix = false ∧ nextLine = true)) := by
  cases owner <;> cases inDelegationRecovery <;> cases inUnsupportedSuffix <;>
    cases nextLine <;> cases callBodyFollows <;> simp [supertypeDeinitDecision]

/-- T4: Exact characterization of all 6 delegation lambda tail event classes under line-break and paired-closer guards. -/
theorem classifyDelegationLambdaToken_spec
    (kind : SupertypeTokenKind)
    (hasLineBreak hasCallCloser : Bool) :
    (classifyDelegationLambdaToken kind hasLineBreak hasCallCloser = .trivia ↔
      kind = .trivia) ∧
    (classifyDelegationLambdaToken kind hasLineBreak hasCallCloser = .sameLineClosedCall ↔
      kind = .leftParen ∧ hasLineBreak = false ∧ hasCallCloser = true) ∧
    (classifyDelegationLambdaToken kind hasLineBreak hasCallCloser = .lineBreakLeftParen ↔
      kind = .leftParen ∧ hasLineBreak = true) ∧
    (classifyDelegationLambdaToken kind hasLineBreak hasCallCloser = .unclosedLeftParen ↔
      kind = .leftParen ∧ hasLineBreak = false ∧ hasCallCloser = false) ∧
    (classifyDelegationLambdaToken kind hasLineBreak hasCallCloser = .outerHeaderDelimiter ↔
      kind = .comma ∨ kind = .leftBrace) ∧
    (classifyDelegationLambdaToken kind hasLineBreak hasCallCloser = .other ↔
      kind = .eof ∨ kind = .rightBrace ∨ kind = .other) := by
  cases kind <;> cases hasLineBreak <;> cases hasCallCloser <;>
    simp [classifyDelegationLambdaToken]

/-- T5: Unbounded characterization: `runDelegationLambdaTail` accepts iff an outer header delimiter is preceded solely by trivia and same-line closed calls. -/
theorem runDelegationLambdaTail_iff (events : List DelegationLambdaTailEvent) :
    runDelegationLambdaTail events = true ↔
      ∃ pre suf,
        events = pre ++ .outerHeaderDelimiter :: suf ∧
        ∀ e ∈ pre, e = .trivia ∨ e = .sameLineClosedCall := by
  induction events with
  | nil =>
      constructor
      · intro h
        simp [runDelegationLambdaTail] at h
      · rintro ⟨pre, suf, hEq, _⟩
        cases pre <;> simp at hEq
  | cons ev rest ih =>
      constructor
      · intro h
        cases ev with
        | trivia =>
            simp [runDelegationLambdaTail, stepDelegationLambdaTail] at h
            rcases ih.mp h with ⟨pre, suf, hEq, hAll⟩
            refine ⟨.trivia :: pre, suf, ?_, ?_⟩
            · simp [hEq]
            · intro e he
              simp at he
              rcases he with rfl | hMem
              · left; rfl
              · exact hAll e hMem
        | sameLineClosedCall =>
            simp [runDelegationLambdaTail, stepDelegationLambdaTail] at h
            rcases ih.mp h with ⟨pre, suf, hEq, hAll⟩
            refine ⟨.sameLineClosedCall :: pre, suf, ?_, ?_⟩
            · simp [hEq]
            · intro e he
              simp at he
              rcases he with rfl | hMem
              · right; rfl
              · exact hAll e hMem
        | outerHeaderDelimiter =>
            exact ⟨[], rest, rfl, by intro e he; cases he⟩
        | lineBreakLeftParen =>
            simp [runDelegationLambdaTail, stepDelegationLambdaTail] at h
        | unclosedLeftParen =>
            simp [runDelegationLambdaTail, stepDelegationLambdaTail] at h
        | other =>
            simp [runDelegationLambdaTail, stepDelegationLambdaTail] at h
      · rintro ⟨pre, suf, hEq, hAll⟩
        cases pre with
        | nil =>
            simp at hEq
            rcases hEq with ⟨rfl, rfl⟩
            simp [runDelegationLambdaTail, stepDelegationLambdaTail]
        | cons ev' pre' =>
            simp at hEq
            rcases hEq with ⟨rfl, rfl⟩
            have hHead := hAll ev (by simp)
            rcases hHead with rfl | rfl
            · simp [runDelegationLambdaTail, stepDelegationLambdaTail]
              exact ih.mpr ⟨pre', suf, rfl, fun e he => hAll e (by simp [he])⟩
            · simp [runDelegationLambdaTail, stepDelegationLambdaTail]
              exact ih.mpr ⟨pre', suf, rfl, fun e he => hAll e (by simp [he])⟩

/-- T6: Empty streams and any rejecting prefix (`lineBreakLeftParen`, `unclosedLeftParen`, `other`) before an outer delimiter always return `false`. -/
theorem runDelegationLambdaTail_rejects_breaks_and_eof
    (pre suf : List DelegationLambdaTailEvent)
    (bad : DelegationLambdaTailEvent)
    (hPre : ∀ e ∈ pre, e = .trivia ∨ e = .sameLineClosedCall)
    (hBad : bad = .lineBreakLeftParen ∨ bad = .unclosedLeftParen ∨ bad = .other) :
    runDelegationLambdaTail [] = false ∧
    runDelegationLambdaTail (pre ++ bad :: suf) = false := by
  constructor
  · rfl
  · induction pre with
    | nil =>
        rcases hBad with rfl | rfl | rfl <;>
          simp [runDelegationLambdaTail, stepDelegationLambdaTail]
    | cons ev rest ih =>
        have hHead := hPre ev (by simp)
        rcases hHead with rfl | rfl
        · simp [runDelegationLambdaTail, stepDelegationLambdaTail]
          exact ih (fun e he => hPre e (by simp [he]))
        · simp [runDelegationLambdaTail, stepDelegationLambdaTail]
          exact ih (fun e he => hPre e (by simp [he]))

end F6Supertypes
