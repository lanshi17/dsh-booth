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
  REQUIRE(added[1].new_line == 1);
  REQUIRE(added[2].text == "+ beta");
  REQUIRE(added[2].new_line == 2);

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

TEST_CASE("edit diffs keep shared context lines") {
  TranscriptEntry entry;
  entry.tool = "edit";
  entry.raw_input =
      R"({"file_path":"src/a.cpp","old_string":"a\nb\nc\nd\ne","new_string":"a\nb\nX\nd\ne"})";
  const auto lines = tool_diff(entry);
  REQUIRE(lines.size() == 7);
  REQUIRE(lines[1].text == "  a");
  REQUIRE(lines[2].text == "  b");
  REQUIRE(lines[3].text == "- c");
  REQUIRE(lines[4].text == "+ X");
  REQUIRE(lines[5].text == "  d");
  REQUIRE(lines[6].text == "  e");
}

TEST_CASE("long unchanged runs collapse to an ellipsis") {
  TranscriptEntry entry;
  entry.tool = "edit";
  entry.raw_input =
      R"({"file_path":"src/a.cpp","old_string":"1\n2\n3\n4\n5\n6\n7\n8\n9\n10","new_string":"1\n2\n3\n4\nFIVE\n6\n7\n8\n9\n10"})";
  const auto lines = tool_diff(entry);
  REQUIRE(lines[0].text == "src/a.cpp");
  REQUIRE(lines[1].text == "…");
  REQUIRE(lines[2].text == "  3");
  REQUIRE(lines[3].text == "  4");
  REQUIRE(lines[4].text == "- 5");
  REQUIRE(lines[5].text == "+ FIVE");
  REQUIRE(lines[6].text == "  6");
  REQUIRE(lines[7].text == "  7");
  REQUIRE(lines[8].text == "…");
}

TEST_CASE("an unchanged edit reports no changes") {
  TranscriptEntry entry;
  entry.tool = "edit";
  entry.raw_input = R"({"file_path":"src/a.cpp","old_string":"same","new_string":"same"})";
  const auto lines = tool_diff(entry);
  REQUIRE(lines.size() == 2);
  REQUIRE(lines[1].text == "  (no changes)");
}
