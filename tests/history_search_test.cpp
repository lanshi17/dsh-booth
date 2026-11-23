#include <catch2/catch_test_macros.hpp>

#include <string>
#include <vector>

#include "interaction/history_search.hpp"

using namespace booth;

namespace {

bool press(HistorySearch& search, AppState& state, ftxui::Event event) {
  return search.key(event, state);
}

}  // namespace

TEST_CASE("history search lists newest unique prompts") {
  HistorySearch search;
  AppState state;
  search.open({"alpha", "beta", "alpha"});
  REQUIRE(search.visible());
  REQUIRE(search.matches().size() == 2);
  REQUIRE(search.matches()[0] == "alpha");
  REQUIRE(search.matches()[1] == "beta");
}

TEST_CASE("history search filters and accepts with Enter") {
  HistorySearch search;
  AppState state;
  search.open({"alpha", "beta"});
  REQUIRE(press(search, state, ftxui::Event::Character('a')));
  REQUIRE(press(search, state, ftxui::Event::Character('l')));
  REQUIRE(search.matches().size() == 1);
  REQUIRE(press(search, state, ftxui::Event::Return));
  REQUIRE(state.composer == "alpha");
  REQUIRE_FALSE(search.visible());
}

TEST_CASE("history search navigates, edits the query and cancels") {
  HistorySearch search;
  AppState state;
  search.open({"one", "two"});
  REQUIRE(press(search, state, ftxui::Event::ArrowDown));
  REQUIRE(search.index() == 1);
  REQUIRE(press(search, state, ftxui::Event::ArrowUp));
  REQUIRE(search.index() == 0);
  REQUIRE(press(search, state, ftxui::Event::Backspace));
  REQUIRE(search.query().empty());
  press(search, state, ftxui::Event::Escape);
  REQUIRE_FALSE(search.visible());
  REQUIRE(state.composer.empty());
}
