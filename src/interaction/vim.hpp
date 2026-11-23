#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace booth::vim {

enum class Mode { Insert, Normal, Visual, VisualLine, Replace };

enum class Operator { None, Delete, Yank, Change, Indent, Outdent };

/// A buffer coordinate: line index and byte column within the UTF-8 line.
struct Position {
  int line{0};
  int col{0};

  bool operator==(const Position&) const = default;
};

/// Read-only snapshot the pure state machine resolves motions against.
struct Buffer {
  std::vector<std::string> lines;
  int cursor_line{0};
  int cursor_col{0};
};

/// What an Insert/Replace session typed relative to where it started; `.` replays it.
struct InsertRecord {
  int delete_before{0};
  int delete_after{0};
  std::string text;
};

enum class CommandKind { Move, Mode, Yank, Delete, OpenLine, Paste, Undo, Replace, Join, Indent, Retype };

/// An edit the editor applies. `to` is always an exclusive end.
struct Command {
  CommandKind kind{CommandKind::Move};
  Position from{};
  Position to{};
  Mode mode{Mode::Normal};
  InsertRecord record{};
  std::string text;
  int count{1};
  int from_line{0};
  int to_line{0};
  int lines{1};
  int times{1};
  bool linewise{false};
  bool insert{false};
  bool below{false};
  bool after{false};
  bool out{false};
  bool newline{false};
  bool overwrite{false};
};

/// End-exclusive span covered by a Visual selection.
struct Selection {
  Position from{};
  Position to{};
  bool linewise{false};
};

/// Start offset of the grapheme after `col`, clamped to `text.size()`.
std::size_t next_grapheme_start(std::string_view text, std::size_t col);
/// Start offset of the grapheme before `col`, clamped to 0.
std::size_t prev_grapheme_start(std::string_view text, std::size_t col);
/// Offset of the final grapheme — where a Normal-mode cursor rests.
std::size_t last_grapheme_start(std::string_view text);
/// Offset of the first non-blank grapheme, or 0 for a blank line.
std::size_t first_non_blank(std::string_view text);
std::size_t count_graphemes(std::string_view text);
/// Offset `count` graphemes forward or backward from `offset`, clamped.
std::size_t step_graphemes(std::string_view text, std::size_t offset, int count, bool forward);

/// What a session turned `before` into, anchored at its start `cursor`.
InsertRecord diff_insert_session(std::string_view before, std::size_t cursor, std::string_view after);

/// Buffer flattened to one string plus the cursor's offset in it, so scans can cross lines.
std::pair<std::string, std::size_t> flatten(const Buffer& buf);
Position offset_to_pos(const std::vector<std::string>& lines, std::size_t offset);
/// Normalized, end-exclusive span covered by a Visual selection.
Selection visual_range(const Buffer& buf, Position anchor, bool linewise);

/// Coarse Unicode word kind used by `w`/`b`/`e` (mirrors OMP's word navigation).
enum class WordKind { Whitespace, Delimiter, Cjk, Word, Other };
WordKind word_kind(std::string_view grapheme);
/// One coarse word step, or `count` steps when `count` > 1.
std::size_t move_word_left(std::string_view text, std::size_t cursor);
std::size_t move_word_right(std::string_view text, std::size_t cursor);

struct Motion {
  Position to{};
  /// Inclusive motions cover the grapheme under `to` when used with an operator.
  bool inclusive{false};
  bool linewise{false};
};

struct TextObjectRange {
  Position from{};
  Position to{};
  bool linewise{false};
};

struct VisualChange {
  bool valid{false};
  std::string key;
  bool linewise{false};
  int lines{1};
  int graphemes{0};
  int end_col{0};
  std::optional<InsertRecord> insert;
};

/// The last change `.` repeats.
struct Change {
  bool visual{false};
  std::vector<std::string> keys;
  std::optional<int> count;
  std::optional<InsertRecord> insert;
  VisualChange vchange;
};

/// Pure modal-editing state machine, ported from OMP's `vim.ts`.
class VimState {
 public:
  Mode mode{Mode::Normal};
  /// Fixed end of a Visual selection; the cursor is the moving end.
  std::optional<Position> anchor;

  /// True while a count, operator, `g`, text-object, find, or `r` prefix is half-typed.
  [[nodiscard]] bool Pending() const;
  /// The half-typed command as Vim would echo it (`"2d"`, `"di"`, `"df"`, `"r"`).
  [[nodiscard]] std::string PendingText() const;
  [[nodiscard]] bool Visual() const;
  void Reset();

  /// Handles one key (`"escape"` or a single grapheme). `nullopt` means the key is not ours.
  std::optional<std::vector<Command>> HandleKey(std::string_view key, const Buffer& buf);

  /// Attaches what an Insert/Replace session typed to the change that opened it.
  void RecordInsert(const InsertRecord& record, bool after_arrow);
  /// Commits a staged change when the editor confirms text changed.
  void CommitChange();
  /// Discards a staged change when the command was a no-op.
  void DiscardChange();

 private:
  [[nodiscard]] bool IsCountKey(std::string_view key) const;
  void ClearPending();
  int TakeCount();
  [[nodiscard]] Position ClampNormal(const Buffer& buf, Position pos) const;
  std::vector<Command> Dispatch(std::string_view key, const Buffer& buf);
  std::optional<std::vector<Command>> HandleEscape(const Buffer& buf);
  std::optional<Motion> ResolveMotion(std::string_view key, const Buffer& buf);
  std::optional<Motion> RepeatFind(bool same_direction, const Buffer& buf, int count);
  std::optional<Motion> Seek(std::string_view target, const Buffer& buf, bool forward, bool till, int count,
                             int from_col);
  std::vector<Command> ApplyMotion(const Buffer& buf, const Motion& motion);
  std::vector<Command> Operate(Operator op, Position from, Position to, bool linewise);
  std::optional<TextObjectRange> ResolveTextObject(std::string_view key, bool around, const Buffer& buf, int count);
  std::vector<Command> ApplyTextObject(std::string_view key, const Buffer& buf);
  std::vector<Command> HandleNormalKey(std::string_view key, const Buffer& buf);
  std::vector<Command> HandleVisualKey(std::string_view key, const Buffer& buf);
  std::vector<Command> VisualEdit(std::string_view key, Position from, Position to, bool linewise);
  std::vector<Command> Repeat(const Buffer& buf, std::optional<int> dot_count);
  std::vector<Command> RepeatVisual(const VisualChange& change, const Buffer& buf);

  std::string count_;
  Operator operator_{Operator::None};
  bool pending_g_{false};
  std::string text_object_;
  std::optional<bool> find_forward_;  // set while `f`/`F`/`t`/`T` waits for a target
  std::optional<bool> find_till_;
  bool replace_char_{false};
  struct LastFind {
    bool forward{false};
    bool till{false};
    std::string target;
  };
  std::optional<LastFind> last_find_;

  /// Vim's desired column; `kStickyEnd` is `$`'s sticky end-of-line.
  static constexpr std::int64_t kStickyEnd = INT64_MAX;
  std::optional<std::int64_t> desired_col_;

  std::vector<std::string> cmd_keys_;
  std::string cmd_count_;
  std::optional<Change> last_change_;
  std::optional<Change> staged_change_;
};

}  // namespace booth::vim
