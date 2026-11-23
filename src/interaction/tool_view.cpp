#include "interaction/view.hpp"

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/terminal.hpp>

#include <sstream>
#include <string>

#include "interaction/theme.hpp"
#include "interaction/tool_diff.hpp"

namespace booth {
namespace {

#define kText (theme_color("text", ftxui::Color::RGB(228, 228, 228)))
#define kDim (theme_color("dim", ftxui::Color::RGB(118, 118, 118)))
#define kGreen (theme_color("green", ftxui::Color::RGB(0, 255, 135)))
#define kRed (theme_color("red", ftxui::Color::RGB(255, 95, 95)))

std::vector<std::string> wrap_cells(const std::string& text, std::size_t width) {
  std::vector<std::string> out;
  if (width == 0) width = 1;
  for (const auto& logical : [&] {
         std::vector<std::string> lines;
         std::istringstream input(text);
         std::string line;
         while (std::getline(input, line)) lines.push_back(line);
         if (lines.empty()) lines.push_back("");
         return lines;
       }()) {
    std::string row;
    std::size_t cells = 0;
    for (const char ch : logical) {
      const bool continuation = (static_cast<unsigned char>(ch) & 0xC0) == 0x80;
      if (!continuation && cells == width) {
        out.push_back(row);
        row.clear();
        cells = 0;
      }
      row += ch;
      if (!continuation) ++cells;
    }
    out.push_back(row);
  }
  return out;
}

}  // namespace

ftxui::Element tool_view(const TranscriptEntry& entry, bool expanded) {
  const ftxui::Color border = theme_color("border", ftxui::Color::RGB(108, 108, 108));
  std::string title = entry.text.empty() ? entry.tool : entry.text;
  const int columns = ftxui::Terminal::Size().dimx;
  const std::size_t content_width =
      columns > 12 ? static_cast<std::size_t>(columns - 8) : 20;
  if (title.size() > content_width) title = title.substr(0, content_width - 1) + "…";
  ftxui::Elements rows;
  rows.push_back(ftxui::hbox({
      ftxui::text("╭─── ") | ftxui::color(border),
      ftxui::text(title) | ftxui::bold |
          ftxui::color(entry.status == "failed" ? kRed : kText),
      ftxui::text(" "),
      ftxui::separator() | ftxui::flex | ftxui::color(border),
      ftxui::text("╮") | ftxui::color(border),
  }));
  const auto diff = booth::tool_diff(entry);
  if (!diff.empty()) {
    rows.push_back(ftxui::hbox({
        ftxui::text("├── Diff ") | ftxui::color(border),
        ftxui::separator() | ftxui::flex | ftxui::color(border),
        ftxui::text("┤") | ftxui::color(border),
    }));
    for (const auto& line : diff) {
      const ftxui::Color color = line.change < 0 ? kRed : line.change > 0 ? kGreen : kDim;
      for (const auto& wrapped : wrap_cells(line.text, content_width)) {
        rows.push_back(ftxui::hbox({
            ftxui::text("│ ") | ftxui::color(border),
            ftxui::text(wrapped) | ftxui::flex | ftxui::color(color),
            ftxui::text(" │") | ftxui::color(border),
        }));
      }
    }
  } else if (!entry.input.empty()) {
    for (const auto& line : wrap_cells(entry.input, content_width)) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│   ") | ftxui::color(border),
          ftxui::text(line) | ftxui::flex | ftxui::color(kText),
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  }
  if (!entry.output.empty()) {
    rows.push_back(ftxui::hbox({
        ftxui::text("├── Output ") | ftxui::color(border),
        ftxui::separator() | ftxui::flex | ftxui::color(border),
        ftxui::text("┤") | ftxui::color(border),
    }));
    std::istringstream output(entry.output);
    std::string line;
    std::size_t shown = 0;
    std::size_t hidden = 0;
    while (std::getline(output, line)) {
      if (!expanded && shown >= 12) {
        ++hidden;
        continue;
      }
      ++shown;
      for (const auto& wrapped : wrap_cells(line, content_width)) {
        rows.push_back(ftxui::hbox({
            ftxui::text("│ ") | ftxui::color(border),
            ftxui::text(wrapped) | ftxui::flex | ftxui::color(kText),
            ftxui::text(" │") | ftxui::color(border),
        }));
      }
    }
    if (hidden > 0) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text("… +" + std::to_string(hidden) + " lines · Ctrl+O") | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  }
  std::string footer = entry.status.empty() ? "running" : entry.status;
  if (entry.finished > entry.started && entry.started != std::chrono::system_clock::time_point{}) {
    const auto ms =
        std::chrono::duration_cast<std::chrono::milliseconds>(entry.finished - entry.started);
    footer = std::to_string(ms.count()) + "ms";
  }
  rows.push_back(ftxui::hbox({
      ftxui::text("╰─── ") | ftxui::color(border),
      ftxui::text(footer) | ftxui::dim,
      ftxui::text(" ") ,
      ftxui::separator() | ftxui::flex | ftxui::color(border),
      ftxui::text("╯") | ftxui::color(border),
  }));
  return ftxui::vbox(std::move(rows));
}

}  // namespace booth
