#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <filesystem>

#include "interaction/mention.hpp"

using booth::ModelChoice;

namespace {
std::vector<ModelChoice> models() {
  return {
      {"provider", "model-one", "[\"provider\",\"model-one\"]"},
      {"provider", "model-two", "[\"provider\",\"model-two\"]"},
  };
}
}

TEST_CASE("a leading model mention is removed and returned") {
  auto choices = models();
  std::string text = "^model-one explain this";
  const auto choice = booth::take_model_mention(text, choices);
  REQUIRE(choice.has_value());
  REQUIRE(choice->name == "model-one");
  REQUIRE(text == "explain this");

  std::string unknown = "^missing explain";
  REQUIRE_FALSE(booth::take_model_mention(unknown, choices).has_value());
  REQUIRE(unknown == "^missing explain");
}

TEST_CASE("model mentions expand anywhere into stable mN tags") {
  booth::MentionRegistry registry;
  const auto text = registry.expand("ask ^model-one and ^provider/model-two, then ^model-one.",
                                    models());
  REQUIRE(text == "ask <model agent=\"m1\" name=\"model-one\" provider=\"provider\" model=\"model-one\"/> and <model agent=\"m2\" "
                  "name=\"model-two\" provider=\"provider\" model=\"model-two\"/>, then <model agent=\"m1\" name=\"model-one\" provider=\"provider\" model=\"model-one\"/>.");
  REQUIRE(registry.mentions().size() == 2);
  REQUIRE(registry.preamble().find("m1: provider=\"provider\"") != std::string::npos);
  REQUIRE(registry.preamble().find("model=\"model-two\"") != std::string::npos);
}

TEST_CASE("mention autocomplete finds the token under the cursor") {
  const auto query = booth::active_mention("hello ^mod world", 10);
  REQUIRE(query.has_value());
  REQUIRE(query->start == 6);
  REQUIRE(query->prefix == "mod");
  REQUIRE_FALSE(booth::active_mention("hello world", 11).has_value());
}

TEST_CASE("mention suggestions filter and apply with a trailing space") {
  const auto suggestions = booth::mention_suggestions("mod", models());
  REQUIRE(suggestions.size() == 2);
  REQUIRE(suggestions[0].selector == "provider/model-one");
  const auto query = booth::active_mention("hello ^mod", 10);
  REQUIRE(query.has_value());
  const auto [text, cursor] = booth::apply_mention("hello ^mod", *query, "provider/model-two");
  REQUIRE(text == "hello ^provider/model-two ");
  REQUIRE(cursor == text.size());
}

TEST_CASE("model mentions support effort suffixes") {
  auto choices = models();
  booth::MentionRegistry registry;
  const auto text = registry.expand("use ^provider/model-one:high", choices, {"low", "high"});
  REQUIRE(text == "use <model agent=\"m1\" name=\"model-one · high\" provider=\"provider\" model=\"model-one\" reasoning_effort=\"high\"/>");
  REQUIRE(registry.mentions()[0].effort == "high");
  REQUIRE(registry.preamble().find("reasoning_effort=\"high\"") != std::string::npos);

  const auto rows = booth::mention_suggestions("provider/model-one:h", choices, {"low", "high"});
  REQUIRE(rows.size() == 1);
  REQUIRE(rows[0].selector == "provider/model-one:high");
}

TEST_CASE("mention punctuation is preserved after the tag") {
  booth::MentionRegistry registry;
  const auto text = registry.expand("ask ^model-one, then wait.", models());
  REQUIRE(text == "ask <model agent=\"m1\" name=\"model-one\" provider=\"provider\" model=\"model-one\"/>, then wait.");
}

TEST_CASE("model mention aliases persist per session") {
  const auto path = std::filesystem::temp_directory_path() /
                    ("booth-mentions-" + std::to_string(::getpid()) + ".json");
  booth::MentionRegistry first;
  first.expand("use ^model-one:high", models(), {"low", "high"});
  first.save(path);
  booth::MentionRegistry second;
  second.load(path);
  REQUIRE(second.mentions().size() == 1);
  REQUIRE(second.mentions()[0].agent == "m1");
  REQUIRE(second.mentions()[0].selector == "provider/model-one:high");
  std::filesystem::remove(path);
}
