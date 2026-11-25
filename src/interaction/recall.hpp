#pragma once

#include <cstddef>
#include <deque>
#include <optional>
#include <string>
#include <vector>

namespace booth {

/// Cleared composer text, newest first, merged with submitted prompt history for Up/Down recall.
class PromptRecall {
 public:
  /// Remembers one cleared draft and rewinds navigation.
  void clear(std::string draft);
  /// Rewinds navigation without forgetting cleared drafts.
  void reset();
  /// One step older; nullopt at the oldest entry.
  [[nodiscard]] std::optional<std::string> up(const std::vector<std::string>& history);
  /// One step newer; an empty string returns to a blank composer.
  [[nodiscard]] std::optional<std::string> down(const std::vector<std::string>& history);

 private:
  static constexpr std::size_t kLimit = 100;
  [[nodiscard]] std::string at(std::size_t position, const std::vector<std::string>& history) const;

  std::deque<std::string> cleared_;
  std::size_t position_{0};
};

}  // namespace booth
