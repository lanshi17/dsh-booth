#include "interaction/text_util.hpp"

#include <ftxui/screen/string.hpp>

namespace booth {

std::string ellipsize(std::string_view text, std::size_t max_cells) {
  if (max_cells == 0) return {};
  if (static_cast<std::size_t>(ftxui::string_width(text)) <= max_cells) return std::string(text);
  if (max_cells == 1) return "…";
  const std::size_t budget = max_cells - 1;
  std::string out;
  std::size_t width = 0;
  for (const auto& glyph : ftxui::Utf8ToGlyphs(text)) {
    const int cells = ftxui::string_width(glyph);
    if (width + static_cast<std::size_t>(cells) > budget) break;
    out += glyph;
    width += static_cast<std::size_t>(cells);
  }
  out += "…";
  return out;
}

}  // namespace booth
