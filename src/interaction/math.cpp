#include "interaction/math.hpp"

#include <cctype>
#include <string>
#include <string_view>
#include <utility>

namespace booth {
namespace {

struct Symbol {
  std::string_view command;
  std::string_view glyph;
};

constexpr Symbol kSymbols[] = {
    {"alpha", "α"},     {"beta", "β"},      {"gamma", "γ"},     {"delta", "δ"},
    {"epsilon", "ε"},   {"varepsilon", "ε"}, {"zeta", "ζ"},     {"eta", "η"},
    {"theta", "θ"},     {"iota", "ι"},      {"kappa", "κ"},     {"lambda", "λ"},
    {"mu", "μ"},        {"nu", "ν"},        {"xi", "ξ"},        {"pi", "π"},
    {"rho", "ρ"},       {"sigma", "σ"},     {"tau", "τ"},       {"upsilon", "υ"},
    {"phi", "φ"},       {"varphi", "φ"},    {"chi", "χ"},       {"psi", "ψ"},
    {"omega", "ω"},     {"Gamma", "Γ"},     {"Delta", "Δ"},     {"Theta", "Θ"},
    {"Lambda", "Λ"},    {"Xi", "Ξ"},        {"Pi", "Π"},        {"Sigma", "Σ"},
    {"Phi", "Φ"},       {"Psi", "Ψ"},       {"Omega", "Ω"},
    {"pm", "±"},        {"mp", "∓"},        {"times", "×"},     {"cdot", "·"},
    {"div", "÷"},       {"le", "≤"},        {"leq", "≤"},       {"ge", "≥"},
    {"geq", "≥"},       {"ne", "≠"},        {"neq", "≠"},       {"approx", "≈"},
    {"equiv", "≡"},     {"sim", "∼"},       {"propto", "∝"},    {"infty", "∞"},
    {"sum", "∑"},       {"prod", "∏"},      {"coprod", "∐"},    {"int", "∫"},
    {"iint", "∬"},      {"iiint", "∭"},     {"oint", "∮"},      {"partial", "∂"},
    {"nabla", "∇"},     {"forall", "∀"},    {"exists", "∃"},    {"in", "∈"},
    {"notin", "∉"},     {"subset", "⊂"},    {"subseteq", "⊆"},  {"supset", "⊃"},
    {"supseteq", "⊇"},  {"cup", "∪"},       {"cap", "∩"},       {"to", "→"},
    {"rightarrow", "→"}, {"leftarrow", "←"}, {"Rightarrow", "⇒"}, {"Leftarrow", "⇐"},
    {"leftrightarrow", "↔"}, {"Leftrightarrow", "⇔"}, {"mapsto", "↦"},
    {"cdots", "⋯"},     {"ldots", "…"},     {"dots", "…"},      {"degree", "°"},
    {"circ", "∘"},      {"prime", "′"},     {"therefore", "∴"}, {"because", "∵"},
    {"langle", "⟨"},    {"rangle", "⟩"},    {"lbrace", "{"},    {"rbrace", "}"},
    {"vert", "|"},      {"lvert", "|"},     {"rvert", "|"},     {"mid", "|"},
    {"top", "⊤"},       {"bot", "⊥"},       {"emptyset", "∅"},  {"varnothing", "∅"},
    {"aleph", "ℵ"},     {"hbar", "ℏ"},      {"ell", "ℓ"},       {"Re", "ℝ"},
    {"Im", "𝕀"},       {"checkmark", "✓"},
};

std::string_view symbol_for(std::string_view command) {
  for (const auto& symbol : kSymbols) {
    if (symbol.command == command) return symbol.glyph;
  }
  return {};
}

struct ScriptChar {
  char from;
  const char* to;
};

constexpr ScriptChar kSuperscript[] = {
    {'0', "⁰"}, {'1', "¹"}, {'2', "²"}, {'3', "³"}, {'4', "⁴"}, {'5', "⁵"},
    {'6', "⁶"}, {'7', "⁷"}, {'8', "⁸"}, {'9', "⁹"}, {'+', "⁺"}, {'-', "⁻"},
    {'=', "⁼"}, {'(', "⁽"}, {')', "⁾"}, {'a', "ᵃ"}, {'b', "ᵇ"}, {'c', "ᶜ"},
    {'d', "ᵈ"}, {'e', "ᵉ"}, {'f', "ᶠ"}, {'g', "ᵍ"}, {'h', "ʰ"}, {'i', "ⁱ"},
    {'j', "ʲ"}, {'k', "ᵏ"}, {'l', "ˡ"}, {'m', "ᵐ"}, {'n', "ⁿ"}, {'o', "ᵒ"},
    {'p', "ᵖ"}, {'r', "ʳ"}, {'s', "ˢ"}, {'t', "ᵗ"}, {'u', "ᵘ"}, {'v', "ᵛ"},
    {'w', "ʷ"}, {'x', "ˣ"}, {'y', "ʸ"}, {'z', "ᶻ"},
};

constexpr ScriptChar kSubscript[] = {
    {'0', "₀"}, {'1', "₁"}, {'2', "₂"}, {'3', "₃"}, {'4', "₄"}, {'5', "₅"},
    {'6', "₆"}, {'7', "₇"}, {'8', "₈"}, {'9', "₉"}, {'+', "₊"}, {'-', "₋"},
    {'=', "₌"}, {'(', "₍"}, {')', "₎"}, {'a', "ₐ"}, {'e', "ₑ"}, {'h', "ₕ"},
    {'i', "ᵢ"}, {'j', "ⱼ"}, {'k', "ₖ"}, {'l', "ₗ"}, {'m', "ₘ"}, {'n', "ₙ"},
    {'o', "ₒ"}, {'p', "ₚ"}, {'r', "ᵣ"}, {'s', "ₛ"}, {'t', "ₜ"}, {'u', "ᵤ"},
    {'v', "ᵥ"}, {'x', "ₓ"},
};

const std::pair<std::string_view, std::string_view> kVulgar[] = {
    {"1/2", "½"}, {"1/3", "⅓"}, {"2/3", "⅔"}, {"1/4", "¼"}, {"3/4", "¾"},
    {"1/5", "⅕"}, {"2/5", "⅖"}, {"3/5", "⅗"}, {"4/5", "⅘"}, {"1/6", "⅙"},
    {"5/6", "⅚"}, {"1/8", "⅛"}, {"3/8", "⅜"}, {"5/8", "⅝"}, {"7/8", "⅞"},
    {"1/9", "⅑"}, {"1/10", "⅒"},
};

std::string trim(std::string value) {
  const auto first = value.find_first_not_of(" \t\r\n");
  if (first == std::string::npos) return {};
  const auto last = value.find_last_not_of(" \t\r\n");
  return value.substr(first, last - first + 1);
}

bool needs_parens(const std::string& text) {
  return text.find_first_of(" +-") != std::string::npos;
}

std::string fraction(std::string numerator, std::string denominator) {
  numerator = trim(std::move(numerator));
  denominator = trim(std::move(denominator));
  if (numerator.empty() || denominator.empty()) {
    return numerator + "/" + denominator;
  }
  const std::string simple = numerator + "/" + denominator;
  for (const auto& [key, glyph] : kVulgar) {
    if (key == simple) return std::string(glyph);
  }
  const bool parens = needs_parens(numerator) || needs_parens(denominator);
  return std::string(parens && needs_parens(numerator) ? "(" : "") + numerator +
         (parens && needs_parens(numerator) ? ")" : "") + "/" +
         (parens && needs_parens(denominator) ? "(" : "") + denominator +
         (parens && needs_parens(denominator) ? ")" : "");
}

std::string radical(std::string body, const std::string& index) {
  body = trim(std::move(body));
  if (index == "3") return "∛" + (needs_parens(body) ? "(" + body + ")" : body);
  const std::string head = index.empty() ? "√" : "√[" + index + "]";
  return head + (needs_parens(body) ? "(" + body + ")" : body);
}

std::string mapped_script(const std::string& text, const ScriptChar* table, std::size_t size,
                          char marker) {
  if (text.empty()) return {};
  std::string out;
  for (const char ch : text) {
    if (ch == ' ') {
      out += ' ';
      continue;
    }
    const ScriptChar* found = nullptr;
    for (std::size_t index = 0; index < size; ++index) {
      if (table[index].from == ch) {
        found = &table[index];
        break;
      }
    }
    if (found == nullptr) return std::string(1, marker) + "(" + text + ")";
    out += found->to;
  }
  return out;
}

std::string superscript(const std::string& text) {
  return mapped_script(text, kSuperscript,
                       sizeof(kSuperscript) / sizeof(kSuperscript[0]), '^');
}

std::string subscript(const std::string& text) {
  return mapped_script(text, kSubscript, sizeof(kSubscript) / sizeof(kSubscript[0]), '_');
}

class Parser {
 public:
  explicit Parser(std::string_view source) : source_(source) {}

  std::string render() {
    std::size_t index = 0;
    return sequence(index, '\0');
  }

 private:
  std::string sequence(std::size_t& index, char stop) {
    std::string out;
    while (index < source_.size()) {
      const char ch = source_[index];
      if (stop != '\0' && ch == stop) {
        ++index;
        break;
      }
      if (ch == '}') {
        ++index;
        if (stop != '\0') break;
        continue;
      }
      if (ch == '{') {
        ++index;
        out += sequence(index, '}');
        continue;
      }
      if (ch == '\\') {
        out += command(index);
        continue;
      }
      if (ch == '^' || ch == '_') {
        ++index;
        const std::string argument = script(index);
        out += ch == '^' ? superscript(argument) : subscript(argument);
        continue;
      }
      if (ch == '&') {
        out += ' ';
        ++index;
        continue;
      }
      out += ch;
      ++index;
    }
    return out;
  }

  void skip_spaces(std::size_t& index) {
    while (index < source_.size() && (source_[index] == ' ' || source_[index] == '\t')) ++index;
  }

  std::string glyph(std::size_t& index) {
    if (index >= source_.size()) return {};
    std::size_t end = index + 1;
    while (end < source_.size() && (static_cast<unsigned char>(source_[end]) & 0xC0) == 0x80) {
      ++end;
    }
    std::string out(source_.substr(index, end - index));
    index = end;
    return out;
  }

  std::string atom(std::size_t& index) {
    skip_spaces(index);
    if (index >= source_.size()) return {};
    if (source_[index] == '\\') return command(index);
    if (source_[index] == '{') {
      ++index;
      return sequence(index, '}');
    }
    return glyph(index);
  }

  std::string script(std::size_t& index) {
    skip_spaces(index);
    if (index >= source_.size()) return {};
    if (source_[index] == '{') {
      ++index;
      return sequence(index, '}');
    }
    if (source_[index] == '\\') return command(index);
    return glyph(index);
  }

  std::string group_or_atom(std::size_t& index) {
    skip_spaces(index);
    if (index < source_.size() && source_[index] == '{') {
      ++index;
      return sequence(index, '}');
    }
    return atom(index);
  }

  std::string raw_group(std::size_t& index) {
    skip_spaces(index);
    if (index >= source_.size() || source_[index] != '{') return atom(index);
    ++index;
    std::string out;
    int depth = 1;
    while (index < source_.size()) {
      const char ch = source_[index];
      if (ch == '\\') {
        ++index;
        if (index < source_.size()) {
          out += source_[index];
          ++index;
        }
        continue;
      }
      if (ch == '{') {
        ++depth;
        out += ch;
        ++index;
        continue;
      }
      if (ch == '}') {
        --depth;
        ++index;
        if (depth == 0) break;
        out += ch;
        continue;
      }
      out += ch;
      ++index;
    }
    return out;
  }

  std::string command(std::size_t& index) {
    ++index;
    if (index >= source_.size()) return "\\";
    const char ch = source_[index];
    if (!std::isalpha(static_cast<unsigned char>(ch))) {
      ++index;
      switch (ch) {
        case '{': return "{";
        case '}': return "}";
        case '_': return "_";
        case '%': return "%";
        case '&': return "&";
        case '#': return "#";
        case '$': return "$";
        case ',': return " ";
        case ';': return " ";
        case ':': return " ";
        case '!': return "";
        case '\\': return " ";
        default: return std::string(1, ch);
      }
    }
    const std::size_t start = index;
    while (index < source_.size() &&
           std::isalpha(static_cast<unsigned char>(source_[index]))) {
      ++index;
    }
    const std::string name(source_.substr(start, index - start));
    if (name == "frac" || name == "dfrac" || name == "tfrac" || name == "cfrac") {
      const std::string numerator = group_or_atom(index);
      const std::string denominator = group_or_atom(index);
      return fraction(numerator, denominator);
    }
    if (name == "sqrt") {
      skip_spaces(index);
      std::string root;
      if (index < source_.size() && source_[index] == '[') {
        ++index;
        root = sequence(index, ']');
      }
      return radical(group_or_atom(index), trim(root));
    }
    if (name == "text" || name == "mathrm" || name == "operatorname" || name == "textbf" ||
        name == "textit" || name == "emph" || name == "textnormal" || name == "textsf" ||
        name == "texttt") {
      return raw_group(index);
    }
    if (name == "color") {
      raw_group(index);
      return {};
    }
    if (name == "textcolor") {
      raw_group(index);
      return group_or_atom(index);
    }
    if (name == "left" || name == "right" || name == "middle") {
      skip_spaces(index);
      if (index >= source_.size()) return {};
      if (source_[index] == '\\') return command(index);
      return glyph(index);
    }
    if (name == "begin" || name == "end") {
      raw_group(index);
      return {};
    }
    if (name == "quad") return "  ";
    if (name == "qquad") return "    ";
    const std::string_view mapped = symbol_for(name);
    if (!mapped.empty()) return std::string(mapped);
    return "\\" + name;
  }

  std::string_view source_;
};

}  // namespace

std::string latex_to_unicode(std::string_view source) {
  if (source.empty()) return {};
  const std::string raw = Parser(source).render();
  std::string out;
  for (const char ch : raw) {
    const char normalized = ch == '\n' || ch == '\t' ? ' ' : ch;
    if (normalized == ' ' && !out.empty() && out.back() == ' ') continue;
    out += normalized;
  }
  return out;
}

std::string render_math_in_text(const std::string& text) {
  std::string out;
  for (std::size_t index = 0; index < text.size();) {
    if (text[index] == '\\' && index + 1 < text.size() && text[index + 1] == '$') {
      out += '$';
      index += 2;
      continue;
    }
    if (const auto span = math_span_at(text, index)) {
      out += span->text;
      index = span->end;
      continue;
    }
    out += text[index++];
  }
  return out;
}

std::optional<MathSpan> math_span_at(const std::string& text, std::size_t at) {
  if (at >= text.size()) return std::nullopt;
  std::size_t opener_length = 0;
  std::string close;
  if (text[at] == '$') {
    if (at + 1 < text.size() && text[at + 1] == '$') {
      opener_length = 2;
      close = "$$";
    } else {
      opener_length = 1;
      close = "$";
    }
  } else if (text[at] == '\\' && at + 1 < text.size() && text[at + 1] == '(') {
    opener_length = 2;
    close = "\\)";
  } else if (text[at] == '\\' && at + 1 < text.size() && text[at + 1] == '[') {
    opener_length = 2;
    close = "\\]";
  } else {
    return std::nullopt;
  }

  const std::size_t body_start = at + opener_length;
  if (body_start >= text.size() || std::isspace(static_cast<unsigned char>(text[body_start]))) {
    return std::nullopt;
  }
  std::size_t search = body_start;
  while (true) {
    const std::size_t close_at = text.find(close, search);
    if (close_at == std::string::npos || close_at == body_start) return std::nullopt;
    if (!std::isspace(static_cast<unsigned char>(text[close_at - 1]))) {
      if (opener_length != 1 || close_at + 1 >= text.size() ||
          !std::isdigit(static_cast<unsigned char>(text[close_at + 1]))) {
        const std::string body = text.substr(body_start, close_at - body_start);
        if (body.find('\n') != std::string::npos) return std::nullopt;
        return MathSpan{close_at + close.size(), latex_to_unicode(body)};
      }
    }
    search = close_at + close.size();
  }
}

}  // namespace booth
