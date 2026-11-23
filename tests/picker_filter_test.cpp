#include <catch2/catch_test_macros.hpp>

#include <string>
#include <vector>

#include "interaction/model_picker.hpp"
#include "interaction/view.hpp"

using namespace booth;

TEST_CASE("model picker filters by typing and accepts the match") {
  std::vector<ModelChoice> choices = {
      {"p", "one", "v1"}, {"p", "two", "v2"}, {"p", "three", "v3"}};
  ModelPicker picker;
  picker.open(choices, "one");
  REQUIRE(picker.visible());
  REQUIRE(picker.index() == 0);

  ftxui::Event t = ftxui::Event::Character('t');
  REQUIRE_FALSE(picker.key(t, choices).has_value());
  REQUIRE(picker.query() == "t");
  REQUIRE(picker.matches().size() == 2);

  ftxui::Event h = ftxui::Event::Character('h');
  REQUIRE_FALSE(picker.key(h, choices).has_value());
  REQUIRE(picker.matches().size() == 1);

  ftxui::Event enter = ftxui::Event::Return;
  const auto chosen = picker.key(enter, choices);
  REQUIRE(chosen.has_value());
  REQUIRE(chosen->name == "three");
  REQUIRE_FALSE(picker.visible());
}

TEST_CASE("session picker filters while keeping refresh and close actions") {
  std::vector<SessionRow> sessions(3);
  sessions[0].id = SessionId{"s-1"};
  sessions[0].title = "first session";
  sessions[1].id = SessionId{"s-2"};
  sessions[1].title = "second session";
  sessions[2].id = SessionId{"s-3"};
  sessions[2].title = "third session";

  SessionPicker picker;
  picker.open();
  picker.loaded(sessions);
  ftxui::Event r = ftxui::Event::Character('r');
  REQUIRE(picker.key(r, sessions) == SessionPicker::Action::Refresh);
  picker.loaded(sessions);

  for (const char ch : std::string("second")) {
    ftxui::Event event = ftxui::Event::Character(ch);
    REQUIRE(picker.key(event, sessions) == SessionPicker::Action::None);
  }
  REQUIRE(picker.matches().size() == 1);
  REQUIRE(sessions[picker.matches()[picker.index()]].title == "second session");

  ftxui::Event enter = ftxui::Event::Return;
  REQUIRE(picker.key(enter, sessions) == SessionPicker::Action::Resume);
  REQUIRE_FALSE(picker.visible());
}
