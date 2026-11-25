#include "interaction/library_ui.hpp"

#include <algorithm>
#include <cctype>
#include <map>
#include <utility>

#include "interaction/editor.hpp"
#include "interaction/model_picker.hpp"
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
                                 : " Enter opens this provider's models · Esc closes")
                     | ftxui::dim);
  return ftxui::vbox(std::move(rows)) | ftxui::size(ftxui::WIDTH, ftxui::EQUAL, 64) |
         ftxui::borderRounded | ftxui::center | ftxui::bgcolor(kPanel);
}

bool handle_library_command(const std::string& line, AppState& state, LibraryOverlay& overlay,
                            const std::filesystem::path& cwd) {
  const std::string command = first_word(normalize_command(line));
  if (command == "/skills") {
    auto skills = scan_default_skills(cwd);
    if (skills.empty()) {
      state.notice = "no user-invocable skills under the dsh roots";
      return true;
    }
    std::sort(skills.begin(), skills.end(), [](const SkillInfo& left, const SkillInfo& right) {
      return left.name < right.name;
    });
    std::vector<LibraryRow> rows;
    rows.reserve(skills.size());
    for (const auto& skill : skills) {
      rows.push_back({skill.name, "/" + skill.name, skill.description + " · " + skill.source});
    }
    overlay.open(LibraryKind::Skills, "skills", std::move(rows));
    return true;
  }
  if (command == "/providers") {
    if (state.models.empty()) {
      state.notice = "no models yet; send a prompt first";
      return true;
    }
    std::map<std::string, std::pair<std::size_t, bool>> providers;
    for (const auto& model : state.models) {
      auto& entry = providers[model.provider];
      ++entry.first;
      entry.second = entry.second || model.name == state.model || model.value == state.model;
    }
    std::vector<LibraryRow> rows;
    rows.reserve(providers.size());
    for (const auto& [provider, info] : providers) {
      std::string detail = std::to_string(info.first) + (info.first == 1 ? " model" : " models");
      if (info.second) detail += " · current";
      rows.push_back({provider, provider, std::move(detail)});
    }
    overlay.open(LibraryKind::Providers, "providers", std::move(rows));
    return true;
  }
  return false;
}

bool handle_library_event(ftxui::Event& event, AppState& state, LibraryOverlay& overlay,
                          ModelPicker& models, Editor& editor) {
  if (!overlay.visible()) return false;
  const LibraryKind kind = overlay.kind();
  const auto chosen = overlay.key(event);
  if (!chosen) return true;
  overlay.close();
  if (kind == LibraryKind::Skills) {
    state.composer = "/" + *chosen + " ";
    editor.SetContent(state.composer);
  } else if (kind == LibraryKind::Providers) {
    models.open(state.models, state.model, *chosen);
    state.notice = "provider " + *chosen;
  }
  return true;
}

}  // namespace booth
