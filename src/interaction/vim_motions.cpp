#include "interaction/vim.hpp"

#include <algorithm>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#if defined(__GNUC__)
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

namespace booth::vim {
namespace {

struct Span {
  std::size_t start;
  std::size_t end;
};

std::vector<Span> spans_of(std::string_view text) {
  std::vector<Span> spans;
  for (std::size_t at = 0; at < text.size(); at = next_grapheme_start(text, at)) {
    spans.push_back({at, next_grapheme_start(text, at)});
  }
  return spans;
}

bool at_space(std::string_view text, std::size_t at) {
  if (at >= text.size()) return false;
  const std::size_t end = next_grapheme_start(text, at);
  return word_kind(text.substr(at, end - at)) == WordKind::Whitespace;
}

int word_forward(std::string_view text, int col) {
  std::size_t next = move_word_right(text, static_cast<std::size_t>(col));
  while (at_space(text, next)) next = next_grapheme_start(text, next);
  return static_cast<int>(next);
}

int word_end(std::string_view text, int col) {
  std::size_t from = next_grapheme_start(text, static_cast<std::size_t>(col));
  while (at_space(text, from)) from = next_grapheme_start(text, from);
  return std::max(col, static_cast<int>(prev_grapheme_start(text, move_word_right(text, from))));
}

int find_grapheme(std::string_view text, int from_col, std::string_view target, bool forward, int count) {
  if (count < 1 || target.empty()) return -1;
  const std::vector<Span> spans = spans_of(text);
  int here = -1;
  for (std::size_t i = 0; i < spans.size(); ++i) {
    if (static_cast<int>(spans[i].start) <= from_col) {
      here = static_cast<int>(i);
    } else {
      break;
    }
  }
  int seen = 0;
  if (forward) {
    for (std::size_t i = static_cast<std::size_t>(here + 1); i < spans.size(); ++i) {
      if (text.substr(spans[i].start, spans[i].end - spans[i].start) == target && ++seen == count) {
        return static_cast<int>(spans[i].start);
      }
    }
  } else {
    for (int i = here - 1; i >= 0; --i) {
      const Span& span = spans[static_cast<std::size_t>(i)];
      if (text.substr(span.start, span.end - span.start) == target && ++seen == count) {
        return static_cast<int>(span.start);
      }
    }
  }
  return -1;
}

std::vector<int> quote_marks(std::string_view text, char quote) {
  std::vector<int> marks;
  for (std::size_t i = 0; i < text.size(); ++i) {
    if (text[i] == '\\') {
      ++i;
    } else if (text[i] == quote) {
      marks.push_back(static_cast<int>(i));
    }
  }
  return marks;
}

std::pair<char, bool> bracket_mate(char ch) {
  switch (ch) {
    case '(': return {')', true};
    case ')': return {'(', false};
    case '[': return {']', true};
    case ']': return {'[', false};
    case '{': return {'}', true};
    case '}': return {'{', false};
    default: return {0, false};
  }
}

int quote_mate(const std::vector<int>& marks, int col) {
  for (std::size_t p = 0; p + 1 < marks.size(); p += 2) {
    if (col == marks[p]) return marks[p + 1];
    if (col == marks[p + 1]) return marks[p];
  }
  return -1;
}

std::optional<Position> match_delimiter(const Buffer& buf) {
  const std::string& line = buf.lines[static_cast<std::size_t>(std::max(0, buf.cursor_line))];
  const std::vector<int> doubled = quote_marks(line, '"');
  const std::vector<int> single = quote_marks(line, '\'');
  const auto quote_at = [&](int col) -> char {
    if (std::find(doubled.begin(), doubled.end(), col) != doubled.end()) return '"';
    if (std::find(single.begin(), single.end(), col) != single.end()) return '\'';
    return 0;
  };
  std::size_t col = static_cast<std::size_t>(std::max(0, buf.cursor_col));
  while (col < line.size() && bracket_mate(line[col]).first == 0 && quote_at(static_cast<int>(col)) == 0) ++col;
  if (col >= line.size()) return std::nullopt;
  const char quote = quote_at(static_cast<int>(col));
  if (quote != 0) {
    const int mate = quote_mate(quote == '"' ? doubled : single, static_cast<int>(col));
    return mate < 0 ? std::nullopt : std::optional<Position>(Position{buf.cursor_line, mate});
  }
  const auto [mate_char, forward] = bracket_mate(line[col]);
  const char open = forward ? line[col] : mate_char;
  const char close = forward ? mate_char : line[col];
  const auto [text, cursor] = flatten(Buffer{buf.lines, buf.cursor_line, static_cast<int>(col)});
  long long depth = 0;
  long long match = -1;
  for (long long i = static_cast<long long>(cursor) + (forward ? 1 : -1);
       forward ? i < static_cast<long long>(text.size()) : i >= 0; i += forward ? 1 : -1) {
    const char ch = text[static_cast<std::size_t>(i)];
    if ((forward && ch == open) || (!forward && ch == close)) {
      ++depth;
    } else if ((forward && ch == close) || (!forward && ch == open)) {
      if (depth == 0) {
        match = i;
        break;
      }
      --depth;
    }
  }
  return match < 0 ? std::nullopt
                   : std::optional<Position>(offset_to_pos(buf.lines, static_cast<std::size_t>(match)));
}

bool paragraph_blank(const std::vector<std::string>& lines, int line) {
  return lines[static_cast<std::size_t>(std::max(0, line))].find_first_not_of(" \t\r\n") == std::string::npos;
}

int paragraph_boundary(const std::vector<std::string>& lines, int line, bool forward) {
  const int last = static_cast<int>(lines.size()) - 1;
  if (last < 0) return 0;
  int i = std::max(0, std::min(line, last));
  if (forward) {
    if (i < last && !paragraph_blank(lines, i)) {
      while (i < last && !paragraph_blank(lines, i + 1)) ++i;
      return i < last ? i + 1 : last;
    }
    while (i < last && paragraph_blank(lines, i)) ++i;
    while (i < last && !paragraph_blank(lines, i)) ++i;
    return i;
  }
  if (i > 0 && !paragraph_blank(lines, i)) {
    while (i > 0 && !paragraph_blank(lines, i - 1)) --i;
    return i > 0 ? i - 1 : 0;
  }
  while (i > 0 && paragraph_blank(lines, i)) --i;
  while (i > 0 && !paragraph_blank(lines, i - 1)) --i;
  return i > 0 ? i - 1 : 0;
}

}  // namespace

std::optional<Motion> VimState::ResolveMotion(std::string_view key, const Buffer& buf) {
  const int count = count_.empty() ? 1 : std::stoi(count_);
  const std::string& line = buf.lines[static_cast<std::size_t>(std::max(0, buf.cursor_line))];
  const auto at = [&](int col) { return Position{buf.cursor_line, col}; };
  const std::string k = key == " " ? "l" : std::string(key);
  if (k == "h" || k == "l") {
    int col = buf.cursor_col;
    for (int i = 0; i < count; ++i) {
      col = static_cast<int>(k == "h" ? prev_grapheme_start(line, static_cast<std::size_t>(col))
                                      : next_grapheme_start(line, static_cast<std::size_t>(col)));
    }
    return Motion{at(col), false, false};
  }
  if (k == "j" || k == "k") {
    const int target = std::max(0, std::min(buf.cursor_line + (k == "j" ? count : -count), static_cast<int>(buf.lines.size()) - 1));
    if (!desired_col_) desired_col_ = buf.cursor_col;
    return Motion{{target, static_cast<int>(std::min<std::int64_t>(*desired_col_, INT32_MAX))}, false, true};
  }
  if (k == "0") return Motion{at(0), false, false};
  if (k == "^") return Motion{at(static_cast<int>(first_non_blank(line))), false, false};
  if (k == "$") {
    desired_col_ = kStickyEnd;
    return Motion{at(static_cast<int>(line.size())), false, false};
  }
  if (k == "w") {
    int col = buf.cursor_col;
    if (operator_ == Operator::Change && !at_space(line, static_cast<std::size_t>(col))) {
      for (int i = 0; i < count; ++i) col = word_end(line, col);
      return Motion{at(col), true, false};
    }
    for (int i = 0; i < count; ++i) col = word_forward(line, col);
    return Motion{at(col), false, false};
  }
  if (k == "b") {
    int col = buf.cursor_col;
    for (int i = 0; i < count; ++i) col = static_cast<int>(move_word_left(line, static_cast<std::size_t>(col)));
    return Motion{at(col), false, false};
  }
  if (k == "e") {
    int col = buf.cursor_col;
    for (int i = 0; i < count; ++i) col = word_end(line, col);
    return Motion{at(col), true, false};
  }
  if (k == "%") {
    if (!count_.empty()) {
      if (count > 100) return std::nullopt;
      const int target = std::max(0, (count * static_cast<int>(buf.lines.size()) + 99) / 100 - 1);
      return Motion{{target, static_cast<int>(first_non_blank(buf.lines[static_cast<std::size_t>(target)]))}, false, true};
    }
    const std::optional<Position> to = match_delimiter(buf);
    return to ? std::optional<Motion>(Motion{*to, true, false}) : std::nullopt;
  }
  if (k == "{" || k == "}") {
    int line_no = buf.cursor_line;
    for (int n = 0; n < count; ++n) {
      const int next = paragraph_boundary(buf.lines, line_no, k == "}");
      if (next == line_no) break;
      line_no = next;
    }
    const std::string& text = buf.lines[static_cast<std::size_t>(line_no)];
    const bool at_end = k == "}" && line_no == static_cast<int>(buf.lines.size()) - 1 &&
                        text.find_first_not_of(" \t\r\n") != std::string::npos;
    return Motion{{line_no, at_end ? static_cast<int>(text.size()) : 0}, false, false};
  }
  if (k == "G") {
    const int target = count_.empty() ? static_cast<int>(buf.lines.size()) - 1 : count - 1;
    return Motion{{std::max(0, std::min(target, static_cast<int>(buf.lines.size()) - 1)), 0}, false, true};
  }
  if (k == ";" || k == ",") return RepeatFind(k == ";", buf, count);
  return std::nullopt;
}

std::optional<Motion> VimState::RepeatFind(bool same_direction, const Buffer& buf, int count) {
  if (!last_find_) return std::nullopt;
  const LastFind last = *last_find_;
  const bool forward = same_direction ? last.forward : !last.forward;
  const std::string& text = buf.lines[static_cast<std::size_t>(std::max(0, buf.cursor_line))];
  const int from_col = last.till
                           ? static_cast<int>(forward ? next_grapheme_start(text, static_cast<std::size_t>(buf.cursor_col))
                                                      : prev_grapheme_start(text, static_cast<std::size_t>(buf.cursor_col)))
                           : buf.cursor_col;
  return Seek(last.target, buf, forward, last.till, count, from_col);
}

std::optional<Motion> VimState::Seek(std::string_view target, const Buffer& buf, bool forward, bool till, int count,
                                     int from_col) {
  const std::string& text = buf.lines[static_cast<std::size_t>(std::max(0, buf.cursor_line))];
  const int match = find_grapheme(text, from_col, target, forward, count);
  if (match < 0) return std::nullopt;
  const int land = till ? static_cast<int>(forward ? prev_grapheme_start(text, static_cast<std::size_t>(match))
                                                   : next_grapheme_start(text, static_cast<std::size_t>(match)))
                        : match;
  if (land == buf.cursor_col) return std::nullopt;
  return Motion{{buf.cursor_line, land}, true, false};
}

std::vector<Command> VimState::ApplyMotion(const Buffer& buf, const Motion& motion) {
  const Operator op = operator_;
  operator_ = Operator::None;
  TakeCount();
  if (op == Operator::Indent || op == Operator::Outdent) {
    return {Command{.kind = CommandKind::Indent,
                    .from_line = std::min(buf.cursor_line, motion.to.line),
                    .to_line = std::max(buf.cursor_line, motion.to.line),
                    .out = op == Operator::Outdent}};
  }
  if (op == Operator::None) {
    const Position to = (Visual() || mode == Mode::Replace)
                            ? Position{motion.to.line, std::min(motion.to.col,
                                                                static_cast<int>(buf.lines[static_cast<std::size_t>(std::max(0, motion.to.line))].size()))}
                            : ClampNormal(buf, motion.to);
    return {Command{.kind = CommandKind::Move, .to = to}};
  }
  Position start{buf.cursor_line, buf.cursor_col};
  Position end = motion.to;
  if (end.line < start.line || (end.line == start.line && end.col < start.col)) std::swap(start, end);
  if (motion.inclusive) {
    const std::string& text = buf.lines[static_cast<std::size_t>(std::max(0, end.line))];
    end = {end.line, static_cast<int>(next_grapheme_start(text, static_cast<std::size_t>(end.col)))};
  }
  return Operate(op, start, end, motion.linewise);
}

std::vector<Command> VimState::Operate(Operator op, Position from, Position to, bool linewise) {
  if (op == Operator::Indent || op == Operator::Outdent) {
    return {Command{.kind = CommandKind::Indent,
                    .from_line = std::min(from.line, to.line),
                    .to_line = std::max(from.line, to.line),
                    .out = op == Operator::Outdent}};
  }
  if (op == Operator::Yank) {
    return {Command{.kind = CommandKind::Yank, .from = from, .to = to, .linewise = linewise},
            Command{.kind = CommandKind::Move, .to = from}};
  }
  if (op != Operator::Change) {
    return {Command{.kind = CommandKind::Delete, .from = from, .to = to, .linewise = linewise}};
  }
  mode = Mode::Insert;
  return {Command{.kind = CommandKind::Move, .to = from},
          Command{.kind = CommandKind::Delete, .from = from, .to = to, .insert = true},
          Command{.kind = CommandKind::Mode, .mode = Mode::Insert}};
}

std::vector<Command> VimState::HandleNormalKey(std::string_view key, const Buffer& buf) {
  const std::string& line = buf.lines[static_cast<std::size_t>(std::max(0, buf.cursor_line))];
  const int count = count_.empty() ? 1 : std::stoi(count_);
  const std::string k(key);
  const auto op_of = [](char ch) {
    return ch == 'd' ? Operator::Delete : ch == 'y' ? Operator::Yank : ch == 'c' ? Operator::Change : Operator::None;
  };
  const auto doubled = [&](Operator op) {
    const int span = TakeCount();
    const int last = std::min(buf.cursor_line + span - 1, static_cast<int>(buf.lines.size()) - 1);
    operator_ = Operator::None;
    return Operate(op, {buf.cursor_line, 0},
                   {last, static_cast<int>(buf.lines[static_cast<std::size_t>(last)].size())}, true);
  };
  if (k == "g") {
    pending_g_ = true;
    return {};
  }
  if (k == "i" || k == "a" || k == "I" || k == "A" || k == "o" || k == "O") {
    TakeCount();
    mode = Mode::Insert;
    const Command enter{.kind = CommandKind::Mode, .mode = Mode::Insert};
    if (k == "i") return {enter};
    if (k == "a") return {enter, Command{.kind = CommandKind::Move, .to = {buf.cursor_line, static_cast<int>(next_grapheme_start(line, static_cast<std::size_t>(buf.cursor_col)))}}};
    if (k == "I") return {enter, Command{.kind = CommandKind::Move, .to = {buf.cursor_line, static_cast<int>(first_non_blank(line))}}};
    if (k == "A") return {enter, Command{.kind = CommandKind::Move, .to = {buf.cursor_line, static_cast<int>(line.size())}}};
    return {Command{.kind = CommandKind::OpenLine, .below = k == "o"}, enter};
  }
  if (k == "v" || k == "V") {
    TakeCount();
    mode = k == "v" ? Mode::Visual : Mode::VisualLine;
    anchor = Position{buf.cursor_line, buf.cursor_col};
    return {Command{.kind = CommandKind::Mode, .mode = mode}};
  }
  if (k == "x") {
    TakeCount();
    int col = buf.cursor_col;
    for (int i = 0; i < count && static_cast<std::size_t>(col) < line.size(); ++i) {
      col = static_cast<int>(next_grapheme_start(line, static_cast<std::size_t>(col)));
    }
    if (col == buf.cursor_col) return {};
    return {Command{.kind = CommandKind::Delete, .from = {buf.cursor_line, buf.cursor_col}, .to = {buf.cursor_line, col}}};
  }
  if (k == "D" || k == "C") {
    const int span = TakeCount();
    const int last = std::min(buf.cursor_line + span - 1, static_cast<int>(buf.lines.size()) - 1);
    return Operate(k == "C" ? Operator::Change : Operator::Delete, {buf.cursor_line, buf.cursor_col},
                   {last, static_cast<int>(buf.lines[static_cast<std::size_t>(last)].size())}, false);
  }
  if (k == "d" || k == "y" || k == "c") {
    const Operator op = op_of(k[0]);
    if (operator_ == op) return doubled(op);
    operator_ = op;
    return {};
  }
  if (k == "p" || k == "P") {
    return {Command{.kind = CommandKind::Paste, .count = TakeCount(), .after = k == "p"}};
  }
  if (k == "u") {
    TakeCount();
    return {Command{.kind = CommandKind::Undo}};
  }
  if (k == ".") {
    if (operator_ != Operator::None) {
      ClearPending();
      return {};
    }
    return Repeat(buf, count_.empty() ? std::nullopt : std::optional<int>(TakeCount()));
  }
  if (k == "J") {
    if (operator_ != Operator::None) {
      ClearPending();
      return {};
    }
    return {Command{.kind = CommandKind::Join, .from_line = buf.cursor_line, .lines = std::max(2, TakeCount())}};
  }
  if (k == ">" || k == "<") {
    const Operator op = k == ">" ? Operator::Indent : Operator::Outdent;
    if (operator_ == op) {
      const int span = TakeCount();
      const int last = std::min(buf.cursor_line + span - 1, static_cast<int>(buf.lines.size()) - 1);
      operator_ = Operator::None;
      return {Command{.kind = CommandKind::Indent, .from_line = buf.cursor_line, .to_line = last, .out = k == "<"}};
    }
    if (operator_ != Operator::None) {
      ClearPending();
      return {};
    }
    operator_ = op;
    return {};
  }
  ClearPending();
  return {};
}

std::vector<Command> VimState::HandleVisualKey(std::string_view key, const Buffer& buf) {
  const Position anchor_pos = anchor.value_or(Position{buf.cursor_line, buf.cursor_col});
  const bool linewise = mode == Mode::VisualLine;
  const std::string k(key);
  if (k == "v" || k == "V") {
    const Mode next = k == "v" ? Mode::Visual : Mode::VisualLine;
    if (mode == next) {
      mode = Mode::Normal;
      anchor.reset();
      return {Command{.kind = CommandKind::Mode, .mode = Mode::Normal},
              Command{.kind = CommandKind::Move, .to = ClampNormal(buf, {buf.cursor_line, buf.cursor_col})}};
    }
    mode = next;
    return {Command{.kind = CommandKind::Mode, .mode = next}};
  }
  if (k == "o") {
    anchor = Position{buf.cursor_line, buf.cursor_col};
    return {Command{.kind = CommandKind::Move, .to = anchor_pos}};
  }
  if (k == "y" || k == "d" || k == "x" || k == "c" || k == "s" || k == ">" || k == "<" || k == "J") {
    const Selection range = visual_range(buf, anchor_pos, linewise);
    ClearPending();
    anchor.reset();
    if (k != "y") {
      VisualChange change;
      change.valid = true;
      change.key = k;
      change.linewise = linewise;
      change.lines = range.to.line - range.from.line + 1;
      change.graphemes = range.from.line == range.to.line
                             ? static_cast<int>(count_graphemes(
                                   buf.lines[static_cast<std::size_t>(range.from.line)].substr(
                                       static_cast<std::size_t>(range.from.col),
                                       static_cast<std::size_t>(range.to.col - range.from.col))))
                             : 0;
      change.end_col = range.to.col;
      Change record;
      record.visual = true;
      record.vchange = change;
      if (k == "c" || k == "s") {
        last_change_ = record;
        staged_change_.reset();
      } else {
        staged_change_ = record;
      }
    }
    return VisualEdit(k, range.from, range.to, linewise);
  }
  ClearPending();
  return {};
}

std::vector<Command> VimState::VisualEdit(std::string_view key, Position from, Position to, bool linewise) {
  const std::string k(key);
  if (k == ">" || k == "<" || k == "J") {
    mode = Mode::Normal;
    if (k == "J") {
      return {Command{.kind = CommandKind::Join, .from_line = from.line, .lines = std::max(2, to.line - from.line + 1)},
              Command{.kind = CommandKind::Mode, .mode = Mode::Normal}};
    }
    return {Command{.kind = CommandKind::Indent, .from_line = from.line, .to_line = to.line, .out = k == "<"},
            Command{.kind = CommandKind::Mode, .mode = Mode::Normal}};
  }
  const Operator op = k == "y" ? Operator::Yank : (k == "c" || k == "s" ? Operator::Change : Operator::Delete);
  if (op != Operator::Change) mode = Mode::Normal;
  std::vector<Command> commands = Operate(op, from, to, linewise);
  if (op != Operator::Change) commands.push_back(Command{.kind = CommandKind::Mode, .mode = Mode::Normal});
  return commands;
}

}  // namespace booth::vim
