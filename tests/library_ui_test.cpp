#include <catch2/catch_test_macros.hpp>

#include <string>
#include <vector>

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/screen.hpp>

#include "interaction/editor.hpp"
#include "interaction/library_ui.hpp"
#include "interaction/model_picker.hpp"

using namespace booth;

TEST_CASE("library overlay filters and selects a row") {
  LibraryOverlay overlay;
  overlay.open(LibraryKind::Skills, "skills",
               {{"review", "/review", "review a change"}, {"fix", "/fix", "fix a test"}});
  REQUIRE(overlay.visible());
  for (const char ch : std::string("fix")) {
    ftxui::Event event = ftxui::Event::Character(ch);
    overlay.key(event);
  }
  ftxui::Event enter = ftxui::Event::Return;
  REQUIRE(overlay.key(enter) == "fix");
  REQUIRE(overlay.visible());
  auto view = overlay.view();
  ftxui::Screen screen(70, 12);
  ftxui::Render(screen, std::move(view));
  REQUIRE(screen.ToString().find("skills") != std::string::npos);
}

TEST_CASE("skills selection fills the composer and providers drill into models") {
  AppState state;
  state.models = {{"alpha", "one", "a1"}, {"alpha", "two", "a2"}, {"beta", "three", "b1"}};
  LibraryOverlay overlay;
  Editor editor(state.composer);
  ModelPicker picker;

  std::string content;
  Editor skill_editor(content);
  overlay.open(LibraryKind::Skills, "skills", {{"review", "/review", "review"}});
  ftxui::Event enter = ftxui::Event::Return;
  REQUIRE(handle_library_event(enter, state, overlay, picker, skill_editor));
  REQUIRE(state.composer == "/review ");
  REQUIRE(content == "/review ");

  REQUIRE(handle_library_command("/providers", state, overlay, "."));
  REQUIRE(overlay.visible());
  for (const char ch : std::string("alp")) {
    ftxui::Event event = ftxui::Event::Character(ch);
    overlay.key(event);
  }
  ftxui::Event provider_enter = ftxui::Event::Return;
  REQUIRE(handle_library_event(provider_enter, state, overlay, picker, editor));
  REQUIRE_FALSE(overlay.visible());
  REQUIRE(picker.visible());
  ftxui::Event down = ftxui::Event::ArrowDown;
  const auto chosen = picker.key(down, state.models);
  REQUIRE_FALSE(chosen.has_value());
  ftxui::Event model_enter = ftxui::Event::Return;
  const auto model = picker.key(model_enter, state.models);
  REQUIRE(model.has_value());
  REQUIRE(model->provider == "alpha");
  REQUIRE(model->value == "a2");
}

TEST_CASE("provider overlay groups models and marks the current one") {
  AppState state;
  state.model = "two";
  state.models = {{"alpha", "one", "a1"}, {"alpha", "two", "a2"}, {"beta", "three", "b1"}};
  LibraryOverlay overlay;
  REQUIRE(handle_library_command("/providers", state, overlay, "."));
  auto view = overlay.view();
  ftxui::Screen screen(72, 14);
  ftxui::Render(screen, std::move(view));
  const std::string text = screen.ToString();
  REQUIRE(text.find("alpha") != std::string::npos);
  REQUIRE(text.find("2 models") != std::string::npos);
  REQUIRE(text.find("current") != std::string::npos);
}
