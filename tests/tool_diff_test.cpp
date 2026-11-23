#include <catch2/catch_test_macros.hpp>

#include "interaction/tool_diff.hpp"

using namespace booth;

TEST_CASE("edit arguments become removed and added lines") {
  TranscriptEntry entry;
  entry.tool = "edit";
  entry.raw_input = R"({"file_path":"src/a.cpp","old_string":"old\nline","new_string":"new"})";
  const auto lines = tool_diff(entry);
  REQUIRE(lines.size() == 4);
  REQUIRE(lines[0].text == "src/a.cpp");
  REQUIRE(lines[0].change == 0);
  REQUIRE(lines[1].text == "- old");
  REQUIRE(lines[1].change == -1);
  REQUIRE(lines[2].text == "- line");
  REQUIRE(lines[3].text == "+ new");
  REQUIRE(lines[3].change == 1);
}

TEST_CASE("write and patch arguments become diffs") {
  TranscriptEntry write;
  write.tool = "write";
  write.raw_input = R"({"file_path":"new.txt","content":"alpha\nbeta"})";
  const auto added = tool_diff(write);
  REQUIRE(added.size() == 3);
  REQUIRE(added[1].text == "+ alpha");
  REQUIRE(added[2].text == "+ beta");

  TranscriptEntry patch;
  patch.tool = "apply_patch";
  patch.raw_input = R"({"input":"@@ hunk\n-old\n+new"})";
  const auto lines = tool_diff(patch);
  REQUIRE(lines.size() == 3);
  REQUIRE(lines[1].change == -1);
  REQUIRE(lines[2].change == 1);
}

TEST_CASE("non-edit tools produce no diff") {
  TranscriptEntry entry;
  entry.raw_input = R"({"command":"ls"})";
  REQUIRE(tool_diff(entry).empty());
  entry.raw_input = "not json";
  REQUIRE(tool_diff(entry).empty());
}
