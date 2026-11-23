#include "interaction/emoji.hpp"

#include <array>
#include <utility>

namespace booth {
namespace {

constexpr std::array<std::pair<std::string_view, std::string_view>, 10> kEmoji{{
    {":smile:", "😄"},
    {":tada:", "🎉"},
    {":rocket:", "🚀"},
    {":heart:", "❤️"},
    {":fire:", "🔥"},
    {":thumbsup:", "👍"},
    {":check:", "✅"},
    {":x:", "❌"},
    {":wave:", "👋"},
    {":thinking:", "🤔"},
}};

}  // namespace

std::string expand_emojis(std::string_view text) {
  std::string out;
  out.reserve(text.size());
  for (std::size_t index = 0; index < text.size();) {
    if (text[index] != ':') {
      out.push_back(text[index++]);
      continue;
    }
    bool replaced = false;
    for (const auto& [code, emoji] : kEmoji) {
      if (text.substr(index, code.size()) == code) {
        out += emoji;
        index += code.size();
        replaced = true;
        break;
      }
    }
    if (!replaced) out.push_back(text[index++]);
  }
  return out;
}

}  // namespace booth
