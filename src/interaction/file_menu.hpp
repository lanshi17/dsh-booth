#pragma once

#include <cstddef>
#include <string>
#include <utility>
#include <vector>

#include <ftxui/component/event.hpp>

#include "interaction/mouse_zone.hpp"

namespace booth {

/// Inline `@path` completion over files under the working directory.
class FileMenu {
 public:
  void sync(const std::string& composer, const std::string& cwd);
  void close();

  [[nodiscard]] bool visible() const { return visible_; }
  [[nodiscard]] std::size_t index() const { return index_; }
  [[nodiscard]] const std::string& query() const { return query_; }
  [[nodiscard]] const std::vector<std::string>& matches() const { return matches_; }

  /// Handles navigation and accepts with Tab/Enter by rewriting `composer`.
  bool key(ftxui::Event& event, std::string& composer);

  /// Completes one rendered row, exactly like keyboard selection + Enter.
  bool click(std::size_t row, std::string& composer);

 private:
  bool visible_{false};
  std::string seen_;
  std::string query_;
  std::size_t token_start_{0};
  std::size_t index_{0};
  std::vector<std::string> matches_;
};

/// Extracts existing `@path` mentions as (basename, file URI) resource links.
std::vector<std::pair<std::string, std::string>> file_links(const std::string& text,
                                                            const std::string& cwd);

/// Files under `cwd` matching `prefix`, relative paths, capped.
std::vector<std::string> complete_files(const std::string& cwd, const std::string& prefix,
                                        std::size_t limit = 50);

}  // namespace booth
