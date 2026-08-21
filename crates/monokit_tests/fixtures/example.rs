//! Example Rust source file for outline extraction testing.
//!
//! This module demonstrates public-item extraction at all detail levels.

use std::collections::HashMap;
use std::fmt;

/// Maximum number of retries before giving up.
const MAX_RETRIES: u32 = 3;

/// The global application version.
static VERSION: &str = "0.1.0";

/// Errors that can occur during parsing.
#[derive(Debug)]
pub enum ParseError {
    /// The input was empty.
    EmptyInput,
    /// An invalid character was found at the given position.
    InvalidCharacter(usize),
}

/// Configuration for the parser.
pub struct Config {
    /// Whether to enable verbose logging.
    pub verbose: bool,
    /// Maximum depth for recursive parsing.
    pub max_depth: u32,
}

/// A parsed abstract syntax tree.
pub struct Ast {
    /// The root node of the tree.
    pub root: Node,
    /// Source map from byte offset to line number.
    source_map: HashMap<usize, u32>,
}

/// A single node in the AST.
pub enum Node {
    /// A leaf node containing a literal value.
    Literal(Literal),
    /// A branch node with children.
    Branch { label: String, children: Vec<Node> },
}

/// A literal value in the AST.
pub struct Literal {
    /// The raw string value.
    pub value: String,
    /// The parsed numeric value, if applicable.
    pub numeric: Option<f64>,
}

/// Trait for types that can be parsed from source text.
pub trait Parse {
    /// Parse from a string, returning the parsed value or an error.
    fn parse(input: &str) -> Result<Self, ParseError>
    where
        Self: Sized;

    /// Validate the input without fully parsing it.
    fn validate(input: &str) -> bool {
        !input.is_empty()
    }
}

impl Ast {
    /// Create a new AST from a root node.
    pub fn new(root: Node) -> Self {
        Self {
            root,
            source_map: HashMap::new(),
        }
    }

    /// Look up the line number for a byte offset.
    pub fn line_at(&self, offset: usize) -> Option<u32> {
        self.source_map.get(&offset).copied()
    }
}

/// Parse a string into an AST.
///
/// This is the main entry point for parsing. It delegates to the appropriate
/// parser based on the detected format.
pub fn parse(input: &str) -> Result<Ast, ParseError> {
    if input.is_empty() {
        return Err(ParseError::EmptyInput);
    }
    let root = Node::Literal(Literal {
        value: input.to_string(),
        numeric: None,
    });
    Ok(Ast::new(root))
}

/// Format an AST node for display.
pub fn format_node(node: &Node) -> String {
    match node {
        Node::Literal(lit) => lit.value.clone(),
        Node::Branch { label, children } => {
            let child_strs: Vec<String> = children.iter().map(format_node).collect();
            format!("{}({})", label, child_strs.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        assert!(parse("").is_err());
    }
}