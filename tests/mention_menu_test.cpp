#include <catch2/catch_test_macros.hpp>

#include <string>
#include <vector>

#include <ftxui/component/event.hpp>

#include "interaction/mention_menu.hpp"

using booth::ModelChoice;

namespace {
std::vector<ModelChoice> models() {
  return {{"provider", "model-one", "[\"provider\",\"model-one\"]"},
          {"provider", "model-two", "[\"provider\",\"model-two\"]"}};
}
}

TEST_CASE("mention menu opens, filters and accepts") {
  booth::MentionMenu menu;
  menu.update({"hello ^mod"}, 0, 10, models(), {});
  REQUIRE(menu.visible());
  REQUIRE(menu.matches().size() == 2);
  ftxui::Event down = ftxui::Event::ArrowDown;
  menu.key(down, "hello ^mod");
  ftxui::Event enter = ftxui::Event::Return;
  const auto result = menu.key(enter, "hello ^mod");
  REQUIRE(result.has_value());
  REQUIRE(result->content == "hello ^provider/model-two ");
  REQUIRE(result->cursor == result->content.size());
  REQUIRE_FALSE(menu.visible());
}

TEST_CASE("mention menu closes when the token is gone") {
  booth::MentionMenu menu;
  menu.update({"^mod"}, 0, 4, models(), {});
  REQUIRE(menu.visible());
  menu.update({"plain"}, 0, 5, models(), {});
  REQUIRE_FALSE(menu.visible());
}
