#include "interaction/text_util.hpp"

#include <ftxui/screen/string.hpp>

#include <sstream>

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

std::vector<std::string> wrap_cells(const std::string& text, std::size_t width) {
  std::vector<std::string> out;
  if (width == 0) width = 1;
  std::istringstream input(text);
  std::string line;
  while (std::getline(input, line)) {
    std::string expanded;
    for (const char ch : line) {
      if (ch == '\t') {
        expanded += "    ";
      } else {
        expanded += ch;
      }
    }
    std::string row;
    std::size_t cells = 0;
    for (const auto& glyph : ftxui::Utf8ToGlyphs(expanded)) {
      const int glyph_cells = ftxui::string_width(glyph);
      const std::size_t width_of = static_cast<std::size_t>(glyph_cells);
      if (cells > 0 && cells + width_of > width) {
        out.push_back(row);
        row.clear();
        cells = 0;
      }
      row += glyph;
      cells += width_of;
    }
    out.push_back(row);
  }
  return out;
}

}  // namespace booth
