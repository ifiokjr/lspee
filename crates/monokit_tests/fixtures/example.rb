# Example Ruby file for outline extraction testing

MAX_RETRIES = 3
VERSION = "1.0.0"

# Configuration for the parser.
class Config
  attr_accessor :verbose, :max_depth

  # Create a new configuration with defaults.
  def initialize(verbose: false, max_depth: 10)
    @verbose = verbose
    @max_depth = max_depth
  end
end

# Error raised when parsing fails.
class ParseError < StandardError
  attr_reader :position

  # Create a parse error with message and position.
  def initialize(message, position)
    super(message)
    @position = position
  end
end

# Module containing parsing utilities.
module Parser
  # Parse a string into an AST.
  def self.parse(input)
    raise ParseError.new("empty input", 0) if input.empty?
    Ast.new(Node.new(value: input))
  end

  # Format a node for display.
  def self.format_node(node)
    node.children.empty? ? node.value : node.label
  end
end