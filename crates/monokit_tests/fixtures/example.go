// Example Go file for outline extraction testing

package parser

import "fmt"

// MaxRetries is the maximum number of retries before giving up.
const MaxRetries = 3

// Version is the application version string.
const Version = "1.0.0"

// Config holds configuration for the parser.
type Config struct {
	// Verbose enables debug logging.
	Verbose bool
	// MaxDepth limits recursive descent.
	MaxDepth int
}

// ParseError represents a parsing failure.
type ParseError struct {
	Message  string
	Position int
}

// Error implements the error interface.
func (e *ParseError) Error() string {
	return fmt.Sprintf("parse error at %d: %s", e.Position, e.Message)
}

// Processor defines the interface for parsing inputs.
type Processor interface {
	// Parse turns raw bytes into a structured result.
	Parse(input []byte) (*Ast, error)
	// Validate checks whether the input is well-formed.
	Validate(input []byte) bool
}

// Ast is the root of a parsed abstract syntax tree.
type Ast struct {
	// Root is the top-level node.
	Root     *Node
	sourceMap map[int]int
}

// Node is a single element in the tree.
type Node struct {
	Label    string
	Children []*Node
	Value    string
}

// Parse parses a byte slice into an Ast.
func Parse(input []byte) (*Ast, error) {
	if len(input) == 0 {
		return nil, &ParseError{Message: "empty input", Position: 0}
	}
	return &Ast{Root: &Node{Value: string(input)}}, nil
}

// FormatNode renders a Node as a human-readable string.
func FormatNode(node *Node) string {
	if len(node.Children) == 0 {
		return node.Value
	}
	return node.Label
}