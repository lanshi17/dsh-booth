#include "interaction/copy_picker.hpp"

#include "interaction/text_util.hpp"

#include <algorithm>

#include "interaction/clipboard.hpp"

namespace booth {
namespace {

std::string first_line(const std::string& text, std::size_t limit) {
  auto end = text.find('\n');
  std::string line = text.substr(0, end);
  line = ellipsize(line, limit);
  return line;
}

void collect_code(const std::string& text, std::vector<CopyItem>& items) {
  std::size_t at = 0;
  while (true) {
    const auto open = text.find("```", at);
    if (open == std::string::npos) return;
    const auto language_end = text.find('\n', open + 3);
    if (language_end == std::string::npos) return;
    const auto close = text.find("```", language_end + 1);
    if (close == std::string::npos) return;
    std::string language = text.substr(open + 3, language_end - open - 3);
    language = ellipsize(language, 20);
    const std::string code = text.substr(language_end + 1, close - language_end - 1);
    if (!code.empty()) {
      items.push_back({language.empty() ? "code" : "code · " + language, code});
    }
    at = close + 3;
  }
}

}  // namespace

bool CopyPicker::open(const AppState& state) {
  items_.clear();
  for (auto entry = state.entries.rbegin(); entry != state.entries.rend(); ++entry) {
    if (entry->kind == EntryKind::Tool) {
      if (entry->output.empty() || entry->tool == "read" || entry->tool == "read_file") continue;
      items_.push_back({"tool · " + entry->tool + " · " + first_line(entry->output, 32),
                        entry->output});
      continue;
    }
    if (entry->kind != EntryKind::Assistant) continue;
    if (!entry->text.empty()) {
      items_.push_back({"answer · " + first_line(entry->text, 48), entry->text});
    }
    collect_code(entry->text, items_);
  }
  visible_ = !items_.empty();
  index_ = 0;
  return visible_;
}

void CopyPicker::close() { visible_ = false; }

bool CopyPicker::click(std::size_t row, AppState& state) {
  if (!visible_ || row >= items_.size()) return false;
  index_ = row;
  const auto& item = items_[index_];
  if (copy_to_clipboard(item.text)) {
    state.notice = "copied " + std::to_string(item.text.size()) + " bytes";
  } else {
    state.notice = "nothing to copy";
  }
  close();
  return true;
}

bool CopyPicker::key(ftxui::Event& event, AppState& state) {
  if (!visible_) return false;
  if (event == ftxui::Event::Escape) {
    close();
    return true;
  }
  if (event == ftxui::Event::ArrowUp ||
      (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelUp)) {
    index_ = (index_ + items_.size() - 1) % items_.size();
    return true;
  }
  if (event == ftxui::Event::ArrowDown ||
      (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelDown)) {
    index_ = (index_ + 1) % items_.size();
    return true;
  }
  if (event == ftxui::Event::Return) {
    const auto& item = items_[index_];
    if (copy_to_clipboard(item.text)) {
      state.notice = "copied " + std::to_string(item.text.size()) + " bytes";
    } else {
      state.notice = "nothing to copy";
    }
    close();
    return true;
  }
  return true;
}

}  // namespace booth
