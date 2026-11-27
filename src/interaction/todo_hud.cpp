#include "interaction/view.hpp"

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/terminal.hpp>

#include <algorithm>
#include <optional>
#include <string>
#include <vector>

#include "interaction/text_util.hpp"
#include "interaction/theme.hpp"

namespace booth {

std::optional<ftxui::Element> todo_hud(const std::vector<TodoItem>& todos, bool expanded) {
  if (todos.empty()) return std::nullopt;
  const std::size_t done =
      static_cast<std::size_t>(std::count_if(todos.begin(), todos.end(), [](const TodoItem& item) {
        return item.status == "completed";
      }));
  const std::size_t active =
      static_cast<std::size_t>(std::count_if(todos.begin(), todos.end(), [](const TodoItem& item) {
        return item.status == "in_progress";
      }));
  const std::size_t pending = todos.size() - done - active;
  if (active == 0 && pending == 0 && !expanded) return std::nullopt;

  const auto accent = theme_color("accent", ftxui::Color::RGB(0, 175, 255));
  const auto dim = theme_color("dim", ftxui::Color::RGB(118, 118, 118));
  const auto text = theme_color("text", ftxui::Color::RGB(228, 228, 228));
  const int columns = ftxui::Terminal::Size().dimx;
  const std::size_t width = columns > 12 ? static_cast<std::size_t>(columns - 8) : 20;

  ftxui::Elements rows;
  std::string head = "  Todo " + std::to_string(done) + "/" + std::to_string(todos.size());
  head += active > 0 ? " · " + std::to_string(active) + " active"
                     : " · " + std::to_string(pending) + " pending";
  rows.push_back(ftxui::text(head) | ftxui::color(accent) | ftxui::bold);

  const std::size_t kRows = expanded ? todos.size() : 3;
  std::size_t shown = 0;
  std::size_t open = active + pending;
  const auto add_rows = [&](const std::string& status) {
    for (const auto& item : todos) {
      if (shown >= kRows || item.status != status) continue;
      const bool current = status == "in_progress";
      const bool completed_row = status == "completed";
      rows.push_back(ftxui::hbox({
          ftxui::text(completed_row ? "  ☑ " : "  ☐ ") | ftxui::color(current ? accent : dim),
          ftxui::text(ellipsize(item.content, width - 6)) |
              ftxui::color(current ? text : dim) | (current ? ftxui::bold : ftxui::nothing),
      }));
      ++shown;
      --open;
    }
  };
  add_rows("in_progress");
  add_rows("pending");
  if (expanded) add_rows("completed");
  if (open > 0) {
    rows.push_back(ftxui::text("  … +" + std::to_string(open) + " open") | ftxui::dim);
  }
  return ftxui::vbox(std::move(rows));
}

}  // namespace booth
