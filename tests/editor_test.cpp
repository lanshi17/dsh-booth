#include <catch2/catch_test_macros.hpp>

#include <string>

#include <ftxui/component/event.hpp>

#include "interaction/editor.hpp"

using booth::Editor;
using ftxui::Event;

TEST_CASE("the editor inserts, splits and walks lines") {
  std::string content;
  Editor editor(content);

  REQUIRE(editor.HandleEvent(Event::Character('a')));
  REQUIRE(editor.HandleEvent(Event::Character('b')));
  REQUIRE(content == "ab");
  REQUIRE(editor.HandleEvent(Event::Backspace));
  REQUIRE(content == "a");

  REQUIRE(editor.HandleEvent(Event::Special("\x1b\r")));
  REQUIRE(content == "a\n");
  REQUIRE(editor.HandleEvent(Event::Character('b')));
  REQUIRE(content == "a\nb");
  REQUIRE(editor.Lines() == std::vector<std::string>{"a", "b"});
  REQUIRE(editor.CursorLine() == 1);
  REQUIRE(editor.CursorColumn() == 1);

  REQUIRE(editor.HandleEvent(Event::ArrowUp));
  REQUIRE(editor.CursorLine() == 0);
  REQUIRE(editor.CursorColumn() == 1);
  REQUIRE(editor.HandleEvent(Event::Home));
  REQUIRE(editor.CursorColumn() == 0);
  REQUIRE(editor.HandleEvent(Event::End));
  REQUIRE(editor.CursorColumn() == 1);
  REQUIRE(editor.HandleEvent(Event::Delete));
  REQUIRE(content == "ab");
  REQUIRE_FALSE(editor.HandleEvent(Event::Return));

  editor.SetContent("one\ntwo");
  REQUIRE(editor.CursorLine() == 1);
  REQUIRE(editor.CursorColumn() == 3);
  editor.Reset();
  REQUIRE(content.empty());
  REQUIRE(editor.Lines() == std::vector<std::string>{""});
}
