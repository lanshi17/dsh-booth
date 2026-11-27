#include <catch2/catch_test_macros.hpp>

#include <iostream>
#include <sstream>
#include <string>

#include "interaction/copy_picker.hpp"
#include "interaction/link_picker.hpp"

using namespace booth;

TEST_CASE("copy picker lists assistant answers and fenced code") {
  AppState state;
  TranscriptEntry answer;
  answer.kind = EntryKind::Assistant;
  answer.text = "Here is the code:\n\n```cpp\nint x = 1;\n```\n";
  state.entries.push_back(answer);
  TranscriptEntry user;
  user.kind = EntryKind::User;
  user.text = "question";
  state.entries.push_back(user);

  CopyPicker picker;
  REQUIRE(picker.open(state));
  REQUIRE(picker.items().size() == 3);
  REQUIRE(picker.items()[0].label == "prompt · question");
  REQUIRE(picker.items()[1].label.rfind("answer ·", 0) == 0);
  REQUIRE(picker.items()[2].label == "code · cpp");
  REQUIRE(picker.items()[2].text == "int x = 1;\n");
}

TEST_CASE("copy picker includes user prompts") {
  AppState state;
  TranscriptEntry prompt;
  prompt.kind = EntryKind::User;
  prompt.text = "please fix the build";
  state.entries.push_back(prompt);
  CopyPicker picker;
  REQUIRE(picker.open(state));
  REQUIRE(picker.items().size() == 1);
  REQUIRE(picker.items()[0].label.rfind("prompt ·", 0) == 0);
  REQUIRE(picker.items()[0].text == "please fix the build");
}

TEST_CASE("copy picker navigates, copies and cancels") {
  AppState state;
  TranscriptEntry answer;
  answer.kind = EntryKind::Assistant;
  answer.text = "first\nsecond";
  state.entries.push_back(answer);

  CopyPicker picker;
  REQUIRE(picker.open(state));
  std::ostringstream captured;
  auto* previous = std::cout.rdbuf(captured.rdbuf());
  ftxui::Event enter = ftxui::Event::Return;
  REQUIRE(picker.key(enter, state));
  std::cout.rdbuf(previous);
  REQUIRE_FALSE(picker.visible());
  REQUIRE(state.notice.find("copied") == 0);
  REQUIRE(captured.str().find("\x1b]52;c;") != std::string::npos);

  REQUIRE(picker.open(state));
  ftxui::Event escape = ftxui::Event::Escape;
  REQUIRE(picker.key(escape, state));
  REQUIRE_FALSE(picker.visible());
}

TEST_CASE("copy picker reports an empty transcript") {
  AppState state;
  CopyPicker picker;
  REQUIRE_FALSE(picker.open(state));
  REQUIRE_FALSE(picker.visible());
}

TEST_CASE("copy picker includes tool output") {
  AppState state;
  TranscriptEntry tool;
  tool.kind = EntryKind::Tool;
  tool.tool = "bash";
  tool.output = "echo hi\n";
  state.entries.push_back(tool);
  CopyPicker picker;
  REQUIRE(picker.open(state));
  REQUIRE(picker.items().size() == 1);
  REQUIRE(picker.items()[0].label.rfind("tool · bash", 0) == 0);
  REQUIRE(picker.items()[0].text == "echo hi\n");
}

TEST_CASE("copy picker includes shell commands and blockquotes") {
  AppState state;
  TranscriptEntry tool;
  tool.kind = EntryKind::Tool;
  tool.tool = "bash";
  tool.input = "printf hi";
  tool.output = "hi";
  state.entries.push_back(tool);
  TranscriptEntry answer;
  answer.kind = EntryKind::Assistant;
  answer.text = "intro\n> quoted line\n> second line\noutro";
  state.entries.push_back(answer);

  CopyPicker picker;
  REQUIRE(picker.open(state));
  REQUIRE(picker.items().size() == 4);
  REQUIRE(picker.items()[0].label.rfind("answer ·", 0) == 0);
  REQUIRE(picker.items()[1].label.rfind("quote ·", 0) == 0);
  REQUIRE(picker.items()[1].text == "quoted line\nsecond line");
  REQUIRE(picker.items()[2].label.rfind("command · bash", 0) == 0);
  REQUIRE(picker.items()[2].text == "printf hi");
  REQUIRE(picker.items()[3].label.rfind("tool · bash", 0) == 0);
}

TEST_CASE("copy picker flattens read output") {
  AppState state;
  TranscriptEntry read;
  read.kind = EntryKind::Tool;
  read.tool = "read";
  read.output =
      "<path>src/a.cpp</path>\n<type>file</type>\n<content>\n1: one\n(End of file)\n</content>";
  state.entries.push_back(read);
  CopyPicker picker;
  REQUIRE(picker.open(state));
  REQUIRE(picker.items().size() == 1);
  REQUIRE(picker.items()[0].label.rfind("read ·", 0) == 0);
  REQUIRE(picker.items()[0].text.find("1: one") != std::string::npos);
  REQUIRE(picker.items()[0].text.find("<content>") == std::string::npos);
}

TEST_CASE("copy picker filters by label") {
  AppState state;
  TranscriptEntry first;
  first.kind = EntryKind::Assistant;
  first.text = "first";
  state.entries.push_back(first);
  TranscriptEntry second;
  second.kind = EntryKind::Assistant;
  second.text = "second";
  state.entries.push_back(second);
  CopyPicker picker;
  REQUIRE(picker.open(state));
  REQUIRE(picker.matches().size() == 2);
  ftxui::Event f = ftxui::Event::Character('f');
  REQUIRE(picker.key(f, state));
  REQUIRE(picker.query() == "f");
  REQUIRE(picker.matches().size() == 1);
  REQUIRE(picker.items()[picker.matches()[0]].text == "first");
  ftxui::Event back = ftxui::Event::Backspace;
  REQUIRE(picker.key(back, state));
  REQUIRE(picker.query().empty());
  REQUIRE(picker.matches().size() == 2);
}

TEST_CASE("copy helpers extract the last code block, shell command and link") {
  AppState state;
  TranscriptEntry answer;
  answer.kind = EntryKind::Assistant;
  answer.text = "first\n```sh\necho one\n```\nsecond\n```cpp\nint two;\n```\nhttps://example.com/a";
  state.entries.push_back(answer);
  TranscriptEntry tool;
  tool.kind = EntryKind::Tool;
  tool.tool = "bash";
  tool.input = "echo last";
  state.entries.push_back(tool);

  REQUIRE(last_code_block(state) == "int two;\n");
  REQUIRE(last_shell_command(state) == "echo last");
  REQUIRE(last_link(state) == "https://example.com/a");
}

TEST_CASE("copy helpers return nullopt without matching content") {
  AppState state;
  TranscriptEntry answer;
  answer.kind = EntryKind::Assistant;
  answer.text = "plain answer";
  state.entries.push_back(answer);
  REQUIRE_FALSE(last_code_block(state).has_value());
  REQUIRE_FALSE(last_shell_command(state).has_value());
  REQUIRE_FALSE(last_link(state).has_value());
}
