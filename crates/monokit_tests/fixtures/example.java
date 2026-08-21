// Example Java file for outline extraction testing

package com.example.parser;

/** Maximum number of retries before giving up. */
public static final int MAX_RETRIES = 3;

/** Application version. */
public static final String VERSION = "1.0.0";

/**
 * Configuration for the parser.
 */
public class Config {
    /** Whether verbose logging is enabled. */
    public boolean verbose;
    /** Maximum depth for recursive parsing. */
    public int maxDepth;

    /** Creates a new configuration with default settings. */
    public Config() {
        this.verbose = false;
        this.maxDepth = 10;
    }
}

/**
 * Error thrown when parsing fails.
 */
public class ParseError extends Exception {
    /** Byte position where the error occurred. */
    public int position;

    /** Creates a parse error with a message and position. */
    public ParseError(String message, int position) {
        super(message);
        this.position = position;
    }
}

/**
 * Interface for types that can parse input.
 */
public interface Parseable {
    /** Parse the given input string. */
    Ast parse(String input) throws ParseError;
}

/**
 * Abstract syntax tree with a root node.
 */
public class Ast {
    /** The root node of the tree. */
    public Node root;

    /** Create an AST with the given root. */
    public Ast(Node root) {
        this.root = root;
    }
}