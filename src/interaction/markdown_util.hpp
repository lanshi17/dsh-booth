#pragma once

#include <string>
#include <string_view>

namespace booth {

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
