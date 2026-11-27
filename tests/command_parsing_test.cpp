#include <catch2/catch_test_macros.hpp>

#include <filesystem>
#include <fstream>
#include <string>

#include <unistd.h>

#include "interaction/btw_history.hpp"
#include "interaction/command_parsing.hpp"

using namespace booth;

TEST_CASE("compact requests map OMP modes onto dsh compaction") {
  auto request = parse_compact_request("");
  REQUIRE(request.line == "/compact");
  REQUIRE(request.prefix.empty());
  REQUIRE(request.error.empty());

  request = parse_compact_request("soft");
  REQUIRE(request.prefix == "compaction (soft): ");
  request = parse_compact_request("remote");
  REQUIRE(request.prefix.find("local compaction") != std::string::npos);
  request = parse_compact_request("handoff");
  REQUIRE(request.prefix == "handoff checkpoint: ");
  request = parse_compact_request("review the auth path");
  REQUIRE(request.prefix.find("focus instructions ignored") != std::string::npos);
  request = parse_compact_request("snapcompact");
  REQUIRE(request.error.find("unavailable") != std::string::npos);
  request = parse_compact_request("SOFT");
  REQUIRE(request.prefix == "compaction (soft): ");
  request = parse_compact_request("Handoff review");
  REQUIRE(request.prefix.find("handoff checkpoint") == 0);
  REQUIRE(request.prefix.find("focus instructions ignored") != std::string::npos);
}

TEST_CASE("goal requests map onto dsh goal verbs and reject budgets") {
  auto request = parse_goal_request("");
  REQUIRE(request.line == "/goal");
  request = parse_goal_request("show");
  REQUIRE(request.line == "/goal");
  request = parse_goal_request("set ship it");
  REQUIRE(request.line == "/goal ship it");
  request = parse_goal_request("drop");
  REQUIRE(request.line == "/goal clear");
  request = parse_goal_request("pause");
  REQUIRE(request.line == "/goal pause");
  request = parse_goal_request("budget 1000");
  REQUIRE(request.error.find("no token budget") != std::string::npos);
  request = parse_goal_request("SET ship it");
  REQUIRE(request.line == "/goal ship it");
  request = parse_goal_request("set");
  REQUIRE(request.error == "usage: /goal set <objective>");
  request = parse_goal_request("DROP");
  REQUIRE(request.line == "/goal clear");
}

TEST_CASE("btw history round-trips questions and answers") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-btw-" + std::to_string(::getpid()));
  std::filesystem::remove_all(dir);
  const auto path = dir / "btw-history.jsonl";
  REQUIRE(btw_history_text(path, "s-1").find("no BTW history") != std::string::npos);
  append_btw_history(path, "s-1", "what changed?", "the parser");
  const auto text = btw_history_text(path, "s-1");
  REQUIRE(text.find("Q: what changed?") != std::string::npos);
  REQUIRE(text.find("A: the parser") != std::string::npos);
  REQUIRE(btw_history_text(path, "s-2").find("no BTW history") != std::string::npos);
  std::filesystem::remove_all(dir);
}
