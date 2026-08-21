// Example C# file for outline extraction testing

using System;

namespace Parser
{
    /// <summary>
    /// Maximum number of retries before giving up.
    /// </summary>
    public static class Constants
    {
        public const int MaxRetries = 3;
        public const string Version = "1.0.0";
    }

    /// <summary>
    /// Configuration for the parser.
    /// </summary>
    public class Config
    {
        /// <summary>
        /// Whether to enable verbose logging.
        /// </summary>
        public bool Verbose { get; set; }

        /// <summary>
        /// Maximum depth for recursive parsing.
        /// </summary>
        public int MaxDepth { get; set; } = 10;
    }

    /// <summary>
    /// Error thrown when parsing fails.
    /// </summary>
    public class ParseError : Exception
    {
        /// <summary>
        /// Byte position where the error occurred.
        /// </summary>
        public int Position { get; }

        public ParseError(string message, int position)
            : base(message)
        {
            Position = position;
        }
    }

    /// <summary>
    /// Interface for types that can parse input.
    /// </summary>
    public interface IParseable
    {
        /// <summary>
        /// Parse the given input text.
        /// </summary>
        Ast Parse(string input);
    }

    /// <summary>
    /// Abstract syntax tree.
    /// </summary>
    public class Ast
    {
        /// <summary>
        /// Root node of the tree.
        /// </summary>
        public Node Root { get; set; }
    }
}