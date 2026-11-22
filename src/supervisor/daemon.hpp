#pragma once

#include <filesystem>
#include <string>

#include "supervisor/config.hpp"

namespace booth {

/// Connects to the daemon socket. Returns -1 when nothing is listening.
int connect_daemon(const std::filesystem::path& socket_path);

/// Runs the daemon in the foreground until SIGTERM/SIGINT. Exit code for the process.
int run_daemon(const Config& config, const Env& env, std::string client_version);

/// Spawns a detached daemon and waits for its socket.
Result<void> start_daemon(const Config& config, const Env& env);

/// SIGTERM, wait, then SIGKILL; removes a stale socket.
Result<void> stop_daemon(const Config& config);

}  // namespace booth
