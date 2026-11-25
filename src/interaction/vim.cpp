#include "interaction/vim.hpp"

#include <algorithm>
#include <cstdint>
#include <memory>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include <unicode/brkiter.h>
#include <unicode/locid.h>
#include <unicode/uchar.h>
#include <unicode/uscript.h>
#include <unicode/utf8.h>

#if defined(__GNUC__)
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

namespace booth::vim {
namespace {

struct Span {
  std::size_t start;
  std::size_t end;
};

/// Grapheme clusters as UTF-8 byte spans. ICU's UText reports native (byte) indices.
std::vector<Span> grapheme_spans(std::string_view text) {
  std::vector<Span> spans;
  if (text.empty()) return spans;
  UErrorCode status = U_ZERO_ERROR;
  std::unique_ptr<icu::BreakIterator> it(icu::BreakIterator::createCharacterInstance(icu::Locale::getRoot(), status));
  if (U_SUCCESS(status) && it) {
    UText* ut = utext_openUTF8(nullptr, text.data(), static_cast<int64_t>(text.size()), &status);
    if (U_SUCCESS(status) && ut != nullptr) {
      it->setText(ut, status);
      if (U_SUCCESS(status)) {
        int32_t start = it->first();
        for (int32_t end = it->next(); end != icu::BreakIterator::DONE; start = end, end = it->next()) {
          spans.push_back({static_cast<std::size_t>(start), static_cast<std::size_t>(end)});
        }
        utext_close(ut);
        return spans;
      }
      utext_close(ut);
    }
  }
  // Defensive fallback: one span per code point when ICU cannot segment the text.
  int32_t i = 0;
  const int32_t n = static_cast<int32_t>(text.size());
  while (i < n) {
    const int32_t start = i;
    UChar32 cp = 0;
    U8_NEXT(text.data(), i, n, cp);
    if (i <= start) i = start + 1;
    spans.push_back({static_cast<std::size_t>(start), static_cast<std::size_t>(i)});
  }
  return spans;
}

std::vector<std::size_t> grapheme_boundaries(std::string_view text) {
  std::vector<std::size_t> bounds{0};
  for (const Span& span : grapheme_spans(text)) {
    if (span.end != bounds.back()) bounds.push_back(span.end);
  }
  if (bounds.back() != text.size()) bounds.push_back(text.size());
  return bounds;
}

UChar32 first_code_point(std::string_view text) {
  int32_t i = 0;
  UChar32 cp = 0;
  U8_NEXT(text.data(), i, static_cast<int32_t>(text.size()), cp);
  return cp < 0 ? 0 : cp;
}

bool is_white_space(std::string_view text) {
  int32_t i = 0;
  const int32_t n = static_cast<int32_t>(text.size());
  while (i < n) {
    UChar32 cp = 0;
    U8_NEXT(text.data(), i, n, cp);
    if (cp < 0 || !u_isUWhiteSpace(cp)) return false;
  }
  return true;
}

bool is_letter_or_number(UChar32 cp) { return (U_MASK(u_charType(cp)) & (U_GC_L_MASK | U_GC_N_MASK)) != 0; }

bool is_punctuation_or_symbol(UChar32 cp) {
  return u_ispunct(cp) != 0 || (U_MASK(u_charType(cp)) & (U_GC_P_MASK | U_GC_S_MASK)) != 0;
}

bool is_word_joiner(std::string_view grapheme) {
  const UChar32 cp = first_code_point(grapheme);
  return cp == U'\'' || cp == 0x2019 || cp == U'-' || cp == 0x2010 || cp == 0x2011;
}

Command mode_command(Mode mode) { return Command{.kind = CommandKind::Mode, .mode = mode}; }
Command move_command(Position to) { return Command{.kind = CommandKind::Move, .to = to}; }

bool is_insert_key(const std::string& key) {
  static const std::vector<std::string> keys{"i", "a", "I", "A", "o", "O", "R"};
  return std::find(keys.begin(), keys.end(), key) != keys.end();
}

bool is_edit_kind(CommandKind kind) {
  switch (kind) {
    case CommandKind::Delete:
    case CommandKind::Replace:
    case CommandKind::Join:
    case CommandKind::Indent:
    case CommandKind::Paste:
    case CommandKind::OpenLine:
      return true;
    default:
      return false;
  }
}

const char* operator_text(Operator op) {
  switch (op) {
    case Operator::Delete: return "d";
    case Operator::Yank: return "y";
    case Operator::Change: return "c";
    case Operator::Indent: return ">";
    case Operator::Outdent: return "<";
    case Operator::None: return "";
  }
  return "";
}

}  // namespace

std::size_t next_grapheme_start(std::string_view text, std::size_t col) {
  if (col >= text.size()) return text.size();
  for (const Span& span : grapheme_spans(text)) {
    if (span.start >= col) return std::min(span.end, text.size());
  }
  return text.size();
}

std::size_t prev_grapheme_start(std::string_view text, std::size_t col) {
  if (col == 0) return 0;
  std::size_t last = 0;
  for (const Span& span : grapheme_spans(text)) {
    if (span.start >= col) break;
    last = span.start;
  }
  return last;
}

std::size_t last_grapheme_start(std::string_view text) { return prev_grapheme_start(text, text.size()); }

std::size_t first_non_blank(std::string_view text) {
  for (const Span& span : grapheme_spans(text)) {
    if (!is_white_space(text.substr(span.start, span.end - span.start))) return span.start;
  }
  return 0;
}

std::size_t count_graphemes(std::string_view text) { return grapheme_spans(text).size(); }

std::size_t step_graphemes(std::string_view text, std::size_t offset, int count, bool forward) {
  if (count <= 0) return offset;
  std::vector<std::size_t> bounds;
  for (const Span& span : grapheme_spans(text)) bounds.push_back(span.start);
  bounds.push_back(text.size());
  std::size_t i = 0;
  while (i + 1 < bounds.size() && bounds[i + 1] <= offset) ++i;
  const std::size_t back = static_cast<std::size_t>(count);
  if (forward) return bounds[std::min(i + back, bounds.size() - 1)];
  const std::size_t base = i - std::min(i, back) + ((i >= back && bounds[i] < offset) ? 1 : 0);
  return bounds[std::min(base, bounds.size() - 1)];
}

InsertRecord diff_insert_session(std::string_view before, std::size_t cursor, std::string_view after) {
  const std::vector<std::size_t> bb = grapheme_boundaries(before);
  const std::vector<std::size_t> ab = grapheme_boundaries(after);
  std::size_t cursor_bound = 0;
  while (cursor_bound + 1 < bb.size() && bb[cursor_bound + 1] <= cursor) ++cursor_bound;
  std::size_t prefix = 0;
  while (prefix < cursor_bound && prefix + 1 < ab.size() &&
         before.substr(bb[prefix], bb[prefix + 1] - bb[prefix]) == after.substr(ab[prefix], ab[prefix + 1] - ab[prefix])) {
    ++prefix;
  }
  const std::size_t before_end = bb.size() - 1;
  const std::size_t after_end = ab.size() - 1;
  std::size_t suffix = 0;
  while (before_end - suffix > cursor_bound && after_end - suffix > prefix &&
         before.substr(bb[before_end - suffix - 1], bb[before_end - suffix] - bb[before_end - suffix - 1]) ==
             after.substr(ab[after_end - suffix - 1], ab[after_end - suffix] - ab[after_end - suffix - 1])) {
    ++suffix;
  }
  return InsertRecord{static_cast<int>(cursor_bound - prefix), static_cast<int>(before_end - suffix - cursor_bound),
                      std::string(after.substr(ab[prefix], ab[after_end - suffix] - ab[prefix]))};
}

std::pair<std::string, std::size_t> flatten(const Buffer& buf) {
  std::string text;
  std::size_t cursor = static_cast<std::size_t>(std::max(0, buf.cursor_col));
  for (int i = 0; i < buf.cursor_line && i < static_cast<int>(buf.lines.size()); ++i) cursor += buf.lines[static_cast<std::size_t>(i)].size() + 1;
  for (std::size_t i = 0; i < buf.lines.size(); ++i) {
    if (i != 0) text += '\n';
    text += buf.lines[i];
  }
  return {std::move(text), cursor};
}

Position offset_to_pos(const std::vector<std::string>& lines, std::size_t offset) {
  std::size_t remaining = offset;
  for (std::size_t line = 0; line < lines.size(); ++line) {
    if (remaining <= lines[line].size()) return {static_cast<int>(line), static_cast<int>(remaining)};
    remaining -= lines[line].size() + 1;
  }
  return lines.empty() ? Position{0, 0}
                       : Position{static_cast<int>(lines.size() - 1), static_cast<int>(lines.back().size())};
}

Selection visual_range(const Buffer& buf, Position anchor, bool linewise) {
  Position from = anchor;
  Position to{static_cast<int>(buf.cursor_line), static_cast<int>(buf.cursor_col)};
  if (to.line < from.line || (to.line == from.line && to.col < from.col)) std::swap(from, to);
  const int last_line = std::max(0, linewise ? std::min(to.line, static_cast<int>(buf.lines.size()) - 1) : to.line);
  const std::string& text = buf.lines[static_cast<std::size_t>(last_line)];
  if (linewise) return {{from.line, 0}, {last_line, static_cast<int>(text.size())}, true};
  return {from, {to.line, static_cast<int>(next_grapheme_start(text, static_cast<std::size_t>(to.col)))}, false};
}

WordKind word_kind(std::string_view grapheme) {
  if (grapheme.empty()) return WordKind::Other;
  const UChar32 cp = first_code_point(grapheme);
  if (cp == 0) return WordKind::Other;
  if (u_isUWhiteSpace(cp)) return WordKind::Whitespace;
  if (cp == U'_') return WordKind::Word;
  if (is_punctuation_or_symbol(cp)) return WordKind::Delimiter;
  UErrorCode status = U_ZERO_ERROR;
  const UScriptCode script = uscript_getScript(cp, &status);
  if (U_SUCCESS(status) && (script == USCRIPT_HAN || script == USCRIPT_HIRAGANA || script == USCRIPT_KATAKANA || script == USCRIPT_HANGUL)) {
    return WordKind::Cjk;
  }
  return is_letter_or_number(cp) ? WordKind::Word : WordKind::Other;
}

std::size_t move_word_right(std::string_view text, std::size_t cursor) {
  if (text.empty()) return 0;
  std::size_t i = std::min(cursor, text.size());
  if (i == text.size()) return text.size();
  const std::vector<Span> spans = grapheme_spans(text);
  std::size_t idx = 0;
  while (idx < spans.size() && spans[idx].start < i) ++idx;
  const auto seg = [&](std::size_t at) { return text.substr(spans[at].start, spans[at].end - spans[at].start); };
  while (idx < spans.size() && word_kind(seg(idx)) == WordKind::Whitespace) i = spans[idx++].end;
  if (idx >= spans.size()) return i;
  const WordKind first = word_kind(seg(idx));
  if (first == WordKind::Delimiter || first == WordKind::Cjk) {
    while (idx < spans.size() && word_kind(seg(idx)) == first) i = spans[idx++].end;
    return i;
  }
  if (first == WordKind::Word) {
    bool has_left = false;
    while (idx < spans.size()) {
      const WordKind kind = word_kind(seg(idx));
      if (kind == WordKind::Word) {
        has_left = true;
        i = spans[idx++].end;
        continue;
      }
      if (has_left && kind == WordKind::Delimiter && is_word_joiner(seg(idx)) && idx + 1 < spans.size() &&
          word_kind(seg(idx + 1)) == WordKind::Word) {
        i = spans[idx].end;
        ++idx;
        continue;
      }
      break;
    }
    return i;
  }
  return spans[idx].end;
}

std::size_t move_word_left(std::string_view text, std::size_t cursor) {
  if (text.empty()) return 0;
  std::size_t i = std::min(cursor, text.size());
  if (i == 0) return 0;
  std::vector<Span> graphemes;
  for (const Span& span : grapheme_spans(text)) {
    if (span.end > i) break;
    graphemes.push_back(span);
  }
  const auto seg = [&](const Span& span) { return text.substr(span.start, span.end - span.start); };
  while (!graphemes.empty() && word_kind(seg(graphemes.back())) == WordKind::Whitespace) {
    i = graphemes.back().start;
    graphemes.pop_back();
  }
  if (i == 0 || graphemes.empty()) return i;
  const WordKind kind = word_kind(seg(graphemes.back()));
  if (kind == WordKind::Delimiter || kind == WordKind::Cjk) {
    while (!graphemes.empty() && word_kind(seg(graphemes.back())) == kind) {
      i = graphemes.back().start;
      graphemes.pop_back();
    }
    return i;
  }
  bool has_right = false;
  while (kind == WordKind::Word && !graphemes.empty()) {
    const WordKind current = word_kind(seg(graphemes.back()));
    if (current == WordKind::Word) {
      has_right = true;
      i = graphemes.back().start;
      graphemes.pop_back();
      continue;
    }
    if (has_right && current == WordKind::Delimiter && is_word_joiner(seg(graphemes.back())) && graphemes.size() >= 2 &&
        word_kind(seg(graphemes[graphemes.size() - 2])) == WordKind::Word) {
      i = graphemes.back().start;
      graphemes.pop_back();
      continue;
    }
    break;
  }
  if (kind == WordKind::Word) return i;
  return graphemes.empty() ? i : graphemes.back().start;
}

bool VimState::Pending() const {
  return register_pending_ || register_ != 0 || !count_.empty() || operator_ != Operator::None || pending_g_ ||
         !text_object_.empty() || find_forward_ || replace_char_;
}

std::string VimState::PendingText() const {
  std::string text;
  if (register_pending_) text += "\"";
  else if (register_ != 0) text += std::string("\"") + register_;
  text += count_ + operator_text(operator_);
  if (pending_g_) text += "g";
  text += text_object_;
  if (find_forward_) text += find_till_.value_or(false) ? (*find_forward_ ? "t" : "T") : (*find_forward_ ? "f" : "F");
  if (replace_char_) text += "r";
  return text;
}

bool VimState::Visual() const { return mode == Mode::Visual || mode == Mode::VisualLine; }

Position VimState::ClampNormal(const Buffer& buf, Position pos) const {
  const int line = std::max(0, std::min(pos.line, static_cast<int>(buf.lines.size()) - 1));
  const std::string& text = buf.lines[static_cast<std::size_t>(line)];
  return {line, std::max(0, std::min(pos.col, static_cast<int>(last_grapheme_start(text))))};
}

std::optional<std::vector<Command>> VimState::HandleKey(std::string_view key, const Buffer& buf) {
  if (key == "escape") {
    cmd_keys_.clear();
    cmd_count_.clear();
    return HandleEscape(buf);
  }
  if (mode == Mode::Insert) return std::nullopt;
  const bool recording = mode == Mode::Normal;
  const bool dot_is_target = find_forward_.has_value() || replace_char_;
  if (recording) (IsCountKey(key) ? cmd_count_ += std::string(key) : cmd_keys_.emplace_back(key));
  const char captured_register = register_;
  std::vector<Command> commands = Dispatch(key, buf);
  const bool command_pending = !count_.empty() || operator_ != Operator::None || pending_g_ ||
                               !text_object_.empty() || find_forward_ || replace_char_;
  if (captured_register != 0 && !command_pending) {
    for (Command& command : commands) {
      if (command.kind == CommandKind::Yank || command.kind == CommandKind::Delete ||
          command.kind == CommandKind::Paste) {
        command.reg = captured_register;
      }
    }
    register_ = 0;
  }
  if (recording && !Pending()) {
    const bool editing = mode == Mode::Insert || mode == Mode::Replace;
    if ((key != "." || dot_is_target) &&
        (editing || std::any_of(commands.begin(), commands.end(),
                                [](const Command& command) { return is_edit_kind(command.kind); }))) {
      Change change;
      change.keys = cmd_keys_;
      if (!(is_insert_key(cmd_keys_.front())) && !cmd_count_.empty()) change.count = std::stoi(cmd_count_);
      if (editing) {
        last_change_ = change;
        staged_change_.reset();
      } else {
        staged_change_ = change;
      }
    }
    cmd_keys_.clear();
    cmd_count_.clear();
  }
  return commands;
}

void VimState::RecordInsert(const InsertRecord& record, bool after_arrow) {
  if (after_arrow) {
    last_change_ = Change{.keys = {"i"}, .insert = record};
  } else if (last_change_) {
    last_change_->insert = record;
  } else if (!record.text.empty() || record.delete_before > 0 || record.delete_after > 0) {
    last_change_ = Change{.keys = {"i"}, .insert = record};
  }
}

void VimState::CommitChange() {
  if (staged_change_) {
    last_change_ = staged_change_;
    staged_change_.reset();
  }
}

void VimState::DiscardChange() { staged_change_.reset(); }

std::optional<std::vector<Command>> VimState::HandleEscape(const Buffer& buf) {
  if (Pending()) {
    ClearPending();
    return std::vector<Command>{};
  }
  if (Visual()) {
    mode = Mode::Normal;
    anchor.reset();
    return std::vector<Command>{mode_command(Mode::Normal), move_command(ClampNormal(buf, {buf.cursor_line, buf.cursor_col}))};
  }
  if (mode == Mode::Insert || mode == Mode::Replace) {
    mode = Mode::Normal;
    const std::string& text = buf.lines[static_cast<std::size_t>(std::max(0, buf.cursor_line))];
    return std::vector<Command>{mode_command(Mode::Normal),
                                move_command({buf.cursor_line, static_cast<int>(prev_grapheme_start(text, static_cast<std::size_t>(buf.cursor_col)))})};
  }
  return std::nullopt;
}

std::vector<Command> VimState::Dispatch(std::string_view key, const Buffer& buf) {
  const std::string k(key);
  const auto take_count = [&]() { return count_.empty() ? 1 : std::max(1, std::stoi(count_)); };
  if (find_forward_) {
    const bool forward = *find_forward_;
    const bool till = find_till_.value_or(false);
    find_forward_.reset();
    find_till_.reset();
    desired_col_.reset();
    last_find_ = LastFind{forward, till, k};
    const int count = take_count();
    const std::optional<Motion> motion = Seek(k, buf, forward, till, count, buf.cursor_col);
    if (!motion) {
      operator_ = Operator::None;
      count_.clear();
      return {};
    }
    return ApplyMotion(buf, *motion);
  }
  if (replace_char_) {
    replace_char_ = false;
    const int count = take_count();
    const std::string& line = buf.lines[static_cast<std::size_t>(std::max(0, buf.cursor_line))];
    int end = -1;
    int taken = 0;
    for (std::size_t at = 0; at < line.size(); at = next_grapheme_start(line, at)) {
      if (static_cast<int>(at) < buf.cursor_col) continue;
      if (++taken == count) {
        end = static_cast<int>(next_grapheme_start(line, at));
        break;
      }
    }
    count_.clear();
    if (end < 0) return {};
    std::string text;
    for (int i = 0; i < count; ++i) text += k;
    return {Command{.kind = CommandKind::Replace, .from = {buf.cursor_line, buf.cursor_col}, .to = {buf.cursor_line, end}, .text = text}};
  }
  if (register_pending_) {
    register_pending_ = false;
    if (k.size() == 1) register_ = k[0];
    return {};
  }
  if (IsCountKey(key)) {
    count_ += k;
    return {};
  }
  if (pending_g_) {
    pending_g_ = false;
    if (k != "g") {
      ClearPending();
      return {};
    }
    desired_col_.reset();
    const int line = std::min(TakeCount() - 1, static_cast<int>(buf.lines.size()) - 1);
    return ApplyMotion(buf, Motion{{line, 0}, false, true});
  }
  if (!text_object_.empty()) {
    desired_col_.reset();
    return ApplyTextObject(k, buf);
  }
  if (k == "\"" && operator_ == Operator::None) {
    register_pending_ = true;
    return {};
  }
  if ((k == "i" || k == "a") && (operator_ != Operator::None || Visual())) {
    text_object_ = k;
    return {};
  }
  if (k == "f" || k == "F" || k == "t" || k == "T") {
    desired_col_.reset();
    find_forward_ = k == "f" || k == "t";
    find_till_ = k == "t" || k == "T";
    return {};
  }
  if ((k == "r" || k == "R") && operator_ == Operator::None && !Visual()) {
    desired_col_.reset();
    if (k == "R") {
      TakeCount();
      mode = Mode::Replace;
      return {mode_command(Mode::Replace)};
    }
    replace_char_ = true;
    return {};
  }
  if (k != "j" && k != "k") desired_col_.reset();
  const std::optional<Motion> motion = ResolveMotion(k, buf);
  if (motion) return ApplyMotion(buf, *motion);
  return Visual() ? HandleVisualKey(k, buf) : HandleNormalKey(k, buf);
}

std::vector<Command> VimState::Repeat(const Buffer& buf, std::optional<int> dot_count) {
  if (!last_change_) return {};
  const Change change = *last_change_;
  std::vector<Command> commands;
  std::optional<int> count;
  if (!change.visual) {
    count = dot_count ? dot_count : change.count;
    if (count) {
      for (const char digit : std::to_string(*count)) commands = Dispatch(std::string(1, digit), buf);
    }
    for (const std::string& key : change.keys) commands = Dispatch(key, buf);
    Change updated = change;
    updated.count = count;
    last_change_ = updated;
  } else {
    commands = RepeatVisual(change.vchange, buf);
  }
  if (mode == Mode::Insert || mode == Mode::Replace) {
    const bool insert_key = !change.visual && !change.keys.empty() && is_insert_key(change.keys.front());
    Command retype{.kind = CommandKind::Retype,
                   .record = change.insert.value_or(InsertRecord{}),
                   .times = insert_key ? (count ? *count : 1) : 1,
                   .newline = insert_key && (change.keys.front() == "o" || change.keys.front() == "O"),
                   .overwrite = mode == Mode::Replace};
    commands.push_back(retype);
    mode = Mode::Normal;
    commands.push_back(mode_command(Mode::Normal));
  }
  return commands;
}

std::vector<Command> VimState::RepeatVisual(const VisualChange& change, const Buffer& buf) {
  const int from_line = buf.cursor_line;
  const int to_line = std::min(from_line + change.lines - 1, static_cast<int>(buf.lines.size()) - 1);
  const auto line_at = [&](int line) -> const std::string& { return buf.lines[static_cast<std::size_t>(line)]; };
  Position from;
  Position to;
  if (change.linewise) {
    from = {from_line, 0};
    to = {to_line, static_cast<int>(line_at(to_line).size())};
  } else if (change.lines == 1) {
    from = {buf.cursor_line, buf.cursor_col};
    to = {from_line, static_cast<int>(step_graphemes(line_at(from_line), static_cast<std::size_t>(buf.cursor_col), change.graphemes, true))};
  } else {
    from = {buf.cursor_line, buf.cursor_col};
    const std::string& text = line_at(to_line);
    const std::size_t target = std::min(static_cast<std::size_t>(change.end_col), text.size());
    std::size_t end_col = text.size();
    for (std::size_t at = 0; at < text.size(); at = next_grapheme_start(text, at)) {
      if (at >= target) {
        end_col = at;
        break;
      }
      end_col = next_grapheme_start(text, at);
    }
    to = {to_line, static_cast<int>(end_col)};
  }
  return VisualEdit(change.key, from, to, change.linewise);
}

}  // namespace booth::vim
