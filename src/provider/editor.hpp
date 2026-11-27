#pragma once

#include <string>
#include <vector>

#include <ftxui/component/component_base.hpp>
#include <ftxui/component/event.hpp>
#include <ftxui/component/screen_interactive.hpp>
#include <ftxui/dom/elements.hpp>

#include "control/client.hpp"
#include "provider/model.hpp"

namespace booth::provider {

/// Full-screen provider manager. The list is the root screen; Enter edits a
/// route in place, `n` starts a new one and Esc returns from the form.
class Editor : public ftxui::ComponentBase {
 public:
  Editor(ftxui::ScreenInteractive& screen, ControlClient& control, Listing listing,
         std::string provider, bool create);

  ftxui::Element OnRender() override;
  bool OnEvent(ftxui::Event event) override;

 private:
  enum class Mode { List, Form };

  void reload();
  void open_form(std::string provider, bool create);
  void set_form_status(std::string text, bool error);
  void move_form(int step);
  void erase_form_last();
  bool last_editable() const;
  bool handle_form(ftxui::Event event);
  ftxui::Element render_list() const;
  ftxui::Element render_form() const;

  ftxui::ScreenInteractive& screen_;
  ControlClient& control_;
  std::vector<Row> rows_;
  std::size_t selected_{0};
  Mode mode_{Mode::List};
  Form form_;
  std::string notice_;
  bool notice_error_{false};
};

/// Run the provider manager until the user quits.
int run_editor(ControlClient& control, Listing listing, std::string provider, bool create);

}  // namespace booth::provider
