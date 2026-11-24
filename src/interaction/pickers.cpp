#include "interaction/text_util.hpp"
#include "interaction/theme.hpp"
#include "interaction/view.hpp"

#include <algorithm>
#include <cctype>
#include <string>
#include <utility>
#include <vector>

#include <ftxui/dom/elements.hpp>

namespace booth {
namespace {

#define kAccent (theme_color("accent", ftxui::Color::RGB(0, 175, 255)))
#define kText (theme_color("text", ftxui::Color::RGB(228, 228, 228)))
#define kDim (theme_color("dim", ftxui::Color::RGB(118, 118, 118)))
#define kGreen (theme_color("green", ftxui::Color::RGB(0, 255, 135)))
#define kPanel (theme_color("panel", ftxui::Color::RGB(18, 18, 18)))
#define kHover (theme_color("hover", ftxui::Color::RGB(40, 40, 40)))

bool row_hovered(const MouseRouter* mouse, MouseSurface surface, std::size_t row,
                 bool selected) {
  return !selected && mouse != nullptr && mouse->hovered(surface, row);
}

void paint_row(ftxui::Element& line, bool selected, bool hovered) {
  if (selected) {
    line = line | ftxui::bgcolor(kPanel);
  } else if (hovered) {
    line = line | ftxui::bgcolor(kHover) | ftxui::underlined;
  }
}

ftxui::Element hit_row(ftxui::Element line, MouseSurface surface, std::size_t row,
                       MouseRouter* mouse) {
  if (mouse == nullptr) return line;
  return mouse->zone(std::move(line), surface, row);
}

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

SlashAction SlashMenu::click(std::size_t row, AppState& state) {
  const auto matches = matching_commands(state.composer);
  if (row >= matches.size()) return SlashAction::None;
  index_ = row;
  moved_ = true;
  ftxui::Event enter = ftxui::Event::Return;
  return key(enter, state);
}

ftxui::Element command_menu_view(const std::string& composer, std::size_t index,
                                 MouseRouter* mouse) {
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
    const bool hovered = row_hovered(mouse, MouseSurface::Slash, row, active);
    ftxui::Element line = ftxui::hbox({
        ftxui::text(active ? " › " : "   ") | ftxui::color(kAccent),
        ftxui::hbox(std::move(label)),
        ftxui::text("  " + std::string(matches[row]->description)) | ftxui::dim,
        ftxui::filler(),
    });
    paint_row(line, active, hovered);
    rows.push_back(hit_row(std::move(line), MouseSurface::Slash, row, mouse));
  }
  if (end < matches.size()) {
    rows.push_back(ftxui::text("   ↓ " + std::to_string(matches.size() - end) + " more") |
                   ftxui::dim);
  }
  return ftxui::vbox(std::move(rows));
}

void SessionPicker::open() {
  visible_ = true;
  loading_ = true;
  index_ = 0;
  query_.clear();
  matches_.clear();
}

void SessionPicker::close() { visible_ = false; }

void SessionPicker::filter(const std::vector<SessionRow>& sessions) {
  matches_.clear();
  std::string needle = query_;
  for (auto& ch : needle) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  for (std::size_t row = 0; row < sessions.size(); ++row) {
    const auto& session = sessions[row];
    const std::string searchable = session.title + " " + session.id.value + " " + session.cwd;
    std::string lowered = searchable;
    for (auto& ch : lowered) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
    if (needle.empty() || lowered.find(needle) != std::string::npos) matches_.push_back(row);
  }
  if (index_ >= matches_.size()) index_ = 0;
}

void SessionPicker::loaded(const std::vector<SessionRow>& sessions) {
  loading_ = false;
  filter(sessions);
}

SessionPicker::Action SessionPicker::key(ftxui::Event& event,
                                         const std::vector<SessionRow>& sessions) {
  if (!visible_) return Action::None;
  if (event == ftxui::Event::Escape) {
    if (!query_.empty()) {
      query_.clear();
      filter(sessions);
    } else {
      close();
    }
  } else if (event == ftxui::Event::Backspace) {
    if (!query_.empty()) {
      std::size_t at = query_.size() - 1;
      while (at > 0 && (static_cast<unsigned char>(query_[at]) & 0xC0) == 0x80) --at;
      query_.erase(at);
      filter(sessions);
    }
  } else if (event == ftxui::Event::ArrowUp && !matches_.empty()) {
    index_ = (index_ + matches_.size() - 1) % matches_.size();
  } else if (event == ftxui::Event::ArrowDown && !matches_.empty()) {
    index_ = (index_ + 1) % matches_.size();
  } else if (event.is_mouse() && !matches_.empty() &&
             event.mouse().button == ftxui::Mouse::WheelUp) {
    index_ = (index_ + matches_.size() - 1) % matches_.size();
  } else if (event.is_mouse() && !matches_.empty() &&
             event.mouse().button == ftxui::Mouse::WheelDown) {
    index_ = (index_ + 1) % matches_.size();
  } else if (event == ftxui::Event::Character('r') && query_.empty()) {
    loading_ = true;
    return Action::Refresh;
  } else if (event == ftxui::Event::Character('c') && query_.empty()) {
    close();
    return Action::Close;
  } else if (event == ftxui::Event::Return && !loading_ && !matches_.empty()) {
    close();
    return Action::Resume;
  } else if (event.is_character()) {
    query_ += event.input();
    filter(sessions);
  }
  return Action::None;
}

SessionPicker::Action SessionPicker::click(std::size_t row,
                                           const std::vector<SessionRow>& /*sessions*/) {
  if (!visible_ || loading_ || row >= matches_.size()) return Action::None;
  index_ = row;
  close();
  return Action::Resume;
}

ftxui::Element file_menu_view(const std::string& query,
                              const std::vector<std::string>& matches, std::size_t index,
                              MouseRouter* mouse) {
  ftxui::Elements rows;
  rows.push_back(ftxui::hbox({ftxui::text(" files ") | ftxui::bold | ftxui::color(kAccent),
                              ftxui::text("↑/↓ · Tab/Enter · Esc") | ftxui::dim}));
  if (!query.empty()) rows.push_back(ftxui::text(" @" + query) | ftxui::color(kText));
  constexpr std::size_t kRows = 10;
  std::size_t start = index >= kRows / 2 ? index - kRows / 2 : 0;
  if (start + kRows > matches.size()) start = matches.size() > kRows ? matches.size() - kRows : 0;
  const std::size_t end = std::min(start + kRows, matches.size());
  if (start > 0) rows.push_back(ftxui::text("   ↑ " + std::to_string(start) + " more") | ftxui::dim);
  for (std::size_t row = start; row < end; ++row) {
    const bool selected = row == index;
    const bool hovered = row_hovered(mouse, MouseSurface::File, row, selected);
    ftxui::Element line = ftxui::hbox({
        ftxui::text(selected ? " › " : "   ") | ftxui::color(kAccent),
        ftxui::text(matches[row]) | (selected ? ftxui::bold : ftxui::nothing),
        ftxui::filler(),
    });
    paint_row(line, selected, hovered);
    rows.push_back(hit_row(std::move(line), MouseSurface::File, row, mouse));
  }
  if (end < matches.size()) {
    rows.push_back(ftxui::text("   ↓ " + std::to_string(matches.size() - end) + " more") |
                   ftxui::dim);
  }
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 62) |
         ftxui::borderRounded | ftxui::hcenter | ftxui::bgcolor(kPanel);
}

ftxui::Element copy_picker_view(const std::vector<CopyItem>& items, std::size_t index,
                                MouseRouter* mouse) {
  ftxui::Elements rows;
  rows.push_back(ftxui::hbox({ftxui::text(" copy ") | ftxui::bold | ftxui::color(kAccent),
                              ftxui::text("↑/↓ · Enter · Esc") | ftxui::dim}));
  constexpr std::size_t kRows = 10;
  std::size_t start = index >= kRows / 2 ? index - kRows / 2 : 0;
  if (start + kRows > items.size()) start = items.size() > kRows ? items.size() - kRows : 0;
  const std::size_t end = std::min(start + kRows, items.size());
  if (start > 0) rows.push_back(ftxui::text("   ↑ " + std::to_string(start) + " more") | ftxui::dim);
  for (std::size_t row = start; row < end; ++row) {
    const bool selected = row == index;
    const bool hovered = row_hovered(mouse, MouseSurface::Copy, row, selected);
    ftxui::Element line = ftxui::hbox({
        ftxui::text(selected ? " › " : "   ") | ftxui::color(kAccent),
        ftxui::text(items[row].label) | (selected ? ftxui::bold : ftxui::nothing),
        ftxui::filler(),
    });
    paint_row(line, selected, hovered);
    rows.push_back(hit_row(std::move(line), MouseSurface::Copy, row, mouse));
  }
  if (end < items.size()) {
    rows.push_back(ftxui::text("   ↓ " + std::to_string(items.size() - end) + " more") |
                   ftxui::dim);
  }
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 62) |
         ftxui::borderRounded | ftxui::hcenter | ftxui::bgcolor(kPanel);
}

ftxui::Element history_search_view(const std::string& query,
                                   const std::vector<std::string>& matches,
                                   std::size_t index, MouseRouter* mouse) {
  ftxui::Elements rows;
  rows.push_back(ftxui::hbox({ftxui::text(" history ") | ftxui::bold | ftxui::color(kAccent),
                              ftxui::text("↑/↓ · Enter · Esc") | ftxui::dim}));
  rows.push_back(ftxui::text(" /" + query + (matches.empty() ? "  no matches" : "")) |
                 ftxui::color(matches.empty() ? kDim : kText));
  if (!matches.empty()) {
    constexpr std::size_t kRows = 8;
    std::size_t start = index >= kRows / 2 ? index - kRows / 2 : 0;
    if (start + kRows > matches.size()) start = matches.size() > kRows ? matches.size() - kRows : 0;
    const std::size_t end = std::min(start + kRows, matches.size());
    if (start > 0) rows.push_back(ftxui::text("   ↑ " + std::to_string(start) + " more") | ftxui::dim);
    for (std::size_t row = start; row < end; ++row) {
      const bool selected = row == index;
      const bool hovered = row_hovered(mouse, MouseSurface::History, row, selected);
      std::string label = matches[row].substr(0, matches[row].find('\n'));
      label = ellipsize(label, 52);
      ftxui::Element line = ftxui::hbox({
          ftxui::text(selected ? " › " : "   ") | ftxui::color(kAccent),
          ftxui::text(label) | (selected ? ftxui::bold : ftxui::nothing),
          ftxui::filler(),
      });
      paint_row(line, selected, hovered);
      rows.push_back(hit_row(std::move(line), MouseSurface::History, row, mouse));
    }
    if (end < matches.size()) {
      rows.push_back(ftxui::text("   ↓ " + std::to_string(matches.size() - end) + " more") |
                     ftxui::dim);
    }
  }
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 62) |
         ftxui::borderRounded | ftxui::hcenter | ftxui::bgcolor(kPanel);
}

ftxui::Element model_picker_view(const std::vector<ModelChoice>& choices,
                                 const std::vector<std::size_t>& matches, std::size_t index,
                                 const std::string& current, const std::string& query,
                                 MouseRouter* mouse) {
  ftxui::Elements rows;
  rows.push_back(ftxui::hbox({ftxui::text(" models ") | ftxui::bold | ftxui::color(kAccent),
                              ftxui::text("type to filter · ↑/↓ · Enter · Esc") | ftxui::dim}));
  if (!query.empty()) rows.push_back(ftxui::text(" /" + query) | ftxui::color(kText));
  if (matches.empty()) {
    rows.push_back(ftxui::text(" no matches") | ftxui::dim);
  } else {
    constexpr std::size_t kRows = 10;
    std::size_t start = index >= kRows / 2 ? index - kRows / 2 : 0;
    if (start + kRows > matches.size()) start = matches.size() > kRows ? matches.size() - kRows : 0;
    const std::size_t end = std::min(start + kRows, matches.size());
    if (start > 0) rows.push_back(ftxui::text("   ↑ " + std::to_string(start) + " more") | ftxui::dim);
    std::string provider;
    for (std::size_t row = start; row < end; ++row) {
      const auto& choice = choices[matches[row]];
      if (choice.provider != provider) {
        provider = choice.provider;
        if (!provider.empty()) rows.push_back(ftxui::text("   " + provider) | ftxui::dim);
      }
      const bool selected = row == index;
      const bool active = choice.name == current;
      const bool hovered = row_hovered(mouse, MouseSurface::Model, row, selected);
      ftxui::Element line = ftxui::hbox({
          ftxui::text(selected ? " › " : "   ") | ftxui::color(kAccent),
          ftxui::text(active ? "● " : "  ") | ftxui::color(kGreen),
          ftxui::text((provider.empty() ? "" : "  ") + choice.name) |
              (selected ? ftxui::bold : ftxui::nothing),
          ftxui::filler(),
      });
      paint_row(line, selected, hovered);
      rows.push_back(hit_row(std::move(line), MouseSurface::Model, row, mouse));
    }
    if (end < matches.size()) {
      rows.push_back(ftxui::text("   ↓ " + std::to_string(matches.size() - end) + " more") |
                     ftxui::dim);
    }
  }
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 44) |
         ftxui::borderRounded | ftxui::hcenter | ftxui::bgcolor(kPanel);
}

ftxui::Element session_picker_view(const std::vector<SessionRow>& sessions,
                                   const std::vector<std::size_t>& matches, std::size_t index,
                                   bool loading, const std::string& query, MouseRouter* mouse) {
  ftxui::Elements rows;
  rows.push_back(ftxui::hbox({ftxui::text(" sessions ") | ftxui::bold | ftxui::color(kAccent),
                              ftxui::text("type to filter · ↑/↓ · Enter · r refresh · c close · Esc") |
                                  ftxui::dim}));
  if (!query.empty()) rows.push_back(ftxui::text(" /" + query) | ftxui::color(kText));
  if (matches.empty()) {
    rows.push_back(ftxui::text(loading ? " loading…" : " no saved sessions") | ftxui::dim);
  } else {
    constexpr std::size_t kWindow = 8;
    std::size_t start = index >= kWindow ? index - kWindow / 2 : 0;
    if (start + kWindow > matches.size() && matches.size() > kWindow) {
      start = matches.size() - kWindow;
    }
    const std::size_t end = std::min(start + kWindow, matches.size());
    if (start > 0) {
      rows.push_back(ftxui::text("   ↑ " + std::to_string(start) + " more") | ftxui::dim);
    }
    for (std::size_t row = start; row < end; ++row) {
      const auto& session = sessions[matches[row]];
      const bool selected = row == index;
      const bool hovered = row_hovered(mouse, MouseSurface::Session, row, selected);
      std::string title = session.title.empty() ? session.id.value : session.title;
      title = ellipsize(title, 40);
      ftxui::Element line = ftxui::hbox({
          ftxui::text(selected ? " › " : "   ") | ftxui::color(kAccent),
          ftxui::text(title) | (selected ? ftxui::bold : ftxui::nothing),
          ftxui::text("  " + ellipsize(session.cwd, 16)) | ftxui::dim,
          ftxui::filler(),
      });
      paint_row(line, selected, hovered);
      rows.push_back(hit_row(std::move(line), MouseSurface::Session, row, mouse));
    }
    if (end < matches.size()) {
      rows.push_back(ftxui::text("   ↓ " + std::to_string(matches.size() - end) + " more") |
                     ftxui::dim);
    }
    if (loading) rows.push_back(ftxui::text(" refreshing…") | ftxui::dim);
  }
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 62) |
         ftxui::borderRounded | ftxui::hcenter | ftxui::bgcolor(kPanel);
}

}  // namespace booth
