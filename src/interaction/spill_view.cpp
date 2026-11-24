#include "interaction/spill_view.hpp"

#include <string>

#include <ftxui/dom/elements.hpp>

#include "interaction/text_util.hpp"
#include "interaction/theme.hpp"

namespace booth {

ftxui::Element spill_notice_row(const SpillNotice& notice) {
  const auto border = theme_color("border", ftxui::Color::RGB(108, 108, 108));
  const int columns = ftxui::Terminal::Size().dimx;
  const std::size_t width = columns > 12 ? static_cast<std::size_t>(columns - 8) : 20;
  std::string label = notice.summary + " · " + notice.path;
  return ftxui::hbox({
      ftxui::text("│ ") | ftxui::color(border),
      ftxui::text(ellipsize(label, width)) | ftxui::dim | ftxui::flex,
      ftxui::text(" │") | ftxui::color(border),
  });
}

}  // namespace booth
