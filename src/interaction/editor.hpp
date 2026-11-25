#pragma once

#include <cstddef>
#include <map>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include <ftxui/component/event.hpp>

#include "interaction/vim.hpp"

namespace booth {

/// A small multiline prompt editor: bytes are inserted/removed at a grapheme boundary.
/// Supports undo, word motions, kill/yank and line edits without a terminal dependency.
/// With Vim enabled it delegates modal editing to a pure `vim::VimState` and applies the
/// commands it returns, owning undo, the kill register and insert-session records.
class Editor {
 public:
  explicit Editor(std::string& content);

  /// Handles editing keys. Returns false for keys the caller owns (Enter submits).
  bool HandleEvent(const ftxui::Event& event);

  void Reset();
  void SetContent(const std::string& content);
  /// Inserts a paste as one undo group; large multi-line/long pastes collapse to a `[Paste #N]` marker.
  void Paste(const std::string& text);
  /// Inserts the paste literally, bypassing marker collapse.
  void PasteRaw(const std::string& text);
  /// Replaces every live `[Paste #N]` marker with its stored text.
  [[nodiscard]] std::string Expanded(const std::string& text) const;
  /// Places the caret at `offset`, snapped down to a grapheme boundary and clamped to the buffer.
  /// Splits any open Insert/Replace session so the next edit starts a fresh undo group.
  void SetCursor(std::size_t offset);
  [[nodiscard]] std::size_t CursorOffset() const { return cursor_; }

  /// Enables modal editing. Starts in Insert mode, as OMP does when the setting is flipped on.
  void EnableVim(bool enabled);
  [[nodiscard]] bool VimEnabled() const { return vim_ != nullptr; }
  [[nodiscard]] bool VimNormal() const;
  [[nodiscard]] vim::Mode VimMode() const;
  [[nodiscard]] bool VimConsumesEscape() const;
  [[nodiscard]] const std::string& VimPending() const;
  [[nodiscard]] std::optional<vim::Position> VimAnchor() const;
  /// Active Visual selection, end-exclusive, or nullopt when no selection is shown.
  [[nodiscard]] std::optional<vim::Selection> VimSelection() const;
  /// The same selection split into one end-exclusive span per line, ready to style row by row.
  [[nodiscard]] std::vector<vim::Selection> VimSelectionSpans() const;
  [[nodiscard]] std::size_t VimSelectedLines() const;

  /// Mode chrome, with the half-typed command appended (`NORMAL 2d`, `VISUAL`, `REPLACE`).
  [[nodiscard]] std::string ModeLabel() const;

  [[nodiscard]] std::vector<std::string> Lines() const;
  [[nodiscard]] std::size_t CursorLine() const;
  [[nodiscard]] std::size_t CursorColumn() const;

 private:
  struct Snapshot {
    std::string text;
    std::size_t cursor{0};
  };
  struct ReplaceEntry {
    int line{0};
    int col{0};
    std::string removed;
    std::string text;
  };

  bool HandleEditingEvent(const ftxui::Event& event);
  bool HandleVimEvent(const ftxui::Event& event);
  bool HandleVimInsert(const ftxui::Event& event);
  bool HandleVimReplace(const ftxui::Event& event);
  bool RunVimKey(const std::string& key);
  void ApplyVimCommands(const std::vector<vim::Command>& commands);
  void PushUndoSnapshot();
  void EnsureVimSession();
  void OpenVimSession(bool arrowed);
  void FinalizeVimSession();
  void BreakVimSession();
  void TypeInsertText(const std::string& text);
  void TypeReplaceGrapheme(const std::string& grapheme);
  void TrackVimSessionEdit(const std::string& before);
  void ReplaceBackspace();
  void ReplaceNewline();
  [[nodiscard]] vim::Buffer MakeVimBuffer() const;
  [[nodiscard]] bool VimEditing() const;
  [[nodiscard]] static bool IsCaretMove(const ftxui::Event& event);
  [[nodiscard]] static std::optional<std::string> MapVimNav(const ftxui::Event& event);

  void Insert(const std::string& text);
  void MoveVertical(int direction);
  void MoveWord(int direction);
  void Kill(std::size_t from, std::size_t to);
  void RememberKill(std::string text);
  void WriteVimRegister(std::string text, bool linewise, bool yank, char reg);
  [[nodiscard]] std::string ReadVimRegister(char reg) const;
  void Undo();
  void Redo();
  void BeforeEdit(bool typing);
  [[nodiscard]] std::size_t LineStart(std::size_t at) const;
  [[nodiscard]] std::size_t LineEnd(std::size_t at) const;
  [[nodiscard]] std::size_t WordStart(std::size_t at) const;
  [[nodiscard]] std::size_t WordEnd(std::size_t at) const;

  std::string& content_;
  std::size_t cursor_{0};
  std::vector<Snapshot> undo_;
  std::vector<Snapshot> redo_;
  std::vector<std::string> kill_ring_;
  std::map<char, std::string> vim_registers_;
  std::map<int, std::string> pastes_;
  int next_paste_{1};
  std::size_t kill_pos_{0};
  std::size_t yank_start_{0};
  std::size_t yank_len_{0};
  bool yanked_{false};
  bool typing_{false};
  std::unique_ptr<vim::VimState> vim_;
  std::string vim_pending_;

  bool session_open_{false};
  bool session_arrowed_{false};
  bool session_replace_{false};
  bool session_snapshot_{false};
  std::string session_text_;
  std::size_t session_cursor_{0};
  std::size_t left_end_{0};
  std::size_t right_start_{0};
  int delete_before_{0};
  int delete_after_{0};
  bool tracked_{true};
  std::string replace_typed_;
  std::vector<ReplaceEntry> replace_log_;
};

}  // namespace booth
