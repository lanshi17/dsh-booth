#include "interaction/error_view.hpp"

#include <algorithm>
#include <string>
#include <vector>

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/terminal.hpp>

#include "interaction/text_util.hpp"
#include "interaction/theme.hpp"

namespace booth {

ftxui::Element error_banner_view(const std::string& error) {
  const auto red = theme_color("red", ftxui::Color::RGB(255, 95, 95));
  const int columns = ftxui::Terminal::Size().dimx;
  const std::size_t width = columns > 12 ? static_cast<std::size_t>(columns - 8) : 20;
  ftxui::Elements rows;
  rows.push_back(ftxui::text("  error · /retry to try again") | ftxui::color(red) | ftxui::bold);
  constexpr std::size_t kErrorLines = 4;
  std::vector<std::string> lines = wrap_cells(error, width);
  for (std::size_t index = 0; index < std::min(lines.size(), kErrorLines); ++index) {
    rows.push_back(ftxui::text("  " + lines[index]) | ftxui::color(red));
  }
  if (lines.size() > kErrorLines) {
    rows.push_back(ftxui::text("  … +" + std::to_string(lines.size() - kErrorLines) + " lines") |
                   ftxui::dim);
  }
  return ftxui::vbox(std::move(rows));
}

}  // namespace booth
