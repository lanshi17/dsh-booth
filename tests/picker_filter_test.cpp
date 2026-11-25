#include <catch2/catch_test_macros.hpp>

#include <filesystem>
#include <string>
#include <vector>

#include "interaction/model_picker.hpp"
#include <unistd.h>

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

TEST_CASE("model picker navigates providers before models") {
  std::vector<ModelChoice> choices = {
      {"alpha", "one", "a1"}, {"alpha", "two", "a2"}, {"beta", "three", "b3"}};
  ModelPicker picker;
  picker.open(choices, "three");
  REQUIRE(picker.provider().empty());
  REQUIRE_FALSE(picker.model_focus());
  ftxui::Event down = ftxui::Event::ArrowDown;
  REQUIRE_FALSE(picker.key(down, choices).has_value());
  REQUIRE(picker.provider() == "alpha");
  REQUIRE(picker.matches().size() == 2);
  ftxui::Event right = ftxui::Event::ArrowRight;
  REQUIRE_FALSE(picker.key(right, choices).has_value());
  REQUIRE(picker.model_focus());
  REQUIRE_FALSE(picker.key(down, choices).has_value());
  ftxui::Event enter = ftxui::Event::Return;
  const auto chosen = picker.key(enter, choices);
  REQUIRE(chosen.has_value());
  REQUIRE(chosen->value == "a2");
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

TEST_CASE("pinned sessions persist and sort first") {
  const auto path = std::filesystem::temp_directory_path() /
                    ("booth-pins-" + std::to_string(::getpid()) + ".txt");
  std::filesystem::remove(path);
  {
    std::vector<SessionRow> sessions(3);
    sessions[0].id = SessionId{"s-1"};
    sessions[0].title = "first";
    sessions[1].id = SessionId{"s-2"};
    sessions[1].title = "second";
    sessions[2].id = SessionId{"s-3"};
    sessions[2].title = "third";
    SessionPicker picker;
    picker.open(path);
    picker.loaded(sessions);
    ftxui::Event down = ftxui::Event::ArrowDown;
    REQUIRE(picker.key(down, sessions) == SessionPicker::Action::None);
    ftxui::Event pin = ftxui::Event::Special("\x1bk");
    REQUIRE(picker.key(pin, sessions) == SessionPicker::Action::None);
    REQUIRE(sessions[1].pinned);
    REQUIRE(sessions[picker.matches()[0]].id.value == "s-2");
    REQUIRE(sessions[picker.matches()[picker.index()]].id.value == "s-2");
  }
  {
    std::vector<SessionRow> sessions(3);
    sessions[0].id = SessionId{"s-1"};
    sessions[1].id = SessionId{"s-2"};
    sessions[2].id = SessionId{"s-3"};
    SessionPicker picker;
    picker.open(path);
    picker.loaded(sessions);
    REQUIRE(sessions[1].pinned);
    REQUIRE(picker.matches().size() == 3);
    REQUIRE(sessions[picker.matches()[0]].id.value == "s-2");
  }
  std::filesystem::remove(path);
}
