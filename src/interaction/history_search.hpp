#pragma once

#include <cstddef>
#include <string>
#include <vector>

#include <ftxui/component/event.hpp>

#include "interaction/mouse_zone.hpp"
#include "interaction/state.hpp"

namespace booth {

/// Ctrl+R prompt-history search: newest matching prompts, deduplicated.
class HistorySearch {
 public:
  void open(const std::vector<std::string>& history);
  void close();

  [[nodiscard]] bool visible() const { return visible_; }
  [[nodiscard]] const std::string& query() const { return query_; }
  [[nodiscard]] const std::vector<std::string>& matches() const { return matches_; }
  [[nodiscard]] std::size_t index() const { return index_; }

  /// Handles query typing, navigation and acceptance. Returns true when Enter accepts a match.
  bool key(ftxui::Event& event, AppState& state);

  /// Accepts one rendered row, exactly like keyboard selection + Enter.
  bool click(std::size_t row, AppState& state);

  enum class Result { None, Handled, Accepted };

  /// Opens on Ctrl+R or handles a key while visible.
  Result handle(ftxui::Event& event, AppState& state,
                const std::vector<std::string>& history, bool open_key);

 private:
  void filter();

  bool visible_{false};
  std::string query_;
  std::vector<std::string> all_;
  std::vector<std::string> matches_;
  std::size_t index_{0};
};

}  // namespace booth
