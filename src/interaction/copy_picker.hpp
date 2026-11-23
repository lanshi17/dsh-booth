#pragma once

#include <cstddef>
#include <string>
#include <vector>

#include <ftxui/component/event.hpp>

#include "interaction/mouse_zone.hpp"
#include "interaction/state.hpp"

namespace booth {

struct CopyItem {
  std::string label;
  std::string text;
};

/// `/copy` picker over assistant answers and their fenced code blocks.
class CopyPicker {
 public:
  /// Collects copyable text; returns false when the transcript has nothing to copy.
  bool open(const AppState& state);
  void close();

  [[nodiscard]] bool visible() const { return visible_; }
  [[nodiscard]] std::size_t index() const { return index_; }
  [[nodiscard]] const std::vector<CopyItem>& items() const { return items_; }

  /// Handles navigation and copies on Enter. Returns true when the picker consumed the key.
  bool key(ftxui::Event& event, AppState& state);

  /// Copies one rendered row, exactly like keyboard selection + Enter.
  bool click(std::size_t row, AppState& state);

 private:
  bool visible_{false};
  std::size_t index_{0};
  std::vector<CopyItem> items_;
};

}  // namespace booth
