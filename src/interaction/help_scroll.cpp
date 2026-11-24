#include "interaction/help_scroll.hpp"

#include <algorithm>

#include "interaction/view.hpp"

namespace booth {

void HelpScroll::clamp() { offset_ = std::clamp(offset_, 0, help_max_offset()); }

int HelpScroll::offset() {
  clamp();
  return offset_;
}

bool HelpScroll::key(const ftxui::Event& event) {
  const int page = help_visible_lines();
  const int maximum = help_max_offset();
  if (event == ftxui::Event::ArrowUp) offset_ = std::max(0, offset_ - 1);
  else if (event == ftxui::Event::ArrowDown) offset_ = std::min(maximum, offset_ + 1);
  else if (event == ftxui::Event::PageUp) offset_ = std::max(0, offset_ - page);
  else if (event == ftxui::Event::PageDown) offset_ = std::min(maximum, offset_ + page);
  else if (event == ftxui::Event::Home) offset_ = 0;
  else if (event == ftxui::Event::End) offset_ = maximum;
  else return false;
  return true;
}

bool HelpScroll::wheel(ftxui::Event& event) {
  if (event.mouse().button == ftxui::Mouse::WheelUp) {
    offset_ = std::max(0, offset_ - 3);
  } else if (event.mouse().button == ftxui::Mouse::WheelDown) {
    offset_ = std::min(help_max_offset(), offset_ + 3);
  } else {
    return false;
  }
  return true;
}

}  // namespace booth
