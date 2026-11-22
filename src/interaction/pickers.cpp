#include "interaction/view.hpp"

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

SlashAction slash_menu_key(const ftxui::Event& event, AppState& state, std::size_t& index) {
  const auto matches = matching_commands(state.composer);
  if (matches.empty()) return SlashAction::None;
  if (index >= matches.size()) index = 0;
  if (event == ftxui::Event::ArrowUp) {
    index = (index + matches.size() - 1) % matches.size();
    return SlashAction::Handled;
  }
  if (event == ftxui::Event::ArrowDown) {
    index = (index + 1) % matches.size();
    return SlashAction::Handled;
  }
  if (event == ftxui::Event::Tab) {
    state.composer = std::string(matches[index]->name) + " ";
    return SlashAction::Handled;
  }
  if (event == ftxui::Event::Return) {
    state.composer = matches[index]->name;
    return SlashAction::Submit;
  }
  return SlashAction::None;
}

ftxui::Element command_menu_view(const std::string& composer, std::size_t index) {
  ftxui::Elements rows;
  const auto matches = matching_commands(composer);
  for (std::size_t row = 0; row < matches.size(); ++row) {
    const bool selected = row == index % std::max<std::size_t>(matches.size(), 1);
    ftxui::Element line = ftxui::hbox({
        ftxui::text(selected ? " › " : "   ") | ftxui::color(kAccent),
        ftxui::text(std::string(matches[row]->name)) |
            (selected ? ftxui::bold : ftxui::nothing),
        ftxui::text(std::string("  ") + matches[row]->description) | ftxui::dim,
    });
    if (selected) line = line | ftxui::bgcolor(ftxui::Color::RGB(18, 18, 18));
    rows.push_back(std::move(line));
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
