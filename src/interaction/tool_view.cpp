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

struct ReadEnvelope {
  std::string path;
  std::vector<std::string> rows;
  std::string footer;
};

ReadEnvelope parse_read_envelope(const std::string& output) {
  ReadEnvelope envelope;
  const auto path_start = output.find("<path>");
  if (path_start != std::string::npos) {
    const auto path_end = output.find("</path>", path_start + 6);
    if (path_end != std::string::npos) {
      envelope.path = output.substr(path_start + 6, path_end - path_start - 6);
    }
  }
  const auto content_start = output.find("<content>");
  const auto content_end = content_start == std::string::npos
                               ? std::string::npos
                               : output.find("</content>", content_start + 9);
  if (content_start == std::string::npos) return envelope;
  const std::string body =
      output.substr(content_start + 9, content_end == std::string::npos
                                             ? std::string::npos
                                             : content_end - content_start - 9);
  std::istringstream input(body);
  std::string line;
  while (std::getline(input, line)) {
    if (!line.empty() && line.back() == '\r') line.pop_back();
    envelope.rows.push_back(line);
  }
  while (!envelope.rows.empty() && envelope.rows.front().empty()) envelope.rows.erase(envelope.rows.begin());
  while (!envelope.rows.empty() && envelope.rows.back().empty()) envelope.rows.pop_back();
  if (!envelope.rows.empty() && envelope.rows.back().size() >= 2 &&
      envelope.rows.back().front() == '(' && envelope.rows.back().back() == ')') {
    envelope.footer = envelope.rows.back();
    envelope.rows.pop_back();
    while (!envelope.rows.empty() && envelope.rows.back().empty()) envelope.rows.pop_back();
  }
  return envelope;
}

}  // namespace

ftxui::Element tool_view(const TranscriptEntry& entry, bool expanded) {
  const ftxui::Color border = theme_color("border", ftxui::Color::RGB(108, 108, 108));
  const bool read_tool = entry.tool == "read" || entry.tool == "read_file";
  const ReadEnvelope read = read_tool ? parse_read_envelope(entry.output) : ReadEnvelope{};
  const bool has_envelope = read_tool && (!read.rows.empty() || !read.footer.empty());
  std::string title = entry.text.empty() ? entry.tool : entry.text;
  if (!read.path.empty()) title = read.path;
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
    std::size_t shown = 0;
    std::size_t hidden = 0;
    if (has_envelope) {
      const std::size_t limit = expanded ? read.rows.size() : 3;
      for (; shown < std::min(limit, read.rows.size()); ++shown) {
        for (const auto& wrapped : wrap_cells(read.rows[shown], content_width)) {
          rows.push_back(ftxui::hbox({
              ftxui::text("│ ") | ftxui::color(border),
              ftxui::text(wrapped) | ftxui::flex | ftxui::color(kText),
              ftxui::text(" │") | ftxui::color(border),
          }));
        }
      }
      hidden = read.rows.size() - shown;
    } else {
      std::istringstream output(entry.output);
      std::string line;
      const std::size_t limit = !expanded && read_tool ? 3 : 12;
      while (std::getline(output, line)) {
        if (!expanded && shown >= limit) {
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
    }
    if (hidden > 0) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text("… +" + std::to_string(hidden) + " lines · Ctrl+O") | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
    if (has_envelope && !read.footer.empty()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text(read.footer) | ftxui::dim | ftxui::flex,
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
