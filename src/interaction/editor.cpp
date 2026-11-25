#include "interaction/editor.hpp"

#include <algorithm>
#include <cctype>

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

bool is_space(char ch) { return std::isspace(static_cast<unsigned char>(ch)) != 0; }

std::vector<std::string> split_lines(const std::string& text) {
  std::vector<std::string> lines;
  std::size_t start = 0;
  while (true) {
    const std::size_t end = text.find('\n', start);
    if (end == std::string::npos) {
      lines.push_back(text.substr(start));
      break;
    }
    lines.push_back(text.substr(start, end - start));
    start = end + 1;
  }
  return lines;
}

}  // namespace

Editor::Editor(std::string& content) : content_(content), cursor_(content.size()) {}

void Editor::Reset() {
  content_.clear();
  cursor_ = 0;
  undo_.clear();
  redo_.clear();
  kill_.clear();
  typing_ = false;
  session_open_ = false;
  replace_log_.clear();
  replace_typed_.clear();
  if (vim_) {
    vim_->Reset();
    vim_->mode = vim::Mode::Insert;
    vim_pending_.clear();
    OpenVimSession(false);
  }
}

void Editor::SetContent(const std::string& content) {
  FinalizeVimSession();
  content_ = content;
  cursor_ = content_.size();
  undo_.clear();
  redo_.clear();
  typing_ = false;
  replace_log_.clear();
  replace_typed_.clear();
  if (vim_) {
    if (VimEditing()) OpenVimSession(false);
    vim_pending_ = vim_->PendingText();
  }
}

void Editor::Paste(const std::string& text) {
  if (text.empty()) return;
  BeforeEdit(false);
  Insert(text);
  typing_ = false;
}

void Editor::SetCursor(std::size_t offset) {
  offset = std::min(offset, content_.size());
  std::size_t snapped = 0;
  while (snapped < content_.size()) {
    const std::size_t end = vim::next_grapheme_start(content_, snapped);
    if (offset < end || end <= snapped) break;
    snapped = end;
  }
  cursor_ = std::min(snapped, content_.size());
  typing_ = false;
  if (!vim_) return;
  if (vim_->Pending()) vim_->HandleKey("escape", MakeVimBuffer());
  if (session_open_) {
    FinalizeVimSession();
    if (VimEditing()) OpenVimSession(true);
  }
  if (vim_->mode == vim::Mode::Normal) {
    const std::size_t start = LineStart(cursor_);
    const std::size_t last = vim::last_grapheme_start(content_.substr(start, LineEnd(cursor_) - start));
    if (cursor_ - start > last) cursor_ = start + last;
  }
  vim_pending_ = vim_->PendingText();
}

void Editor::EnableVim(bool enabled) {
  if (enabled == (vim_ != nullptr)) return;
  session_open_ = false;
  replace_log_.clear();
  replace_typed_.clear();
  typing_ = false;
  vim_pending_.clear();
  vim_.reset();
  if (enabled) {
    vim_ = std::make_unique<vim::VimState>();
    vim_->mode = vim::Mode::Insert;
    OpenVimSession(false);
  }
}

bool Editor::VimNormal() const { return vim_ && vim_->mode == vim::Mode::Normal; }

vim::Mode Editor::VimMode() const { return vim_ ? vim_->mode : vim::Mode::Insert; }

bool Editor::VimEditing() const {
  return vim_ && (vim_->mode == vim::Mode::Insert || vim_->mode == vim::Mode::Replace);
}

bool Editor::VimConsumesEscape() const {
  return vim_ && (vim_->Pending() || vim_->mode != vim::Mode::Normal);
}

const std::string& Editor::VimPending() const { return vim_pending_; }

std::optional<vim::Position> Editor::VimAnchor() const {
  if (!vim_ || !vim_->Visual() || !vim_->anchor) return std::nullopt;
  return vim_->anchor;
}

std::optional<vim::Selection> Editor::VimSelection() const {
  if (!vim_ || !vim_->Visual() || !vim_->anchor) return std::nullopt;
  return vim::visual_range(MakeVimBuffer(), *vim_->anchor, vim_->mode == vim::Mode::VisualLine);
}

std::size_t Editor::VimSelectedLines() const {
  const std::optional<vim::Selection> selection = VimSelection();
  return selection ? static_cast<std::size_t>(selection->to.line - selection->from.line + 1) : 0;
}

std::vector<vim::Selection> Editor::VimSelectionSpans() const {
  const std::optional<vim::Selection> selection = VimSelection();
  if (!selection) return {};
  const std::vector<std::string> lines = Lines();
  if (selection->from.line == selection->to.line) return {*selection};
  std::vector<vim::Selection> spans;
  for (int line = selection->from.line; line <= selection->to.line; ++line) {
    const int width = static_cast<int>(lines[static_cast<std::size_t>(line)].size());
    const int from = line == selection->from.line ? selection->from.col : 0;
    const int to = line == selection->to.line ? selection->to.col : width;
    spans.push_back({{line, from}, {line, to}, selection->linewise});
  }
  return spans;
}

std::string Editor::ModeLabel() const {
  if (!vim_) return "";
  const char* base = "NORMAL";
  switch (vim_->mode) {
    case vim::Mode::Insert: base = "INSERT"; break;
    case vim::Mode::Normal: base = "NORMAL"; break;
    case vim::Mode::Visual: base = "VISUAL"; break;
    case vim::Mode::VisualLine: base = "VISUAL LINE"; break;
    case vim::Mode::Replace: base = "REPLACE"; break;
  }
  std::string label = base;
  if (!vim_pending_.empty()) label += " " + vim_pending_;
  return label;
}

vim::Buffer Editor::MakeVimBuffer() const {
  return vim::Buffer{Lines(), static_cast<int>(CursorLine()), static_cast<int>(CursorColumn())};
}

void Editor::PushUndoSnapshot() {
  if (undo_.size() >= 200) undo_.erase(undo_.begin());
  undo_.push_back({content_, cursor_});
  redo_.clear();
  typing_ = false;
}

void Editor::EnsureVimSession() {
  if (!vim_ || session_open_) return;
  OpenVimSession(false);
}

void Editor::OpenVimSession(bool arrowed) {
  session_open_ = true;
  session_arrowed_ = arrowed;
  session_replace_ = vim_ && vim_->mode == vim::Mode::Replace;
  session_text_ = content_;
  session_cursor_ = cursor_;
  left_end_ = cursor_;
  right_start_ = cursor_;
  delete_before_ = 0;
  delete_after_ = 0;
  tracked_ = true;
  session_snapshot_ = false;
  replace_typed_.clear();
  replace_log_.clear();
}

void Editor::FinalizeVimSession() {
  if (!session_open_) return;
  vim::InsertRecord record;
  if (session_replace_) {
    record.text = replace_typed_;
  } else if (tracked_) {
    record.delete_before = delete_before_;
    record.delete_after = delete_after_;
    record.text = content_.substr(left_end_, right_start_ - left_end_);
  } else {
    record = vim::diff_insert_session(session_text_, session_cursor_, content_);
  }
  if (vim_) {
    if (!session_arrowed_) {
      vim_->RecordInsert(record, false);
    } else if (!record.text.empty() || record.delete_before > 0 || record.delete_after > 0) {
      vim_->RecordInsert(record, true);
    }
  }
  session_open_ = false;
  session_snapshot_ = false;
  replace_typed_.clear();
  replace_log_.clear();
}

void Editor::BreakVimSession() {
  FinalizeVimSession();
  if (!vim_ || !VimEditing()) return;
  OpenVimSession(true);
}

void Editor::BeforeEdit(bool typing) {
  if (vim_ && VimEditing()) {
    EnsureVimSession();
    if (!session_snapshot_) {
      PushUndoSnapshot();
      session_snapshot_ = true;
    }
    typing_ = true;
    return;
  }
  const bool same_burst = typing && typing_ && !undo_.empty();
  if (!same_burst) {
    if (undo_.size() >= 200) undo_.erase(undo_.begin());
    undo_.push_back({content_, cursor_});
  }
  redo_.clear();
  typing_ = typing;
}

void Editor::Undo() {
  if (undo_.empty()) return;
  redo_.push_back({content_, cursor_});
  Snapshot previous = std::move(undo_.back());
  undo_.pop_back();
  content_ = std::move(previous.text);
  cursor_ = std::min(previous.cursor, content_.size());
  typing_ = false;
}

void Editor::Redo() {
  if (redo_.empty()) return;
  undo_.push_back({content_, cursor_});
  Snapshot next = std::move(redo_.back());
  redo_.pop_back();
  content_ = std::move(next.text);
  cursor_ = std::min(next.cursor, content_.size());
  typing_ = false;
}

void Editor::Insert(const std::string& text) {
  content_.insert(cursor_, text);
  cursor_ += text.size();
}

void Editor::Kill(std::size_t from, std::size_t to) {
  if (from >= to) return;
  BeforeEdit(false);
  kill_ = content_.substr(from, to - from);
  content_.erase(from, to - from);
  cursor_ = from;
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
    cursor_ = std::min(LineStart(previous_end) + column, previous_end);
  } else {
    const std::size_t end = LineEnd(cursor_);
    if (end == content_.size()) return;
    const std::size_t next_start = end + 1;
    cursor_ = std::min(next_start + column, LineEnd(next_start));
  }
}

std::size_t Editor::WordStart(std::size_t at) const {
  at = std::min(at, content_.size());
  while (at > 0) {
    const std::size_t previous = prev_glyph(content_, at);
    if (!is_space(content_[previous])) break;
    at = previous;
  }
  while (at > 0) {
    const std::size_t previous = prev_glyph(content_, at);
    if (is_space(content_[previous])) break;
    at = previous;
  }
  return at;
}

std::size_t Editor::WordEnd(std::size_t at) const {
  at = std::min(at, content_.size());
  while (at < content_.size() && !is_space(content_[at])) at = next_glyph(content_, at);
  while (at < content_.size() && is_space(content_[at])) at = next_glyph(content_, at);
  return at;
}

void Editor::MoveWord(int direction) {
  cursor_ = direction < 0 ? WordStart(cursor_) : WordEnd(cursor_);
  typing_ = false;
}

bool Editor::HandleEvent(const ftxui::Event& event) {
  if (vim_) return HandleVimEvent(event);
  return HandleEditingEvent(event);
}

bool Editor::HandleEditingEvent(const ftxui::Event& event) {
  if (event.input() == "\x1b\r" || event.input() == "\x1b\n") {
    BeforeEdit(false);
    Insert("\n");
    return true;
  }
  if (event == ftxui::Event::Backspace) {
    if (cursor_ == 0) return true;
    BeforeEdit(false);
    const std::size_t from = prev_glyph(content_, cursor_);
    content_.erase(from, cursor_ - from);
    cursor_ = from;
    return true;
  }
  if (event == ftxui::Event::Delete) {
    if (cursor_ >= content_.size()) return true;
    BeforeEdit(false);
    const std::size_t to = next_glyph(content_, cursor_);
    content_.erase(cursor_, to - cursor_);
    return true;
  }
  if (event == ftxui::Event::ArrowLeft) {
    cursor_ = prev_glyph(content_, cursor_);
    typing_ = false;
    return true;
  }
  if (event == ftxui::Event::ArrowRight) {
    cursor_ = next_glyph(content_, cursor_);
    typing_ = false;
    return true;
  }
  if (event == ftxui::Event::ArrowUp) {
    MoveVertical(-1);
    typing_ = false;
    return true;
  }
  if (event == ftxui::Event::ArrowDown) {
    MoveVertical(1);
    typing_ = false;
    return true;
  }
  if (event == ftxui::Event::Home) {
    cursor_ = LineStart(cursor_);
    typing_ = false;
    return true;
  }
  if (event == ftxui::Event::End) {
    cursor_ = LineEnd(cursor_);
    typing_ = false;
    return true;
  }
  if (event == ftxui::Event::ArrowLeftCtrl || event.input() == "\x1b[1;3D") {
    MoveWord(-1);
    return true;
  }
  if (event == ftxui::Event::ArrowRightCtrl || event.input() == "\x1b[1;3C") {
    MoveWord(1);
    return true;
  }
  if (event.input() == "\x01") {
    cursor_ = LineStart(cursor_);
    typing_ = false;
    return true;
  }
  if (event.input() == "\x05") {
    cursor_ = LineEnd(cursor_);
    typing_ = false;
    return true;
  }
  if (event.input() == "\x1a") {
    Undo();
    return true;
  }
  if (event.input() == "\x1bz") {
    Redo();
    return true;
  }
  if (event.input() == "\x17" || event.input() == "\x1b\x7f") {
    Kill(WordStart(cursor_), cursor_);
    return true;
  }
  if (event.input() == "\x0b") {
    Kill(cursor_, LineEnd(cursor_));
    return true;
  }
  if (event.input() == "\x15") {
    Kill(LineStart(cursor_), cursor_);
    return true;
  }
  if (event.input() == "\x19") {
    if (kill_.empty()) return true;
    BeforeEdit(false);
    Insert(kill_);
    return true;
  }
  if (event.is_character()) {
    const std::string input = event.input();
    if (input.size() == 1 && (static_cast<unsigned char>(input[0]) < 0x20)) return false;
    BeforeEdit(true);
    Insert(input);
    return true;
  }
  return false;
}

std::vector<std::string> Editor::Lines() const { return split_lines(content_); }

std::size_t Editor::CursorLine() const {
  return static_cast<std::size_t>(std::count(content_.begin(), content_.begin() + cursor_, '\n'));
}

std::size_t Editor::CursorColumn() const { return cursor_ - LineStart(cursor_); }

}  // namespace booth
