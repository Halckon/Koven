namespace F6BlockBoundary

inductive BlockLexemeKind where
  | eof
  | trivia
  | invalid
  | ident
  | semicolon
  | leftBrace
  | rightBrace
  | leftParen
  | plus
  | minus
  | kwVal
  | kwVar
  | kwWhile
  | kwFor
  | kwConst
  | kwFun
  | kwClass
  | kwInterface
  | kwEnum
  | kwObject
  | kwCompanion
  | otherToken
  deriving DecidableEq, Repr

def unsupportedBlockElement (kind : BlockLexemeKind) : Bool :=
  match kind with
  | .kwConst | .kwFun | .kwFor | .kwWhile
  | .kwClass | .kwInterface | .kwEnum | .kwObject | .kwCompanion => true
  | _ => false

def blockExprLineBreakBoundary (blockElements : Bool) (kind : BlockLexemeKind) (hasLineBreak : Bool) : Bool :=
  blockElements && hasLineBreak &&
    match kind with
    | .leftParen | .plus | .minus => true
    | _ => false

def isExplicitBlockStart
    (currentKind : BlockLexemeKind)
    (isBlockIdent : Bool)
    (nextKind : Option BlockLexemeKind)
    (hasLineBreak : Bool) : Bool :=
  currentKind == .ident && isBlockIdent && nextKind == some .leftBrace && !hasLineBreak

def blockElementSeparatorRequired
    (currentKind : BlockLexemeKind)
    (outerHardStop isPoison isUnsupported canStartExpr prevIsError hasLineBreak : Bool) : Bool :=
  let startsLoop := currentKind == .kwWhile || currentKind == .kwFor
  let isDelimOrEof :=
    currentKind == .eof || currentKind == .semicolon || currentKind == .rightBrace
  let startsNext :=
    canStartExpr ||
      currentKind == .kwVal || currentKind == .kwVar ||
      currentKind == .kwWhile || currentKind == .kwFor
  !isDelimOrEof &&
    !outerHardStop &&
    !isPoison &&
    !(isUnsupported && !startsLoop) &&
    startsNext &&
    !prevIsError &&
    !hasLineBreak

def isContentLexeme (kind : BlockLexemeKind) : Bool :=
  match kind with
  | .trivia | .eof => false
  | _ => true

def stepMissingBlockRootEnd (currentEnd : Nat) (kind : BlockLexemeKind) (lexemeEnd : Nat) : Nat × Bool :=
  match kind with
  | .eof => (currentEnd, true)
  | .trivia => (currentEnd, false)
  | _ => (lexemeEnd, false)

def runMissingBlockRootEnd (initialEnd : Nat) : List (BlockLexemeKind × Nat) → Nat
  | [] => initialEnd
  | (kind, lexEnd) :: rest =>
      let (nextEnd, stop) := stepMissingBlockRootEnd initialEnd kind lexEnd
      if stop then nextEnd else runMissingBlockRootEnd nextEnd rest

def takeBeforeEof : List (BlockLexemeKind × Nat) → List (BlockLexemeKind × Nat)
  | [] => []
  | (kind, lexEnd) :: rest =>
      if kind == .eof then [] else (kind, lexEnd) :: takeBeforeEof rest

def lastContentEnd (initialEnd : Nat) (xs : List (BlockLexemeKind × Nat)) : Nat :=
  (xs.filter (fun p => isContentLexeme p.1)).foldl (fun _ p => p.2) initialEnd

/-- B1: Exact characterization of `unsupportedBlockElement` across all lexeme classes. -/
theorem unsupportedBlockElement_spec (kind : BlockLexemeKind) :
    unsupportedBlockElement kind = true ↔
      kind = .kwConst ∨ kind = .kwFun ∨ kind = .kwFor ∨ kind = .kwWhile ∨
      kind = .kwClass ∨ kind = .kwInterface ∨ kind = .kwEnum ∨
      kind = .kwObject ∨ kind = .kwCompanion := by
  cases kind <;> simp [unsupportedBlockElement]

/-- B2: Exact characterization of `blockExprLineBreakBoundary`. -/
theorem blockExprLineBreakBoundary_spec
    (blockElements : Bool) (kind : BlockLexemeKind) (hasLineBreak : Bool) :
    blockExprLineBreakBoundary blockElements kind hasLineBreak = true ↔
      blockElements = true ∧ hasLineBreak = true ∧
        (kind = .leftParen ∨ kind = .plus ∨ kind = .minus) := by
  cases blockElements <;> cases kind <;> cases hasLineBreak <;>
    simp [blockExprLineBreakBoundary]

/-- B3: Exact characterization of `isExplicitBlockStart`. -/
theorem isExplicitBlockStart_spec
    (currentKind : BlockLexemeKind)
    (isBlockIdent : Bool)
    (nextKind : Option BlockLexemeKind)
    (hasLineBreak : Bool) :
    isExplicitBlockStart currentKind isBlockIdent nextKind hasLineBreak = true ↔
      currentKind = .ident ∧ isBlockIdent = true ∧
        nextKind = some .leftBrace ∧ hasLineBreak = false := by
  cases currentKind <;> cases isBlockIdent <;> cases hasLineBreak <;>
    cases nextKind with
    | none => simp [isExplicitBlockStart]
    | some nk => cases nk <;> simp [isExplicitBlockStart]

/-- B4: Exact characterization of `blockElementSeparatorRequired`. -/
theorem blockElementSeparatorRequired_spec
    (currentKind : BlockLexemeKind)
    (outerHardStop isPoison isUnsupported canStartExpr prevIsError hasLineBreak : Bool) :
    blockElementSeparatorRequired
      currentKind outerHardStop isPoison isUnsupported canStartExpr prevIsError hasLineBreak = true ↔
      currentKind ≠ .eof ∧ currentKind ≠ .semicolon ∧ currentKind ≠ .rightBrace ∧
      outerHardStop = false ∧ isPoison = false ∧
      (isUnsupported = false ∨ currentKind = .kwWhile ∨ currentKind = .kwFor) ∧
      (canStartExpr = true ∨ currentKind = .kwVal ∨ currentKind = .kwVar ∨
        currentKind = .kwWhile ∨ currentKind = .kwFor) ∧
      prevIsError = false ∧ hasLineBreak = false := by
  unfold blockElementSeparatorRequired
  cases outerHardStop <;> cases isPoison <;> cases prevIsError <;> cases hasLineBreak <;>
    cases isUnsupported <;> cases canStartExpr <;>
    simp [not_or, or_assoc, and_assoc]

/-- B5: Same-line `while`/`for` remain exempt from unsupported-element suppression, while all other unsupported keywords are suppressed. -/
theorem blockElementSeparatorRequired_while_for_exempt_from_unsupported
    (currentKind : BlockLexemeKind)
    (canStartExpr : Bool) :
    (currentKind = .kwWhile ∨ currentKind = .kwFor →
      blockElementSeparatorRequired currentKind false false true canStartExpr false false = true) ∧
    (currentKind ≠ .kwWhile ∧ currentKind ≠ .kwFor →
      blockElementSeparatorRequired currentKind false false true canStartExpr false false = false) := by
  cases currentKind <;> cases canStartExpr <;> simp [blockElementSeparatorRequired]

/-- B6: Unbounded specification: `runMissingBlockRootEnd` equals the end of the last non-trivia, non-EOF lexeme before the first EOF (or `initialEnd` if none). -/
theorem runMissingBlockRootEnd_eq_last_content_before_eof
    (lexemes : List (BlockLexemeKind × Nat)) (initialEnd : Nat) :
    runMissingBlockRootEnd initialEnd lexemes =
      lastContentEnd initialEnd (takeBeforeEof lexemes) := by
  induction lexemes generalizing initialEnd with
  | nil =>
      rfl
  | cons hd tl ih =>
      rcases hd with ⟨kind, lexEnd⟩
      cases kind <;>
        simp [runMissingBlockRootEnd, stepMissingBlockRootEnd, takeBeforeEof, lastContentEnd,
          isContentLexeme] <;>
        exact ih _

/-- B7: Trailing trivia before EOF and any lexemes after the first EOF never change `runMissingBlockRootEnd`. -/
theorem runMissingBlockRootEnd_ignores_trailing_trivia_and_after_eof
    (pre suf : List (BlockLexemeKind × Nat))
    (triviaEnd eofEnd initialEnd : Nat) :
    runMissingBlockRootEnd initialEnd (pre ++ (.trivia, triviaEnd) :: (.eof, eofEnd) :: suf) =
      runMissingBlockRootEnd initialEnd pre := by
  induction pre generalizing initialEnd with
  | nil =>
      simp [runMissingBlockRootEnd, stepMissingBlockRootEnd]
  | cons hd tl ih =>
      rcases hd with ⟨kind, lexEnd⟩
      cases kind <;>
        simp [runMissingBlockRootEnd, stepMissingBlockRootEnd] <;>
        exact ih _

end F6BlockBoundary
