#pragma once

#include <cstddef>
#include <optional>
#include <string>
#include <vector>

#include <ftxui/component/event.hpp>

#include "contract/options.hpp"
#include "interaction/mouse_zone.hpp"

namespace booth {

/// Searchable `/model` picker.
class ModelPicker {
 public:
  void open(const std::vector<ModelChoice>& choices, const std::string& current,
            const std::string& provider = {});
  void close();

  [[nodiscard]] bool visible() const { return visible_; }
  [[nodiscard]] std::size_t index() const { return index_; }
  [[nodiscard]] const std::string& query() const { return query_; }
  [[nodiscard]] const std::string& provider() const { return provider_; }
  [[nodiscard]] bool model_focus() const { return model_focus_; }
  [[nodiscard]] const std::vector<std::string>& providers() const { return providers_; }
  [[nodiscard]] const std::vector<std::size_t>& matches() const { return matches_; }

  /// Navigates/filters, and returns the chosen model on Enter.
  std::optional<ModelChoice> key(ftxui::Event& event, const std::vector<ModelChoice>& choices);

  /// Chooses one rendered row, exactly like keyboard selection + Enter.
  std::optional<ModelChoice> click(std::size_t row, const std::vector<ModelChoice>& choices);

 private:
  void filter(const std::vector<ModelChoice>& choices);

  bool visible_{false};
  std::size_t index_{0};
  std::string query_;
  std::string provider_;
  std::vector<std::string> providers_;
  std::size_t provider_index_{0};
  bool model_focus_{false};
  std::vector<std::size_t> matches_;
};

}  // namespace booth
