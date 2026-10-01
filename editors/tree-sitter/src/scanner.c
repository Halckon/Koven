#include "tree_sitter/parser.h"

#include <stdbool.h>
#include <stddef.h>
#include <string.h>

enum TokenType {
  IDENTIFIER,
  DESTRUCTURING_IDENTIFIER,
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
  if ((!valid_symbols[IDENTIFIER] && !valid_symbols[DESTRUCTURING_IDENTIFIER]) ||
      !is_identifier_start(lexer->lookahead)) {
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
