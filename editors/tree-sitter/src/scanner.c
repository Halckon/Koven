#include "tree_sitter/parser.h"

#include <stdbool.h>
#include <stddef.h>
#include <string.h>

enum TokenType {
  IDENTIFIER,
  DESTRUCTURING_IDENTIFIER,
  OWN,
  BORROW,
  INOUT,
  LOOP,
  BLOCK,
  CONTROL_LBRACE,
  LAMBDA_LBRACE,
  SAME_LINE_TRAILING_LBRACE,
  CROSSLINE_LAMBDA_NL,
  MOVE_LAMBDA,
  MOVE_TYPE,
  TO,
  BY,
  IN,
  IS,
  AS,
  NOT_IN,
  NOT_IS,
  SHL,
  SHR,
  USHR,
  BIT_AND,
  BIT_XOR,
  BIT_OR,
};

static const char *const RESERVED_WORDS[] = {
    "class",    "companion", "const",    "enum",     "extern",
    "fun",      "import",    "interface", "object",   "package",
    "typealias", "val",       "var",      "vararg",   "break",
    "continue",  "else",     "for",      "if",       "in",
    "is",        "return",   "when",     "while",    "unsafe",
    "internal",  "private",  "public",   "as",       "false",
    "null",      "operator", "override", "super",    "this",
    "true",      "async",    "await",    "suspend",  "actor",
    "spawn",     "sealed",   "dyn",      "where",    "yield",
    "macro",     "reify",
};

static bool is_identifier_start(int32_t character) {
  return (character >= 'A' && character <= 'Z') ||
         (character >= 'a' && character <= 'z') || character == '_';
}

static bool is_identifier_continue(int32_t character) {
  return is_identifier_start(character) ||
         (character >= '0' && character <= '9');
}

static bool is_reserved_word(const char *text, size_t length) {
  const size_t count = sizeof(RESERVED_WORDS) / sizeof(RESERVED_WORDS[0]);
  for (size_t index = 0; index < count; index++) {
    const char *word = RESERVED_WORDS[index];
    if (strlen(word) == length && memcmp(word, text, length) == 0) {
      return true;
    }
  }
  return false;
}

// Look ahead across horizontal whitespace and block comments without skipping
// bytes. Returns true iff no line break was crossed.
static bool peek_context(TSLexer *lexer) {
  bool same_line = true;
  for (;;) {
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
           lexer->lookahead == '\r' || lexer->lookahead == '\f') {
      lexer->advance(lexer, false);
    }
    if (lexer->lookahead != '/') return same_line;
    lexer->advance(lexer, false);
    if (lexer->lookahead == '/') {
      while (!lexer->eof(lexer) && lexer->lookahead != '\n') {
        lexer->advance(lexer, false);
      }
      return false;
    }
    if (lexer->lookahead != '*') return same_line;
    lexer->advance(lexer, false);
    bool star = false;
    while (!lexer->eof(lexer)) {
      const int32_t character = lexer->lookahead;
      if (character == '\n') same_line = false;
      lexer->advance(lexer, false);
      if (star && character == '/') break;
      star = character == '*';
    }
  }
}

// Advance past whitespace and comments (optionally including newlines) during
// pure lookahead (after mark_end has already been placed).
static void peek_trivia(TSLexer *lexer, bool allow_newlines) {
  for (;;) {
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
           lexer->lookahead == '\r' || lexer->lookahead == '\f' ||
           (allow_newlines && lexer->lookahead == '\n')) {
      lexer->advance(lexer, false);
    }
    if (lexer->lookahead != '/') return;
    lexer->advance(lexer, false);
    if (lexer->lookahead == '/') {
      while (!lexer->eof(lexer) && lexer->lookahead != '\n') {
        lexer->advance(lexer, false);
      }
      if (!allow_newlines) return;
      continue;
    }
    if (lexer->lookahead == '*') {
      lexer->advance(lexer, false);
      bool star = false;
      while (!lexer->eof(lexer)) {
        const int32_t ch = lexer->lookahead;
        lexer->advance(lexer, false);
        if (star && ch == '/') break;
        star = ch == '*';
      }
      continue;
    }
    return;
  }
}

static bool skip_balanced(TSLexer *lexer, int32_t open, int32_t close) {
  if (lexer->lookahead != open) return false;
  lexer->advance(lexer, false);
  unsigned depth = 1;
  while (!lexer->eof(lexer) && depth > 0) {
    const int32_t ch = lexer->lookahead;
    if (ch == '"') {
      lexer->advance(lexer, false);
      while (!lexer->eof(lexer) && lexer->lookahead != '"' &&
             lexer->lookahead != '\n') {
        if (lexer->lookahead == '\\') {
          lexer->advance(lexer, false);
          if (!lexer->eof(lexer)) lexer->advance(lexer, false);
        } else {
          lexer->advance(lexer, false);
        }
      }
      if (lexer->lookahead == '"') lexer->advance(lexer, false);
      continue;
    }
    if (ch == '\'') {
      lexer->advance(lexer, false);
      while (!lexer->eof(lexer) && lexer->lookahead != '\'' &&
             lexer->lookahead != '\n') {
        if (lexer->lookahead == '\\') {
          lexer->advance(lexer, false);
          if (!lexer->eof(lexer)) lexer->advance(lexer, false);
        } else {
          lexer->advance(lexer, false);
        }
      }
      if (lexer->lookahead == '\'') lexer->advance(lexer, false);
      continue;
    }
    if (ch == '/') {
      lexer->advance(lexer, false);
      if (lexer->lookahead == '/') {
        while (!lexer->eof(lexer) && lexer->lookahead != '\n') {
          lexer->advance(lexer, false);
        }
      } else if (lexer->lookahead == '*') {
        lexer->advance(lexer, false);
        bool star = false;
        while (!lexer->eof(lexer)) {
          const int32_t inner = lexer->lookahead;
          lexer->advance(lexer, false);
          if (star && inner == '/') break;
          star = inner == '*';
        }
      }
      continue;
    }
    if (ch == open) {
      depth++;
    } else if (ch == close) {
      depth--;
    }
    lexer->advance(lexer, false);
  }
  return depth == 0;
}

// Mirror the Rust parser's `brace_when_entry_follows`: if `{ ... }` is
// followed on the same `when` condition (including postfix chains, binary
// operators like `&& flag`, or `,` condition lists, and line-continuations) by
// a top-level `->`, it starts a `when` entry rather than a cross-line trailing
// lambda.
static bool is_line_continuation_char(int32_t ch) {
  return ch == '&' || ch == '|' || ch == ',' || ch == '+' || ch == '-' ||
         ch == '*' || ch == '/' || ch == '%' || ch == '=' || ch == '!' ||
         ch == '<' || ch == '>' || ch == ':' || ch == '?' || ch == '.';
}

static bool brace_starts_when_entry(TSLexer *lexer) {
  if (!skip_balanced(lexer, '{', '}')) return false;
  int32_t last_sig = '}';
  while (!lexer->eof(lexer)) {
    peek_trivia(lexer, false);
    if (lexer->eof(lexer)) return false;
    const int32_t ch = lexer->lookahead;
    if (ch == '\n') {
      peek_trivia(lexer, true);
      if (lexer->eof(lexer)) return false;
      const int32_t next = lexer->lookahead;
      if (!is_line_continuation_char(last_sig) && next != '.' && next != '?') {
        return false;
      }
      continue;
    }
    if (ch == ';' || ch == '}') return false;
    if (ch == '(') {
      if (!skip_balanced(lexer, '(', ')')) return false;
      last_sig = ')';
      continue;
    }
    if (ch == '[') {
      if (!skip_balanced(lexer, '[', ']')) return false;
      last_sig = ']';
      continue;
    }
    if (ch == '{') {
      if (!skip_balanced(lexer, '{', '}')) return false;
      last_sig = '}';
      continue;
    }
    if (ch == '"') {
      lexer->advance(lexer, false);
      while (!lexer->eof(lexer) && lexer->lookahead != '"' &&
             lexer->lookahead != '\n') {
        if (lexer->lookahead == '\\') {
          lexer->advance(lexer, false);
          if (!lexer->eof(lexer)) lexer->advance(lexer, false);
        } else {
          lexer->advance(lexer, false);
        }
      }
      if (lexer->lookahead == '"') lexer->advance(lexer, false);
      last_sig = '"';
      continue;
    }
    if (ch == '\'') {
      lexer->advance(lexer, false);
      while (!lexer->eof(lexer) && lexer->lookahead != '\'' &&
             lexer->lookahead != '\n') {
        if (lexer->lookahead == '\\') {
          lexer->advance(lexer, false);
          if (!lexer->eof(lexer)) lexer->advance(lexer, false);
        } else {
          lexer->advance(lexer, false);
        }
      }
      if (lexer->lookahead == '\'') lexer->advance(lexer, false);
      last_sig = '\'';
      continue;
    }
    if (ch == '-') {
      lexer->advance(lexer, false);
      if (lexer->lookahead == '>') return true;
      last_sig = '-';
      continue;
    }
    last_sig = ch;
    lexer->advance(lexer, false);
  }
  return false;
}

void *tree_sitter_koven_external_scanner_create(void) { return NULL; }

void tree_sitter_koven_external_scanner_destroy(void *payload) {
  (void)payload;
}

unsigned tree_sitter_koven_external_scanner_serialize(void *payload,
                                                       char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}

void tree_sitter_koven_external_scanner_deserialize(void *payload,
                                                     const char *buffer,
                                                     unsigned length) {
  (void)payload;
  (void)buffer;
  (void)length;
}

bool tree_sitter_koven_external_scanner_scan(void *payload, TSLexer *lexer,
                                             const bool *valid_symbols) {
  (void)payload;
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
         lexer->lookahead == '\r' || lexer->lookahead == '\f') {
    lexer->advance(lexer, true);
  }

  // Never fire during error recovery (when all external tokens are valid).
  const bool in_error_recovery = valid_symbols[DESTRUCTURING_IDENTIFIER] &&
                                 valid_symbols[NOT_IN] &&
                                 valid_symbols[CONTROL_LBRACE];

  // Cross-line trailing lambda after closed `(...)`: consume only `\n` so
  // intervening comments stay regular `extras` nodes in the syntax tree.
  if (!in_error_recovery && valid_symbols[CROSSLINE_LAMBDA_NL] &&
      lexer->lookahead == '\n') {
    lexer->advance(lexer, false);
    lexer->mark_end(lexer);
    peek_trivia(lexer, true);
    if (lexer->lookahead == '{' && !brace_starts_when_entry(lexer)) {
      lexer->result_symbol = CROSSLINE_LAMBDA_NL;
      return true;
    }
    return false;
  }

  // `{` disambiguation: control_block > trailing lambda > value lambda.
  // Bare `{ ... }` at statement start (where BLOCK is valid) is rejected.
  if (!in_error_recovery && lexer->lookahead == '{') {
    if (valid_symbols[CONTROL_LBRACE]) {
      lexer->advance(lexer, false);
      lexer->mark_end(lexer);
      lexer->result_symbol = CONTROL_LBRACE;
      return true;
    }
    if (valid_symbols[SAME_LINE_TRAILING_LBRACE]) {
      lexer->advance(lexer, false);
      lexer->mark_end(lexer);
      lexer->result_symbol = SAME_LINE_TRAILING_LBRACE;
      return true;
    }
    if (valid_symbols[LAMBDA_LBRACE] && !valid_symbols[BLOCK]) {
      lexer->advance(lexer, false);
      lexer->mark_end(lexer);
      lexer->result_symbol = LAMBDA_LBRACE;
      return true;
    }
    return false;
  }

  const bool negated = lexer->lookahead == '!';
  if (negated) lexer->advance(lexer, false);
  if (!is_identifier_start(lexer->lookahead)) {
    return false;
  }

  char spelling[16];
  size_t length = 0;
  bool overflow = false;
  do {
    if (length < sizeof(spelling)) {
      spelling[length] = (char)lexer->lookahead;
    } else {
      overflow = true;
    }
    length++;
    lexer->advance(lexer, false);
  } while (is_identifier_continue(lexer->lookahead));
  lexer->mark_end(lexer);

  // !in / !is are adjacent, whole-word tokens (Guide 01); !input remains
  // punctuation plus an identifier, never a membership operator plus `put`.
  if (negated) {
    if (length == 2 && spelling[0] == 'i' && spelling[1] == 'n' && valid_symbols[NOT_IN]) {
      lexer->result_symbol = NOT_IN;
      return true;
    }
    if (length == 2 && spelling[0] == 'i' && spelling[1] == 's' && valid_symbols[NOT_IS]) {
      lexer->result_symbol = NOT_IS;
      return true;
    }
    return false;
  }

  // Select soft keywords only in grammar states and lookahead contexts that own
  // them; e.g. `own: Int` and ordinary `move()` calls remain identifiers.
  const bool safe_cast = lexer->lookahead == '?';
  const bool same_line = peek_context(lexer);
  const int32_t same_line_next = lexer->lookahead;
  const bool next_name = is_identifier_start(same_line_next);
  peek_trivia(lexer, true);
  const int32_t crossline_next = lexer->lookahead;
  const struct {
    const char *word;
    enum TokenType symbol;
    bool context;
  } contextual[] = {
      {"own", OWN, next_name || same_line_next == '('},
      {"borrow", BORROW, next_name || same_line_next == '('},
      {"inout", INOUT, next_name || same_line_next == '('},
      {"loop", LOOP, same_line_next == '{'},
      {"block", BLOCK, same_line && same_line_next == '{'},
      {"move", MOVE_LAMBDA, crossline_next == '{'},
      {"move", MOVE_TYPE, crossline_next == '('},
      {"to", TO, true},
      {"by", BY, true},
      {"in", IN, true},
      {"is", IS, true},
      {"as", AS, !safe_cast},
      {"shl", SHL, true},
      {"shr", SHR, true},
      {"ushr", USHR, true},
      {"and", BIT_AND, true},
      {"xor", BIT_XOR, true},
      {"or", BIT_OR, true},
  };
  for (size_t index = 0; index < sizeof(contextual) / sizeof(contextual[0]); index++) {
    if (!overflow && valid_symbols[contextual[index].symbol] &&
        contextual[index].context && strlen(contextual[index].word) == length &&
        memcmp(contextual[index].word, spelling, length) == 0) {
      lexer->result_symbol = contextual[index].symbol;
      return true;
    }
  }
  if (!overflow && is_reserved_word(spelling, length)) {
    return false;
  }
  if (valid_symbols[IDENTIFIER]) {
    lexer->result_symbol = IDENTIFIER;
  } else if (valid_symbols[DESTRUCTURING_IDENTIFIER] &&
             !(length == 1 && spelling[0] == '_')) {
    lexer->result_symbol = DESTRUCTURING_IDENTIFIER;
  } else {
    return false;
  }
  return true;
}
