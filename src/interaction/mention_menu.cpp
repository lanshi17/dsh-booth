#include "interaction/mention_menu.hpp"

#include <algorithm>

#include "interaction/mention.hpp"
#include "interaction/theme.hpp"

namespace booth {

void MentionMenu::update(const std::vector<std::string>& lines, std::size_t line,
                         std::size_t column, const std::vector<ModelChoice>& models,
                         const std::vector<std::string>& efforts) {
  std::string joined;
  for (std::size_t row = 0; row < lines.size(); ++row) {
    if (row > 0) joined.push_back('\n');
    joined += lines[row];
  }
  std::size_t cursor = column;
  for (std::size_t row = 0; row < line && row < lines.size(); ++row) {
    cursor += lines[row].size() + 1;
  }
  const auto query = active_mention(joined, cursor);
  if (!query) {
    close();
    return;
  }
  auto matches = mention_suggestions(query->prefix, models, efforts);
  if (matches.empty()) {
    close();
    return;
  }
  if (!visible_ || query_.start != query->start || query_.prefix != query->prefix) index_ = 0;
  visible_ = true;
  query_ = *query;
  matches_ = std::move(matches);
}

void MentionMenu::close() {
  visible_ = false;
  index_ = 0;
  query_ = {};
  matches_.clear();
}

std::optional<MentionMenu::Result> MentionMenu::key(ftxui::Event& event,
                                                    const std::string& content) {
  if (!visible_ || matches_.empty()) return std::nullopt;
  if (event == ftxui::Event::Escape) {
    close();
    return std::nullopt;
  }
  if (event == ftxui::Event::ArrowUp) {
    index_ = (index_ + matches_.size() - 1) % matches_.size();
    return std::nullopt;
  }
  if (event == ftxui::Event::ArrowDown) {
    index_ = (index_ + 1) % matches_.size();
    return std::nullopt;
  }
  if (event == ftxui::Event::Tab || event == ftxui::Event::Return) {
    auto [text, cursor] = apply_mention(content, query_, matches_[index_].selector);
    close();
    return Result{std::move(text), cursor};
  }
  return std::nullopt;
}

std::optional<MentionMenu::Result> MentionMenu::click(std::size_t row, const std::string& content) {
  if (!visible_ || row >= matches_.size()) return std::nullopt;
  index_ = row;
  ftxui::Event accept = ftxui::Event::Return;
  return key(accept, content);
}

ftxui::Element MentionMenu::view(MouseRouter* mouse) const {
  ftxui::Elements rows;
  const ftxui::Color accent = theme_color("accent", ftxui::Color::RGB(0, 175, 255));
  for (std::size_t row = 0; row < matches_.size(); ++row) {
    std::string line = "  ^" + matches_[row].selector + "  " + matches_[row].name;
    auto element = ftxui::text(line);
    if (row == index_) element = element | ftxui::color(accent) | ftxui::bold;
    else if (mouse != nullptr && mouse->hovered(MouseSurface::Mention, row))
      element = element | ftxui::bgcolor(theme_color("hover", ftxui::Color::RGB(40, 40, 40)));
    else element = element | ftxui::dim;
    if (mouse != nullptr) element = mouse->zone(std::move(element), MouseSurface::Mention, row);
    rows.push_back(std::move(element));
  }
  return ftxui::vbox(std::move(rows));
}

}  // namespace booth
