#include <catch2/catch_test_macros.hpp>

#include <iostream>
#include <sstream>
#include <string>

#include "interaction/copy_picker.hpp"

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
  REQUIRE(picker.items().size() == 2);
  REQUIRE(picker.items()[0].label.rfind("answer ·", 0) == 0);
  REQUIRE(picker.items()[1].label == "code · cpp");
  REQUIRE(picker.items()[1].text == "int x = 1;\n");
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
