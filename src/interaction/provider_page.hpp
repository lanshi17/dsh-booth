#pragma once

#include <cstddef>
#include <string>
#include <vector>

#include <ftxui/component/event.hpp>
#include <ftxui/dom/elements.hpp>

#include "control/client.hpp"
#include "interaction/state.hpp"

namespace booth {

/// In-TUI provider editor opened from the provider list. No external $EDITOR:
/// Enter on a provider opens this page, Ctrl+S saves and Esc cancels.
class ProviderPage {
 public:
  /// Load one provider into the form. `create` starts a blank, name-editable route.
  void open(ControlClient& control, AppState& state, std::string provider, bool create);
  void close();
  [[nodiscard]] bool visible() const { return visible_; }

  /// Handle navigation/editing/save/delete. Returns true when the page consumed the event.
  bool key(const ftxui::Event& event, ControlClient& control, AppState& state);
  [[nodiscard]] ftxui::Element view() const;

 private:
  struct Field {
    std::string label;
    std::string value;
    std::string hint;
    bool secret{false};
    bool read_only{false};
  };

  void move(int step);
  void save(ControlClient& control, AppState& state);
  void remove(ControlClient& control, AppState& state);
  void discover(ControlClient& control, AppState& state);
  void erase_last();
  [[nodiscard]] std::size_t editable_count() const;
  [[nodiscard]] bool last_editable() const;

  bool visible_{false};
  bool create_{false};
  bool configured_{false};
  std::string provider_;
  std::string status_;
  bool status_error_{false};
  std::vector<Field> fields_;
  std::vector<std::string> initial_;
  std::size_t active_{0};
};

}  // namespace booth
