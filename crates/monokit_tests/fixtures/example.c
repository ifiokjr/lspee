/* Example C file for outline extraction testing */

#include <stddef.h>

#define MAX_RETRIES 3
#define VERSION "1.0.0"

typedef struct {
    int verbose;
    int max_depth;
} Config;

typedef struct {
    char* message;
    int position;
} ParseError;

typedef struct Node {
    char* label;
    char* value;
    struct Node** children;
    int child_count;
} Node;

typedef struct {
    Node* root;
} Ast;

/* Parse a string into an AST. */
Ast* parse(const char* input) {
    if (input == NULL) {
        return NULL;
    }
    Node* root = malloc(sizeof(Node));
    root->value = (char*)input;
    return &(Ast){.root = root};
}

/* Format a node for display. */
char* format_node(Node* node) {
    if (node->child_count == 0) {
        return node->value;
    }
    return node->label;
}