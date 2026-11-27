#include "interaction/loop_controller.hpp"

#include <fcntl.h>
#include <spawn.h>
#include <sys/wait.h>
#include <unistd.h>

#include <algorithm>
#include <cctype>
#include <cerrno>
#include <chrono>
#include <cstring>
#include <thread>
#include <vector>

extern char** environ;

namespace booth {
namespace {

using Clock = std::chrono::steady_clock;

std::string trim(std::string_view text) {
  const auto first = text.find_first_not_of(" \t\r\n");
  if (first == std::string_view::npos) return {};
  const auto last = text.find_last_not_of(" \t\r\n");
  return std::string(text.substr(first, last - first + 1));
}

long long unit_ms(std::string_view unit) {
  if (unit == "s" || unit == "sec" || unit == "secs" || unit == "second" || unit == "seconds") {
    return 1000;
  }
  if (unit == "m" || unit == "min" || unit == "mins" || unit == "minute" || unit == "minutes") {
    return 60000;
  }
  if (unit == "h" || unit == "hr" || unit == "hrs" || unit == "hour" || unit == "hours") {
    return 3600000;
  }
  return 0;
}

/// Read one shell word, honoring single/double quotes and backslash escapes.
bool read_shell_word(std::string_view text, std::string& value, std::string_view& rest) {
  std::size_t at = text.find_first_not_of(" \t");
  if (at == std::string_view::npos) {
    rest = {};
    return false;
  }
  value.clear();
  char quote = 0;
  while (at < text.size()) {
    const char ch = text[at++];
    if (quote == 0) {
      if (ch == ' ' || ch == '\t') break;
      if (ch == '\'' || ch == '"') {
        quote = ch;
        continue;
      }
      if (ch == '\\' && at < text.size()) {
        value += text[at++];
        continue;
      }
      value += ch;
      continue;
    }
    if (ch == quote) {
      quote = 0;
      continue;
    }
    if (ch == '\\' && quote == '"' && at < text.size()) {
      value += text[at++];
      continue;
    }
    value += ch;
  }
  rest = text.substr(std::min(at, text.size()));
  return quote == 0;
}

struct LimitTake {
  std::optional<LoopLimitConfig> limit;
  std::string rest;
};

LimitTake take_limit(const std::string& input, std::string& error) {
  LimitTake result;
  const auto first_space = input.find_first_of(" \t");
  const std::string token = first_space == std::string::npos ? input : input.substr(0, first_space);
  const std::string after = first_space == std::string::npos
                                ? std::string{}
                                : trim(std::string_view(input).substr(first_space));
  if (token.empty() || !(std::isdigit(static_cast<unsigned char>(token.front())) ||
                         ((token.front() == '+' || token.front() == '-') && token.size() > 1 &&
                          std::isdigit(static_cast<unsigned char>(token[1]))))) {
    result.rest = input;
    return result;
  }
  const bool all_digits = std::all_of(token.begin(), token.end(), [](char ch) {
    return std::isdigit(static_cast<unsigned char>(ch));
  });
  if (all_digits) {
    const auto unit_space = after.find_first_of(" \t");
    const std::string unit = after.substr(0, unit_space);
    if (const long long ms = unit_ms(unit); ms > 0) {
      try {
        const int amount = std::stoi(token);
        if (amount <= 0) {
          error = "Loop duration must be positive.";
          return result;
        }
        result.rest = unit_space == std::string::npos ? std::string{} : trim(std::string_view(after).substr(unit_space));
        result.limit = LoopLimitConfig{true, 0, static_cast<long long>(amount) * ms};
        return result;
      } catch (const std::exception&) {
        error = "Loop duration is too large.";
        return result;
      }
    }
    try {
      const int amount = std::stoi(token);
      if (amount <= 0) {
        error = "Loop count must be a positive integer.";
        return result;
      }
      result.rest = after;
      result.limit = LoopLimitConfig{false, amount, 0};
      return result;
    } catch (const std::exception&) {
      error = "Loop count must be a positive integer.";
      return result;
    }
  }

  // Compound duration: 10m, 90s, 1h30m.
  long long total = 0;
  std::size_t at = 0;
  while (at < token.size()) {
    const std::size_t digits_start = at;
    while (at < token.size() && std::isdigit(static_cast<unsigned char>(token[at]))) ++at;
    if (at == digits_start) {
      error = "Loop count must be a positive integer.";
      return result;
    }
    const std::size_t unit_start = at;
    while (at < token.size() && std::isalpha(static_cast<unsigned char>(token[at]))) ++at;
    const long long ms = unit_ms(std::string_view(token).substr(unit_start, at - unit_start));
    if (ms == 0) {
      error = "Loop duration unit must be seconds, minutes, or hours.";
      return result;
    }
    const int amount = std::stoi(token.substr(digits_start, at - digits_start));
    if (amount <= 0) {
      error = "Loop duration must be positive.";
      return result;
    }
    total += static_cast<long long>(amount) * ms;
  }
  result.rest = after;
  result.limit = LoopLimitConfig{true, 0, total};
  return result;
}

struct ConditionTake {
  std::optional<LoopConditionConfig> condition;
  std::string rest;
};

ConditionTake take_condition(const std::string& input, std::string& error) {
  ConditionTake result;
  const std::string trimmed = trim(input);
  if (trimmed.rfind("--", 0) != 0) {
    result.rest = trimmed;
    return result;
  }
  const auto space = trimmed.find_first_of(" =");
  const std::string flag = trimmed.substr(0, space);
  const bool until = flag == "--until";
  if (!until && flag != "--while") {
    error = "Unknown /loop flag " + flag + ". Usage: /loop [count|duration] [--while|--until '<command>'] [prompt]";
    return result;
  }
  std::string_view value_text = trimmed;
  value_text.remove_prefix(flag.size());
  if (!value_text.empty() && value_text.front() == '=') value_text.remove_prefix(1);
  std::string command;
  std::string_view after;
  if (!read_shell_word(value_text, command, after) || command.empty()) {
    error = flag + " needs a shell command. Quote it when it contains spaces: /loop " + flag +
            " 'bun test'.";
    return result;
  }
  result.rest = trim(after);
  if (result.rest.rfind("--", 0) == 0) {
    error = "Use only one of --while or --until.";
    return result;
  }
  result.condition = LoopConditionConfig{command, until};
  return result;
}

void disable(AppState& state) {
  state.loop_enabled = false;
  state.loop_paused = false;
  state.loop_ready = false;
  state.loop_duration = false;
  state.loop_iterations = 0;
  state.loop_deadline = {};
  state.loop_prompt.clear();
  state.loop_condition.clear();
  state.loop_until = false;
  state.loop_condition_running = false;
}

bool budget_left(AppState& state) {
  if (!state.loop_duration) return state.loop_iterations > 0;
  return Clock::now() < state.loop_deadline;
}

bool consume(AppState& state) {
  if (!budget_left(state)) return false;
  if (!state.loop_duration && state.loop_iterations > 0) --state.loop_iterations;
  return true;
}

}  // namespace

ParsedLoopArgs parse_loop_args(std::string_view args) {
  ParsedLoopArgs parsed;
  std::string remaining(args);
  auto limit = take_limit(remaining, parsed.error);
  if (!parsed.error.empty()) return parsed;
  parsed.limit = limit.limit;
  remaining = std::move(limit.rest);
  auto condition = take_condition(remaining, parsed.error);
  if (!parsed.error.empty()) return parsed;
  parsed.condition = condition.condition;
  parsed.prompt = trim(condition.rest);
  return parsed;
}

void loop_command(AppState& state, std::string_view line, std::string& notice) {
  std::string_view args = line;
  if (args.rfind("/loop", 0) == 0) args.remove_prefix(5);
  const std::string text = trim(args);
  if (state.loop_enabled || text == "off" || text == "stop") {
    disable(state);
    state.loop_pending_prompt.clear();
    notice = "Loop mode disabled.";
    return;
  }
  const auto parsed = parse_loop_args(text);
  if (!parsed.error.empty()) {
    notice = parsed.error;
    return;
  }
  state.loop_enabled = true;
  state.loop_paused = false;
  state.loop_ready = false;
  state.loop_duration = parsed.limit && parsed.limit->duration;
  state.loop_iterations = parsed.limit && !parsed.limit->duration ? parsed.limit->iterations : 0;
  state.loop_deadline = state.loop_duration
                            ? Clock::now() + std::chrono::milliseconds(parsed.limit->duration_ms)
                            : Clock::time_point{};
  state.loop_condition = parsed.condition ? parsed.condition->command : std::string{};
  state.loop_until = parsed.condition && parsed.condition->until;
  state.loop_prompt = parsed.prompt;
  state.loop_condition_running = false;
  if (!parsed.prompt.empty()) {
    if (!consume(state)) {
      disable(state);
      notice = "Loop limit reached. Loop mode disabled.";
      return;
    }
    state.loop_pending_prompt = parsed.prompt;
  }
  notice = "Loop mode on";
  if (parsed.limit) notice += " · " + (state.loop_duration ? std::string("time limit")
                                                           : std::to_string(parsed.limit->iterations) +
                                                                 " iterations");
  if (!state.loop_condition.empty()) notice += state.loop_until ? " · until" : " · while";
  if (parsed.prompt.empty()) notice += " · send a prompt to repeat";
}

void loop_note_prompt(AppState& state, std::string_view text) {
  if (!state.loop_enabled || state.loop_paused || !state.loop_prompt.empty()) return;
  state.loop_prompt = std::string(text);
  state.loop_ready = false;
}

void loop_turn_finished(AppState& state) {
  if (!state.loop_enabled || state.loop_paused || state.loop_prompt.empty()) return;
  if (state.loop_condition_running) return;
  state.loop_ready = true;
}

void loop_pause(AppState& state, std::string& notice) {
  if (!state.loop_enabled) return;
  state.loop_paused = true;
  state.loop_ready = false;
  notice = "Loop paused.";
}

void loop_reset(AppState& state) {
  disable(state);
  state.loop_pending_prompt.clear();
}

LoopStep loop_next(AppState& state) {
  LoopStep step;
  if (!state.loop_enabled || state.loop_paused || !state.loop_ready ||
      state.loop_prompt.empty() || state.loop_condition_running) {
    return step;
  }
  if (!budget_left(state)) {
    disable(state);
    step.notice = state.loop_duration ? "Loop time limit reached. Loop mode disabled."
                                      : "Loop limit reached. Loop mode disabled.";
    return step;
  }
  state.loop_ready = false;
  if (!state.loop_condition.empty()) {
    state.loop_condition_running = true;
    step.condition = state.loop_condition;
    return step;
  }
  consume(state);
  step.prompt = state.loop_prompt;
  return step;
}

LoopStep loop_condition_result(AppState& state, int exit_code) {
  LoopStep step;
  state.loop_condition_running = false;
  if (!state.loop_enabled || state.loop_paused || state.loop_prompt.empty()) return step;
  const bool passed = state.loop_until ? exit_code != 0 : exit_code == 0;
  if (!passed) {
    disable(state);
    step.notice = state.loop_until ? "Loop stop condition met. Loop mode disabled."
                                   : "Loop condition failed. Loop mode disabled.";
    return step;
  }
  if (!budget_left(state)) {
    disable(state);
    step.notice = state.loop_duration ? "Loop time limit reached. Loop mode disabled."
                                      : "Loop limit reached. Loop mode disabled.";
    return step;
  }
  consume(state);
  step.prompt = state.loop_prompt;
  return step;
}

int run_shell_status(const std::string& command, const std::string& cwd, int timeout_ms) {
  const int devnull = ::open("/dev/null", O_RDWR | O_CLOEXEC);
  if (devnull < 0) return 127;
  posix_spawn_file_actions_t actions;
  posix_spawn_file_actions_init(&actions);
  posix_spawn_file_actions_adddup2(&actions, devnull, STDOUT_FILENO);
  posix_spawn_file_actions_adddup2(&actions, devnull, STDERR_FILENO);
  posix_spawn_file_actions_addclose(&actions, devnull);
  if (!cwd.empty()) posix_spawn_file_actions_addchdir_np(&actions, cwd.c_str());

  std::vector<std::string> storage{"/bin/sh", "-c", command};
  std::vector<char*> argv;
  for (auto& value : storage) argv.push_back(value.data());
  argv.push_back(nullptr);
  pid_t pid = -1;
  const int spawned = ::posix_spawnp(&pid, "/bin/sh", &actions, nullptr, argv.data(), environ);
  posix_spawn_file_actions_destroy(&actions);
  ::close(devnull);
  if (spawned != 0) return 127;

  const auto deadline = Clock::now() + std::chrono::milliseconds(timeout_ms);
  int status = 0;
  for (;;) {
    const pid_t waited = ::waitpid(pid, &status, WNOHANG);
    if (waited == pid) break;
    if (waited < 0 && errno != EINTR) return 127;
    if (Clock::now() >= deadline) {
      ::kill(pid, SIGKILL);
      (void)::waitpid(pid, &status, 0);
      return 124;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(20));
  }
  if (WIFEXITED(status)) return WEXITSTATUS(status);
  return 128 + WTERMSIG(status);
}

}  // namespace booth
