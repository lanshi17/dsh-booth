#include "interaction/scroll.hpp"

#include <algorithm>

namespace booth {

EntryWindow entry_window(std::size_t count, std::size_t scroll, std::size_t limit) {
  EntryWindow window;
  window.max_scroll = count > limit ? count - limit : 0;
  window.scroll = std::min(scroll, window.max_scroll);
  window.end = count - window.scroll;
  window.begin = window.end > limit ? window.end - limit : 0;
  return window;
}

void TranscriptScroll::up(std::size_t count, std::size_t entries) {
  const auto max = entry_window(count, 0, kWindow).max_scroll;
  scroll_ = std::min(scroll_ + entries, max);
}

void TranscriptScroll::down(std::size_t entries) {
  scroll_ = scroll_ > entries ? scroll_ - entries : 0;
}

void TranscriptScroll::page_up(std::size_t count) { up(count, kPage); }

void TranscriptScroll::page_down() { down(kPage); }

void TranscriptScroll::follow() { scroll_ = 0; }

void TranscriptScroll::resized(std::size_t before, std::size_t after) {
  if (after < before || after <= kWindow) {
    scroll_ = 0;
    return;
  }
  if (scroll_ == 0) return;
  scroll_ += after - before;
  scroll_ = std::min(scroll_, after - kWindow);
}

EntryWindow TranscriptScroll::window(std::size_t count) const {
  return entry_window(count, scroll_, kWindow);
}

}  // namespace booth
