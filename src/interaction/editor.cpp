#include "interaction/editor.hpp"

#include <algorithm>

namespace booth {
namespace {

bool is_continuation(char ch) { return (static_cast<unsigned char>(ch) & 0xC0) == 0x80; }

std::size_t next_glyph(const std::string& text, std::size_t at) {
  if (at >= text.size()) return text.size();
  ++at;
  while (at < text.size() && is_continuation(text[at])) ++at;
  return at;
}

std::size_t prev_glyph(const std::string& text, std::size_t at) {
  if (at == 0) return 0;
  --at;
  while (at > 0 && is_continuation(text[at])) --at;
  return at;
}

}  // namespace

Editor::Editor(std::string& content) : content_(content), cursor_(content.size()) {}

void Editor::Reset() {
  content_.clear();
  cursor_ = 0;
}

void Editor::SetContent(const std::string& content) {
  content_ = content;
  cursor_ = content_.size();
}

void Editor::Insert(const std::string& text) {
  content_.insert(cursor_, text);
  cursor_ += text.size();
}

std::size_t Editor::LineStart(std::size_t at) const {
  if (at > content_.size()) at = content_.size();
  const auto found = content_.rfind('\n', at == 0 ? 0 : at - 1);
  return found == std::string::npos ? 0 : found + 1;
}

std::size_t Editor::LineEnd(std::size_t at) const {
  const auto found = content_.find('\n', at);
  return found == std::string::npos ? content_.size() : found;
}

void Editor::MoveVertical(int direction) {
  const std::size_t start = LineStart(cursor_);
  const std::size_t column = cursor_ - start;
  if (direction < 0) {
    if (start == 0) return;
    const std::size_t previous_end = start - 1;
    const std::size_t previous_start = LineStart(previous_end);
    cursor_ = std::min(previous_start + column, previous_end);
  } else {
    const std::size_t end = LineEnd(cursor_);
    if (end == content_.size()) return;
    const std::size_t next_start = end + 1;
    cursor_ = std::min(next_start + column, LineEnd(next_start));
  }
}

bool Editor::HandleEvent(const ftxui::Event& event) {
  if (event.input() == "\x1b\r" || event.input() == "\x1b\n") {
    Insert("\n");
    return true;
  }
  if (event == ftxui::Event::Backspace) {
    if (cursor_ == 0) return true;
    const std::size_t from = prev_glyph(content_, cursor_);
    content_.erase(from, cursor_ - from);
    cursor_ = from;
    return true;
  }
  if (event == ftxui::Event::Delete) {
    if (cursor_ >= content_.size()) return true;
    const std::size_t to = next_glyph(content_, cursor_);
    content_.erase(cursor_, to - cursor_);
    return true;
  }
  if (event == ftxui::Event::ArrowLeft) {
    cursor_ = prev_glyph(content_, cursor_);
    return true;
  }
  if (event == ftxui::Event::ArrowRight) {
    cursor_ = next_glyph(content_, cursor_);
    return true;
  }
  if (event == ftxui::Event::ArrowUp) {
    MoveVertical(-1);
    return true;
  }
  if (event == ftxui::Event::ArrowDown) {
    MoveVertical(1);
    return true;
  }
  if (event == ftxui::Event::Home) {
    cursor_ = LineStart(cursor_);
    return true;
  }
  if (event == ftxui::Event::End) {
    cursor_ = LineEnd(cursor_);
    return true;
  }
  if (event.is_character()) {
    const std::string input = event.input();
    if (input.size() == 1 && (static_cast<unsigned char>(input[0]) < 0x20)) return false;
    Insert(input);
    return true;
  }
  return false;
}

std::vector<std::string> Editor::Lines() const {
  std::vector<std::string> lines;
  std::size_t start = 0;
  while (true) {
    const std::size_t end = content_.find('\n', start);
    if (end == std::string::npos) {
      lines.push_back(content_.substr(start));
      break;
    }
    lines.push_back(content_.substr(start, end - start));
    start = end + 1;
  }
  return lines;
}

std::size_t Editor::CursorLine() const {
  return static_cast<std::size_t>(std::count(content_.begin(), content_.begin() + cursor_, '\n'));
}

std::size_t Editor::CursorColumn() const {
  return cursor_ - LineStart(cursor_);
}

}  // namespace booth
