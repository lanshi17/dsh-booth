#pragma once

#include <cstddef>
#include <optional>
#include <string>
#include <vector>

#include <ftxui/component/event.hpp>
#include <ftxui/dom/elements.hpp>

#include "contract/options.hpp"
#include "interaction/mention.hpp"
#include "interaction/mouse_zone.hpp"

namespace booth {

/// Inline `^model` completion shown below the composer.
class MentionMenu {
 public:
  struct Result {
    std::string content;
    std::size_t cursor;
  };

  void update(const std::vector<std::string>& lines, std::size_t line, std::size_t column,
              const std::vector<ModelChoice>& models, const std::vector<std::string>& efforts);
  void close();
  [[nodiscard]] bool visible() const { return visible_; }
  [[nodiscard]] std::size_t index() const { return index_; }
  [[nodiscard]] const std::vector<MentionSuggestion>& matches() const { return matches_; }
  [[nodiscard]] const std::string& prefix() const { return query_.prefix; }

  /// Navigates/filters; returns the edited content when Enter/Tab accepts a row.
  std::optional<Result> key(ftxui::Event& event, const std::string& content);

  /// Accepts one rendered row, exactly like keyboard selection + Enter.
  std::optional<Result> click(std::size_t row, const std::string& content);

  ftxui::Element view(MouseRouter* mouse = nullptr) const;

 private:
  bool visible_{false};
  std::size_t index_{0};
  MentionQuery query_;
  std::vector<MentionSuggestion> matches_;
};

}  // namespace booth
