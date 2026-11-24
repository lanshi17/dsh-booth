#pragma once

#include <cctype>
#include <string>
#include <string_view>

namespace booth {


/// Return the index of the ')' closing a link target, honoring balanced parentheses.
inline std::size_t link_end(std::string_view text, std::size_t start) {
  int depth = 0;
  for (std::size_t index = start; index < text.size(); ++index) {
    if (text[index] == '\\' && index + 1 < text.size()) {
      ++index;
      continue;
    }
    if (text[index] == '(') {
      ++depth;
    } else if (text[index] == ')') {
      if (depth == 0) return index;
      --depth;
    }
  }
  return std::string_view::npos;
}

/// True when a non-quote line starts a block that must end a lazy blockquote continuation.
inline bool lazy_quote_stop(const std::string& line) {
  const auto first = line.find_first_not_of(" \t");
  if (first == std::string::npos) return true;
  const char ch = line[first];
  if (ch == '>' || ch == '#' || ch == '`' || ch == '|') return true;
  if ((ch == '-' || ch == '*' || ch == '+') && first + 1 < line.size() && line[first + 1] == ' ') {
    return true;
  }
  if (std::isdigit(static_cast<unsigned char>(ch)) &&
      line.find(". ", first) != std::string::npos) {
    return true;
  }
  if (line.compare(first, 2, "$$") == 0 || line.compare(first, 7, "\\begin{") == 0) return true;
  return false;
}

/// Decodes the named HTML entities CommonMark recognises in prose.
inline std::string decode_html_entities(std::string_view text) {
  struct Entity {
    std::string_view name;
    char value;
  };
  static constexpr Entity kEntities[] = {
      {"&amp;", '&'}, {"&lt;", '<'}, {"&gt;", '>'}, {"&quot;", '"'},
      {"&#39;", '\''}, {"&apos;", '\''}, {"&nbsp;", ' '},
  };
  std::string out;
  out.reserve(text.size());
  for (std::size_t index = 0; index < text.size();) {
    if (text[index] == '&') {
      bool matched = false;
      for (const auto& entity : kEntities) {
        if (text.compare(index, entity.name.size(), entity.name) == 0) {
          out += entity.value;
          index += entity.name.size();
          matched = true;
          break;
        }
      }
      if (matched) continue;
    }
    out += text[index++];
  }
  return out;
}

}  // namespace booth
