#pragma once

#include <ftxui/component/event.hpp>

namespace booth {

/// Scroll offset for the help card; keeps every key and wheel step in range.
class HelpScroll {
 public:
  /// Clamps to this frame's terminal height, then returns the offset.
  int offset();
  void reset() { offset_ = 0; }
  void clamp();

  /// Handles arrows, PageUp/PageDown and Home/End; false for any other key.
  bool key(const ftxui::Event& event);

  /// Handles the wheel; false for any other mouse event.
  bool wheel(ftxui::Event& event);

 private:
  int offset_{0};
};

}  // namespace booth
