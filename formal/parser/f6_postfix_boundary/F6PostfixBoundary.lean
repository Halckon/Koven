namespace F6PostfixBoundary

inductive PostfixLexemeKind where
  | eof
  | trivia
  | invalid
  | ident
  | intLit
  | floatLit
  | charLit
  | stringEnd
  | kwTrue
  | kwFalse
  | kwNull
  | kwThis
  | dot
  | questionDot
  | leftParen
  | rightParen
  | leftBracket
  | rightBracket
  | leftBrace
  | rightBrace
  | comma
  | bangBang
  | question
  | colonColon
  | otherToken
  deriving DecidableEq, Repr

def isPostfixStart (kind : PostfixLexemeKind) : Bool :=
  match kind with
  | .dot | .questionDot | .leftParen | .leftBracket | .bangBang | .question | .colonColon => true
  | _ => false

def typedCallSuffixContinues (suffixKind : PostfixLexemeKind) (hasLineBreak : Bool) : Bool :=
  suffixKind != .leftBrace || !hasLineBreak

def callArgumentBoundary
    (kind : PostfixLexemeKind)
    (outerContains canStartExpr atBlockSeparator : Bool) : Bool :=
  kind == .eof || kind == .comma || kind == .rightParen ||
    (outerContains && !canStartExpr) || atBlockSeparator

def trailingLambdaAttachesAfterCall
    (closed : Bool) (currentKind : PostfixLexemeKind) (nextWhenEntry : Bool) : Bool :=
  closed && currentKind == .leftBrace && !nextWhenEntry

def isRecoveryCalleeIgnored (kind : PostfixLexemeKind) : Bool :=
  match kind with
  | .trivia | .question | .bangBang => true
  | _ => false

def isRecoveryCalleeTerminal (kind : PostfixLexemeKind) : Bool :=
  match kind with
  | .ident | .intLit | .floatLit | .charLit | .stringEnd
  | .kwTrue | .kwFalse | .kwNull | .kwThis
  | .rightParen | .rightBracket | .rightBrace => true
  | _ => false

def lastNonIgnored : List PostfixLexemeKind → Option PostfixLexemeKind
  | [] => none
  | k :: rest =>
      match lastNonIgnored rest with
      | some found => some found
      | none => if isRecoveryCalleeIgnored k then none else some k

def hasRecoveryCalleeEnd (kinds : List PostfixLexemeKind) : Bool :=
  match lastNonIgnored kinds with
  | some k => isRecoveryCalleeTerminal k
  | none => false

/-- P1: Exact characterization of `isPostfixStart` across all lexeme classes. -/
theorem isPostfixStart_spec (kind : PostfixLexemeKind) :
    isPostfixStart kind = true ↔
      kind = .dot ∨ kind = .questionDot ∨ kind = .leftParen ∨ kind = .leftBracket ∨
      kind = .bangBang ∨ kind = .question ∨ kind = .colonColon := by
  cases kind <;> simp [isPostfixStart]

/-- P2: Exact characterization of `typedCallSuffixContinues`: only `{` across a line break stops continuation. -/
theorem typedCallSuffixContinues_spec (suffixKind : PostfixLexemeKind) (hasLineBreak : Bool) :
    typedCallSuffixContinues suffixKind hasLineBreak = true ↔
      suffixKind ≠ .leftBrace ∨ hasLineBreak = false := by
  cases suffixKind <;> cases hasLineBreak <;> simp [typedCallSuffixContinues]

/-- P3: Exact characterization of `callArgumentBoundary`. -/
theorem callArgumentBoundary_spec
    (kind : PostfixLexemeKind)
    (outerContains canStartExpr atBlockSeparator : Bool) :
    callArgumentBoundary kind outerContains canStartExpr atBlockSeparator = true ↔
      kind = .eof ∨ kind = .comma ∨ kind = .rightParen ∨
      (outerContains = true ∧ canStartExpr = false) ∨ atBlockSeparator = true := by
  unfold callArgumentBoundary
  cases outerContains <;> cases canStartExpr <;> cases atBlockSeparator <;>
    simp [or_assoc]

/-- P4: Exact characterization of `trailingLambdaAttachesAfterCall`. -/
theorem trailingLambdaAttachesAfterCall_spec
    (closed : Bool) (currentKind : PostfixLexemeKind) (nextWhenEntry : Bool) :
    trailingLambdaAttachesAfterCall closed currentKind nextWhenEntry = true ↔
      closed = true ∧ currentKind = .leftBrace ∧ nextWhenEntry = false := by
  cases closed <;> cases currentKind <;> cases nextWhenEntry <;>
    simp [trailingLambdaAttachesAfterCall]

/-- P5: Ignored postfix markers (`trivia`, `?`, `!!`) and callee terminals are strictly disjoint. -/
theorem recoveryCalleeIgnored_and_terminal_disjoint (kind : PostfixLexemeKind) :
    ¬ (isRecoveryCalleeIgnored kind = true ∧ isRecoveryCalleeTerminal kind = true) := by
  cases kind <;> simp [isRecoveryCalleeIgnored, isRecoveryCalleeTerminal]

private theorem lastNonIgnored_eq_reverse_find (kinds : List PostfixLexemeKind) :
    lastNonIgnored kinds = kinds.reverse.find? (fun k => !isRecoveryCalleeIgnored k) := by
  induction kinds with
  | nil =>
      rfl
  | cons k rest ih =>
      simp only [lastNonIgnored, ih, List.reverse_cons, List.find?_append]
      cases h : rest.reverse.find? (fun k => !isRecoveryCalleeIgnored k) with
      | some found =>
          rfl
      | none =>
          cases hIgn : isRecoveryCalleeIgnored k <;> simp [List.find?, hIgn]

/-- P6: Unbounded specification: `hasRecoveryCalleeEnd` matches reverse-scanning for the first non-ignored lexeme and checking `isRecoveryCalleeTerminal`. -/
theorem hasRecoveryCalleeEnd_iff_last_non_ignored_terminal (kinds : List PostfixLexemeKind) :
    hasRecoveryCalleeEnd kinds = true ↔
      ∃ k, kinds.reverse.find? (fun x => !isRecoveryCalleeIgnored x) = some k ∧
        isRecoveryCalleeTerminal k = true := by
  unfold hasRecoveryCalleeEnd
  rw [lastNonIgnored_eq_reverse_find]
  cases h : kinds.reverse.find? (fun x => !isRecoveryCalleeIgnored x) with
  | none =>
      simp
  | some k =>
      simp

/-- P7: Appending any suffix of ignored lexemes (`trivia`, `?`, `!!`) never changes `hasRecoveryCalleeEnd`. -/
theorem hasRecoveryCalleeEnd_invariant_under_ignored_suffix
    (pre suf : List PostfixLexemeKind)
    (hSuf : ∀ k ∈ suf, isRecoveryCalleeIgnored k = true) :
    hasRecoveryCalleeEnd (pre ++ suf) = hasRecoveryCalleeEnd pre := by
  have hLast : lastNonIgnored (pre ++ suf) = lastNonIgnored pre := by
    induction pre with
    | nil =>
        induction suf with
        | nil => rfl
        | cons s rest ihS =>
            have hs : isRecoveryCalleeIgnored s = true := hSuf s (by simp)
            have hRest : lastNonIgnored rest = none := ihS (fun k hk => hSuf k (by simp [hk]))
            simp [lastNonIgnored, hRest, hs]
    | cons p rest ih =>
        simp [lastNonIgnored, ih]
  simp [hasRecoveryCalleeEnd, hLast]

end F6PostfixBoundary
