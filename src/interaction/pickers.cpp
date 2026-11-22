#include "interaction/view.hpp"

#include <algorithm>
#include <cctype>
#include <string>
#include <utility>
#include <vector>

#include <ftxui/dom/elements.hpp>

namespace booth {
namespace {

const ftxui::Color kAccent = ftxui::Color::RGB(0, 175, 255);
const ftxui::Color kText = ftxui::Color::RGB(228, 228, 228);
const ftxui::Color kDim = ftxui::Color::RGB(118, 118, 118);
const ftxui::Color kGreen = ftxui::Color::RGB(0, 255, 135);
const ftxui::Color kPanel = ftxui::Color::RGB(18, 18, 18);

}  // namespace

void SlashMenu::sync(const std::string& composer) {
  if (composer == seen_) return;
  seen_ = composer;
  hidden_ = false;
  moved_ = false;
  index_ = 0;
}

bool SlashMenu::visible(const std::string& composer) const {
  return !hidden_ && !matching_commands(composer).empty();
}

SlashAction SlashMenu::key(ftxui::Event& event, AppState& state) {
  const auto matches = matching_commands(state.composer);
  if (matches.empty()) return SlashAction::None;
  if (index_ >= matches.size()) index_ = 0;
  constexpr std::size_t kPage = 10;

  if (event == ftxui::Event::Escape) {
    hidden_ = true;
    seen_ = state.composer;
    return SlashAction::Handled;
  }
  if (event == ftxui::Event::ArrowUp) {
    index_ = (index_ + matches.size() - 1) % matches.size();
    moved_ = true;
    return SlashAction::Handled;
  }
  if (event == ftxui::Event::ArrowDown) {
    index_ = (index_ + 1) % matches.size();
    moved_ = true;
    return SlashAction::Handled;
  }
  if (event == ftxui::Event::PageUp) {
    index_ = index_ >= kPage ? index_ - kPage : 0;
    moved_ = true;
    return SlashAction::Handled;
  }
  if (event == ftxui::Event::PageDown) {
    index_ = std::min(index_ + kPage, matches.size() - 1);
    moved_ = true;
    return SlashAction::Handled;
  }
  if (event == ftxui::Event::Home) {
    index_ = 0;
    moved_ = true;
    return SlashAction::Handled;
  }
  if (event == ftxui::Event::End) {
    index_ = matches.size() - 1;
    moved_ = true;
    return SlashAction::Handled;
  }
  if (event.is_mouse()) {
    const auto button = event.mouse().button;
    if (button == ftxui::Mouse::WheelUp) {
      index_ = (index_ + matches.size() - 1) % matches.size();
      moved_ = true;
      return SlashAction::Handled;
    }
    if (button == ftxui::Mouse::WheelDown) {
      index_ = (index_ + 1) % matches.size();
      moved_ = true;
      return SlashAction::Handled;
    }
  }
  if (event == ftxui::Event::Tab) {
    state.composer = std::string(matches[index_]->name) + " ";
    return SlashAction::Handled;
  }
  if (event == ftxui::Event::Return) {
    if (!moved_) {
      const auto query = slash_query(state.composer).value_or("");
      if (!command_name_matches(*matches[index_], query)) return SlashAction::None;
    }
    state.composer = matches[index_]->name;
    return SlashAction::Submit;
  }
  return SlashAction::None;
}

ftxui::Element command_menu_view(const std::string& composer, std::size_t index) {
  ftxui::Elements rows;
  const auto matches = matching_commands(composer);
  if (matches.empty()) return ftxui::text("");
  const std::string query = slash_query(composer).value_or("");
  std::size_t name_width = 0;
  for (const auto* match : matches) name_width = std::max(name_width, std::string(match->name).size());

  constexpr std::size_t kRows = 10;
  std::size_t start = 0;
  if (matches.size() > kRows) {
    start = index >= kRows / 2 ? index - kRows / 2 : 0;
    start = std::min(start, matches.size() - kRows);
  }
  const std::size_t end = std::min(start + kRows, matches.size());
  const std::size_t selected = std::min(index, matches.size() - 1);

  if (start > 0) {
    rows.push_back(ftxui::text("   ↑ " + std::to_string(start) + " more") | ftxui::dim);
  }
  for (std::size_t row = start; row < end; ++row) {
    const std::string name = matches[row]->name;
    ftxui::Elements label;
    label.push_back(ftxui::text("/") | ftxui::color(kDim));
    std::string run;
    bool run_hit = false;
    std::size_t matched = 0;
    const auto flush = [&] {
      if (run.empty()) return;
      label.push_back(ftxui::text(run) |
                      (run_hit ? (ftxui::bold | ftxui::color(kAccent))
                               : ftxui::color(kText)));
      run.clear();
    };
    for (std::size_t at = 1; at < name.size(); ++at) {
      const bool hit = matched < query.size() &&
                       static_cast<char>(std::tolower(static_cast<unsigned char>(name[at]))) ==
                           query[matched];
      if (hit) ++matched;
      if (!run.empty() && hit != run_hit) flush();
      run_hit = hit;
      run += name[at];
    }
    flush();
    label.push_back(ftxui::text(std::string(name_width - name.size(), ' ')));
    const bool active = row == selected;
    ftxui::Element line = ftxui::hbox({
        ftxui::text(active ? " › " : "   ") | ftxui::color(kAccent),
        ftxui::hbox(std::move(label)),
        ftxui::text("  " + std::string(matches[row]->description)) | ftxui::dim,
        ftxui::filler(),
    });
    if (active) line = line | ftxui::bgcolor(kPanel);
    rows.push_back(std::move(line));
  }
  if (end < matches.size()) {
    rows.push_back(ftxui::text("   ↓ " + std::to_string(matches.size() - end) + " more") |
                   ftxui::dim);
  }
  return ftxui::vbox(std::move(rows));
}

ftxui::Element model_picker_view(const std::vector<ModelChoice>& choices, std::size_t index,
                                 const std::string& current) {
  ftxui::Elements rows;
  rows.push_back(ftxui::hbox({ftxui::text(" models ") | ftxui::bold | ftxui::color(kAccent),
                              ftxui::text("↑/↓/wheel · Enter · Esc") | ftxui::dim}));
  std::string provider;
  for (std::size_t row = 0; row < choices.size(); ++row) {
    if (choices[row].provider != provider) {
      provider = choices[row].provider;
      if (!provider.empty()) {
        rows.push_back(ftxui::text("   " + provider) | ftxui::dim);
      }
    }
    const bool selected = row == index;
    const bool active = choices[row].name == current;
    ftxui::Element line = ftxui::hbox({
        ftxui::text(selected ? " › " : "   ") | ftxui::color(kAccent),
        ftxui::text(active ? "● " : "  ") | ftxui::color(kGreen),
        ftxui::text((provider.empty() ? "" : "  ") + choices[row].name) |
            (selected ? ftxui::bold : ftxui::nothing),
    });
    if (selected) line = line | ftxui::bgcolor(ftxui::Color::RGB(18, 18, 18));
    rows.push_back(std::move(line));
  }
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 44) |
         ftxui::borderRounded | ftxui::hcenter | ftxui::bgcolor(kPanel);
}

ftxui::Element session_picker_view(const std::vector<SessionRow>& sessions, std::size_t index,
                                   bool loading) {
  ftxui::Elements rows;
  rows.push_back(ftxui::hbox({ftxui::text(" sessions ") | ftxui::bold | ftxui::color(kAccent),
                              ftxui::text("↑/↓/wheel · Enter · r refresh · c close · Esc") |
                                  ftxui::dim}));
  if (sessions.empty()) {
    rows.push_back(ftxui::text(loading ? " loading…" : " no saved sessions") | ftxui::dim);
  } else {
    constexpr std::size_t kWindow = 8;
    std::size_t start = index >= kWindow ? index - kWindow / 2 : 0;
    if (start + kWindow > sessions.size() && sessions.size() > kWindow) {
      start = sessions.size() - kWindow;
    }
    const std::size_t end = std::min(start + kWindow, sessions.size());
    if (start > 0) {
      rows.push_back(ftxui::text("   ↑ " + std::to_string(start) + " more") | ftxui::dim);
    }
    for (std::size_t row = start; row < end; ++row) {
      const bool selected = row == index;
      std::string title = sessions[row].title.empty() ? sessions[row].id.value
                                                     : sessions[row].title;
      if (title.size() > 40) title = title.substr(0, 39) + "…";
      ftxui::Element line = ftxui::hbox({
          ftxui::text(selected ? " › " : "   ") | ftxui::color(kAccent),
          ftxui::text(title) | (selected ? ftxui::bold : ftxui::nothing),
          ftxui::text("  " + sessions[row].cwd) | ftxui::dim,
      });
      if (selected) line = line | ftxui::bgcolor(ftxui::Color::RGB(18, 18, 18));
      rows.push_back(std::move(line));
    }
    if (end < sessions.size()) {
      rows.push_back(ftxui::text("   ↓ " + std::to_string(sessions.size() - end) + " more") |
                     ftxui::dim);
    }
    if (loading) rows.push_back(ftxui::text(" refreshing…") | ftxui::dim);
  }
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 62) |
         ftxui::borderRounded | ftxui::hcenter | ftxui::bgcolor(kPanel);
}

}  // namespace booth
