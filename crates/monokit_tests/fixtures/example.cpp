// Example C++ file for outline extraction testing

#include <string>
#include <vector>
#include <stdexcept>

namespace parser {

constexpr int MAX_RETRIES = 3;
constexpr const char* VERSION = "1.0.0";

class Config {
public:
    bool verbose = false;
    int max_depth = 10;

    Config() = default;
};

class ParseError : public std::runtime_error {
public:
    int position;

    ParseError(const std::string& msg, int pos)
        : std::runtime_error(msg), position(pos) {}
};

class Node {
public:
    std::string label;
    std::string value;
    std::vector<Node> children;
};

class Ast {
public:
    Node root;

    Ast(Node root_node) : root(std::move(root_node)) {}
};

Ast parse(const std::string& input);
std::string format_node(const Node& node);

} // namespace parser