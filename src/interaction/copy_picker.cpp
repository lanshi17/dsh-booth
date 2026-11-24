#include "interaction/copy_picker.hpp"

#include "interaction/text_util.hpp"

#include <algorithm>
#include <cctype>

#include "interaction/clipboard.hpp"
#include "interaction/tool_parse.hpp"
#include "interaction/view.hpp"

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

void collect_quotes(const std::string& text, std::vector<CopyItem>& items) {
  std::istringstream input(text);
  std::string quote;
  std::string line;
  const auto flush = [&] {
    if (quote.empty()) return;
    items.push_back({"quote · " + first_line(quote, 48), quote});
    quote.clear();
  };
  while (std::getline(input, line)) {
    const auto first = line.find_first_not_of(" \t");
    if (first != std::string::npos && line[first] == '>') {
      if (!quote.empty()) quote += '\n';
      std::string rest = line.substr(first + 1);
      if (!rest.empty() && rest.front() == ' ') rest.erase(0, 1);
      quote += rest;
    } else {
      flush();
    }
  }
  flush();
}

std::string lower_copy(const std::string& text) {
  std::string out(text);
  for (char& ch : out) ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
  return out;
}

}  // namespace

void CopyPicker::rebuild() {
  matches_.clear();
  const std::string needle = lower_copy(query_);
  for (std::size_t index = 0; index < items_.size(); ++index) {
    if (lower_copy(items_[index].label).find(needle) != std::string::npos) {
      matches_.push_back(index);
    }
  }
  if (matches_.empty()) {
    index_ = 0;
  } else {
    index_ = std::min(index_, matches_.size() - 1);
  }
}

bool CopyPicker::open(const AppState& state) {
  items_.clear();
  for (auto entry = state.entries.rbegin(); entry != state.entries.rend(); ++entry) {
    if (entry->kind == EntryKind::Tool) {
      if (!entry->input.empty() && is_shell_tool(entry->tool)) {
        items_.push_back({"command · " + entry->tool + " · " +
                              first_line(entry->input, 48),
                          entry->input});
      }
      const bool read = entry->tool == "read" || entry->tool == "read_file";
      const std::string text = read ? read_output_text(entry->output) : entry->output;
      if (text.empty()) continue;
      const std::string label = read ? "read · " : "tool · " + entry->tool + " · ";
      items_.push_back({label + first_line(text, 32), text});
      continue;
    }
    if (entry->kind != EntryKind::Assistant) continue;
    if (!entry->text.empty()) {
      items_.push_back({"answer · " + first_line(entry->text, 48), entry->text});
    }
    collect_code(entry->text, items_);
    collect_quotes(entry->text, items_);
  }
  visible_ = !items_.empty();
  index_ = 0;
  query_.clear();
  rebuild();
  return visible_;
}

void CopyPicker::close() { visible_ = false; }

bool CopyPicker::click(std::size_t row, AppState& state) {
  if (!visible_ || row >= matches_.size()) return false;
  index_ = row;
  const auto& item = items_[matches_[index_]];
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
  if (event == ftxui::Event::Backspace) {
    if (!query_.empty()) {
      query_.pop_back();
      rebuild();
    }
    return true;
  }
  if (event.is_character()) {
    const std::string ch = event.character();
    if (!ch.empty()) {
      query_ += ch;
      rebuild();
    }
    return true;
  }
  if (event == ftxui::Event::ArrowUp ||
      (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelUp)) {
    if (matches_.empty()) return true;
    index_ = (index_ + matches_.size() - 1) % matches_.size();
    return true;
  }
  if (event == ftxui::Event::ArrowDown ||
      (event.is_mouse() && event.mouse().button == ftxui::Mouse::WheelDown)) {
    if (matches_.empty()) return true;
    index_ = (index_ + 1) % matches_.size();
    return true;
  }
  if (event == ftxui::Event::Return) {
    if (matches_.empty()) return true;
    const auto& item = items_[matches_[index_]];
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
