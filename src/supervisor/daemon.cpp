#include "supervisor/daemon.hpp"

#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>

#include <atomic>
#include <cstdint>
#include <functional>
#include <iostream>
#include <cerrno>
#include <chrono>
#include <cstring>
#include <fstream>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <unordered_map>
#include <utility>
#include <vector>

#include "subagent/acp/codec.hpp"
#include "supervisor/logs.hpp"
#include "supervisor/process.hpp"
#include "supervisor/supervisor.hpp"

namespace booth {
namespace {

volatile sig_atomic_t g_stop = 0;

void on_signal(int) { g_stop = 1; }

sockaddr_un address_of(const std::filesystem::path& path) {
  sockaddr_un address{};
  address.sun_family = AF_UNIX;
  const std::string text = path.string();
  std::strncpy(address.sun_path, text.c_str(), sizeof(address.sun_path) - 1);
  return address;
}

bool alive(pid_t pid) {
  if (pid <= 0 || ::kill(pid, 0) != 0) return false;
  std::ifstream stat("/proc/" + std::to_string(pid) + "/stat");
  std::string content;
  if (!std::getline(stat, content)) return true;
  const auto close = content.rfind(')');
  if (close == std::string::npos || close + 2 >= content.size()) return true;
  return content[close + 2] != 'Z';
}

bool write_line(int fd, const std::string& line) {
  std::size_t sent = 0;
  while (sent < line.size()) {
    const ssize_t wrote = ::write(fd, line.data() + sent, line.size() - sent);
    if (wrote < 0) {
      if (errno == EINTR) continue;
      return false;
    }
    sent += static_cast<std::size_t>(wrote);
  }
  return true;
}

struct Bridge {
  std::mutex mutex;
  int fd{-1};
  std::mutex ids_mutex;
  std::unordered_map<std::string, acp::Json> pending;
  std::atomic<std::uint64_t> next{1};
};

std::vector<std::pair<std::string, std::string>> daemon_env(const Env& env) {
  std::vector<std::pair<std::string, std::string>> out;
  constexpr const char* kKeys[] = {
      "PATH", "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "TERM", "TMPDIR", "DSH_HOME",
      "BOOTH_DSH_BIN", "BOOTH_PROFILE", "BOOTH_BASE_PROFILE", "BOOTH_STATE_DIR",
      "BOOTH_HANDSHAKE_TIMEOUT_S", "TYPESAFE_BASE_URL", "TYPESAFE_PATH", "TYPESAFE_API_KEY",
      "TYPESAFE_MODEL", "DEEPSEEK_API_KEY", "COMMANDCODE_API_KEY", "OPENAI_API_KEY",
      "ANTHROPIC_API_KEY", "GOOGLE_API_KEY", "GEMINI_API_KEY", "QWEN_API_KEY",
  };
  for (const char* key : kKeys) {
    if (auto value = env(key)) out.emplace_back(key, *value);
  }
  return out;
}

void serve_client(int fd, std::shared_ptr<Bridge> bridge, acp::Client& kernel) {
  std::string buffer;
  std::vector<char> chunk(65536);
  bool open = true;
  while (open) {
    const ssize_t got = ::read(fd, chunk.data(), chunk.size());
    if (got <= 0) break;
    buffer.append(chunk.data(), static_cast<std::size_t>(got));
    std::size_t newline = 0;
    while ((newline = buffer.find('\n')) != std::string::npos) {
      const std::string line = buffer.substr(0, newline);
      buffer.erase(0, newline + 1);
      auto decoded = acp::decode(line);
      if (!decoded) {
        open = false;
        break;
      }
      acp::Json out = *decoded;
      if (out.contains("method") && out.contains("id")) {
        const std::string forwarded =
            "booth-" + std::to_string(bridge->next.fetch_add(1));
        {
          std::lock_guard<std::mutex> lock(bridge->ids_mutex);
          bridge->pending[forwarded] = out["id"];
        }
        out["id"] = forwarded;
      }
      kernel.send_raw(out);
    }
  }
  {
    std::lock_guard<std::mutex> lock(bridge->mutex);
    if (bridge->fd == fd) bridge->fd = -1;
  }
  ::close(fd);
}

}  // namespace

int connect_daemon(const std::filesystem::path& socket_path) {
  const std::string text = socket_path.string();
  if (text.empty() || text.size() >= sizeof(sockaddr_un::sun_path)) return -1;
  const int fd = ::socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
  if (fd < 0) return -1;
  const sockaddr_un address = address_of(socket_path);
  if (::connect(fd, reinterpret_cast<const sockaddr*>(&address), sizeof(address)) != 0) {
    ::close(fd);
    return -1;
  }
  return fd;
}

Result<void> start_daemon(const Config& config, const Env& env) {
  if (const int existing = connect_daemon(config.socket_file); existing >= 0) {
    ::close(existing);
    return {};
  }

  std::error_code error;
  std::filesystem::create_directories(config.state_dir, error);
  if (error) {
    return std::unexpected(Error{Layer::State, "cannot create the state directory", std::nullopt});
  }
  std::filesystem::permissions(config.state_dir,
                               std::filesystem::perms::owner_all,
                               std::filesystem::perm_options::replace, error);
  std::filesystem::create_directories(config.work_dir, error);

  std::error_code self_error;
  const auto self = std::filesystem::read_symlink("/proc/self/exe", self_error);
  const std::string program = self_error ? std::string{"booth"} : self.string();

  SpawnSpec spec;
  spec.program = program;
  spec.args = {"supervisor"};
  spec.cwd = config.work_dir;
  spec.env = daemon_env(env);
  spec.stderr_fd = -1;

  auto pid = spawn_detached(spec);
  if (!pid) return std::unexpected(pid.error());

  for (int attempt = 0; attempt < 200; ++attempt) {
    if (const int fd = connect_daemon(config.socket_file); fd >= 0) {
      ::close(fd);
      return {};
    }
    if (!alive(*pid)) {
      return std::unexpected(Error{Layer::Process, "the daemon exited before listening", std::nullopt});
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
  }
  return std::unexpected(Error{Layer::Process, "the daemon did not listen within 20 s", std::nullopt});
}

Result<void> stop_daemon(const Config& config) {
  pid_t pid = -1;
  {
    std::ifstream input(config.pid_file);
    std::string line;
    if (std::getline(input, line)) {
      try {
        pid = static_cast<pid_t>(std::stol(line));
      } catch (const std::exception&) {
        pid = -1;
      }
    }
  }

  if (pid > 0 && ::kill(pid, SIGTERM) == 0) {
    for (int attempt = 0; attempt < 100; ++attempt) {
      if (!alive(pid)) break;
      std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }
    if (alive(pid)) {
      ::kill(pid, SIGKILL);
      std::this_thread::sleep_for(std::chrono::milliseconds(100));
    }
  }

  std::error_code ignored;
  std::filesystem::remove(config.socket_file, ignored);
  std::filesystem::remove(config.pid_file, ignored);
  return {};
}

int run_daemon(const Config& config, const Env& env, std::string client_version) {
  if (const int existing = connect_daemon(config.socket_file); existing >= 0) {
    ::close(existing);
    std::cerr << "booth: another supervisor is listening on " << config.socket_file << '\n';
    return 1;
  }

  Supervisor supervisor(config, env, [](Event) {}, std::move(client_version));
  auto up = supervisor.up();
  if (!up) {
    std::cerr << "booth: " << to_string(up.error()) << '\n';
    return 1;
  }

  std::error_code ignored;
  std::filesystem::remove(config.socket_file, ignored);

  const int listen_fd = ::socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
  if (listen_fd < 0) {
    supervisor.down();
    std::cerr << "booth: cannot create the socket\n";
    return 1;
  }
  const sockaddr_un address = address_of(config.socket_file);
  if (::bind(listen_fd, reinterpret_cast<const sockaddr*>(&address), sizeof(address)) != 0 ||
      ::listen(listen_fd, 1) != 0) {
    ::close(listen_fd);
    supervisor.down();
    std::cerr << "booth: cannot bind " << config.socket_file << '\n';
    return 1;
  }
  ::chmod(config.socket_file.c_str(), 0600);

  auto bridge = std::make_shared<Bridge>();
  supervisor.client()->set_raw_sink([bridge](const acp::Json& message) {
    acp::Json out = message;
    std::lock_guard<std::mutex> lock(bridge->mutex);
    if (bridge->fd < 0) return;
    if (message.contains("id") && !message.contains("method") && message["id"].is_string()) {
      std::lock_guard<std::mutex> ids(bridge->ids_mutex);
      const auto found = bridge->pending.find(message["id"].get<std::string>());
      if (found != bridge->pending.end()) {
        out["id"] = found->second;
        bridge->pending.erase(found);
      }
    }
    write_line(bridge->fd, acp::encode(out));
  });

  struct sigaction action {};
  action.sa_handler = on_signal;
  sigemptyset(&action.sa_mask);
  ::sigaction(SIGTERM, &action, nullptr);
  ::sigaction(SIGINT, &action, nullptr);

  Log log(config.log_file);
  log.append("daemon listening on " + config.socket_file.string(), "supervisor");

  std::thread client_thread;
  const auto join_client = [&] {
    if (client_thread.joinable()) client_thread.join();
  };

  while (g_stop == 0) {
    pollfd poll_fd{listen_fd, POLLIN, 0};
    const int ready = ::poll(&poll_fd, 1, 500);
    if (ready <= 0) continue;

    join_client();
    const int fd = ::accept4(listen_fd, nullptr, nullptr, SOCK_CLOEXEC);
    if (fd < 0) continue;

    {
      std::lock_guard<std::mutex> lock(bridge->mutex);
      if (bridge->fd >= 0) {
        ::close(fd);
        continue;
      }
      bridge->fd = fd;
    }
    client_thread = std::thread(serve_client, fd, bridge, std::ref(*supervisor.client()));
  }

  {
    std::lock_guard<std::mutex> lock(bridge->mutex);
    if (bridge->fd >= 0) {
      ::shutdown(bridge->fd, SHUT_RDWR);
    }
  }
  join_client();
  ::close(listen_fd);
  std::filesystem::remove(config.socket_file, ignored);
  supervisor.down();
  return 0;
}

}  // namespace booth
