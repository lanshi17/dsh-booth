#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include <ftxui/component/mouse.hpp>
#include <ftxui/dom/elements.hpp>
#include <ftxui/screen/screen.hpp>

#include "interaction/file_menu.hpp"
#include "interaction/history_search.hpp"
#include "interaction/model_picker.hpp"
#include "interaction/mouse_zone.hpp"
#include "interaction/state.hpp"
#include "interaction/view.hpp"

using namespace booth;

namespace {

ftxui::Event mouse_move(int x, int y) {
  ftxui::Mouse mouse;
  mouse.x = x;
  mouse.y = y;
  mouse.button = ftxui::Mouse::None;
  mouse.motion = ftxui::Mouse::Moved;
  return ftxui::Event::Mouse("", mouse);
}

void render(ftxui::Element element, int width = 90, int height = 30) {
  ftxui::Screen screen(width, height);
  ftxui::Render(screen, std::move(element));
}

std::size_t zone_count(const MouseRouter& router, MouseSurface surface) {
  std::size_t count = 0;
  for (const auto& zone : router.zones()) {
    if (zone.surface == surface) ++count;
  }
  return count;
}

std::filesystem::path make_root(std::string_view name) {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-mouse-" + std::string(name) + "-" + std::to_string(::getpid()));
  std::filesystem::remove_all(dir);
  std::filesystem::create_directories(dir / "dir");
  { std::ofstream out(dir / "alpha.txt"); out << "a"; }
  { std::ofstream out(dir / "dir" / "beta.md"); out << "b"; }
  return dir;
}

std::vector<SessionRow> sessions() {
  std::vector<SessionRow> rows(2);
  rows[0].id = SessionId{"s-1"};
  rows[0].title = "first";
  rows[0].cwd = "/a";
  rows[1].id = SessionId{"s-2"};
  rows[1].title = "second";
  rows[1].cwd = "/b";
  return rows;
}

}  // namespace

TEST_CASE("slash menu clicks activate the rendered row") {
  SlashMenu menu;
  AppState state;
  state.composer = "/";
  menu.sync(state.composer);
  const auto matches = matching_commands(state.composer);
  REQUIRE(matches.size() > 2);

  REQUIRE(menu.click(2, state) == SlashAction::Submit);
  REQUIRE(state.composer == matches[2]->name);
  REQUIRE(menu.index() == 2);
}

TEST_CASE("file menu clicks complete the token") {
  const auto root = make_root("file");
  FileMenu menu;
  std::string composer = "see @bet";
  menu.sync(composer, root.string());
  REQUIRE(menu.visible());
  REQUIRE(menu.click(0, composer));
  REQUIRE(composer == "see @dir/beta.md ");
  REQUIRE_FALSE(menu.visible());
  REQUIRE_FALSE(menu.click(0, composer));
  std::filesystem::remove_all(root);
}

TEST_CASE("model picker clicks choose and close") {
  std::vector<ModelChoice> choices = {
      {"p", "one", "v1"}, {"p", "two", "v2"}, {"p", "three", "v3"}};
  ModelPicker picker;
  picker.open(choices, "one");
  const auto chosen = picker.click(2, choices);
  REQUIRE(chosen.has_value());
  REQUIRE(chosen->name == "three");
  REQUIRE_FALSE(picker.visible());
  REQUIRE_FALSE(picker.click(0, choices).has_value());
}

TEST_CASE("session picker clicks resume a rendered row") {
  auto rows = sessions();
  SessionPicker picker;
  picker.open();
  REQUIRE(picker.click(0, rows) == SessionPicker::Action::None);
  picker.loaded(rows);
  REQUIRE(picker.click(1, rows) == SessionPicker::Action::Resume);
  REQUIRE(picker.index() == 1);
  REQUIRE_FALSE(picker.visible());
  REQUIRE(picker.click(0, rows) == SessionPicker::Action::None);
}

TEST_CASE("history search clicks accept a rendered row") {
  HistorySearch search;
  AppState state;
  search.open({"alpha", "beta"});
  const std::string expected = search.matches()[1];
  REQUIRE(search.click(1, state));
  REQUIRE(state.composer == expected);
  REQUIRE_FALSE(search.visible());
  REQUIRE_FALSE(search.click(0, state));
}

TEST_CASE("copy picker clicks copy a rendered row") {
  AppState state;
  TranscriptEntry answer;
  answer.kind = EntryKind::Assistant;
  answer.text = "first\nsecond";
  state.entries.push_back(answer);

  CopyPicker picker;
  REQUIRE(picker.open(state));
  std::ostringstream captured;
  auto* previous = std::cout.rdbuf(captured.rdbuf());
  REQUIRE(picker.click(0, state));
  std::cout.rdbuf(previous);
  REQUIRE_FALSE(picker.visible());
  REQUIRE(state.notice.find("copied") == 0);
  REQUIRE(captured.str().find("\x1b]52;c;") != std::string::npos);
  REQUIRE_FALSE(picker.click(0, state));
}

TEST_CASE("picker views record exact row zones") {
  MouseRouter router;

  AppState state;
  state.composer = "/";
  router.begin_frame();
  render(ftxui::vbox({command_menu_view(state.composer, 0, &router)}));
  REQUIRE(zone_count(router, MouseSurface::Slash) == 10);
  REQUIRE(router.zones().front().index == 0);
  REQUIRE(router.zones().back().index == 9);

  router.begin_frame();
  render(file_menu_view("", {"alpha", "beta"}, 0, &router));
  REQUIRE(zone_count(router, MouseSurface::File) == 2);
  REQUIRE(router.zones()[0].surface == MouseSurface::File);
  REQUIRE(router.zones()[0].index == 0);
  REQUIRE(router.zones()[1].index == 1);

  const std::vector<ModelChoice> choices = {
      {"p", "one", "v1"}, {"p", "two", "v2"}, {"p", "three", "v3"}};
  router.begin_frame();
  render(model_picker_view(choices, {0, 1, 2}, 0, "one", "", &router));
  REQUIRE(zone_count(router, MouseSurface::Model) == 3);
  REQUIRE(router.zones().back().index == 2);

  router.begin_frame();
  render(session_picker_view(sessions(), {0, 1}, 0, false, "", &router));
  REQUIRE(zone_count(router, MouseSurface::Session) == 2);

  router.begin_frame();
  render(history_search_view("", {"alpha", "beta"}, 0, &router));
  REQUIRE(zone_count(router, MouseSurface::History) == 2);

  router.begin_frame();
  render(copy_picker_view({{"answer", "a"}, {"code", "b"}}, 0, &router));
  REQUIRE(zone_count(router, MouseSurface::Copy) == 2);
}

TEST_CASE("hover renders a distinct band and clears when the pointer leaves") {
  MouseRouter router;
  router.begin_frame();
  auto view = [&] {
    return file_menu_view("", {"alpha", "beta"}, 0, &router);
  };
  render(view());
  REQUIRE(router.zones().size() == 2);
  const int hover_y = router.zones()[1].y_min;
  const int hover_x = router.zones()[1].x_min + 5;

  REQUIRE(router.handle(mouse_move(hover_x, hover_y)).kind == MouseInput::Kind::Moved);
  REQUIRE(router.hovered(MouseSurface::File, 1));

  router.begin_frame();
  ftxui::Screen hovered(90, 30);
  ftxui::Render(hovered, view());
  REQUIRE(hovered.CellAt(hover_x, hover_y).underlined);
  REQUIRE(hovered.CellAt(hover_x, router.zones()[0].y_min).underlined == false);

  REQUIRE(router.handle(mouse_move(0, router.zones()[0].y_min)).hit.valid() == false);
  router.begin_frame();
  ftxui::Screen cleared(90, 30);
  ftxui::Render(cleared, view());
  REQUIRE_FALSE(cleared.CellAt(hover_x, hover_y).underlined);
}

TEST_CASE("composer zones expose the text origin for cursor placement") {
  MouseRouter router;
  AppState state;
  GitStatus git;
  std::vector<std::string> lines = {"hello", "world"};
  router.begin_frame();
  render(composer_view(state, "/tmp", git, "title", -1, "", lines, 0, 0, "", 0, "", 0,
                       &router),
         90, 12);

  std::optional<MouseZone> composer;
  for (const auto& zone : router.zones()) {
    if (zone.surface == MouseSurface::Composer) composer = zone;
  }
  REQUIRE(composer.has_value());
  REQUIRE(composer->index == 1);
  REQUIRE(composer->column == 3);  // last row uses the "╰─ " prefix

  const auto hit = router.hit(composer->x_min + 3 + 2, composer->y_min);
  REQUIRE(hit.has_value());
  REQUIRE(hit->surface == MouseSurface::Composer);
  REQUIRE(hit->index == 1);
  REQUIRE(hit->column == 2);
}
