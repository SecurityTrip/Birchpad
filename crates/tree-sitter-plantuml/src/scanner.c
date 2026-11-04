// The end of the text as the end of its last line, so that a last line without a line break
// (`@enduml` at the very end of a file) is a line like the others. The regular expression `\n`
// ends the other lines; this ends one only where the text ends, and never during error
// recovery, when every token is asked for.

#include "tree_sitter/parser.h"

enum TokenType {
  END_OF_TEXT,
  ERROR_RECOVERY,
};

void *tree_sitter_plantuml_external_scanner_create(void) { return NULL; }

void tree_sitter_plantuml_external_scanner_destroy(void *payload) { (void)payload; }

unsigned tree_sitter_plantuml_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}

void tree_sitter_plantuml_external_scanner_deserialize(void *payload, const char *buffer,
                                                       unsigned length) {
  (void)payload;
  (void)buffer;
  (void)length;
}

bool tree_sitter_plantuml_external_scanner_scan(void *payload, TSLexer *lexer,
                                                const bool *valid_symbols) {
  (void)payload;
  if (valid_symbols[ERROR_RECOVERY] || !valid_symbols[END_OF_TEXT]) {
    return false;
  }
  if (lexer->eof(lexer)) {
    lexer->result_symbol = END_OF_TEXT;
    return true;
  }
  return false;
}
