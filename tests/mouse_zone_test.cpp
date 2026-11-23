#include <catch2/catch_test_macros.hpp>

#include <string>

#include <ftxui/component/mouse.hpp>
#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/screen.hpp>

#include "interaction/mouse_zone.hpp"
#include "interaction/open_link.hpp"

using namespace booth;

namespace {

ftxui::Event press(int x, int y) {
  ftxui::Mouse mouse;
  mouse.x = x;
  mouse.y = y;
  mouse.button = ftxui::Mouse::Left;
  mouse.motion = ftxui::Mouse::Pressed;
  return ftxui::Event::Mouse("", mouse);
}

ftxui::Event move(int x, int y) {
  ftxui::Mouse mouse;
  mouse.x = x;
  mouse.y = y;
  mouse.button = ftxui::Mouse::None;
  mouse.motion = ftxui::Mouse::Moved;
  return ftxui::Event::Mouse("", mouse);
}

ftxui::Event wheel(int x, int y, ftxui::Mouse::Button button) {
  ftxui::Mouse mouse;
  mouse.x = x;
  mouse.y = y;
  mouse.button = button;
  mouse.motion = ftxui::Mouse::Pressed;
  return ftxui::Event::Mouse("", mouse);
}

}  // namespace

TEST_CASE("mouse zones record the rendered rectangle and hit-test it") {
  MouseRouter router;
  auto element = ftxui::vbox({
      router.zone(ftxui::text("aa"), MouseSurface::File, 3),
      ftxui::text("gap"),
      router.zone(ftxui::text("bb"), MouseSurface::File, 4),
  });
  ftxui::Screen screen(8, 3);
  ftxui::Render(screen, element);

  REQUIRE(router.zones().size() == 2);
  const auto& first = router.zones()[0];
  REQUIRE(first.surface == MouseSurface::File);
  REQUIRE(first.index == 3);
  REQUIRE(first.y_min == 0);
  REQUIRE(first.y_max == 0);
  REQUIRE(first.x_min == 0);
  REQUIRE(first.x_max == 7);

  const auto top = router.hit(1, 0);
  REQUIRE(top.has_value());
  REQUIRE(top->surface == MouseSurface::File);
  REQUIRE(top->index == 3);

  const auto bottom = router.hit(5, 2);
  REQUIRE(bottom.has_value());
  REQUIRE(bottom->index == 4);

  REQUIRE_FALSE(router.hit(0, 1).has_value());
}

TEST_CASE("mouse motion drives hover and clears it on empty space") {
  MouseRouter router;
  auto element = ftxui::vbox({
      router.zone(ftxui::text("aa"), MouseSurface::Copy, 1),
      ftxui::text("gap"),
  });
  ftxui::Screen screen(8, 2);
  ftxui::Render(screen, element);

  auto input = router.handle(move(0, 0));
  REQUIRE(input.kind == MouseInput::Kind::Moved);
  REQUIRE(input.hit.valid());
  REQUIRE(input.hit.index == 1);
  REQUIRE(router.hovered(MouseSurface::Copy, 1));
  REQUIRE(router.hover_index(MouseSurface::Copy) == 1);

  input = router.handle(move(0, 1));
  REQUIRE(input.kind == MouseInput::Kind::Moved);
  REQUIRE_FALSE(input.hit.valid());
  REQUIRE_FALSE(router.hover().has_value());

  input = router.handle(move(0, 1));
  REQUIRE(input.kind == MouseInput::Kind::None);
}

TEST_CASE("left presses report a hit and wheel reports stay routing-only") {
  MouseRouter router;
  auto element = ftxui::vbox({
      router.zone(ftxui::text("aa"), MouseSurface::Model, 2),
  });
  ftxui::Screen screen(8, 2);
  ftxui::Render(screen, element);

  const auto click = router.handle(press(1, 0));
  REQUIRE(click.kind == MouseInput::Kind::Click);
  REQUIRE(click.hit.valid());
  REQUIRE(click.hit.surface == MouseSurface::Model);
  REQUIRE(click.hit.index == 2);

  const auto blank = router.handle(press(6, 1));
  REQUIRE(blank.kind == MouseInput::Kind::Click);
  REQUIRE_FALSE(blank.hit.valid());

  const auto up = router.handle(wheel(6, 1, ftxui::Mouse::WheelUp));
  REQUIRE(up.kind == MouseInput::Kind::WheelUp);
  REQUIRE_FALSE(up.hit.valid());

  const auto down = router.handle(wheel(6, 1, ftxui::Mouse::WheelDown));
  REQUIRE(down.kind == MouseInput::Kind::WheelDown);

  REQUIRE(router.handle(ftxui::Event::Return).kind == MouseInput::Kind::None);
}

TEST_CASE("hyperlink cells win over rendered zones") {
  ftxui::Screen screen(20, 2);
  auto element = ftxui::hbox({
      ftxui::hyperlink("https://example.com/doc", ftxui::text("link")),
      ftxui::text(" tail"),
  });
  ftxui::Render(screen, element);

  MouseRouter router;
  auto zone = ftxui::vbox({
      router.zone(ftxui::text("link"), MouseSurface::Transcript, 7),
  });
  ftxui::Screen zoned(20, 2);
  ftxui::Render(zoned, zone);

  const auto input = router.handle(press(1, 0), &screen);
  REQUIRE(input.kind == MouseInput::Kind::Click);
  REQUIRE(input.hit.surface == MouseSurface::Link);
  REQUIRE(input.hit.url == "https://example.com/doc");
  REQUIRE(hyperlink_at(screen, 1, 0) == "https://example.com/doc");
  REQUIRE(hyperlink_at(screen, 10, 0).empty());
  REQUIRE(hyperlink_at(screen, -1, 0).empty());
}

TEST_CASE("openable URLs reject unsafe schemes and controls") {
  REQUIRE(is_openable_url("https://example.com/a?b=c"));
  REQUIRE(is_openable_url("http://localhost:8080"));
  REQUIRE(is_openable_url("mailto:dev@example.com"));

  REQUIRE_FALSE(is_openable_url(""));
  REQUIRE_FALSE(is_openable_url("example.com"));
  REQUIRE_FALSE(is_openable_url("javascript:alert(1)"));
  REQUIRE_FALSE(is_openable_url("file:///etc/passwd"));
  REQUIRE_FALSE(is_openable_url("https://exa mple.com"));
  REQUIRE_FALSE(is_openable_url("https://"));
  REQUIRE_FALSE(is_openable_url(std::string("https://x\n")));

  REQUIRE_FALSE(open_url("not a url"));
}
