#include "interaction/view.hpp"

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/terminal.hpp>

#include <algorithm>
#include <cctype>
#include <optional>
#include <sstream>
#include <string>

#include "interaction/fetch_view.hpp"
#include "interaction/patch_view.hpp"
#include "interaction/text_util.hpp"
#include "interaction/theme.hpp"
#include "interaction/tool_diff.hpp"
#include "interaction/tool_parse.hpp"

namespace booth {
namespace {

struct ReadRowParts {
  std::string number;
  std::string text;
};

ReadRowParts split_read_row(const std::string& row) {
  std::size_t digits = 0;
  while (digits < row.size() && std::isdigit(static_cast<unsigned char>(row[digits]))) ++digits;
  if (digits == 0 || digits + 1 >= row.size() || row[digits] != ':' || row[digits + 1] != ' ') {
    return {};
  }
  return {row.substr(0, digits), row.substr(digits + 2)};
}

std::size_t read_gutter(const std::vector<std::string>& rows) {
  std::size_t width = 1;
  for (const auto& row : rows) {
    const auto parts = split_read_row(row);
    if (!parts.number.empty()) width = std::max(width, parts.number.size());
  }
  return width;
}

#define kText (theme_color("text", ftxui::Color::RGB(228, 228, 228)))
#define kDim (theme_color("dim", ftxui::Color::RGB(118, 118, 118)))
#define kAccent (theme_color("accent", ftxui::Color::RGB(0, 175, 255)))
#define kGreen (theme_color("green", ftxui::Color::RGB(0, 255, 135)))
#define kRed (theme_color("red", ftxui::Color::RGB(255, 95, 95)))

}  // namespace

ftxui::Element tool_view(const TranscriptEntry& entry, bool expanded) {
  const ftxui::Color border = theme_color("border", ftxui::Color::RGB(108, 108, 108));
  const bool read_tool = entry.tool == "read" || entry.tool == "read_file";
  const bool shell_tool = is_shell_tool(entry.tool);
  const std::optional<GrepResult> grep_result =
      entry.tool == "grep" && entry.status != "failed" ? parse_grep(entry.output) : std::nullopt;
  const std::optional<GlobResult> glob_result =
      entry.tool == "glob" && entry.status != "failed" ? parse_glob(entry.output) : std::nullopt;
  const std::optional<WebResult> web_result =
      entry.tool == "web_search" && entry.status != "failed" ? parse_web_search(entry.output)
                                                             : std::nullopt;
  const std::optional<std::vector<PatchFile>> patch =
      entry.tool == "apply_patch" && entry.status != "failed" ? codex_patch(entry) : std::nullopt;
  const std::optional<WebFetchResult> web_fetch =
      entry.tool == "web_fetch" && entry.status != "failed" ? parse_web_fetch(entry.output)
                                                            : std::nullopt;
  std::string output = entry.output;
  const ShellStatus shell_status = shell_tool ? take_shell_status(output) : ShellStatus{};
  const std::vector<ReadEnvelope> reads =
      read_tool ? parse_read_envelopes(entry.output) : std::vector<ReadEnvelope>{};
  const bool has_envelope = !reads.empty();
  const std::optional<std::vector<TodoItem>> todos = parse_todos(entry);
  const auto diff = booth::tool_diff(entry);
  const std::optional<std::string> json_output =
      !read_tool && !shell_tool && !todos && !grep_result && !glob_result && !web_result &&
              !patch && !web_fetch && entry.status != "failed" && diff.empty()
          ? pretty_json(entry.output)
          : std::nullopt;
  std::string title = entry.text.empty() ? entry.tool : entry.text;
  if (reads.size() == 1 && !reads[0].path.empty()) title = reads[0].path;
  if (todos) {
    title = "Todo";
    if (!todos->empty()) {
      const auto done = std::count_if(todos->begin(), todos->end(), [](const TodoItem& row) {
        return row.status == "completed";
      });
      title += " · " + std::to_string(done) + "/" + std::to_string(todos->size());
    }
  }
  if (grep_result) {
    title = "Grep";
    const std::string pattern = grep_pattern(entry);
    if (!pattern.empty()) title += " \"" + pattern + "\"";
    if (grep_result->total > 0) {
      title += " · " + std::to_string(grep_result->total) +
               (grep_result->total == 1 ? " match" : " matches");
    }
  }
  if (glob_result) {
    title = "Glob";
    const std::string pattern = glob_pattern(entry);
    if (!pattern.empty()) title += " \"" + pattern + "\"";
    if (!glob_result->paths.empty()) {
      title += " · " + std::to_string(glob_result->paths.size()) +
               (glob_result->paths.size() == 1 ? " file" : " files");
    }
  }
  if (json_output) title += " · JSON";
  if (patch) title = patch_title(*patch);
  if (web_fetch) title = web_fetch_title(*web_fetch);
  if (web_result) {
    title = "Web search";
    const std::string query = web_queries(entry);
    if (!query.empty()) title += " \"" + query + "\"";
    if (!web_result->sources.empty()) {
      title += " · " + std::to_string(web_result->sources.size()) +
               (web_result->sources.size() == 1 ? " source" : " sources");
    }
  }
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
  if (web_fetch) for (auto& row : web_fetch_rows(*web_fetch, expanded, content_width)) rows.push_back(std::move(row));
  if (patch) {
    for (auto& row : patch_rows(*patch, expanded, content_width)) rows.push_back(std::move(row));
  }
  if (!diff.empty() && !patch) {
    std::size_t added = 0;
    std::size_t removed = 0;
    for (const auto& line : diff) {
      if (line.change > 0) ++added;
      else if (line.change < 0) ++removed;
    }
    std::string diff_head = "├── Diff";
    if (added > 0 || removed > 0) {
      diff_head += " +" + std::to_string(added) + " −" + std::to_string(removed);
    }
    diff_head += " ";
    rows.push_back(ftxui::hbox({
        ftxui::text(diff_head) | ftxui::color(border),
        ftxui::separator() | ftxui::flex | ftxui::color(border),
        ftxui::text("┤") | ftxui::color(border),
    }));
    constexpr std::size_t kDiffLines = 40;
    std::size_t number_width = 1;
    for (const auto& line : diff) {
      const int number = line.change < 0 ? line.old_line : line.new_line;
      if (number > 0) number_width = std::max(number_width, std::to_string(number).size());
    }
    const std::size_t shown_limit = expanded ? diff.size() : std::min(diff.size(), kDiffLines);
    for (std::size_t index = 0; index < shown_limit; ++index) {
      const auto& line = diff[index];
      const ftxui::Color color = line.change < 0 ? kRed : line.change > 0 ? kGreen : kDim;
      const int number = line.change < 0 ? line.old_line : line.new_line;
      std::string gutter(number_width + 1, ' ');
      if (number > 0) {
        const std::string digits = std::to_string(number);
        gutter = std::string(number_width - digits.size(), ' ') + digits + " ";
      }
      for (const auto& wrapped : wrap_cells(line.text, content_width - number_width - 1)) {
        rows.push_back(ftxui::hbox({
            ftxui::text("│ " + gutter) | ftxui::dim | ftxui::color(border),
            ftxui::text(wrapped) | ftxui::flex | ftxui::color(color),
            ftxui::text(" │") | ftxui::color(border),
        }));
      }
    }
    if (shown_limit < diff.size()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text("… +" + std::to_string(diff.size() - shown_limit) +
                      " lines · Ctrl+O") | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  } else if (!entry.input.empty() && !todos && !grep_result && !glob_result && !web_result &&
             !patch && !web_fetch) {
    const std::size_t input_width =
        shell_tool ? std::max<std::size_t>(1, content_width - 2) : content_width;
    bool first = true;
    for (const auto& line : wrap_cells(entry.input, input_width)) {
      std::string prefix = shell_tool ? (first ? "$ " : "  ") : "  ";
      first = false;
      rows.push_back(ftxui::hbox({
          ftxui::text("│ " + prefix) | ftxui::color(border),
          ftxui::text(line) | ftxui::flex | ftxui::color(kText),
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  }
  if (todos) {
    const std::size_t limit = expanded ? todos->size() : 8;
    std::size_t shown = 0;
    for (const auto& todo : *todos) {
      if (shown >= limit) break;
      const bool done = todo.status == "completed";
      const bool active = todo.status == "in_progress";
      const std::string box = done ? "☑ " : "☐ ";
      const ftxui::Color color = done ? kGreen : active ? kAccent : kDim;
      bool first = true;
      const std::size_t width = std::max<std::size_t>(1, content_width - 2);
      std::vector<std::string> lines = wrap_cells(todo.content, width);
      if (lines.empty()) lines.emplace_back();
      for (const auto& line : lines) {
        const std::string prefix = first ? "│ " + box : "│   ";
        first = false;
        ftxui::Element label = ftxui::text(line) | ftxui::flex | ftxui::color(color);
        if (done) label = label | ftxui::strikethrough;
        else if (active) label = label | ftxui::bold;
        rows.push_back(ftxui::hbox({
            ftxui::text(prefix) | ftxui::color(border),
            std::move(label),
            ftxui::text(" │") | ftxui::color(border),
        }));
      }
      ++shown;
    }
    if (todos->empty()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text("No todos") | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    } else if (shown < todos->size()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text("… +" + std::to_string(todos->size() - shown) + " todos · Ctrl+O") |
              ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  }
  if (web_result) {
    if (!web_result->answer.empty()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("├── Answer ") | ftxui::color(border),
          ftxui::separator() | ftxui::flex | ftxui::color(border),
          ftxui::text("┤") | ftxui::color(border),
      }));
      std::vector<std::string> answer_lines;
      std::istringstream answer(web_result->answer);
      std::string line;
      while (std::getline(answer, line)) answer_lines.push_back(line);
      const std::size_t limit = expanded ? answer_lines.size() : 6;
      for (std::size_t index = 0; index < std::min(limit, answer_lines.size()); ++index) {
        for (const auto& wrapped : wrap_cells(answer_lines[index], content_width)) {
          rows.push_back(ftxui::hbox({
              ftxui::text("│ ") | ftxui::color(border),
              ftxui::text(wrapped) | ftxui::flex | ftxui::color(kText),
              ftxui::text(" │") | ftxui::color(border),
          }));
        }
      }
      if (!expanded && answer_lines.size() > limit) {
        rows.push_back(ftxui::hbox({
            ftxui::text("│ ") | ftxui::color(border),
            ftxui::text("… +" + std::to_string(answer_lines.size() - limit) +
                        " answer lines · Ctrl+O") | ftxui::dim | ftxui::flex,
            ftxui::text(" │") | ftxui::color(border),
        }));
      }
    }
    if (!web_result->sources.empty()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("├── Sources ") | ftxui::color(border),
          ftxui::separator() | ftxui::flex | ftxui::color(border),
          ftxui::text("┤") | ftxui::color(border),
      }));
      constexpr std::size_t kWebSources = 5;
      const std::size_t limit = expanded ? web_result->sources.size() : kWebSources;
      for (std::size_t index = 0; index < std::min(limit, web_result->sources.size()); ++index) {
        const auto& source = web_result->sources[index];
        const std::string number = std::to_string(index + 1) + ". ";
        ftxui::Element title_element =
            ftxui::hyperlink(source.url, ftxui::text(ellipsize(source.title, content_width - 4)));
        rows.push_back(ftxui::hbox({
            ftxui::text("│ " + number) | ftxui::color(kDim),
            std::move(title_element) | ftxui::flex | ftxui::color(kAccent),
            ftxui::text(" │") | ftxui::color(border),
        }));
        rows.push_back(ftxui::hbox({
            ftxui::text("│   ") | ftxui::color(border),
            ftxui::text(ellipsize(source.url, content_width - 2)) | ftxui::dim | ftxui::flex,
            ftxui::text(" │") | ftxui::color(border),
        }));
        if (!source.excerpt.empty()) {
          const std::size_t excerpt_limit = expanded ? 1000 : 2;
          std::size_t shown = 0;
          for (const auto& wrapped : wrap_cells(source.excerpt, content_width - 2)) {
            if (shown++ >= excerpt_limit) break;
            rows.push_back(ftxui::hbox({
                ftxui::text("│   ") | ftxui::color(border),
                ftxui::text(wrapped) | ftxui::dim | ftxui::flex,
                ftxui::text(" │") | ftxui::color(border),
            }));
          }
        }
      }
      if (!expanded && web_result->sources.size() > limit) {
        rows.push_back(ftxui::hbox({
            ftxui::text("│ ") | ftxui::color(border),
            ftxui::text("… +" + std::to_string(web_result->sources.size() - limit) +
                        " sources · Ctrl+O") | ftxui::dim | ftxui::flex,
            ftxui::text(" │") | ftxui::color(border),
        }));
      }
    } else if (web_result->answer.empty()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text("No results") | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
    if (!web_result->footer.empty()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text(web_result->footer) | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  }
  if (json_output) {
    std::istringstream input(*json_output);
    std::vector<std::string> lines;
    std::string line;
    while (std::getline(input, line)) lines.push_back(line);
    constexpr std::size_t kJsonLines = 12;
    const std::size_t limit = expanded ? lines.size() : kJsonLines;
    for (std::size_t index = 0; index < std::min(limit, lines.size()); ++index) {
      for (const auto& wrapped : wrap_cells(lines[index], content_width)) {
        rows.push_back(ftxui::hbox({
            ftxui::text("│ ") | ftxui::color(border),
            ftxui::text(wrapped) | ftxui::flex | ftxui::color(kText),
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
  }
  if (grep_result) {
    constexpr std::size_t kGrepMatches = 10;
    const std::size_t limit = expanded ? grep_result->total : kGrepMatches;
    std::size_t shown = 0;
    for (const auto& group : grep_result->groups) {
      if (shown >= limit) break;
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text(group.path) | ftxui::flex | ftxui::color(kText),
          ftxui::text(" │") | ftxui::color(border),
      }));
      for (const auto& match : group.lines) {
        if (shown >= limit) break;
        rows.push_back(ftxui::hbox({
            ftxui::text("│ ") | ftxui::color(border),
            ftxui::text(" " + std::to_string(match.number) + ": ") | ftxui::color(kDim),
            ftxui::text(match.text) | ftxui::flex | ftxui::color(kText),
            ftxui::text(" │") | ftxui::color(border),
        }));
        ++shown;
      }
    }
    if (grep_result->groups.empty()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text("No matches") | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
    if (!expanded && shown < grep_result->total) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text("… +" + std::to_string(grep_result->total - shown) +
                      " matches · Ctrl+O") | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
    if (!grep_result->footer.empty()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text(grep_result->footer) | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  } else if (glob_result) {
    constexpr std::size_t kGlobPaths = 12;
    struct Group {
      std::string dir;
      std::vector<std::string> names;
    };
    std::vector<Group> groups;
    for (const auto& path : glob_result->paths) {
      const auto slash = path.rfind('/');
      const std::string dir = slash == std::string::npos ? "" : path.substr(0, slash + 1);
      const std::string name = slash == std::string::npos ? path : path.substr(slash + 1);
      if (name.empty()) continue;
      auto group = std::find_if(groups.begin(), groups.end(),
                                [&](const Group& candidate) { return candidate.dir == dir; });
      if (group == groups.end()) {
        groups.push_back({dir, {}});
        group = groups.end() - 1;
      }
      group->names.push_back(name);
    }
    const std::size_t limit = expanded ? glob_result->paths.size() : kGlobPaths;
    std::size_t shown = 0;
    for (const auto& group : groups) {
      if (shown >= limit) break;
      if (!group.dir.empty()) {
        rows.push_back(ftxui::hbox({
            ftxui::text("│ ") | ftxui::color(border),
            ftxui::text(group.dir) | ftxui::dim | ftxui::flex,
            ftxui::text(" │") | ftxui::color(border),
        }));
      }
      for (const auto& name : group.names) {
        if (shown >= limit) break;
        rows.push_back(ftxui::hbox({
            ftxui::text(group.dir.empty() ? "│ " : "│   ") | ftxui::color(border),
            ftxui::text(name) | ftxui::flex | ftxui::color(kText),
            ftxui::text(" │") | ftxui::color(border),
        }));
        ++shown;
      }
    }
    if (glob_result->paths.empty()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text("No files") | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
    if (!expanded && shown < glob_result->paths.size()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text("… +" + std::to_string(glob_result->paths.size() - shown) +
                      " paths · Ctrl+O") | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
    if (!glob_result->footer.empty()) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text(glob_result->footer) | ftxui::dim | ftxui::flex,
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  } else if (!output.empty() && (!todos || entry.status == "failed") && !web_result &&
             !json_output && !patch && !web_fetch) {
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
    const auto push_read_row = [&](const std::string& row, std::size_t gutter) {
      const auto parts = split_read_row(row);
      if (parts.number.empty()) {
        push_output(row, kText, false);
        return;
      }
      const std::string pad(gutter - parts.number.size(), ' ');
      std::vector<std::string> lines =
          wrap_cells(parts.text, std::max<std::size_t>(1, content_width - gutter - 2));
      if (lines.empty()) lines.emplace_back();
      bool first = true;
      for (const auto& line : lines) {
        const std::string prefix =
            first ? "│ " + pad + parts.number + ": " : "│ " + std::string(gutter + 2, ' ');
        first = false;
        rows.push_back(ftxui::hbox({
            ftxui::text(prefix) | ftxui::color(kDim),
            ftxui::text(line) | ftxui::flex | ftxui::color(kText),
            ftxui::text(" │") | ftxui::color(border),
        }));
      }
    };
    if (has_envelope && reads.size() == 1) {
      const ReadEnvelope& read = reads[0];
      const std::size_t gutter = read_gutter(read.rows);
      const std::size_t limit = expanded ? read.rows.size() : 3;
      for (; shown < std::min(limit, read.rows.size()); ++shown) {
        push_read_row(read.rows[shown], gutter);
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
        const std::size_t gutter = read_gutter(read.rows);
        for (const auto& row : read.rows) push_read_row(row, gutter);
        if (!read.footer.empty()) push_output(read.footer, kDim, true);
      }
    } else if (shell_tool) {
      std::vector<std::string> lines;
      std::istringstream input(output);
      std::string line;
      while (std::getline(input, line)) lines.push_back(line);
      const std::size_t limit = expanded ? lines.size() : 10;
      const std::size_t start = lines.size() > limit ? lines.size() - limit : 0;
      if (start > 0) {
        push_output("… " + std::to_string(start) + " earlier lines · Ctrl+O", kDim, true);
      }
      for (std::size_t index = start; index < lines.size(); ++index) {
        push_output(lines[index], kText, false);
      }
    } else {
      std::istringstream input(output);
      std::string line;
      const std::size_t limit = !expanded && read_tool ? 3 : 12;
      while (std::getline(input, line)) {
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
  std::string exit_note = shell_status.note;
  if (exit_note.empty() && shell_tool && entry.status == "completed") exit_note = "exit 0";
  if (!exit_note.empty()) footer += " · " + exit_note;
  const bool footer_failed = entry.status == "failed" || shell_status.failed;
  ftxui::Element status = ftxui::text(footer) | ftxui::dim;
  if (footer_failed) status = ftxui::text(footer) | ftxui::color(kRed);
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
