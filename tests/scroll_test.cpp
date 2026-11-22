#include "interaction/scroll.hpp"

#include <catch2/catch_test_macros.hpp>

using namespace booth;

TEST_CASE("the window follows the latest entries by default") {
  auto window = entry_window(100, 0, 40);
  REQUIRE(window.begin == 60);
  REQUIRE(window.end == 100);
  REQUIRE(window.scroll == 0);
  REQUIRE(window.max_scroll == 60);

  window = entry_window(100, 10, 40);
  REQUIRE(window.begin == 50);
  REQUIRE(window.end == 90);
  REQUIRE(window.scroll == 10);
}

TEST_CASE("a short transcript has nothing to scroll") {
  const auto window = entry_window(30, 20, 40);
  REQUIRE(window.begin == 0);
  REQUIRE(window.end == 30);
  REQUIRE(window.scroll == 0);
  REQUIRE(window.max_scroll == 0);
}

TEST_CASE("wheel steps scroll by a few entries") {
  TranscriptScroll scroll;
  scroll.up(100, 3);
  REQUIRE(scroll.window(100).scroll == 3);
  scroll.up(100, 3);
  REQUIRE(scroll.window(100).scroll == 6);
  scroll.down(3);
  REQUIRE(scroll.window(100).scroll == 3);
  scroll.down(30);
  REQUIRE(scroll.window(100).scroll == 0);
}

TEST_CASE("paging clamps at both ends") {
  TranscriptScroll scroll;
  scroll.page_up(100);
  REQUIRE(scroll.window(100).scroll == 20);
  scroll.page_up(100);
  REQUIRE(scroll.window(100).scroll == 40);
  scroll.page_up(100);
  REQUIRE(scroll.window(100).scroll == 60);
  scroll.page_up(100);
  REQUIRE(scroll.window(100).scroll == 60);

  scroll.page_down();
  REQUIRE(scroll.window(100).scroll == 40);
  scroll.follow();
  REQUIRE(scroll.window(100).scroll == 0);
}

TEST_CASE("new entries keep the entries the reader is looking at") {
  TranscriptScroll scroll;
  scroll.page_up(100);
  const auto before = scroll.window(100);
  scroll.resized(100, 110);
  const auto after = scroll.window(110);
  REQUIRE(after.begin == before.begin);
  REQUIRE(after.end == before.end);
  REQUIRE(after.scroll == before.scroll + 10);

  scroll.resized(110, 5);
  REQUIRE(scroll.window(5).scroll == 0);
}
