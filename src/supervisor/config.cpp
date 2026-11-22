#include "supervisor/config.hpp"

#include <cstdlib>
#include <stdexcept>
#include <string>

namespace booth {
namespace {

std::optional<std::string> non_empty(std::optional<std::string> value) {
  if (!value || value->empty()) return std::nullopt;
  return value;
}

}  // namespace

Env system_env() {
  return [](std::string_view key) -> std::optional<std::string> {
    const std::string name(key);
    if (const char* value = std::getenv(name.c_str())) return std::string(value);
    return std::nullopt;
  };
}

Result<Config> Config::from_env(const Env& env) {
  Config config;

  if (auto value = non_empty(env("BOOTH_DSH_BIN"))) config.dsh_bin = *value;
  if (auto value = non_empty(env("BOOTH_PROFILE"))) config.profile = *value;
  if (auto value = non_empty(env("BOOTH_BASE_PROFILE"))) config.base_profile = *value;

  if (auto value = non_empty(env("DSH_HOME"))) {
    config.dsh_home = *value;
  } else if (auto home = non_empty(env("HOME"))) {
    config.dsh_home = std::filesystem::path(*home) / ".dsh";
  } else {
    return std::unexpected(Error{Layer::Config, "neither DSH_HOME nor HOME is set", std::nullopt});
  }
  if (!config.dsh_home.is_absolute()) {
    return std::unexpected(Error{Layer::Config, "DSH_HOME must be absolute", std::nullopt});
  }

  config.state_dir = config.dsh_home / "state" / config.profile;
  if (auto value = non_empty(env("BOOTH_STATE_DIR"))) {
    config.state_dir = *value;
    if (!config.state_dir.is_absolute()) {
      return std::unexpected(
          Error{Layer::Config, "BOOTH_STATE_DIR must be absolute", std::nullopt});
    }
  }

  config.work_dir = config.state_dir / "work";
  config.log_file = config.state_dir / "booth.log";
  config.pid_file = config.state_dir / "booth.pid";
  config.lock_file = config.state_dir / "booth-service.lock";
  config.history_file = config.state_dir / "history.jsonl";
  config.socket_file = config.state_dir / "booth.sock";

  if (auto value = non_empty(env("BOOTH_HANDSHAKE_TIMEOUT_S"))) {
    try {
      const int parsed = std::stoi(*value);
      if (parsed <= 0) throw std::invalid_argument("must be positive");
      config.handshake_timeout_s = parsed;
    } catch (const std::exception&) {
      return std::unexpected(
          Error{Layer::Config, "BOOTH_HANDSHAKE_TIMEOUT_S must be a positive integer",
                std::nullopt});
    }
  }

  return config;
}

}  // namespace booth
