#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <chrono>
#include <filesystem>
#include <fstream>
#include <map>
#include <mutex>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "subagent/acp/messages.hpp"
#include "supervisor/config.hpp"
#include "supervisor/logs.hpp"
#include "supervisor/process.hpp"
#include "supervisor/supervisor.hpp"

using namespace booth;

namespace {

std::filesystem::path make_temp_dir(std::string_view name) {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-test-" + std::string(name) + "-" + std::to_string(::getpid()));
  std::error_code error;
  std::filesystem::remove_all(dir, error);
  std::filesystem::create_directories(dir);
  return dir;
}

Env env_of(std::map<std::string, std::string> values) {
  return [values = std::move(values)](std::string_view key) -> std::optional<std::string> {
    const auto found = values.find(std::string(key));
    if (found == values.end()) return std::nullopt;
    return found->second;
  };
}

}  // namespace

TEST_CASE("config derives every path from the environment") {
  const auto dir = make_temp_dir("config");
  auto config = Config::from_env(env_of({{"HOME", dir.string()}}));
  REQUIRE(config.has_value());
  REQUIRE(config->dsh_home == dir / ".dsh");
  REQUIRE(config->state_dir == dir / ".dsh" / "state" / "booth");
  REQUIRE(config->log_file == config->state_dir / "booth.log");
  REQUIRE(config->profile == "booth");
  REQUIRE(config->base_profile == "booth");

  auto overridden = Config::from_env(env_of({{"HOME", dir.string()},
                                             {"DSH_HOME", (dir / "h").string()},
                                             {"BOOTH_STATE_DIR", (dir / "s").string()},
                                             {"BOOTH_PROFILE", "work"},
                                             {"BOOTH_BASE_PROFILE", "acp"},
                                             {"BOOTH_HANDSHAKE_TIMEOUT_S", "5"}}));
  REQUIRE(overridden.has_value());
  REQUIRE(overridden->dsh_home == dir / "h");
  REQUIRE(overridden->state_dir == dir / "s");
  REQUIRE(overridden->profile == "work");
  REQUIRE(overridden->handshake_timeout_s == 5);

  auto no_home = Config::from_env(env_of({}));
  REQUIRE_FALSE(no_home.has_value());
  REQUIRE(no_home.error().layer == Layer::Config);

  auto relative = Config::from_env(env_of({{"HOME", dir.string()}, {"DSH_HOME", "rel"}}));
  REQUIRE_FALSE(relative.has_value());

  std::filesystem::remove_all(dir);
}

TEST_CASE("the log appends, tails and rotates one generation") {
  const auto dir = make_temp_dir("logs");
  const auto path = dir / "booth.log";

  Log log(path);
  REQUIRE(log.append("first", "supervisor").has_value());
  REQUIRE(log.append("second").has_value());
  REQUIRE(log.append("third").has_value());

  auto tail = log.tail(2);
  REQUIRE(tail.has_value());
  REQUIRE(tail->size() == 2);
  REQUIRE((*tail)[0] == "second");
  REQUIRE((*tail)[1] == "third");

  auto missing = Log(dir / "missing.log").tail(3);
  REQUIRE(missing.has_value());
  REQUIRE(missing->empty());

  const auto rotated = dir / "small.log";
  Log small(rotated, 11);
  REQUIRE(small.append("aaaaaaaaaa").has_value());
  REQUIRE(small.append("bbbbbbbbbb").has_value());
  REQUIRE(std::filesystem::exists(rotated.string() + ".1"));
  auto after = small.tail(1);
  REQUIRE(after.has_value());
  REQUIRE(after->size() == 1);
  REQUIRE((*after)[0] == "bbbbbbbbbb");

  std::filesystem::remove_all(dir);
}

TEST_CASE("a child is spawned with pipes and reaped with its exit code") {
  SpawnSpec spec;
  spec.program = "/bin/sh";
  spec.args = {"-c", "printf hi; exit 3"};
  spec.env = {{"PATH", "/usr/bin:/bin"}};
  spec.cwd = std::filesystem::temp_directory_path();

  auto child = spawn_child(spec);
  REQUIRE(child.has_value());
  REQUIRE(child->pid > 0);
  REQUIRE(child->stdin_fd >= 0);
  REQUIRE(child->stdout_fd >= 0);

  std::string output;
  char buffer[64];
  for (;;) {
    const ssize_t got = ::read(child->stdout_fd, buffer, sizeof(buffer));
    if (got <= 0) break;
    output.append(buffer, static_cast<std::size_t>(got));
  }
  ::close(child->stdin_fd);
  ::close(child->stdout_fd);
  REQUIRE(output == "hi");
  auto code = wait_child(child->pid);
  REQUIRE(code.has_value());
  REQUIRE(*code == 3);
}

TEST_CASE("probe_status reads the pid file without connecting") {
  const auto dir = make_temp_dir("probe");
  auto config = Config::from_env(
      env_of({{"HOME", dir.string()}, {"BOOTH_STATE_DIR", (dir / "state").string()}}));
  REQUIRE(config.has_value());
  std::filesystem::create_directories(config->state_dir);

  auto down = probe_status(*config);
  REQUIRE(down.phase == DshPhase::Down);

  {
    std::ofstream out(config->pid_file);
    out << ::getpid() << '\n';
  }
  auto ready = probe_status(*config);
  REQUIRE(ready.phase == DshPhase::Ready);
  REQUIRE(ready.pid == ::getpid());

  std::filesystem::remove_all(dir);
}

TEST_CASE("the supervisor spawns a kernel, handshakes and stops it") {
  const auto dir = make_temp_dir("lifecycle");
  const Env env = env_of({{"HOME", dir.string()},
                          {"BOOTH_STATE_DIR", (dir / "state").string()},
                          {"BOOTH_DSH_BIN", FAKE_KERNEL_PATH}});
  auto config = Config::from_env(env);
  REQUIRE(config.has_value());

  std::mutex mutex;
  std::vector<Event> events;
  Supervisor supervisor(
      *config, env,
      [&](Event event) {
        std::lock_guard<std::mutex> lock(mutex);
        events.push_back(std::move(event));
      },
      "test");

  auto up = supervisor.up();
  REQUIRE(up.has_value());
  const auto status = supervisor.status();
  REQUIRE(status.phase == DshPhase::Ready);
  REQUIRE(status.connected);
  REQUIRE(status.pid > 0);
  REQUIRE(std::filesystem::exists(config->pid_file));

  auto handshake = supervisor.request("initialize", acp::initialize_params("test"),
                                      std::chrono::seconds(1));
  REQUIRE(handshake.has_value());
  REQUIRE((*handshake)["protocolVersion"] == 1);

  auto down = supervisor.down();
  REQUIRE(down.has_value());
  REQUIRE(supervisor.status().phase == DshPhase::Down);
  REQUIRE_FALSE(std::filesystem::exists(config->pid_file));

  std::filesystem::remove_all(dir);
}

TEST_CASE("a second supervisor refuses the same state directory") {
  const auto dir = make_temp_dir("lock");
  const Env env = env_of({{"HOME", dir.string()},
                          {"BOOTH_STATE_DIR", (dir / "state").string()},
                          {"BOOTH_DSH_BIN", FAKE_KERNEL_PATH}});
  auto config = Config::from_env(env);
  REQUIRE(config.has_value());

  Supervisor first(*config, env, [](Event) {}, "test");
  REQUIRE(first.up().has_value());

  Supervisor second(*config, env, [](Event) {}, "test");
  auto refused = second.up();
  REQUIRE_FALSE(refused.has_value());
  REQUIRE(refused.error().layer == Layer::State);

  REQUIRE(first.down().has_value());
  std::filesystem::remove_all(dir);
}
