#include "supervisor/process.hpp"

#include <fcntl.h>
#include <signal.h>
#include <spawn.h>
#include <sys/wait.h>
#include <unistd.h>

#include <cerrno>
#include <cstring>
#include <string>
#include <thread>
#include <vector>

extern char** environ;

namespace booth {
namespace {

Result<void> close_pair(int fds[2]) {
  for (const int fd : {fds[0], fds[1]}) {
    if (fd >= 0) ::close(fd);
  }
  return {};
}

int exit_code(int status) {
  if (WIFEXITED(status)) return WEXITSTATUS(status);
  if (WIFSIGNALED(status)) return 128 + WTERMSIG(status);
  return 1;
}

}  // namespace

Result<Child> spawn_child(const SpawnSpec& spec) {
  int stdin_pipe[2] = {-1, -1};
  int stdout_pipe[2] = {-1, -1};
  if (::pipe2(stdin_pipe, O_CLOEXEC) != 0) {
    return std::unexpected(Error{Layer::Process, "cannot create the stdin pipe", std::nullopt});
  }
  if (::pipe2(stdout_pipe, O_CLOEXEC) != 0) {
    close_pair(stdin_pipe);
    return std::unexpected(Error{Layer::Process, "cannot create the stdout pipe", std::nullopt});
  }

  posix_spawn_file_actions_t actions;
  posix_spawn_file_actions_init(&actions);
  posix_spawn_file_actions_adddup2(&actions, stdin_pipe[0], STDIN_FILENO);
  posix_spawn_file_actions_adddup2(&actions, stdout_pipe[1], STDOUT_FILENO);
  if (spec.stderr_fd >= 0) {
    posix_spawn_file_actions_adddup2(&actions, spec.stderr_fd, STDERR_FILENO);
  }
  for (const int fd : {stdin_pipe[0], stdin_pipe[1], stdout_pipe[0], stdout_pipe[1]}) {
    posix_spawn_file_actions_addclose(&actions, fd);
  }
  if (!spec.cwd.empty()) {
    posix_spawn_file_actions_addchdir_np(&actions, spec.cwd.c_str());
  }

  std::vector<std::string> argv_storage;
  argv_storage.push_back(spec.program);
  for (const auto& arg : spec.args) argv_storage.push_back(arg);
  std::vector<char*> argv;
  argv.reserve(argv_storage.size() + 1);
  for (auto& arg : argv_storage) argv.push_back(arg.data());
  argv.push_back(nullptr);

  std::vector<std::string> env_storage;
  env_storage.reserve(spec.env.size());
  for (const auto& [key, value] : spec.env) env_storage.push_back(key + "=" + value);
  std::vector<char*> envp;
  envp.reserve(env_storage.size() + 1);
  for (auto& entry : env_storage) envp.push_back(entry.data());
  envp.push_back(nullptr);

  pid_t pid = -1;
  const int spawned = ::posix_spawnp(&pid, spec.program.c_str(), &actions, nullptr, argv.data(),
                                     envp.data());
  posix_spawn_file_actions_destroy(&actions);

  if (spawned != 0) {
    close_pair(stdin_pipe);
    close_pair(stdout_pipe);
    return std::unexpected(Error{Layer::Process,
                                 std::string("cannot spawn ") + spec.program + ": " +
                                     std::strerror(spawned),
                                 spawned});
  }

  ::close(stdin_pipe[0]);
  ::close(stdout_pipe[1]);
  Child child;
  child.pid = pid;
  child.stdin_fd = stdin_pipe[1];
  child.stdout_fd = stdout_pipe[0];
  return child;
}

Result<pid_t> spawn_detached(const SpawnSpec& spec) {
  const int devnull = ::open("/dev/null", O_RDWR | O_CLOEXEC);
  if (devnull < 0) {
    return std::unexpected(Error{Layer::Process, "cannot open /dev/null", std::nullopt});
  }

  posix_spawn_file_actions_t actions;
  posix_spawn_file_actions_init(&actions);
  posix_spawn_file_actions_adddup2(&actions, devnull, STDIN_FILENO);
  posix_spawn_file_actions_adddup2(&actions, devnull, STDOUT_FILENO);
  posix_spawn_file_actions_adddup2(&actions, spec.stderr_fd >= 0 ? spec.stderr_fd : devnull,
                                   STDERR_FILENO);
  posix_spawn_file_actions_addclose(&actions, devnull);
  if (!spec.cwd.empty()) {
    posix_spawn_file_actions_addchdir_np(&actions, spec.cwd.c_str());
  }

  posix_spawnattr_t attributes;
  posix_spawnattr_init(&attributes);
  posix_spawnattr_setflags(&attributes, POSIX_SPAWN_SETSID);

  std::vector<std::string> argv_storage;
  argv_storage.push_back(spec.program);
  for (const auto& arg : spec.args) argv_storage.push_back(arg);
  std::vector<char*> argv;
  for (auto& arg : argv_storage) argv.push_back(arg.data());
  argv.push_back(nullptr);

  std::vector<std::string> env_storage;
  for (const auto& [key, value] : spec.env) env_storage.push_back(key + "=" + value);
  std::vector<char*> envp;
  for (auto& entry : env_storage) envp.push_back(entry.data());
  envp.push_back(nullptr);

  pid_t pid = -1;
  const int spawned = ::posix_spawnp(&pid, spec.program.c_str(), &actions, &attributes,
                                     argv.data(), envp.data());
  posix_spawnattr_destroy(&attributes);
  posix_spawn_file_actions_destroy(&actions);
  ::close(devnull);

  if (spawned != 0) {
    return std::unexpected(Error{Layer::Process,
                                 std::string("cannot spawn ") + spec.program + ": " +
                                     std::strerror(spawned),
                                 spawned});
  }
  return pid;
}

Result<int> wait_child(pid_t pid) {
  int status = 0;
  for (;;) {
    const pid_t waited = ::waitpid(pid, &status, 0);
    if (waited == pid) return exit_code(status);
    if (waited < 0 && errno == EINTR) continue;
    return std::unexpected(Error{Layer::Process, "cannot wait for the child", std::nullopt});
  }
}

Result<int> terminate_child(pid_t pid, std::chrono::milliseconds grace) {
  if (pid <= 0) return 0;
  if (::kill(pid, SIGTERM) != 0 && errno != ESRCH) {
    return std::unexpected(Error{Layer::Process, "cannot signal the child", std::nullopt});
  }

  const auto deadline = std::chrono::steady_clock::now() + grace;
  int status = 0;
  while (std::chrono::steady_clock::now() < deadline) {
    const pid_t waited = ::waitpid(pid, &status, WNOHANG);
    if (waited == pid) return exit_code(status);
    if (waited < 0 && errno != EINTR) break;
    std::this_thread::sleep_for(std::chrono::milliseconds(20));
  }

  ::kill(pid, SIGKILL);
  const pid_t waited = ::waitpid(pid, &status, 0);
  if (waited == pid) return exit_code(status);
  return std::unexpected(Error{Layer::Process, "cannot reap the child", std::nullopt});
}

}  // namespace booth
