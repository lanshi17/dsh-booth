#include "interaction/vim.hpp"

#include <algorithm>
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

int char_class(std::string_view grapheme, bool big) {
  const WordKind kind = word_kind(grapheme);
  if (kind == WordKind::Whitespace) return 0;
  return (big || kind == WordKind::Word || kind == WordKind::Cjk) ? 1 : 2;
}

struct Chunk {
  int start;
  int end;
  int cls;
};

/// Split a line into maximal same-class runs — the atoms a counted `iw`/`aw` walks over.
std::vector<Chunk> chunk_line(std::string_view text, bool big) {
  std::vector<Chunk> chunks;
  for (const Span& span : spans_of(text)) {
    const int cls = char_class(text.substr(span.start, span.end - span.start), big);
    if (!chunks.empty() && chunks.back().cls == cls) {
      chunks.back().end = static_cast<int>(span.end);
    } else {
      chunks.push_back({static_cast<int>(span.start), static_cast<int>(span.end), cls});
    }
  }
  return chunks;
}

std::optional<std::pair<int, int>> word_object(std::string_view text, int col, bool around, bool big, int count) {
  const std::vector<Chunk> chunks = chunk_line(text, big);
  if (chunks.empty()) return std::nullopt;
  const int last = static_cast<int>(chunks.size()) - 1;
  int first = last;
  for (int i = 0; i <= last; ++i) {
    if (col < chunks[static_cast<std::size_t>(i)].end) {
      first = i;
      break;
    }
  }
  int end = first;
  if (!around) {
    end = std::min(first + count - 1, last);
  } else if (chunks[static_cast<std::size_t>(first)].cls == 0) {
    end = std::min(first + 2 * count - 1, last);
  } else {
    for (int n = 0; n < count; ++n) {
      if (n > 0 && end < last) ++end;
      if (end < last && chunks[static_cast<std::size_t>(end + 1)].cls == 0) ++end;
    }
    if (chunks[static_cast<std::size_t>(end)].cls != 0 && first > 0 &&
        chunks[static_cast<std::size_t>(first - 1)].cls == 0) {
      --first;
    }
  }
  return std::pair<int, int>{chunks[static_cast<std::size_t>(first)].start,
                             chunks[static_cast<std::size_t>(end)].end};
}

/// Quote columns, skipping a backslash and the character after it.
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

std::optional<std::pair<int, int>> quote_object(std::string_view text, int col, char quote, bool around) {
  const std::vector<int> marks = quote_marks(text, quote);
  for (std::size_t p = 0; p + 1 < marks.size(); p += 2) {
    const int open = marks[p];
    const int close = marks[p + 1];
    if (col > close) continue;
    if (!around) return std::pair<int, int>{open + 1, close};
    int end = close + 1;
    int start = open;
    while (static_cast<std::size_t>(end) < text.size() && at_space(text, static_cast<std::size_t>(end))) ++end;
    if (end == close + 1) {
      while (start > 0 && at_space(text, static_cast<std::size_t>(start - 1))) --start;
    }
    return std::pair<int, int>{start, end};
  }
  return std::nullopt;
}

std::optional<TextObjectRange> bracket_object(const Buffer& buf, char open, char close, bool around) {
  const auto [text, cursor] = flatten(buf);
  long long depth = 0;
  long long open_at = -1;
  for (long long i = std::min(static_cast<long long>(cursor), static_cast<long long>(text.size()) - 1); i >= 0; --i) {
    const char ch = text[static_cast<std::size_t>(i)];
    if (ch == close && i != static_cast<long long>(cursor)) {
      ++depth;
    } else if (ch == open) {
      if (depth == 0) {
        open_at = i;
        break;
      }
      --depth;
    }
  }
  if (open_at < 0) return std::nullopt;
  depth = 0;
  long long close_at = -1;
  for (long long i = open_at + 1; i < static_cast<long long>(text.size()); ++i) {
    const char ch = text[static_cast<std::size_t>(i)];
    if (ch == open) {
      ++depth;
    } else if (ch == close) {
      if (depth == 0) {
        close_at = i;
        break;
      }
      --depth;
    }
  }
  if (close_at < 0) return std::nullopt;
  return TextObjectRange{offset_to_pos(buf.lines, static_cast<std::size_t>(around ? open_at : open_at + 1)),
                         offset_to_pos(buf.lines, static_cast<std::size_t>(around ? close_at + 1 : close_at)), false};
}

std::optional<TextObjectRange> paragraph_object(const Buffer& buf, bool around) {
  const int last = static_cast<int>(buf.lines.size()) - 1;
  const auto blank = [&](int line) {
    return buf.lines[static_cast<std::size_t>(line)].find_first_not_of(" \t\r\n") == std::string::npos;
  };
  const bool target = blank(buf.cursor_line);
  int first = buf.cursor_line;
  int end = buf.cursor_line;
  while (first > 0 && blank(first - 1) == target) --first;
  while (end < last && blank(end + 1) == target) ++end;
  if (around) {
    const int stop = end;
    while (end < last && blank(end + 1) != target) ++end;
    if (end == stop) {
      while (first > 0 && blank(first - 1) != target) --first;
    }
  }
  return TextObjectRange{{first, 0}, {end, static_cast<int>(buf.lines[static_cast<std::size_t>(end)].size())}, true};
}

}  // namespace

std::optional<TextObjectRange> VimState::ResolveTextObject(std::string_view key, bool around, const Buffer& buf,
                                                           int count) {
  const std::string& line = buf.lines[static_cast<std::size_t>(std::max(0, buf.cursor_line))];
  const auto on_line = [&](std::optional<std::pair<int, int>> range) -> std::optional<TextObjectRange> {
    if (!range) return std::nullopt;
    return TextObjectRange{{buf.cursor_line, range->first}, {buf.cursor_line, range->second}, false};
  };
  const char ch = key.empty() ? 0 : key[0];
  switch (ch) {
    case 'w':
    case 'W':
      return on_line(word_object(line, buf.cursor_col, around, ch == 'W', count));
    case '"':
    case '\'':
    case '`':
      return on_line(quote_object(line, buf.cursor_col, ch, around));
    case '(':
    case ')':
    case 'b':
      return bracket_object(buf, '(', ')', around);
    case '[':
    case ']':
      return bracket_object(buf, '[', ']', around);
    case '{':
    case '}':
    case 'B':
      return bracket_object(buf, '{', '}', around);
    case '<':
    case '>':
      return bracket_object(buf, '<', '>', around);
    case 'p':
      return paragraph_object(buf, around);
    default:
      return std::nullopt;
  }
}

std::vector<Command> VimState::ApplyTextObject(std::string_view key, const Buffer& buf) {
  const bool around = text_object_ == "a";
  const Operator op = operator_;
  text_object_.clear();
  operator_ = Operator::None;
  const std::optional<TextObjectRange> range = ResolveTextObject(key, around, buf, TakeCount());
  if (!range) return {};
  if (op != Operator::None) return Operate(op, range->from, range->to, range->linewise);
  anchor = range->from;
  std::vector<Command> commands;
  if (range->linewise && mode == Mode::Visual) {
    mode = Mode::VisualLine;
    commands.push_back(Command{.kind = CommandKind::Mode, .mode = mode});
  }
  Position to = range->to;
  if (to.col > 0) {
    to = {to.line, static_cast<int>(prev_grapheme_start(buf.lines[static_cast<std::size_t>(std::max(0, to.line))],
                                                        static_cast<std::size_t>(to.col)))};
  } else if (to.line > range->from.line) {
    to = {to.line - 1, static_cast<int>(last_grapheme_start(buf.lines[static_cast<std::size_t>(to.line - 1)]))};
  } else {
    to = range->from;
  }
  commands.push_back(Command{.kind = CommandKind::Move, .to = to});
  return commands;
}

}  // namespace booth::vim
