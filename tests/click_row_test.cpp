#include <catch2/catch_test_macros.hpp>

#include <vector>

#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/screen.hpp>

#include "interaction/click_row.hpp"

TEST_CASE("click rows record the rendered box") {
  std::vector<booth::ClickRow> rows;
  auto element = ftxui::vbox({booth::click_row(ftxui::text("a\nb"), 7, &rows),
                             ftxui::text("c")});
  ftxui::Screen screen(10, 5);
  ftxui::Render(screen, element);
  REQUIRE(rows.size() == 1);
  REQUIRE(rows[0].index == 7);
  REQUIRE(rows[0].y_min == 0);
  REQUIRE(rows[0].y_max == 1);
  REQUIRE(rows[0].x_min == 0);
  REQUIRE(rows[0].x_max == 9);
}
