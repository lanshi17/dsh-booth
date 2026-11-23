#include "interaction/shell_command.hpp"

#include <string_view>

#include <cerrno>
#include <cstring>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#include <algorithm>
#include <array>
#include <chrono>
#include <string>

namespace booth {
namespace {

void append_output(ShellCommandResult& result, const char* bytes, std::size_t count,
                   std::size_t limit) {
  const std::size_t available = limit > result.output.size() ? limit - result.output.size() : 0;
  const std::size_t kept = std::min(available, count);
  result.output.append(bytes, kept);
  if (kept < count) result.truncated = true;
}

int exit_code(int status) {
  if (WIFEXITED(status)) return WEXITSTATUS(status);
  if (WIFSIGNALED(status)) return 128 + WTERMSIG(status);
  return -1;
}

}  // namespace

ShellCommandResult run_shell_command(const std::string& command, const std::string& cwd,
                                     std::chrono::milliseconds timeout,
                                     std::size_t output_limit) {
  ShellCommandResult result;
  int output_pipe[2];
  if (::pipe(output_pipe) != 0) {
    result.error = std::strerror(errno);
    return result;
  }
  for (int descriptor : output_pipe) {
    const int flags = ::fcntl(descriptor, F_GETFD);
    if (flags >= 0) ::fcntl(descriptor, F_SETFD, flags | FD_CLOEXEC);
  }
  const pid_t child = ::fork();
  if (child < 0) {
    result.error = std::strerror(errno);
    ::close(output_pipe[0]);
    ::close(output_pipe[1]);
    return result;
  }
  if (child == 0) {
    ::setpgid(0, 0);
    ::close(output_pipe[0]);
    if (::dup2(output_pipe[1], STDOUT_FILENO) < 0 ||
        ::dup2(output_pipe[1], STDERR_FILENO) < 0) _exit(127);
    ::close(output_pipe[1]);
    if (::chdir(cwd.c_str()) != 0) {
      const char message[] = "chdir failed\n";
      (void)::write(STDERR_FILENO, message, sizeof(message) - 1);
      _exit(127);
    }
    ::execl("/bin/sh", "sh", "-c", command.c_str(), static_cast<char*>(nullptr));
    const char message[] = "exec /bin/sh failed\n";
    (void)::write(STDERR_FILENO, message, sizeof(message) - 1);
    _exit(127);
  }

  ::close(output_pipe[1]);
  (void)::setpgid(child, child);
  const int flags = ::fcntl(output_pipe[0], F_GETFL);
  if (flags >= 0) (void)::fcntl(output_pipe[0], F_SETFL, flags | O_NONBLOCK);
  const auto deadline = std::chrono::steady_clock::now() + timeout;
  bool child_reaped = false;
  bool pipe_eof = false;
  bool killed_for_timeout = false;
  int status = 0;
  std::array<char, 8192> buffer{};

  while (!child_reaped || !pipe_eof) {
    if (!killed_for_timeout && std::chrono::steady_clock::now() >= deadline) {
      killed_for_timeout = true;
      result.timed_out = true;
      (void)::kill(-child, SIGKILL);
      (void)::kill(child, SIGKILL);
    }

    if (!pipe_eof) {
      int wait_ms = 50;
      const auto remaining = std::chrono::duration_cast<std::chrono::milliseconds>(
          deadline - std::chrono::steady_clock::now()).count();
      if (!killed_for_timeout) wait_ms = static_cast<int>(std::clamp<long long>(remaining, 0, 50));
      pollfd descriptor{output_pipe[0], POLLIN | POLLHUP, 0};
      int polled;
      do {
        polled = ::poll(&descriptor, 1, wait_ms);
      } while (polled < 0 && errno == EINTR);
      if (polled < 0) {
        result.error = std::strerror(errno);
        (void)::kill(-child, SIGKILL);
      }
      if (polled > 0 && descriptor.revents != 0) {
        for (;;) {
          const ssize_t count = ::read(output_pipe[0], buffer.data(), buffer.size());
          if (count > 0) {
            append_output(result, buffer.data(), static_cast<std::size_t>(count), output_limit);
          } else if (count == 0) {
            pipe_eof = true;
            ::close(output_pipe[0]);
            break;
          } else if (errno == EINTR) {
            continue;
          } else if (errno == EAGAIN || errno == EWOULDBLOCK) {
            break;
          } else {
            result.error = std::strerror(errno);
            pipe_eof = true;
            ::close(output_pipe[0]);
            break;
          }
        }
      }
    }

    if (!child_reaped) {
      const pid_t waited = ::waitpid(child, &status, WNOHANG);
      if (waited == child) child_reaped = true;
      else if (waited < 0 && errno != EINTR) {
        result.error = std::strerror(errno);
        child_reaped = true;
      }
    }
  }

  if (!child_reaped) {
    while (::waitpid(child, &status, 0) < 0 && errno == EINTR) {}
  }
  result.exit_code = result.timed_out ? 124 : exit_code(status);
  return result;
}

namespace {

TranscriptEntry local_tool_entry(const ShellCommandCompletion& completion, std::string id,
                                 const char* tool) {
  TranscriptEntry entry;
  entry.kind = EntryKind::Tool;
  entry.id = std::move(id);
  entry.text = tool;
  entry.tool = tool;
  entry.status = "completed";
  entry.input = completion.display.empty() ? completion.command : completion.display;
  entry.output = completion.result.output;
  const auto note = [&entry](const std::string& text) {
    if (!entry.output.empty() && entry.output.back() != '\n') entry.output.push_back('\n');
    entry.output += text;
  };
  if (completion.result.truncated) note("[output truncated at 256 KiB]");
  if (completion.result.timed_out) note("[command timed out]");
  if (completion.result.exit_code != 0) {
    note("[exit code: " + std::to_string(completion.result.exit_code) + "]");
  }
  if (!completion.result.error.empty()) note("[error: " + completion.result.error + "]");
  entry.started = completion.started;
  entry.finished = completion.finished;
  return entry;
}

}  // namespace

TranscriptEntry shell_tool_entry(const ShellCommandCompletion& completion, std::string id) {
  return local_tool_entry(completion, std::move(id), "bash");
}

TranscriptEntry python_tool_entry(const ShellCommandCompletion& completion, std::string id) {
  return local_tool_entry(completion, std::move(id), "python");
}

std::string python_shell_command(std::string_view code) {
  static constexpr std::string_view wrapper =
      "import ast,base64,sys\n"
      "src=base64.b64decode(sys.argv[1]).decode()\n"
      "tree=ast.parse(src)\n"
      "if len(tree.body)==1 and isinstance(tree.body[0], ast.Expr):\n"
      " v=eval(compile(ast.Expression(tree.body[0].value),'<booth>','eval'))\n"
      " if v is not None: print(repr(v))\n"
      "else:\n"
      " exec(compile(tree,'<booth>','exec'))\n";
  static const char* alphabet =
      "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  std::string encoded;
  encoded.reserve((code.size() + 2) / 3 * 4);
  for (std::size_t i = 0; i < code.size(); i += 3) {
    const unsigned b0 = static_cast<unsigned char>(code[i]);
    const unsigned b1 = i + 1 < code.size() ? static_cast<unsigned char>(code[i + 1]) : 0;
    const unsigned b2 = i + 2 < code.size() ? static_cast<unsigned char>(code[i + 2]) : 0;
    const unsigned value = (b0 << 16) | (b1 << 8) | b2;
    encoded.push_back(alphabet[(value >> 18) & 63]);
    encoded.push_back(alphabet[(value >> 12) & 63]);
    encoded.push_back(i + 1 < code.size() ? alphabet[(value >> 6) & 63] : '=');
    encoded.push_back(i + 2 < code.size() ? alphabet[value & 63] : '=');
  }
  return "python3 -c \"" + std::string(wrapper) + "\" " + encoded;
}

ShellCommandResult run_python_command(std::string_view code, const std::string& cwd,
                                      std::chrono::milliseconds timeout,
                                      std::size_t output_limit) {
  return run_shell_command(python_shell_command(code), cwd, timeout, output_limit);
}

ShellCommandJob::~ShellCommandJob() { join(); }

bool ShellCommandJob::start(std::string command, std::string cwd,
                            std::function<void()> completed, std::string display) {
  std::lock_guard<std::mutex> lock(mutex_);
  if (busy_ || thread_.joinable()) return false;
  busy_ = true;
  if (display.empty()) display = command;
  try {
    thread_ = std::thread([this, command = std::move(command), cwd = std::move(cwd),
                           completed = std::move(completed), display = std::move(display)]() mutable {
      const auto started = std::chrono::system_clock::now();
      auto result = run_shell_command(command, cwd);
      const auto finished = std::chrono::system_clock::now();
      {
        std::lock_guard<std::mutex> completed_lock(mutex_);
        completed_.push_back(ShellCommandCompletion{
            std::move(command), std::move(display), std::move(result), started, finished});
      }
      try {
        if (completed) completed();
      } catch (...) {
      }
    });
  } catch (...) {
    busy_ = false;
    return false;
  }
  return true;
}

bool ShellCommandJob::busy() const {
  std::lock_guard<std::mutex> lock(mutex_);
  return busy_;
}

std::vector<ShellCommandCompletion> ShellCommandJob::take_completed() {
  std::vector<ShellCommandCompletion> result;
  {
    std::lock_guard<std::mutex> lock(mutex_);
    result.swap(completed_);
    if (!result.empty()) busy_ = false;
  }
  if (!result.empty()) join();
  return result;
}

void ShellCommandJob::join() {
  if (thread_.joinable()) thread_.join();
}

}  // namespace booth
