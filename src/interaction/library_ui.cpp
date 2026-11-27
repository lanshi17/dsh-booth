#include "interaction/library_ui.hpp"

#include <algorithm>
#include <cctype>
#include <utility>

#include "interaction/commands.hpp"
#include "interaction/editor.hpp"
#include "interaction/model_picker.hpp"
#include "interaction/mouse_mode.hpp"
#include "interaction/notify.hpp"
#include "interaction/skill_catalog.hpp"
#include "interaction/theme.hpp"
#include "interaction/view.hpp"

namespace booth {
namespace {

#define kAccent (theme_color("accent", ftxui::Color::RGB(0, 175, 255)))
#define kText (theme_color("text", ftxui::Color::RGB(228, 228, 228)))
#define kDim (theme_color("dim", ftxui::Color::RGB(118, 118, 118)))
#define kPanel (theme_color("panel", ftxui::Color::RGB(18, 18, 18)))
#define kHover (theme_color("hover", ftxui::Color::RGB(40, 40, 40)))

std::string lower(std::string_view text) {
  std::string out(text);
  for (char& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}


std::string first_word(const std::string& line) {
  const auto first = line.find_first_not_of(" \t");
  if (first == std::string::npos) return {};
  const auto end = line.find_first_of(" \t", first);
  return line.substr(first, end == std::string::npos ? std::string::npos : end - first);
}

void erase_last_glyph(std::string& text) {
  if (text.empty()) return;
  std::size_t at = text.size() - 1;
  while (at > 0 && (static_cast<unsigned char>(text[at]) & 0xC0) == 0x80) --at;
  text.erase(at);
}

std::string ellipsize(std::string text, std::size_t width) {
  if (text.size() <= width) return text;
  if (width <= 1) return "…";
  text.resize(width - 1);
  text += "…";
  return text;
}

std::vector<LibraryRow> settings_rows(const LibrarySettings& settings, const Editor& editor) {
  std::vector<LibraryRow> rows;
  rows.push_back({"thinking", "thinking", settings.show_thoughts != nullptr && *settings.show_thoughts
                                                    ? "shown"
                                                    : "hidden"});
  rows.push_back({"tools", "tool cards", settings.tools_hidden != nullptr && *settings.tools_hidden
                                               ? "hidden"
                                               : "shown"});
  rows.push_back({"expanded", "tool output", settings.tools_expanded != nullptr && *settings.tools_expanded
                                                    ? "expanded"
                                                    : "collapsed"});
  rows.push_back({"notifications", "notifications",
                  notifications_enabled(settings.state_dir) ? "on" : "off"});
  rows.push_back({"mouse", "mouse capture", settings.mouse_enabled != nullptr && *settings.mouse_enabled
                                                  ? "on"
                                                  : "off"});
  rows.push_back({"vim", "Vim mode", editor.VimEnabled() ? "on" : "off"});
  return rows;
}

}  // namespace

void LibraryOverlay::open(LibraryKind kind, std::string title, std::vector<LibraryRow> rows) {
  kind_ = kind;
  title_ = std::move(title);
  rows_ = std::move(rows);
  query_.clear();
  index_ = 0;
  visible_ = true;
  filter();
}

void LibraryOverlay::close() { visible_ = false; }

void LibraryOverlay::filter() {
  const std::string needle = lower(query_);
  std::vector<std::pair<int, std::size_t>> ranked;
  for (std::size_t row = 0; row < rows_.size(); ++row) {
    const auto& item = rows_[row];
    const std::string label = lower(item.label);
    const std::string value = lower(item.value);
    const std::string detail = lower(item.detail);
    int score = 0;
    if (needle.empty()) score = 10;
    else if (value == needle || label == needle) score = 100;
    else if (value.rfind(needle, 0) == 0 || label.rfind(needle, 0) == 0) score = 80;
    else if (label.find(needle) != std::string::npos) score = 60;
    else if (value.find(needle) != std::string::npos) score = 50;
    else if (detail.find(needle) != std::string::npos) score = 20;
    if (score > 0) ranked.push_back({score, row});
  }
  std::stable_sort(ranked.begin(), ranked.end(),
                   [](const auto& left, const auto& right) { return left.first > right.first; });
  matches_.clear();
  for (const auto& [score, row] : ranked) {
    (void)score;
    matches_.push_back(row);
  }
  if (index_ >= matches_.size()) index_ = 0;
}

std::optional<std::string> LibraryOverlay::key(ftxui::Event& event) {
  if (!visible_) return std::nullopt;
  if (event == ftxui::Event::Escape) {
    close();
    return std::nullopt;
  }
  if (event == ftxui::Event::Backspace) {
    erase_last_glyph(query_);
    filter();
    return std::nullopt;
  }
  if (event == ftxui::Event::ArrowUp ||
      (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelUp)) {
    if (!matches_.empty()) index_ = (index_ + matches_.size() - 1) % matches_.size();
    return std::nullopt;
  }
  if (event == ftxui::Event::ArrowDown ||
      (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelDown)) {
    if (!matches_.empty()) index_ = (index_ + 1) % matches_.size();
    return std::nullopt;
  }
  if (event == ftxui::Event::Home) { index_ = 0; return std::nullopt; }
  if (event == ftxui::Event::End) {
    if (!matches_.empty()) index_ = matches_.size() - 1;
    return std::nullopt;
  }
  if (event == ftxui::Event::PageUp) { index_ = index_ >= 10 ? index_ - 10 : 0; return std::nullopt; }
  if (event == ftxui::Event::PageDown) {
    if (!matches_.empty()) index_ = std::min(index_ + 10, matches_.size() - 1);
    return std::nullopt;
  }
  if (event == ftxui::Event::Return) {
    if (matches_.empty()) return std::nullopt;
    return rows_[matches_[index_]].value;
  }
  if (event.is_character()) {
    query_ += event.input();
    filter();
  }
  return std::nullopt;
}

ftxui::Element LibraryOverlay::view() const {
  ftxui::Elements rows;
  rows.push_back(ftxui::hbox({
      ftxui::text(" " + title_ + " ") | ftxui::bold | ftxui::color(kAccent),
      ftxui::text("type to filter · ↑/↓ · Enter · Esc") | ftxui::dim,
  }));
  if (!query_.empty()) rows.push_back(ftxui::text(" /" + query_) | ftxui::color(kText));
  if (matches_.empty()) {
    rows.push_back(ftxui::text(" no matches") | ftxui::dim);
  } else {
    constexpr std::size_t kRows = 14;
    const std::size_t start = index_ >= kRows / 2 ? index_ - kRows / 2 : 0;
    const std::size_t end = std::min(start + kRows, matches_.size());
    for (std::size_t row = start; row < end; ++row) {
      const auto& item = rows_[matches_[row]];
      const bool active = row == index_;
      ftxui::Element line = ftxui::hbox({
          ftxui::text(active ? " › " : "   ") | ftxui::color(kAccent),
          ftxui::text(ellipsize(item.label, 24)) | (active ? ftxui::bold : ftxui::nothing),
          ftxui::text("  " + ellipsize(item.detail, 44)) | ftxui::dim,
          ftxui::filler(),
      });
      if (active) line = line | ftxui::bgcolor(kHover);
      rows.push_back(std::move(line));
    }
    if (end < matches_.size()) {
      rows.push_back(ftxui::text("   ↓ " + std::to_string(matches_.size() - end) + " more") | ftxui::dim);
    }
  }
  rows.push_back(ftxui::text(kind_ == LibraryKind::Skills
                                 ? " Enter inserts /name · Esc closes"
                                 : " Enter toggles · Esc closes")
                     | ftxui::dim);
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 64) |
         ftxui::borderRounded | ftxui::center | ftxui::bgcolor(kPanel);
}

bool handle_library_command(const std::string& line, AppState& state, LibraryOverlay& overlay,
                            const std::filesystem::path& cwd, const LibrarySettings& settings,
                            const Editor& editor) {
  const std::string command = first_word(normalize_command(line));
  if (command == "/skills") {
    auto skills = load_skill_commands(cwd);
    const auto project_first = [](const SkillInfo& skill) {
      return skill.source.rfind("project", 0) == 0;
    };
    std::sort(skills.begin(), skills.end(), [&](const SkillInfo& left, const SkillInfo& right) {
      if (project_first(left) != project_first(right)) return project_first(left);
      return left.name < right.name;
    });
    const auto rest = line.size() > command.size() ? line.substr(command.size()) : std::string{};
    const auto first = rest.find_first_not_of(" \t");
    const auto second = first == std::string::npos ? std::string::npos : rest.find_first_of(" \t", first);
    const std::string verb = first == std::string::npos ? std::string{} : lower(rest.substr(first, second - first));
    const auto argument_start =
        second == std::string::npos ? std::string::npos : rest.find_first_not_of(" \t", second);
    const std::string argument =
        argument_start == std::string::npos ? std::string{} : rest.substr(argument_start);
    if (verb == "help") {
      state.notice =
          "Usage: /skills [search <query>|installed|install <name>|update]  (no argument opens the picker)";
      return true;
    }
    if (verb == "installed") {
      if (!argument.empty()) {
        state.notice = "usage: /skills installed";
        return true;
      }
      if (skills.empty()) {
        state.notice = "no installed skills under the dsh roots";
        return true;
      }
      std::string out = "installed skills (" + std::to_string(skills.size()) + ")";
      constexpr std::size_t kRows = 20;
      for (std::size_t index = 0; index < skills.size() && index < kRows; ++index) {
        const auto& skill = skills[index];
        out += "\n  /" + skill.name + " — " + ellipsize(skill.description, 90) + " (" + skill.source + ")";
      }
      if (skills.size() > kRows) out += "\n  … +" + std::to_string(skills.size() - kRows) + " more";
      state.notice = std::move(out);
      return true;
    }
    if (verb == "search") {
      if (argument.empty()) {
        state.notice = "usage: /skills search <query>";
        return true;
      }
      const std::string needle = lower(argument);
      std::string out;
      std::size_t matches = 0;
      constexpr std::size_t kRows = 20;
      for (const auto& skill : skills) {
        if (lower(skill.name).find(needle) == std::string::npos &&
            lower(skill.description).find(needle) == std::string::npos) {
          continue;
        }
        ++matches;
        if (matches <= kRows) out += "\n  /" + skill.name + " — " + ellipsize(skill.description, 90);
      }
      if (matches > kRows) out += "\n  … +" + std::to_string(matches - kRows) + " more";
      state.notice = matches == 0 ? "no matching skills"
                                  : "skills matching \"" + argument + "\" (" +
                                        std::to_string(matches) + ")" + out;
      return true;
    }
    if (verb == "install" || verb == "update") {
      state.notice =
          "skill registry is unavailable: dsh loads skills from .dsh/skills and the user skill roots";
      return true;
    }
    if (!verb.empty()) {
      state.notice =
          "usage: /skills [search <query>|installed|install <name>|update]  (no argument opens the picker)";
      return true;
    }
    if (skills.empty()) {
      state.notice = "no user-invocable skills under the dsh roots";
      return true;
    }
    std::vector<LibraryRow> rows;
    rows.reserve(skills.size());
    for (const auto& skill : skills) {
      rows.push_back({skill.name, "/" + skill.name, skill.description + " · " + skill.source});
    }
    overlay.open(LibraryKind::Skills, "skills", std::move(rows));
    return true;
  }
  if (command == "/settings") {
    overlay.open(LibraryKind::Settings, "settings", settings_rows(settings, editor));
    return true;
  }
  return false;
}

bool handle_library_event(ftxui::Event& event, AppState& state, LibraryOverlay& overlay,
                          Editor& editor, const LibrarySettings& settings) {
  if (!overlay.visible()) return false;
  const LibraryKind kind = overlay.kind();
  const auto chosen = overlay.key(event);
  if (!chosen) return true;
  if (kind == LibraryKind::Settings) {
    if (*chosen == "thinking" && settings.show_thoughts != nullptr) {
      *settings.show_thoughts = !*settings.show_thoughts;
    } else if (*chosen == "tools" && settings.tools_hidden != nullptr) {
      *settings.tools_hidden = !*settings.tools_hidden;
    } else if (*chosen == "expanded" && settings.tools_expanded != nullptr) {
      *settings.tools_expanded = !*settings.tools_expanded;
    } else if (*chosen == "notifications") {
      const bool enabled = notifications_enabled(settings.state_dir);
      if (!persist_notifications_enabled(settings.state_dir, !enabled)) {
        state.notice = "could not write notification setting";
      }
    } else if (*chosen == "mouse" && settings.mouse_enabled != nullptr) {
      *settings.mouse_enabled = !*settings.mouse_enabled;
      save_mouse_enabled(settings.state_dir, *settings.mouse_enabled);
      if (settings.screen != nullptr) apply_mouse_capture(*settings.screen, *settings.mouse_enabled);
    } else if (*chosen == "vim") {
      editor.EnableVim(!editor.VimEnabled());
    }
    overlay.open(LibraryKind::Settings, "settings", settings_rows(settings, editor));
    return true;
  }
  overlay.close();
  if (kind == LibraryKind::Skills) {
    state.composer = "/" + *chosen + " ";
    editor.SetContent(state.composer);
  }
  return true;
}

}  // namespace booth
