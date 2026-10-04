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
  MOVE_LAMBDA,
  MOVE_TYPE,
  TO,
  BY,
  IN,
  IS,
  AS,
  NOT_IN,
  NOT_IS,
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

// Look ahead across the same horizontal/comment extras as grammar.js. Never skip
// bytes here: the token start/end must remain the identifier already marked.
static void peek_context(TSLexer *lexer) {
  for (;;) {
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
           lexer->lookahead == '\r' || lexer->lookahead == '\f') {
      lexer->advance(lexer, false);
    }
    if (lexer->lookahead != '/') return;
    lexer->advance(lexer, false);
    if (lexer->lookahead != '*') return;
    lexer->advance(lexer, false);
    bool star = false;
    while (!lexer->eof(lexer)) {
      const int32_t character = lexer->lookahead;
      lexer->advance(lexer, false);
      if (star && character == '/') break;
      star = character == '*';
    }
  }
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
  peek_context(lexer);
  const bool next_name = is_identifier_start(lexer->lookahead);
  const struct {
    const char *word;
    enum TokenType symbol;
    bool context;
  } contextual[] = {
      {"own", OWN, next_name || lexer->lookahead == '('},
      {"borrow", BORROW, next_name || lexer->lookahead == '('},
      {"inout", INOUT, next_name || lexer->lookahead == '('},
      {"loop", LOOP, lexer->lookahead == '{'},
      {"move", MOVE_LAMBDA, lexer->lookahead == '{'},
      {"move", MOVE_TYPE, lexer->lookahead == '('},
      {"to", TO, true},
      {"by", BY, true},
      {"in", IN, true},
      {"is", IS, true},
      {"as", AS, !safe_cast},
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
