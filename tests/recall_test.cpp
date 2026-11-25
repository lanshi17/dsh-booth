#include <catch2/catch_test_macros.hpp>

#include <string>
#include <vector>

#include "interaction/recall.hpp"

using namespace booth;

TEST_CASE("cleared drafts recall before submitted prompt history") {
  PromptRecall recall;
  const std::vector<std::string> history{"old", "new"};
  recall.clear("saved draft");
  REQUIRE(recall.up(history) == "saved draft");
  REQUIRE(recall.up(history) == "new");
  REQUIRE(recall.up(history) == "old");
  REQUIRE_FALSE(recall.up(history).has_value());
  REQUIRE(recall.down(history) == "new");
  REQUIRE(recall.down(history) == "saved draft");
  REQUIRE(recall.down(history) == "");
  REQUIRE_FALSE(recall.down(history).has_value());
}

TEST_CASE("newer cleared drafts stay newest and reset navigation") {
  PromptRecall recall;
  const std::vector<std::string> history{"history"};
  recall.clear("older");
  recall.clear("newer");
  REQUIRE(recall.up(history) == "newer");
  recall.reset();
  REQUIRE(recall.up(history) == "newer");
  REQUIRE(recall.up(history) == "older");
  REQUIRE(recall.up(history) == "history");
}

TEST_CASE("recall keeps the newest hundred cleared drafts") {
  PromptRecall recall;
  for (int index = 0; index < 120; ++index) recall.clear(std::to_string(index));
  for (int index = 119; index >= 20; --index) REQUIRE(recall.up({}) == std::to_string(index));
  REQUIRE_FALSE(recall.up({}).has_value());
}
