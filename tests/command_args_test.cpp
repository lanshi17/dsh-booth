#include <catch2/catch_test_macros.hpp>

#include <string>

#include "interaction/command_args.hpp"

using namespace booth;

TEST_CASE("argument queries canonicalize aliases and keep the prefix") {
  const auto effort = argument_query("/reasoning hi");
  REQUIRE(effort.has_value());
  REQUIRE(effort->command == "/effort");
  REQUIRE(effort->prefix == "hi");
  REQUIRE_FALSE(argument_query("/status").has_value());
  REQUIRE_FALSE(argument_query("/notify").has_value());
  REQUIRE_FALSE(argument_query("notify on").has_value());
  REQUIRE_FALSE(argument_query("/queue line\nline").has_value());
}

TEST_CASE("argument suggestions cover fixed, reasoning and model values") {
  AppState state;
  state.reasoning.choices = {{"", "Low", "low"}, {"", "High", "high"}};
  state.models = {{"p", "one", "v1"}, {"p", "two", "v2"}};

  auto notify = argument_query("/notify on");
  REQUIRE(notify.has_value());
  const auto notify_matches = matching_arguments(*notify, state);
  REQUIRE(notify_matches.size() == 1);
  REQUIRE(notify_matches[0].value == "on");

  state.composer = "/effort ";
  auto effort = argument_query(state.composer);
  REQUIRE(effort.has_value());
  const auto efforts = matching_arguments(*effort, state);
  REQUIRE(efforts.size() == 3);
  REQUIRE(efforts[0].value == "next");
  REQUIRE(efforts[1].value == "Low");

  state.composer = "/copy ";
  auto copy = argument_query(state.composer);
  REQUIRE(copy.has_value());
  const auto copies = matching_arguments(*copy, state);
  REQUIRE(copies.size() == 3);
  REQUIRE(copies[0].value == "answer");
  REQUIRE(copies[2].value == "line");

  state.composer = "/model t";
  auto model = argument_query(state.composer);
  REQUIRE(model.has_value());
  const auto models = matching_arguments(*model, state);
  REQUIRE(models.size() == 1);
  REQUIRE(models[0].value == "two");
  REQUIRE(models[0].description.find("v2") != std::string::npos);
}
