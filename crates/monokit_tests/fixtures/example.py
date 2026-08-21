"""Example Python module for outline extraction testing."""

import os
from typing import Optional, List

MAX_BATCH_SIZE = 100
VERSION = "1.0.0"


class Config:
    """Configuration for the parser."""

    verbose: bool
    max_depth: int

    def __init__(self, verbose: bool = False, max_depth: int = 10) -> None:
        """Create a new configuration."""
        self.verbose = verbose
        self.max_depth = max_depth


class ParseError(Exception):
    """Error raised when parsing fails."""

    def __init__(self, message: str, position: int) -> None:
        """Create a parse error with a message and byte position."""
        self.message = message
        self.position = position
        super().__init__(message)


class Ast:
    """A parsed abstract syntax tree."""

    def __init__(self, root: "Node") -> None:
        """Create an AST from a root node."""
        self.root = root

    def line_at(self, offset: int) -> Optional[int]:
        """Look up the line number for a byte offset."""
        return None


def parse(input_text: str) -> "Ast":
    """Parse a string into an AST.

    This is the main entry point for parsing. It delegates to
    the appropriate parser based on the detected format.

    Args:
        input_text: The source text to parse.

    Returns:
        An Ast representing the parsed input.

    Raises:
        ParseError: If the input cannot be parsed.
    """
    if not input_text:
        raise ParseError("empty input", 0)
    return Ast(Node.literal(input_text))


def format_node(node: "Node") -> str:
    """Format a node for display."""
    if isinstance(node, dict):
        return str(node)
    return str(node)