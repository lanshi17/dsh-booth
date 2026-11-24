#include "interaction/json_view.hpp"

#include <ftxui/dom/elements.hpp>

#include <algorithm>
#include <sstream>
#include <string>
#include <vector>

#include "interaction/text_util.hpp"
#include "interaction/theme.hpp"

namespace booth {

ftxui::Elements json_rows(const std::string& json, bool expanded, std::size_t content_width) {
  const auto border = theme_color("border", ftxui::Color::RGB(108, 108, 108));
  const auto text = theme_color("text", ftxui::Color::RGB(228, 228, 228));
  ftxui::Elements rows;
  std::istringstream input(json);
  std::vector<std::string> lines;
  std::string line;
  while (std::getline(input, line)) lines.push_back(line);
  constexpr std::size_t kJsonLines = 12;
  const std::size_t limit = expanded ? lines.size() : kJsonLines;
  for (std::size_t index = 0; index < std::min(limit, lines.size()); ++index) {
    for (const auto& wrapped : wrap_cells(lines[index], content_width)) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text(wrapped) | ftxui::flex | ftxui::color(text),
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  }
  if (!expanded && lines.size() > limit) {
    rows.push_back(ftxui::hbox({
        ftxui::text("│ ") | ftxui::color(border),
        ftxui::text("… +" + std::to_string(lines.size() - limit) +
                    " lines · Ctrl+O") | ftxui::dim | ftxui::flex,
        ftxui::text(" │") | ftxui::color(border),
    }));
  }
  return rows;
}

}  // namespace booth
