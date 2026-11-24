#include "interaction/skill_view.hpp"

#include <ftxui/dom/elements.hpp>

#include <algorithm>
#include <string>
#include <vector>

#include "interaction/text_util.hpp"
#include "interaction/theme.hpp"

namespace booth {

std::string skill_title(const SkillResult& skill) {
  return skill.name.empty() ? "Skill" : "Skill · " + skill.name;
}

ftxui::Elements skill_rows(const SkillResult& skill, bool expanded, std::size_t content_width) {
  const auto border = theme_color("border", ftxui::Color::RGB(108, 108, 108));
  const auto text = theme_color("text", ftxui::Color::RGB(228, 228, 228));
  ftxui::Elements rows;
  if (!skill.base.empty()) {
    rows.push_back(ftxui::hbox({
        ftxui::text("│ ") | ftxui::color(border),
        ftxui::text(skill.base) | ftxui::dim | ftxui::flex,
        ftxui::text(" │") | ftxui::color(border),
    }));
  }
  constexpr std::size_t kSkillLines = 12;
  const std::size_t limit =
      expanded ? skill.instructions.size() : std::min(skill.instructions.size(), kSkillLines);
  for (std::size_t index = 0; index < limit; ++index) {
    for (const auto& wrapped : wrap_cells(skill.instructions[index], content_width)) {
      rows.push_back(ftxui::hbox({
          ftxui::text("│ ") | ftxui::color(border),
          ftxui::text(wrapped) | ftxui::flex | ftxui::color(text),
          ftxui::text(" │") | ftxui::color(border),
      }));
    }
  }
  if (!expanded && skill.instructions.size() > limit) {
    rows.push_back(ftxui::hbox({
        ftxui::text("│ ") | ftxui::color(border),
        ftxui::text("… +" + std::to_string(skill.instructions.size() - limit) +
                    " lines · Ctrl+O") | ftxui::dim | ftxui::flex,
        ftxui::text(" │") | ftxui::color(border),
    }));
  }
  return rows;
}

}  // namespace booth
