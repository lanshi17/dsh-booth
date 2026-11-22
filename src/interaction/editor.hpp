#pragma once

#include <cstddef>
#include <string>
#include <vector>

#include <ftxui/component/event.hpp>

namespace booth {

/// A small multiline prompt editor: bytes are inserted/removed at a glyph boundary.
class Editor {
 public:
  explicit Editor(std::string& content);

  /// Handles editing keys. Returns false for keys the caller owns (Enter submits).
  bool HandleEvent(const ftxui::Event& event);

  void Reset();
  void SetContent(const std::string& content);

  [[nodiscard]] std::vector<std::string> Lines() const;
  [[nodiscard]] std::size_t CursorLine() const;
  [[nodiscard]] std::size_t CursorColumn() const;

 private:
  void Insert(const std::string& text);
  void MoveVertical(int direction);
  [[nodiscard]] std::size_t LineStart(std::size_t at) const;
  [[nodiscard]] std::size_t LineEnd(std::size_t at) const;

  std::string& content_;
  std::size_t cursor_{0};
};

}  // namespace booth
