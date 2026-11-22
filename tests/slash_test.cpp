#include <catch2/catch_test_macros.hpp>

#include <algorithm>
#include <string>

#include "interaction/state.hpp"
#include "interaction/view.hpp"

using namespace booth;

namespace {

bool contains(const std::vector<const CommandHint*>& matches, std::string_view name) {
  return std::any_of(matches.begin(), matches.end(),
                     [&](const CommandHint* hint) { return name == hint->name; });
}

SlashAction press(SlashMenu& menu, AppState& state, ftxui::Event event) {
  return menu.key(event, state);
}

}  // namespace

TEST_CASE("slash menu lists every command for a bare slash") {
  const auto matches = matching_commands("/");
  REQUIRE(matches.size() == 15);
  REQUIRE(matches.front()->name == std::string("/help"));
}

TEST_CASE("slash menu filters names, aliases and descriptions") {
  auto matches = matching_commands("/co");
  REQUIRE_FALSE(matches.empty());
  REQUIRE(matches[0]->name == std::string("/copy"));
  REQUIRE(contains(matches, "/context"));
  REQUIRE(contains(matches, "/close"));

  matches = matching_commands("/exit");
  REQUIRE_FALSE(matches.empty());
  REQUIRE(matches[0]->name == std::string("/quit"));

  matches = matching_commands("/clipboard");
  REQUIRE_FALSE(matches.empty());
  REQUIRE(matches[0]->name == std::string("/copy"));

  matches = matching_commands("/mdl");
  REQUIRE_FALSE(matches.empty());
  REQUIRE(matches[0]->name == std::string("/model"));

  matches = matching_commands("   /model");
  REQUIRE_FALSE(matches.empty());
  REQUIRE(matches[0]->name == std::string("/model"));

  REQUIRE(matching_commands("/model two").empty());
  REQUIRE(matching_commands("hello /co").empty());
}

TEST_CASE("slash menu navigation wraps and pages") {
  SlashMenu menu;
  AppState state;
  state.composer = "/";
  menu.sync(state.composer);
  REQUIRE(menu.visible(state.composer));
  REQUIRE(menu.index() == 0);

  REQUIRE(press(menu, state, ftxui::Event::ArrowUp) == SlashAction::Handled);
  REQUIRE(menu.index() == 14);
  REQUIRE(press(menu, state, ftxui::Event::ArrowDown) == SlashAction::Handled);
  REQUIRE(menu.index() == 0);
  REQUIRE(press(menu, state, ftxui::Event::PageDown) == SlashAction::Handled);
  REQUIRE(menu.index() == 10);
  REQUIRE(menu.index() == 10);
  REQUIRE(press(menu, state, ftxui::Event::PageUp) == SlashAction::Handled);
  REQUIRE(menu.index() == 0);
  REQUIRE(press(menu, state, ftxui::Event::End) == SlashAction::Handled);
  REQUIRE(menu.index() == 14);
  REQUIRE(press(menu, state, ftxui::Event::Home) == SlashAction::Handled);
  REQUIRE(menu.index() == 0);
}

TEST_CASE("Tab completes, Enter submits and Escape dismisses") {
  SlashMenu menu;
  AppState state;
  state.composer = "/clo";
  menu.sync(state.composer);
  REQUIRE(press(menu, state, ftxui::Event::Tab) == SlashAction::Handled);
  REQUIRE(state.composer == "/close ");

  state.composer = "/exi";
  menu.sync(state.composer);
  REQUIRE(press(menu, state, ftxui::Event::Return) == SlashAction::Submit);
  REQUIRE(state.composer == "/quit");

  state.composer = "/co";
  menu.sync(state.composer);
  REQUIRE(press(menu, state, ftxui::Event::Escape) == SlashAction::Handled);
  REQUIRE_FALSE(menu.visible(state.composer));

  state.composer = "/cop";
  menu.sync(state.composer);
  REQUIRE(menu.visible(state.composer));
}

TEST_CASE("Enter runs a description-only hit only after the selection moves") {
  SlashMenu menu;
  AppState state;
  state.composer = "/clipboard";
  menu.sync(state.composer);
  REQUIRE(menu.visible(state.composer));
  REQUIRE(press(menu, state, ftxui::Event::Return) == SlashAction::None);
  REQUIRE(state.composer == "/clipboard");

  menu.sync(state.composer);
  REQUIRE(press(menu, state, ftxui::Event::ArrowDown) == SlashAction::Handled);
  REQUIRE(press(menu, state, ftxui::Event::Return) == SlashAction::Submit);
  REQUIRE(state.composer == "/copy");
}

TEST_CASE("Enter submits an exact command name immediately") {
  SlashMenu menu;
  AppState state;
  state.composer = "/copy";
  menu.sync(state.composer);
  REQUIRE(press(menu, state, ftxui::Event::Return) == SlashAction::Submit);
  REQUIRE(state.composer == "/copy");
}

TEST_CASE("slash menu ignores text that is not a command token") {
  SlashMenu menu;
  AppState state;
  state.composer = "hello";
  menu.sync(state.composer);
  REQUIRE_FALSE(menu.visible(state.composer));
  REQUIRE(press(menu, state, ftxui::Event::Return) == SlashAction::None);
}
