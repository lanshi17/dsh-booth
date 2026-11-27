#include <catch2/catch_test_macros.hpp>

#include <string>

#include "interaction/loop_controller.hpp"

using namespace booth;

TEST_CASE("loop args parse counts, durations and conditions") {
  auto parsed = parse_loop_args("10 fix the tests");
  REQUIRE(parsed.error.empty());
  REQUIRE(parsed.limit.has_value());
  REQUIRE_FALSE(parsed.limit->duration);
  REQUIRE(parsed.limit->iterations == 10);
  REQUIRE(parsed.prompt == "fix the tests");

  parsed = parse_loop_args("10m poll");
  REQUIRE(parsed.limit.has_value());
  REQUIRE(parsed.limit->duration);
  REQUIRE(parsed.limit->duration_ms == 600000);
  REQUIRE(parsed.prompt == "poll");

  parsed = parse_loop_args("1h30m");
  REQUIRE(parsed.limit.has_value());
  REQUIRE(parsed.limit->duration_ms == 5400000);
  REQUIRE(parsed.prompt.empty());

  parsed = parse_loop_args("5 --while 'bun test' keep going");
  REQUIRE(parsed.error.empty());
  REQUIRE(parsed.condition.has_value());
  REQUIRE_FALSE(parsed.condition->until);
  REQUIRE(parsed.condition->command == "bun test");
  REQUIRE(parsed.prompt == "keep going");

  parsed = parse_loop_args("--until=\"git diff --quiet\" fix");
  REQUIRE(parsed.error.empty());
  REQUIRE(parsed.condition->until);
  REQUIRE(parsed.condition->command == "git diff --quiet");
  REQUIRE(parsed.prompt == "fix");
}

TEST_CASE("loop args reject malformed limits and flags") {
  REQUIRE_FALSE(parse_loop_args("0").error.empty());
  REQUIRE_FALSE(parse_loop_args("-1").error.empty());
  REQUIRE_FALSE(parse_loop_args("10x").error.empty());
  REQUIRE_FALSE(parse_loop_args("--nope run").error.empty());
  REQUIRE_FALSE(parse_loop_args("--while").error.empty());
  REQUIRE_FALSE(parse_loop_args("--while 'unterminated").error.empty());
  REQUIRE_FALSE(parse_loop_args("--while 'a' --until 'b'").error.empty());
}

TEST_CASE("loop mode consumes its iteration budget and pauses/resets") {
  AppState state;
  std::string notice;
  loop_command(state, "/loop 2 repeat", notice);
  REQUIRE(state.loop_enabled);
  REQUIRE(state.loop_pending_prompt == "repeat");
  REQUIRE(state.loop_iterations == 1);

  loop_turn_finished(state);
  auto step = loop_next(state);
  REQUIRE(step.prompt.has_value());
  REQUIRE(*step.prompt == "repeat");
  REQUIRE(state.loop_iterations == 0);

  loop_turn_finished(state);
  step = loop_next(state);
  REQUIRE_FALSE(step.prompt.has_value());
  REQUIRE_FALSE(state.loop_enabled);
  REQUIRE(step.notice.find("Loop limit reached") != std::string::npos);

  loop_command(state, "/loop 3", notice);
  REQUIRE(state.loop_enabled);
  loop_note_prompt(state, "next prompt");
  loop_turn_finished(state);
  loop_pause(state, notice);
  REQUIRE(state.loop_paused);
  REQUIRE_FALSE(loop_next(state).prompt.has_value());
  loop_reset(state);
  REQUIRE_FALSE(state.loop_enabled);
  REQUIRE(state.loop_prompt.empty());
}

TEST_CASE("loop conditions gate each iteration") {
  AppState state;
  std::string notice;
  loop_command(state, "/loop 3 --while 'true' go", notice);
  REQUIRE(state.loop_condition == "true");
  REQUIRE_FALSE(state.loop_until);
  REQUIRE(state.loop_pending_prompt == "go");

  loop_turn_finished(state);
  auto step = loop_next(state);
  REQUIRE(step.condition.has_value());
  REQUIRE(*step.condition == "true");
  REQUIRE(state.loop_condition_running);

  step = loop_condition_result(state, 0);
  REQUIRE(step.prompt.has_value());
  REQUIRE(*step.prompt == "go");

  loop_turn_finished(state);
  step = loop_next(state);
  REQUIRE(step.condition.has_value());
  step = loop_condition_result(state, 1);
  REQUIRE_FALSE(state.loop_enabled);
  REQUIRE(step.notice.find("Loop condition failed") != std::string::npos);
}

TEST_CASE("shell conditions report exit status and time out") {
  REQUIRE(run_shell_status("true", "/tmp") == 0);
  REQUIRE(run_shell_status("exit 7", "/tmp") == 7);
  REQUIRE(run_shell_status("sleep 5", "/tmp", 50) == 124);
}
