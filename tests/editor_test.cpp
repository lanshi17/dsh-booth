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

TEST_CASE("typing groups into one undo and redo restores it") {
  std::string content;
  Editor editor(content);
  for (const char ch : std::string("abc")) {
    REQUIRE(editor.HandleEvent(Event::Character(ch)));
  }
  REQUIRE(content == "abc");

  REQUIRE(editor.HandleEvent(Event::Special("\x1a")));
  REQUIRE(content.empty());
  REQUIRE(editor.HandleEvent(Event::Special("\x1bz")));
  REQUIRE(content == "abc");
}

TEST_CASE("word motions and kill/yank work at glyph boundaries") {
  std::string content = "one two";
  Editor editor(content);
  REQUIRE(editor.CursorColumn() == 7);

  REQUIRE(editor.HandleEvent(Event::ArrowLeftCtrl));
  REQUIRE(editor.CursorColumn() == 4);
  REQUIRE(editor.HandleEvent(Event::ArrowLeftCtrl));
  REQUIRE(editor.CursorColumn() == 0);
  REQUIRE(editor.HandleEvent(Event::ArrowRightCtrl));
  REQUIRE(editor.CursorColumn() == 4);
  REQUIRE(editor.HandleEvent(Event::End));
  REQUIRE(editor.CursorColumn() == 7);

  REQUIRE(editor.HandleEvent(Event::Special("\x17")));
  REQUIRE(content == "one ");
  REQUIRE(editor.HandleEvent(Event::Special("\x19")));
  REQUIRE(content == "one two");
}

TEST_CASE("kill to line start and end is reversible with yank") {
  std::string content = "alpha beta";
  Editor editor(content);
  REQUIRE(editor.HandleEvent(Event::Home));
  REQUIRE(editor.HandleEvent(Event::ArrowRight));
  REQUIRE(editor.HandleEvent(Event::Special("\x0b")));
  REQUIRE(content == "a");
  REQUIRE(editor.HandleEvent(Event::Special("\x19")));
  REQUIRE(content == "alpha beta");

  REQUIRE(editor.HandleEvent(Event::End));
  REQUIRE(editor.HandleEvent(Event::Special("\x15")));
  REQUIRE(content.empty());
  REQUIRE(editor.HandleEvent(Event::Special("\x19")));
  REQUIRE(content == "alpha beta");
}

TEST_CASE("vim normal mode edits, pastes and undoes") {
  std::string content;
  Editor editor(content);
  editor.EnableVim(true);
  REQUIRE(editor.ModeLabel() == std::string("INSERT"));

  for (const char ch : std::string("abc")) REQUIRE(editor.HandleEvent(Event::Character(ch)));
  REQUIRE(editor.HandleEvent(Event::Escape));
  REQUIRE(editor.VimNormal());
  REQUIRE(editor.ModeLabel() == std::string("NORMAL"));

  REQUIRE(editor.HandleEvent(Event::Character('0')));
  REQUIRE(editor.HandleEvent(Event::Character('x')));
  REQUIRE(content == "bc");

  REQUIRE(editor.HandleEvent(Event::Character('A')));
  REQUIRE_FALSE(editor.VimNormal());
  REQUIRE(editor.HandleEvent(Event::Character('d')));
  REQUIRE(content == "bcd");

  REQUIRE(editor.HandleEvent(Event::Escape));
  REQUIRE(editor.HandleEvent(Event::Character('d')));
  REQUIRE(editor.HandleEvent(Event::Character('d')));
  REQUIRE(content.empty());

  REQUIRE(editor.HandleEvent(Event::Character('u')));
  REQUIRE(content == "bcd");
  REQUIRE(editor.HandleEvent(Event::Character('p')));
  REQUIRE(content.find("bcd") != std::string::npos);
}
