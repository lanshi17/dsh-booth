#include <catch2/catch_test_macros.hpp>

#include <sys/stat.h>
#include <unistd.h>

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <optional>
#include <string>
#include <vector>

#include <ftxui/component/event.hpp>

#include "interaction/editor.hpp"
#include "interaction/external_editor.hpp"

using booth::Editor;
using ftxui::Event;

namespace {

struct ScratchDir {
  std::string path;

  explicit ScratchDir(const std::string& prefix) {
    std::string pattern = "/tmp/" + prefix + "-XXXXXX";
    std::vector<char> buffer(pattern.begin(), pattern.end());
    buffer.push_back('\0');
    if (::mkdtemp(buffer.data()) != nullptr) path = buffer.data();
    else path.clear();
  }

  ~ScratchDir() {
    if (!path.empty()) {
      std::filesystem::permissions(path, std::filesystem::perms::owner_all,
                                   std::filesystem::perm_options::add);
      std::filesystem::remove_all(path);
    }
  }
};

struct RestoreEnv {
  const char* name;
  std::optional<std::string> previous;

  explicit RestoreEnv(const char* variable) : name(variable) {
    const char* value = std::getenv(variable);
    if (value != nullptr) previous = std::string(value);
  }

  void set(const std::string& value) { ::setenv(name, value.c_str(), 1); }
  void unset() { ::unsetenv(name); }

  ~RestoreEnv() {
    if (previous) ::setenv(name, previous->c_str(), 1);
    else ::unsetenv(name);
  }
};

std::string write_editor(const std::string& dir, const std::string& body) {
  const std::string path = dir + "/editor.sh";
  std::ofstream script(path, std::ios::trunc);
  script << "#!/bin/sh\n" << body << "\n";
  script.close();
  ::chmod(path.c_str(), 0700);
  return path;
}

}  // namespace

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

TEST_CASE("Alt+Y cycles older kills after a yank") {
  std::string content = "one two three";
  Editor editor(content);
  REQUIRE(editor.HandleEvent(Event::Special("\x17")));
  REQUIRE(editor.HandleEvent(Event::Special("\x17")));
  REQUIRE(content == "one ");
  REQUIRE(editor.HandleEvent(Event::Special("\x19")));
  REQUIRE(content == "one two ");
  REQUIRE(editor.HandleEvent(Event::Special("\x1by")));
  REQUIRE(content == "one three");
  REQUIRE(editor.HandleEvent(Event::Special("\x1by")));
  REQUIRE(content == "one two ");
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

TEST_CASE("large pastes collapse into expandable atomic markers") {
  std::string content;
  Editor editor(content);
  std::string large;
  for (int line = 0; line < 12; ++line) large += "line " + std::to_string(line) + "\n";
  large.pop_back();
  editor.Paste(large);
  REQUIRE(content == "[Paste #1, +12 lines]");
  REQUIRE(editor.Expanded(content) == large);

  editor.HandleEvent(Event::Backspace);
  REQUIRE(content.empty());

  editor.Paste(std::string(1001, 'x'));
  REQUIRE(content == "[Paste #2, 1001 chars]");
  REQUIRE(editor.Expanded(content) == std::string(1001, 'x'));

  editor.Reset();
  editor.SetContent("[Paste #1, +12 lines]");
  REQUIRE(editor.Expanded(content) == large);
  editor.Reset();
  editor.Paste("short\ntext");
  REQUIRE(content == "short\ntext");
}

TEST_CASE("vim named registers yank and put whole lines") {
  std::string content = "one\ntwo\nthree";
  Editor editor(content);
  editor.EnableVim(true);
  REQUIRE(editor.HandleEvent(Event::Escape));
  for (const char key : std::string("gg\"ayyj\"ap")) REQUIRE(editor.HandleEvent(Event::Character(key)));
  REQUIRE(content == "one\ntwo\none\nthree");
}

TEST_CASE("vim uppercase named registers append") {
  std::string content = "one\ntwo\nthree";
  Editor editor(content);
  editor.EnableVim(true);
  REQUIRE(editor.HandleEvent(Event::Escape));
  for (const char key : std::string("gg\"ayyj\"Ayyj\"ap")) REQUIRE(editor.HandleEvent(Event::Character(key)));
  REQUIRE(content == "one\ntwo\nthree\none\ntwo");
}

TEST_CASE("vim numbered, small-delete and black-hole registers") {
  std::string content = "alpha beta\ngamma delta";
  Editor editor(content);
  editor.EnableVim(true);
  REQUIRE(editor.HandleEvent(Event::Escape));
  for (const char key : std::string("ggyy")) REQUIRE(editor.HandleEvent(Event::Character(key)));
  for (const char key : std::string("dd\"_dd")) REQUIRE(editor.HandleEvent(Event::Character(key)));
  REQUIRE(content.empty());
  for (const char key : std::string("\"0p")) REQUIRE(editor.HandleEvent(Event::Character(key)));
  REQUIRE(content == "alpha beta");
}

TEST_CASE("vim numbered delete and small-delete registers") {
  {
    std::string content = "one\ntwo";
    Editor editor(content);
    editor.EnableVim(true);
    REQUIRE(editor.HandleEvent(Event::Escape));
    for (const char key : std::string("ggdd")) REQUIRE(editor.HandleEvent(Event::Character(key)));
    for (const char key : std::string("\"1p")) REQUIRE(editor.HandleEvent(Event::Character(key)));
    REQUIRE(content == "two\none");
  }
  {
    std::string content = "ab";
    Editor editor(content);
    editor.EnableVim(true);
    REQUIRE(editor.HandleEvent(Event::Escape));
    REQUIRE(editor.HandleEvent(Event::Character('x')));
    for (const char key : std::string("\"-p")) REQUIRE(editor.HandleEvent(Event::Character(key)));
    REQUIRE(content == "ab");
  }
}

TEST_CASE("vim register prefix is echoed while pending") {
  std::string content = "one";
  Editor editor(content);
  editor.EnableVim(true);
  REQUIRE(editor.HandleEvent(Event::Escape));
  REQUIRE(editor.HandleEvent(Event::Character('"')));
  REQUIRE(editor.HandleEvent(Event::Character('a')));
  REQUIRE(editor.ModeLabel().find("\"a") != std::string::npos);
  REQUIRE(editor.HandleEvent(Event::Escape));
  REQUIRE(editor.VimPending().empty());
}

TEST_CASE("external editor seeds, reads back and trims the scratch file") {
  ScratchDir dir("booth-editor");
  REQUIRE(!dir.path.empty());
  RestoreEnv visual("VISUAL");
  RestoreEnv editor("EDITOR");
  visual.unset();
  editor.set(write_editor(dir.path, "printf 'line one\\nline two\\n' > \"$1\""));

  const auto edited = booth::edit_externally("draft");
  REQUIRE(edited.has_value());
  REQUIRE(*edited == "line one\nline two");
}

TEST_CASE("external editor falls back when TMPDIR is not writable") {
  ScratchDir dir("booth-editor-ro");
  REQUIRE(!dir.path.empty());
  const std::string read_only = dir.path + "/read-only";
  REQUIRE(std::filesystem::create_directory(read_only));
  REQUIRE(::chmod(read_only.c_str(), 0500) == 0);

  RestoreEnv tmp("TMPDIR");
  RestoreEnv visual("VISUAL");
  RestoreEnv editor("EDITOR");
  tmp.set(read_only);
  visual.unset();
  editor.set(write_editor(dir.path, "printf 'fallback text' > \"$1\""));

  const auto edited = booth::edit_externally("draft");
  REQUIRE(edited.has_value());
  REQUIRE(*edited == "fallback text");
}

TEST_CASE("external editor reports a failing editor") {
  ScratchDir dir("booth-editor-fail");
  REQUIRE(!dir.path.empty());
  RestoreEnv visual("VISUAL");
  RestoreEnv editor("EDITOR");
  visual.unset();
  editor.set(write_editor(dir.path, "exit 3"));

  REQUIRE_FALSE(booth::edit_externally("draft").has_value());
}
