#pragma once

#include <sys/types.h>

#include <chrono>
#include <filesystem>
#include <string>
#include <utility>
#include <vector>

#include "contract/error.hpp"

namespace booth {

struct Child {
  pid_t pid{-1};
  int stdin_fd{-1};   // write end of the child's stdin
  int stdout_fd{-1};  // read end of the child's stdout
};

struct SpawnSpec {
  std::string program;  // resolved through PATH, like execvp
  std::vector<std::string> args;
  std::vector<std::pair<std::string, std::string>> env;
  std::filesystem::path cwd;
  int stderr_fd{-1};  // caller-owned; -1 leaves the child's stderr inherited
};

/// Spawn one child with piped stdin/stdout. The caller owns and closes both fds.
Result<Child> spawn_child(const SpawnSpec& spec);

/// Spawn a detached daemon: stdin/stdout to /dev/null, its own session, no pipes.
Result<pid_t> spawn_detached(const SpawnSpec& spec);

/// Wait for one child and return its exit code (128 + signal when signalled).
Result<int> wait_child(pid_t pid);

/// SIGTERM, wait up to `grace`, then SIGKILL. Returns the exit code.
Result<int> terminate_child(pid_t pid, std::chrono::milliseconds grace);

}  // namespace booth
