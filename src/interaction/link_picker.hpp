#pragma once

#include <cstddef>
#include <string>
#include <vector>

#include <ftxui/component/event.hpp>
#include <ftxui/dom/elements.hpp>

#include "interaction/state.hpp"

namespace booth {

struct TranscriptLink {
  std::string url;
  std::string host;
  std::string hint;
};

/// Stable hint label: `a`..`z`, then `A`..`Z`, then `?` beyond 52 links.
std::string link_hint(std::size_t index);

/// Collects openable URLs from committed user and assistant transcript entries.
/// Markdown links and bare `http://`/`https://`/`mailto:` URLs are accepted,
/// deduplicated in first-seen order and filtered through `is_openable_url`.
std::vector<TranscriptLink> collect_transcript_links(
    const std::vector<TranscriptEntry>& entries);

/// Collects the same URL forms from one piece of text, such as an unsent prompt.
std::vector<TranscriptLink> collect_text_links(std::string_view text);

/// Alt+L / `/links`: a keyboard hint picker over the transcript's links.
class LinkPicker {
 public:
  /// Refreshes the links and opens the picker. Returns true when it has links.
  bool open(const AppState& state);
  void close();

  [[nodiscard]] bool visible() const { return visible_; }
  [[nodiscard]] bool empty() const { return items_.empty(); }
  [[nodiscard]] std::size_t index() const { return index_; }
  [[nodiscard]] const std::vector<TranscriptLink>& items() const { return items_; }

  /// Handles navigation, Enter and letter hints. Returns true when consumed.
  bool key(ftxui::Event& event, AppState& state);

  /// Selects the row carrying this hint character without opening it.
  bool select_hint(char hint);

  /// Opens the selected link with `open_url` and records the outcome.
  bool activate(AppState& state);

 private:
  bool visible_{false};
  std::size_t index_{0};
  std::vector<TranscriptLink> items_;
};

/// Picker rows with hint, host and URL; an empty transcript shows the empty state.
ftxui::Element link_picker_view(const std::vector<TranscriptLink>& items, std::size_t index);

}  // namespace booth
