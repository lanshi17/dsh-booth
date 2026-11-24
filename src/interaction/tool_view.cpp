#include "interaction/view.hpp"

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/terminal.hpp>

#include <sstream>
#include <string>

#include "interaction/text_util.hpp"
#include "interaction/theme.hpp"
#include "interaction/tool_diff.hpp"

namespace booth {
namespace {

#define kText (theme_color("text", ftxui::Color::RGB(228, 228, 228)))
#define kDim (theme_color("dim", ftxui::Color::RGB(118, 118, 118)))
#define kGreen (theme_color("green", ftxui::Color::RGB(0, 255, 135)))
#define kRed (theme_color("red", ftxui::Color::RGB(255, 95, 95)))


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

std::vector<ReadEnvelope> parse_read_envelopes(const std::string& output) {
  std::vector<ReadEnvelope> envelopes;
  std::size_t start = output.find("<path>");
  while (start != std::string::npos) {
    const std::size_t next = output.find("<path>", start + 6);
    ReadEnvelope envelope =
        parse_read_envelope(output.substr(start, next == std::string::npos ? std::string::npos
                                                                          : next - start));
    if (!envelope.path.empty() || !envelope.rows.empty() || !envelope.footer.empty()) {
      envelopes.push_back(std::move(envelope));
    }
    start = next;
  }
  return envelopes;
}

}  // namespace

std::string read_output_text(const std::string& output) {
  std::string out;
  for (const auto& read : parse_read_envelopes(output)) {
    if (!out.empty()) out += '\n';
    if (!read.path.empty()) out += read.path + '\n';
    for (const auto& row : read.rows) out += row + '\n';
    if (!read.footer.empty()) out += read.footer + '\n';
  }
  return out;
}

ftxui::Element tool_view(const TranscriptEntry& entry, bool expanded) {
  const ftxui::Color border = theme_color("border", ftxui::Color::RGB(108, 108, 108));
  const bool read_tool = entry.tool == "read" || entry.tool == "read_file";
  const std::vector<ReadEnvelope> reads =
      read_tool ? parse_read_envelopes(entry.output) : std::vector<ReadEnvelope>{};
  const bool has_envelope = !reads.empty();
  std::string title = entry.text.empty() ? entry.tool : entry.text;
  if (reads.size() == 1 && !reads[0].path.empty()) title = reads[0].path;
  const int columns = ftxui::Terminal::Size().dimx;
  const std::size_t content_width =
      columns > 12 ? static_cast<std::size_t>(columns - 8) : 20;
  title = ellipsize(title, content_width);
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
    const bool shell = entry.tool == "bash" || entry.tool == "sh" || entry.tool == "shell";
    const std::size_t input_width = shell ? std::max<std::size_t>(1, content_width - 2) : content_width;
    bool first = true;
    for (const auto& line : wrap_cells(entry.input, input_width)) {
      std::string prefix = shell ? (first ? "$ " : "  ") : "  ";
      first = false;
      rows.push_back(ftxui::hbox({
          ftxui::text("│ " + prefix) | ftxui::color(border),
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
    const auto push_output = [&](const std::string& text, ftxui::Color color, bool dim) {
      for (const auto& wrapped : wrap_cells(text, content_width)) {
        ftxui::Element line = ftxui::text(wrapped) | ftxui::flex | ftxui::color(color);
        if (dim) line = line | ftxui::dim;
        rows.push_back(ftxui::hbox({
            ftxui::text("│ ") | ftxui::color(border),
            std::move(line),
            ftxui::text(" │") | ftxui::color(border),
        }));
      }
    };
    if (has_envelope && reads.size() == 1) {
      const ReadEnvelope& read = reads[0];
      const std::size_t limit = expanded ? read.rows.size() : 3;
      for (; shown < std::min(limit, read.rows.size()); ++shown) {
        push_output(read.rows[shown], kText, false);
      }
      hidden = read.rows.size() - shown;
    } else if (has_envelope && !expanded) {
      for (const auto& read : reads) {
        const std::size_t lines = read.rows.size();
        push_output(read.path + " · " + std::to_string(lines) +
                        (lines == 1 ? " line" : " lines"),
                    kText, false);
      }
      push_output("… Ctrl+O for file contents", kDim, true);
    } else if (has_envelope) {
      for (const auto& read : reads) {
        push_output(read.path, kText, false);
        for (const auto& row : read.rows) push_output(row, kText, false);
        if (!read.footer.empty()) push_output(read.footer, kDim, true);
      }
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
    if (has_envelope && reads.size() == 1 && !reads[0].footer.empty()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text(reads[0].footer) | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  }
  std::string footer = entry.status.empty() ? "running" : entry.status;
  if (entry.finished > entry.started && entry.started != std::chrono::system_clock::time_point{}) {
    const auto ms =
        std::chrono::duration_cast<std::chrono::milliseconds>(entry.finished - entry.started);
    footer += " · " + std::to_string(ms.count()) + "ms";
  }
  ftxui::Element status = ftxui::text(footer) | ftxui::dim;
  if (entry.status == "failed") status = ftxui::text(footer) | ftxui::color(kRed);
  rows.push_back(ftxui::hbox({
      ftxui::text("╰─── ") | ftxui::color(border),
      std::move(status),
      ftxui::text(" ") ,
      ftxui::separator() | ftxui::flex | ftxui::color(border),
      ftxui::text("╯") | ftxui::color(border),
  }));
  return ftxui::vbox(std::move(rows));
}

}  // namespace booth
