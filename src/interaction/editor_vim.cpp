#include "interaction/editor.hpp"

#include <algorithm>
#include <string>
#include <utility>
#include <vector>

namespace booth {
namespace {

int clamped(int value, int low, int high) { return std::max(low, std::min(value, high)); }

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

std::string join_lines(const std::vector<std::string>& lines) {
  std::string text;
  for (std::size_t i = 0; i < lines.size(); ++i) {
    if (i != 0) text += '\n';
    text += lines[i];
  }
  return text;
}

std::size_t offset_of(const std::vector<std::string>& lines, int line, int col) {
  const int target = clamped(line, 0, static_cast<int>(lines.size()) - 1);
  std::size_t offset = static_cast<std::size_t>(std::max(0, col));
  for (int i = 0; i < target; ++i) offset += lines[static_cast<std::size_t>(i)].size() + 1;
  return offset;
}

std::string slice_range(const std::vector<std::string>& lines, vim::Position from, vim::Position to, bool linewise) {
  const int last = static_cast<int>(lines.size()) - 1;
  if (linewise) {
    std::string body;
    for (int i = clamped(from.line, 0, last); i <= clamped(to.line, 0, last); ++i) {
      if (!body.empty() || i != from.line) body += '\n';
      body += lines[static_cast<std::size_t>(i)];
    }
    return body;
  }
  const int from_line = clamped(from.line, 0, last);
  const int to_line = clamped(to.line, 0, last);
  if (from_line == to_line) {
    const std::string& text = lines[static_cast<std::size_t>(from_line)];
    return text.substr(static_cast<std::size_t>(std::max(0, from.col)),
                       static_cast<std::size_t>(std::max(0, to.col - from.col)));
  }
  std::string body = lines[static_cast<std::size_t>(from_line)].substr(static_cast<std::size_t>(std::max(0, from.col)));
  for (int i = from_line + 1; i < to_line; ++i) {
    body += '\n';
    body += lines[static_cast<std::size_t>(i)];
  }
  body += '\n';
  body += lines[static_cast<std::size_t>(to_line)].substr(0, static_cast<std::size_t>(std::max(0, to.col)));
  return body;
}

bool is_edit_command(vim::CommandKind kind) {
  switch (kind) {
    case vim::CommandKind::Delete:
    case vim::CommandKind::Replace:
    case vim::CommandKind::Join:
    case vim::CommandKind::Indent:
    case vim::CommandKind::Paste:
    case vim::CommandKind::OpenLine:
    case vim::CommandKind::Retype:
      return true;
    default:
      return false;
  }
}

}  // namespace

bool Editor::IsCaretMove(const ftxui::Event& event) {
  return event == ftxui::Event::ArrowLeft || event == ftxui::Event::ArrowRight || event == ftxui::Event::ArrowUp ||
         event == ftxui::Event::ArrowDown || event == ftxui::Event::Home || event == ftxui::Event::End;
}

std::optional<std::string> Editor::MapVimNav(const ftxui::Event& event) {
  if (event == ftxui::Event::ArrowUp) return "k";
  if (event == ftxui::Event::ArrowDown) return "j";
  if (event == ftxui::Event::ArrowLeft) return "h";
  if (event == ftxui::Event::ArrowRight) return "l";
  if (event == ftxui::Event::Home) return "0";
  if (event == ftxui::Event::End) return "$";
  if (event == ftxui::Event::Backspace) return "h";
  if (event == ftxui::Event::Delete) return "x";
  return std::nullopt;
}

bool Editor::HandleVimEvent(const ftxui::Event& event) {
  if (!vim_) return false;
  if (event == ftxui::Event::Escape) {
    if (!VimConsumesEscape()) return false;
    return RunVimKey("escape");
  }
  if (vim_->mode == vim::Mode::Insert) return HandleVimInsert(event);
  if (vim_->mode == vim::Mode::Replace) return HandleVimReplace(event);

  const std::string pending = vim_->PendingText();
  const bool awaiting_target = !pending.empty() && (pending.back() == 'f' || pending.back() == 'F' ||
                                                    pending.back() == 't' || pending.back() == 'T' ||
                                                    pending.back() == 'r');
  const std::string input = event.is_character() ? event.input() : std::string();
  if (awaiting_target) {
    if (input == " ") return RunVimKey(" ");
    if (MapVimNav(event) || event == ftxui::Event::Return || event == ftxui::Event::Tab ||
        (!input.empty() && input.size() == 1 && static_cast<unsigned char>(input[0]) < 0x20)) {
      return RunVimKey("escape");
    }
  }
  if (const std::optional<std::string> mapped = MapVimNav(event)) return RunVimKey(*mapped);
  if (event.is_character()) {
    if (input == " ") return RunVimKey(" ");
    if (!input.empty() && input.size() == 1 && static_cast<unsigned char>(input[0]) < 0x20) {
      if (vim_->Pending()) RunVimKey("escape");
      return false;
    }
    for (std::size_t at = 0; at < input.size();) {
      const std::size_t end = vim::next_grapheme_start(input, at);
      RunVimKey(input.substr(at, end - at));
      at = end;
      if (VimEditing()) {
        const std::string rest = input.substr(at);
        if (vim_->mode == vim::Mode::Replace) {
          for (std::size_t pos = 0; pos < rest.size();) {
            const std::size_t stop = vim::next_grapheme_start(rest, pos);
            TypeReplaceGrapheme(rest.substr(pos, stop - pos));
            pos = stop;
          }
        } else {
          TypeInsertText(rest);
        }
        return true;
      }
    }
    return true;
  }
  if (vim_->Pending()) RunVimKey("escape");
  return false;
}

bool Editor::HandleVimInsert(const ftxui::Event& event) {
  if (event == ftxui::Event::Return) return false;
  const auto edit = [&](const ftxui::Event& inner) {
    const std::string before = content_;
    const bool handled = HandleEditingEvent(inner);
    if (handled && session_open_) TrackVimSessionEdit(before);
    return handled;
  };
  if (event.input() == "\x1b\r" || event.input() == "\x1b\n") {
    TypeInsertText("\n");
    return true;
  }
  if (event == ftxui::Event::Backspace || event == ftxui::Event::Delete) return edit(event);
  if (IsCaretMove(event)) {
    const bool handled = HandleEditingEvent(event);
    if (handled) BreakVimSession();
    return handled;
  }
  if (event.is_character()) {
    const std::string input = event.input();
    if (input == "\n") {
      TypeInsertText("\n");
      return true;
    }
    if (!input.empty() && input.size() == 1 && static_cast<unsigned char>(input[0]) < 0x20) return edit(event);
    TypeInsertText(input);
    return true;
  }
  return edit(event);
}

bool Editor::HandleVimReplace(const ftxui::Event& event) {
  if (event == ftxui::Event::Return && !event.is_character()) return false;
  if (event == ftxui::Event::Backspace) {
    ReplaceBackspace();
    return true;
  }
  if (const std::optional<std::string> mapped = MapVimNav(event)) {
    const std::string before = content_;
    const bool handled = RunVimKey(*mapped);
    if (handled && content_ == before) BreakVimSession();
    return handled;
  }
  if (event.is_character()) {
    const std::string input = event.input();
    if (input == "\n") {
      ReplaceNewline();
      return true;
    }
    if (!input.empty() && input.size() == 1 && static_cast<unsigned char>(input[0]) < 0x20) return HandleEditingEvent(event);
    for (std::size_t at = 0; at < input.size();) {
      const std::size_t end = vim::next_grapheme_start(input, at);
      TypeReplaceGrapheme(input.substr(at, end - at));
      at = end;
    }
    return true;
  }
  return HandleEditingEvent(event);
}

void Editor::TypeInsertText(const std::string& text) {
  if (text.empty()) return;
  EnsureVimSession();
  if (!session_snapshot_) {
    PushUndoSnapshot();
    session_snapshot_ = true;
  }
  const std::string before = content_;
  Insert(text);
  if (session_open_) TrackVimSessionEdit(before);
}

void Editor::TrackVimSessionEdit(const std::string& before) {
  if (!tracked_) return;
  const std::string& after = content_;
  std::size_t prefix = 0;
  while (prefix < before.size() && prefix < after.size() && before[prefix] == after[prefix]) ++prefix;
  std::size_t suffix = 0;
  while (suffix < before.size() - prefix && suffix < after.size() - prefix &&
         before[before.size() - 1 - suffix] == after[after.size() - 1 - suffix]) {
    ++suffix;
  }
  const std::size_t removed_end = before.size() - suffix;
  const std::size_t inserted = after.size() - prefix - suffix;
  const std::size_t left_edge = std::min(removed_end, left_end_);
  const std::size_t left_cut = left_edge > prefix ? left_edge - prefix : 0;
  const std::size_t right_edge = std::max(prefix, right_start_);
  const std::size_t right_cut = removed_end > right_edge ? removed_end - right_edge : 0;
  const std::size_t typed_cut = removed_end - prefix - left_cut - right_cut;
  if (inserted > 0 && prefix < left_end_ - left_cut) {
    tracked_ = false;
    return;
  }
  if (left_cut > 0) {
    delete_before_ += static_cast<int>(vim::count_graphemes(std::string_view(before).substr(prefix, left_cut)));
  }
  if (right_cut > 0) {
    delete_after_ += static_cast<int>(vim::count_graphemes(std::string_view(before).substr(removed_end - right_cut, right_cut)));
  }
  left_end_ -= left_cut;
  right_start_ = right_start_ - left_cut - typed_cut + inserted;
}

void Editor::TypeReplaceGrapheme(const std::string& grapheme) {
  if (grapheme.empty()) return;
  EnsureVimSession();
  if (!session_snapshot_) {
    PushUndoSnapshot();
    session_snapshot_ = true;
  }
  const std::size_t start = LineStart(cursor_);
  const std::size_t end = LineEnd(cursor_);
  const std::string line = content_.substr(start, end - start);
  const std::size_t col = cursor_ - start;
  if (col >= line.size()) {
    content_.insert(cursor_, grapheme);
  } else {
    const std::size_t stop = vim::next_grapheme_start(line, col);
    replace_log_.push_back(
        {static_cast<int>(CursorLine()), static_cast<int>(col), line.substr(col, stop - col), grapheme});
    content_.replace(start + col, stop - col, grapheme);
  }
  cursor_ = start + col + grapheme.size();
  replace_typed_ += grapheme;
}

void Editor::ReplaceBackspace() {
  if (replace_log_.empty()) return;
  const ReplaceEntry entry = replace_log_.back();
  std::vector<std::string> lines = Lines();
  if (entry.line < 0 || entry.line >= static_cast<int>(lines.size())) {
    replace_log_.clear();
    return;
  }
  const std::string& line = lines[static_cast<std::size_t>(entry.line)];
  const std::size_t col = static_cast<std::size_t>(entry.col);
  if (line.substr(col, entry.text.size()) != entry.text || CursorLine() != static_cast<std::size_t>(entry.line) ||
      cursor_ != LineStart(cursor_) + col + entry.text.size()) {
    replace_log_.pop_back();
    return;
  }
  const std::size_t start = LineStart(cursor_);
  content_.replace(start + col, entry.text.size(), entry.removed);
  cursor_ = start + col;
  replace_log_.pop_back();
}

void Editor::ReplaceNewline() {
  EnsureVimSession();
  if (!session_snapshot_) {
    PushUndoSnapshot();
    session_snapshot_ = true;
  }
  content_.insert(cursor_, "\n");
  ++cursor_;
  replace_log_.clear();
  replace_typed_ += "\n";
}

bool Editor::RunVimKey(const std::string& key) {
  if (!vim_) return false;
  const bool editing_before = VimEditing();
  const std::string before_text = content_;
  const std::optional<std::vector<vim::Command>> commands = vim_->HandleKey(key, MakeVimBuffer());
  if (!commands) return false;
  const bool editing_after = VimEditing();
  if (editing_before && !editing_after) FinalizeVimSession();
  bool undo = false;
  bool edit = false;
  for (const vim::Command& command : *commands) {
    if (command.kind == vim::CommandKind::Undo) {
      undo = true;
    } else if (is_edit_command(command.kind)) {
      edit = true;
    }
  }
  if (!undo && (edit || (!editing_before && editing_after))) {
    if (editing_before) {
      EnsureVimSession();
      if (!session_snapshot_) {
        PushUndoSnapshot();
        session_snapshot_ = true;
      }
    } else {
      PushUndoSnapshot();
    }
  }
  ApplyVimCommands(*commands);
  if (!editing_before && editing_after) {
    OpenVimSession(false);
    session_snapshot_ = true;
  }
  if (!editing_before && !editing_after) {
    if (content_ != before_text) {
      vim_->CommitChange();
    } else {
      vim_->DiscardChange();
    }
  }
  vim_pending_ = vim_->PendingText();
  return true;
}

void Editor::ApplyVimCommands(const std::vector<vim::Command>& commands) {
  std::vector<std::string> lines = Lines();
  int line = static_cast<int>(CursorLine());
  int col = static_cast<int>(CursorColumn());
  const auto clamp_cursor = [&]() {
    line = clamped(line, 0, static_cast<int>(lines.size()) - 1);
    col = clamped(col, 0, static_cast<int>(lines[static_cast<std::size_t>(line)].size()));
  };
  for (const vim::Command& command : commands) {
    switch (command.kind) {
      case vim::CommandKind::Move:
        line = command.to.line;
        col = command.to.col;
        clamp_cursor();
        break;
      case vim::CommandKind::Mode:
        break;
      case vim::CommandKind::Yank: {
        const std::string body = slice_range(lines, command.from, command.to, command.linewise);
        if (!body.empty()) RememberKill(body + (command.linewise ? "\n" : ""));
        break;
      }
      case vim::CommandKind::Delete: {
        if (command.linewise) {
          const int last_line = std::min(command.to.line, static_cast<int>(lines.size()) - 1);
          const std::string removed = slice_range(lines, command.from, command.to, true);
          if (removed.empty() && command.from.line == last_line && lines.size() == 1) break;
          RememberKill(removed + "\n");
          lines.erase(lines.begin() + clamped(command.from.line, 0, last_line), lines.begin() + last_line + 1);
          if (lines.empty()) lines.emplace_back();
          line = std::min(command.from.line, static_cast<int>(lines.size()) - 1);
          col = 0;
        } else {
          const std::string removed = slice_range(lines, command.from, command.to, false);
          if (removed.empty()) break;
          RememberKill(removed);
          const int start_line = clamped(command.from.line, 0, static_cast<int>(lines.size()) - 1);
          const int end_line = clamped(command.to.line, 0, static_cast<int>(lines.size()) - 1);
          const std::string head = lines[static_cast<std::size_t>(start_line)].substr(
              0, static_cast<std::size_t>(std::max(0, command.from.col)));
          const std::string tail = lines[static_cast<std::size_t>(end_line)].substr(
              static_cast<std::size_t>(std::max(0, command.to.col)));
          lines.erase(lines.begin() + start_line, lines.begin() + end_line + 1);
          lines.insert(lines.begin() + start_line, head + tail);
          line = start_line;
          col = command.from.col;
        }
        clamp_cursor();
        break;
      }
      case vim::CommandKind::OpenLine: {
        line = clamped(line, 0, static_cast<int>(lines.size()) - 1);
        const int at = command.below ? line + 1 : line;
        lines.insert(lines.begin() + at, "");
        line = at;
        col = 0;
        break;
      }
      case vim::CommandKind::Paste: {
        if (kill_ring_.empty()) break;
        const std::string& killed = kill_ring_.back();
        if (killed.back() == '\n') {
          const std::vector<std::string> body = split_lines(killed.substr(0, killed.size() - 1));
          const int at = clamped(command.after ? line + 1 : line, 0, static_cast<int>(lines.size()));
          for (int i = 0; i < std::max(1, command.count); ++i) {
            lines.insert(lines.begin() + at, body.begin(), body.end());
          }
          line = at;
          col = 0;
        } else {
          std::string text;
          for (int i = 0; i < std::max(1, command.count); ++i) text += killed;
          const std::string& source = lines[static_cast<std::size_t>(clamped(line, 0, static_cast<int>(lines.size()) - 1))];
          const std::size_t at =
              command.after ? vim::next_grapheme_start(source, static_cast<std::size_t>(col)) : static_cast<std::size_t>(col);
          lines[static_cast<std::size_t>(line)].insert(at, text);
          col = static_cast<int>(at + vim::last_grapheme_start(text));
        }
        clamp_cursor();
        break;
      }
      case vim::CommandKind::Undo:
        Undo();
        lines = Lines();
        line = static_cast<int>(CursorLine());
        col = static_cast<int>(CursorColumn());
        break;
      case vim::CommandKind::Replace: {
        const int at = clamped(command.from.line, 0, static_cast<int>(lines.size()) - 1);
        const std::string& source = lines[static_cast<std::size_t>(at)];
        lines[static_cast<std::size_t>(at)] =
            source.substr(0, static_cast<std::size_t>(std::max(0, command.from.col))) + command.text +
            source.substr(static_cast<std::size_t>(std::max(0, command.to.col)));
        line = at;
        col = command.from.col + static_cast<int>(vim::last_grapheme_start(command.text));
        clamp_cursor();
        break;
      }
      case vim::CommandKind::Join: {
        const int start = clamped(command.from_line, 0, static_cast<int>(lines.size()) - 1);
        const int end = std::min(start + std::max(command.lines, 2) - 1, static_cast<int>(lines.size()) - 1);
        if (end <= start) break;
        std::string text = lines[static_cast<std::size_t>(start)];
        int cursor_col = static_cast<int>(text.size());
        for (int i = start + 1; i <= end; ++i) {
          const std::string& raw = lines[static_cast<std::size_t>(i)];
          const std::size_t trim = raw.find_first_not_of(" \t");
          const std::string next = raw.substr(trim == std::string::npos ? raw.size() : trim);
          if (i == start + 1) cursor_col = static_cast<int>(text.size());
          const bool no_space =
              text.empty() || next.empty() || text.back() == ' ' || text.back() == '\t' || next.front() == ')';
          const bool sentence = !text.empty() && (text.back() == '.' || text.back() == '!' || text.back() == '?');
          text += no_space ? "" : (sentence ? "  " : " ");
          text += next;
        }
        lines.erase(lines.begin() + start, lines.begin() + end + 1);
        lines.insert(lines.begin() + start, text);
        line = start;
        col = cursor_col;
        clamp_cursor();
        break;
      }
      case vim::CommandKind::Indent: {
        const int first = clamped(command.from_line, 0, static_cast<int>(lines.size()) - 1);
        const int end = std::max(first, clamped(command.to_line, 0, static_cast<int>(lines.size()) - 1));
        for (int i = first; i <= end; ++i) {
          std::string& text = lines[static_cast<std::size_t>(i)];
          if (!command.out) {
            if (!text.empty()) text = (text.front() == '\t' ? "\t" : "  ") + text;
          } else if (!text.empty() && text.front() == '\t') {
            text.erase(0, 1);
          } else {
            const std::size_t spaces = text.find_first_not_of(' ');
            text.erase(0, std::min<std::size_t>(2, spaces == std::string::npos ? text.size() : spaces));
          }
        }
        line = first;
        col = static_cast<int>(vim::first_non_blank(lines[static_cast<std::size_t>(first)]));
        clamp_cursor();
        break;
      }
      case vim::CommandKind::Retype: {
        for (int n = 0; n < std::max(1, command.times); ++n) {
          if (n > 0 && command.newline) {
            line = clamped(line + 1, 0, static_cast<int>(lines.size()));
            lines.insert(lines.begin() + line, "");
            col = 0;
          }
          if (command.overwrite) {
            for (std::size_t at = 0; at < command.record.text.size();) {
              const std::size_t end = vim::next_grapheme_start(command.record.text, at);
              const std::string grapheme = command.record.text.substr(at, end - at);
              at = end;
              if (grapheme == "\n") {
                const std::string& source = lines[static_cast<std::size_t>(line)];
                const std::string tail = source.substr(static_cast<std::size_t>(col));
                lines[static_cast<std::size_t>(line)] = source.substr(0, static_cast<std::size_t>(col));
                lines.insert(lines.begin() + line + 1, tail);
                ++line;
                col = 0;
                continue;
              }
              std::string& target = lines[static_cast<std::size_t>(line)];
              const std::size_t position = static_cast<std::size_t>(col);
              const std::size_t stop = position >= target.size() ? position : vim::next_grapheme_start(target, position);
              target.replace(position, stop - position, grapheme);
              col = static_cast<int>(position + grapheme.size());
            }
            continue;
          }
          const std::string flat = join_lines(lines);
          const std::size_t cursor = offset_of(lines, line, col);
          const std::size_t start = vim::step_graphemes(flat, cursor, command.record.delete_before, false);
          const std::size_t stop = vim::step_graphemes(flat, cursor, command.record.delete_after, true);
          const std::string changed = flat.substr(0, start) + command.record.text + flat.substr(stop);
          lines = split_lines(changed);
          const vim::Position landed = vim::offset_to_pos(lines, start + command.record.text.size());
          line = clamped(landed.line, 0, static_cast<int>(lines.size()) - 1);
          col = landed.col;
          if (command.record.text.empty()) {
            col = static_cast<int>(vim::prev_grapheme_start(lines[static_cast<std::size_t>(line)],
                                                            static_cast<std::size_t>(col)));
          }
        }
        col = static_cast<int>(vim::prev_grapheme_start(
            lines[static_cast<std::size_t>(clamped(line, 0, static_cast<int>(lines.size()) - 1))],
            static_cast<std::size_t>(std::max(0, col))));
        clamp_cursor();
        break;
      }
    }
  }
  content_ = join_lines(lines);
  cursor_ = std::min(offset_of(lines, line, col), content_.size());
  if (vim_ && vim_->mode == vim::Mode::Normal) {
    const int at = clamped(line, 0, static_cast<int>(lines.size()) - 1);
    const std::size_t line_start = offset_of(lines, at, 0);
    const std::size_t last_start = vim::last_grapheme_start(lines[static_cast<std::size_t>(at)]);
    if (cursor_ > line_start + last_start) cursor_ = line_start + last_start;
  }
}

}  // namespace booth
