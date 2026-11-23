#pragma once

#include <cstddef>

namespace booth {

/// The slice of transcript entries that fits the inline window, and how far it is scrolled back.
struct EntryWindow {
  std::size_t begin{0};
  std::size_t end{0};
  std::size_t scroll{0};
  std::size_t max_scroll{0};
};

/// Returns the entry range for `scroll` entries hidden below the latest `limit` entries.
EntryWindow entry_window(std::size_t count, std::size_t scroll, std::size_t limit);

/// Keeps the transcript window pinned while new entries arrive, unless the user scrolled back.
class TranscriptScroll {
 public:
  static constexpr std::size_t kWindow = 40;
  static constexpr std::size_t kPage = 20;

  void up(std::size_t count, std::size_t entries);
  void down(std::size_t entries);
  void page_up(std::size_t count);
  void page_down();
  void follow();

  /// Centers an entry in the visible window.
  void focus(std::size_t count, std::size_t index);

  /// Call after a batch of events appended entries; a shrink resets to the latest entry.
  void resized(std::size_t before, std::size_t after);

  [[nodiscard]] EntryWindow window(std::size_t count) const;

 private:
  std::size_t scroll_{0};
};

}  // namespace booth
