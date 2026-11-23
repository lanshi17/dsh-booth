#include <catch2/catch_test_macros.hpp>

#include <string>
#include <vector>

#include "interaction/mouse_actions.hpp"

using namespace booth;

namespace {
MouseActions make_actions(AppState& state, Editor& editor, SlashMenu& slash, FileMenu& files,
                          ModelPicker& model, SessionPicker& sessions, HistorySearch& history,
                          CopyPicker& copy, MentionMenu& mention) {
  return MouseActions{state, editor, slash, files, model, sessions, history, copy, mention,
                      [] {}, [](const ModelChoice&) {}, [](const SessionRow&) {},
                      [](std::size_t, std::size_t) {}, [](std::size_t) {}, [](const char*) {},
                      [] {}};
}
}

TEST_CASE("mouse action dispatch covers mention and composer targets") {
  AppState state;
  std::string content = "hello ^mod";
  state.composer = content;
  Editor editor(content);
  SlashMenu slash;
  FileMenu files;
  ModelPicker model;
  SessionPicker sessions;
  HistorySearch history;
  CopyPicker copy;
  MentionMenu mention;
  mention.update({content}, 0, content.size(), {{"p", "mod", "[\"p\",\"mod\"]"}}, {});
  auto actions = make_actions(state, editor, slash, files, model, sessions, history, copy, mention);

  MouseHit hit{MouseSurface::Mention, 0, 0, ""};
  REQUIRE(click_mouse(hit, actions));
  REQUIRE(state.composer == "hello ^p/mod ");
  REQUIRE(editor.CursorOffset() == state.composer.size());

  std::size_t cursor_line = 99, cursor_col = 99;
  actions.cursor_to = [&](std::size_t line, std::size_t col) { cursor_line = line; cursor_col = col; };
  REQUIRE(click_mouse(MouseHit{MouseSurface::Composer, 0, 3, ""}, actions));
  REQUIRE(cursor_line == 0);
  REQUIRE(cursor_col == 3);
}

TEST_CASE("mouse action dispatch answers permission buttons by index") {
  AppState state;
  std::string content;
  Editor editor(content);
  SlashMenu slash;
  FileMenu files;
  ModelPicker model;
  SessionPicker sessions;
  HistorySearch history;
  CopyPicker copy;
  MentionMenu mention;
  auto actions = make_actions(state, editor, slash, files, model, sessions, history, copy, mention);
  std::string chosen;
  actions.permission = [&](const char* kind) { chosen = kind; };
  REQUIRE(click_mouse(MouseHit{MouseSurface::Permission, 1, 0, ""}, actions));
  REQUIRE(chosen == "allow_always");
  REQUIRE(click_mouse(MouseHit{MouseSurface::Permission, 3, 0, ""}, actions));
  REQUIRE(chosen == "reject_always");
}
