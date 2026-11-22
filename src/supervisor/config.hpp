#pragma once

#include <filesystem>
#include <functional>
#include <optional>
#include <string>
#include <string_view>

#include "contract/error.hpp"

namespace booth {

using Env = std::function<std::optional<std::string>(std::string_view)>;

Env system_env();

struct Config {
  std::string dsh_bin{"dsh"};          // BOOTH_DSH_BIN; resolved through PATH
  std::string base_profile{"booth"};   // the dsh profile booth owns and starts
  std::string profile{"booth"};        // this profile's name, for display/state
  std::filesystem::path dsh_home;      // DSH_HOME, else $HOME/.dsh
  std::filesystem::path state_dir;     // BOOTH_STATE_DIR, else $DSH_HOME/state/<profile>
  std::filesystem::path work_dir;      // state_dir/work
  std::filesystem::path log_file;      // state_dir/booth.log
  std::filesystem::path pid_file;      // state_dir/booth.pid
  std::filesystem::path lock_file;     // state_dir/booth-service.lock
  std::filesystem::path history_file;  // state_dir/history.jsonl
  std::filesystem::path socket_file;   // state_dir/booth.sock
  int handshake_timeout_s{20};

  static Result<Config> from_env(const Env& env);
};

}  // namespace booth
