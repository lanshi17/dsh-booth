#include "interaction/patch_view.hpp"

#include <ftxui/dom/elements.hpp>

#include <algorithm>
#include <string>
#include <vector>

#include "interaction/text_util.hpp"
#include "interaction/theme.hpp"

namespace booth {
namespace {

std::size_t total_added(const std::vector<PatchFile>& patch) {
  std::size_t added = 0;
  for (const auto& file : patch) added += file.added;
  return added;
}

std::size_t total_removed(const std::vector<PatchFile>& patch) {
  std::size_t removed = 0;
  for (const auto& file : patch) removed += file.removed;
  return removed;
}

}  // namespace

std::string patch_title(const std::vector<PatchFile>& patch) {
  std::string title = "Patch · " + std::to_string(patch.size()) +
                      (patch.size() == 1 ? " file" : " files");
  const std::size_t added = total_added(patch);
  const std::size_t removed = total_removed(patch);
  if (added > 0 || removed > 0) {
    title += " · +" + std::to_string(added) + " −" + std::to_string(removed);
  }
  return title;
}

ftxui::Elements patch_rows(const std::vector<PatchFile>& patch, bool expanded,
                           std::size_t content_width) {
  const auto border = theme_color("border", ftxui::Color::RGB(108, 108, 108));
  const auto accent = theme_color("accent", ftxui::Color::RGB(0, 175, 255));
  const auto green = theme_color("green", ftxui::Color::RGB(0, 255, 135));
  const auto red = theme_color("red", ftxui::Color::RGB(255, 95, 95));
  const auto dim = theme_color("dim", ftxui::Color::RGB(118, 118, 118));
  constexpr std::size_t kPatchLines = 40;
  std::size_t total_lines = 0;
  for (const auto& file : patch) total_lines += file.lines.size();
  const std::size_t limit = expanded ? total_lines : std::min(total_lines, kPatchLines);
  std::size_t shown = 0;
  ftxui::Elements rows;
  for (const auto& file : patch) {
    const char op = file.op == PatchOp::Add ? 'A' : file.op == PatchOp::Delete ? 'D' : 'M';
    const ftxui::Color op_color =
        file.op == PatchOp::Add ? green : file.op == PatchOp::Delete ? red : accent;
    std::string head = std::string(1, op) + " " + file.path;
    if (!file.destination.empty()) head += " -> " + file.destination;
    if (file.added > 0 || file.removed > 0) {
      head += " · +" + std::to_string(file.added) + " −" + std::to_string(file.removed);
    }
    rows.push_back(ftxui::hbox({
        ftxui::text("│ ") | ftxui::color(border),
        ftxui::text(head) | ftxui::flex | ftxui::color(op_color) | ftxui::bold,
        ftxui::text(" │") | ftxui::color(border),
    }));
    for (const auto& line : file.lines) {
      if (shown >= limit) break;
      const ftxui::Color color = line.change < 0 ? red : line.change > 0 ? green : dim;
      for (const auto& wrapped : wrap_cells(line.text, content_width)) {
        rows.push_back(ftxui::hbox({
            ftxui::text("│ ") | ftxui::color(border),
            ftxui::text(wrapped) | ftxui::flex | ftxui::color(color),
            ftxui::text(" │") | ftxui::color(border),
        }));
      }
      ++shown;
    }
  }
  if (!expanded && shown < total_lines) {
    rows.push_back(ftxui::hbox({
        ftxui::text("│ ") | ftxui::color(border),
        ftxui::text("… +" + std::to_string(total_lines - shown) +
                    " lines · Ctrl+O") | ftxui::dim | ftxui::flex,
        ftxui::text(" │") | ftxui::color(border),
    }));
  }
  return rows;
}

}  // namespace booth
