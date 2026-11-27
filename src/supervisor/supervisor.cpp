#include "supervisor/supervisor.hpp"

#include <fcntl.h>
#include <signal.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <unistd.h>

#include <cerrno>
#include <chrono>
#include <cstring>
#include <fstream>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

#include <nlohmann/json.hpp>

#include "control/plugin.hpp"
#include "subagent/acp/messages.hpp"

namespace booth {
namespace {

Result<void> write_atomic(const std::filesystem::path& path, const std::string& content) {
  auto temp = path;
  temp += ".tmp";
  std::ofstream output(temp, std::ios::trunc | std::ios::binary);
  if (!output) {
    return std::unexpected(Error{Layer::State, "cannot write " + temp.string(), std::nullopt});
  }
  output << content;
  output.close();
  if (!output) {
    return std::unexpected(Error{Layer::State, "cannot write " + temp.string(), std::nullopt});
  }
  std::error_code error;
  std::filesystem::rename(temp, path, error);
  if (error) {
    return std::unexpected(Error{Layer::State, "cannot replace " + path.string(), std::nullopt});
  }
  std::filesystem::permissions(path,
                               std::filesystem::perms::owner_read |
                                   std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, error);
  return {};
}

/// Quote one YAML scalar with single quotes; embedded quotes double.
std::string yaml_quote(std::string_view text) {
  std::string out{"'"};
  for (const char ch : text) {
    if (ch == '\'') out += "''";
    else out += ch;
  }
  out += '\'';
  return out;
}

constexpr std::string_view kControlBegin = "# --- booth-control (managed by booth) ---\n";
constexpr std::string_view kControlEnd = "# --- end booth-control ---\n";

/// Install the compiled control plugin and keep one managed insert row in the
/// profile patch. User rows before and after it are preserved byte-for-byte.
Result<void> ensure_control_profile(const Config& config) {
  std::error_code error;
  std::filesystem::create_directories(config.profile_dir, error);
  if (error) {
    return std::unexpected(Error{Layer::State, "cannot create the dsh profile directory", std::nullopt});
  }
  const auto plugin = config.profile_dir / "booth-control.mjs";
  auto written = write_atomic(plugin, std::string(control_plugin_source()));
  if (!written) return written;

  const auto patch = config.profile_dir / "cordis.patch.yml";
  std::string content;
  {
    std::ifstream input(patch, std::ios::binary);
    if (input) content.assign(std::istreambuf_iterator<char>(input), std::istreambuf_iterator<char>());
  }
  for (std::size_t pos = 0; (pos = content.find("- insert:\n", pos)) != std::string::npos;) {
    const auto next = content.find("\n- ", pos + 1);
    const auto end = next == std::string::npos ? content.size() : next;
    if (content.substr(pos, end - pos).find("booth-control") != std::string::npos) {
      content.erase(pos, end - pos);
    } else {
      pos = end;
    }
  }
  if (const auto begin = content.find(kControlBegin); begin != std::string::npos) {
    const auto end = content.find(kControlEnd, begin);
    if (end != std::string::npos) {
      content.erase(begin, end + kControlEnd.size() - begin);
    } else {
      content.erase(begin);
    }
  }
  while (!content.empty() && (content.back() == '\n' || content.back() == '\r' || content.back() == ' ')) {
    content.pop_back();
  }
  if (content == "[]" || content == "---\n[]" || content == "---" || content == "---\r\n[]") content.clear();
  if (!content.empty() && content.back() != '\n') content += '\n';
  content += kControlBegin;
  content += "- insert:\n";
  content += "    - id: booth-control\n";
  content += "      name: " + yaml_quote(plugin.string()) + "\n";
  content += "      config:\n";
  content += "        socket: " + yaml_quote(config.control_socket.string()) + "\n";
  content += "    - id: webserver\n";
  content += "      name: '@deepseek-ai/dsh-host-webserver'\n";
  content += "      config:\n";
  content += "        host: 127.0.0.1\n";
  content += "        port: 0\n";
  nlohmann::json features = nlohmann::json::object();
  {
    std::ifstream input(config.state_dir / "features.json", std::ios::binary);
    if (input) features = nlohmann::json::parse(input, nullptr, false);
  }
  const auto feature_enabled = [&](const char* name) {
    return features.is_object() && features.contains(name) && features[name].is_object() &&
           features[name].value("enabled", false);
  };
  if (feature_enabled("browser")) {
    const bool headless = features["browser"].value("headless", true);
    content += "    - id: browser-use\n";
    content += "      name: '@deepseek-ai/dsh-browser-use'\n";
    content += "    - id: browser-playwright\n";
    content += "      name: '@deepseek-ai/dsh-experimental-browser-use-playwright-mcp'\n";
    content += "      config:\n";
    content += "        mode: launch\n";
    content += std::string("        headless: ") + (headless ? "true" : "false") + "\n";
  }
  if (feature_enabled("computer")) {
    content += "    - id: computer-use\n";
    content += "      name: '@deepseek-ai/dsh-computer-use'\n";
    content += "    - id: computer-cua\n";
    content += "      name: '@deepseek-ai/dsh-experimental-computer-use-cua-driver-native'\n";
  }
  content += kControlEnd;
  auto patched = write_atomic(patch, content);
  if (!patched) return patched;
  return {};
}

Result<void> ensure_dsh_profile(const Config& config) {
  const auto dir = config.dsh_home / "profiles" / config.base_profile;
  const auto package = dir / "package.json";
  std::error_code error;
  if (!std::filesystem::exists(package, error)) {
    std::filesystem::create_directories(dir, error);
    if (error) {
      return std::unexpected(Error{Layer::State, "cannot create the dsh profile directory",
                                   std::nullopt});
    }
    std::ofstream output(package, std::ios::trunc);
    if (!output) {
      return std::unexpected(Error{Layer::State, "cannot write the dsh profile package.json",
                                   std::nullopt});
    }
    output << "{\n"
              "  \"name\": \"dsh-profile-" << config.base_profile << "\",\n"
              "  \"private\": true,\n"
              "  \"dependencies\": {},\n"
              "  \"dsh\": { \"profile\": { \"bundles\": [\"@deepseek-ai/dsh-base\", "
              "\"@deepseek-ai/dsh-acp-app\"] } }\n"
              "}\n";
    if (!output) {
      return std::unexpected(Error{Layer::State, "cannot write the dsh profile package.json",
                                   std::nullopt});
    }
  }
  const auto patch = dir / "cordis.patch.yml";
  if (!std::filesystem::exists(patch, error)) {
    std::ofstream output(patch, std::ios::trunc);
    if (!output) {
      return std::unexpected(Error{Layer::State, "cannot write the dsh profile cordis.patch.yml",
                                   std::nullopt});
    }
    output << "- id: tool-subagent\n"
              "  config:\n"
              "    provider: spawn\n"
              "    toolName: subagent\n"
              "    modelSelection: true\n"
              "    modelDiscovery: true\n";
    if (!output) {
      return std::unexpected(Error{Layer::State, "cannot write the dsh profile cordis.patch.yml",
                                   std::nullopt});
    }
  }
  return ensure_control_profile(config);
}

}  // namespace

Status probe_status(const Config& config) {
  Status status;
  status.profile = config.profile;
  status.base_profile = config.base_profile;
  status.state_dir = config.state_dir;

  std::ifstream pid_file(config.pid_file);
  std::string line;
  std::getline(pid_file, line);
  if (!line.empty()) {
    try {
      const long pid = std::stol(line);
      if (pid > 0 && ::kill(static_cast<pid_t>(pid), 0) == 0) {
        status.phase = DshPhase::Ready;
        status.pid = static_cast<pid_t>(pid);
        status.connected = true;
        status.detail = "supervisor pid " + line;
        return status;
      }
    } catch (const std::exception&) {
      // A malformed pid file is a stopped service.
    }
  }
  status.phase = DshPhase::Down;
  status.detail = "no supervisor";
  return status;
}

std::string status_json(const Status& status) {
  const acp::Json value = {
      {"phase", status.connected ? "ready" : "down"},
      {"detail", status.detail},
      {"pid", status.pid},
      {"connected", status.connected},
      {"profile", status.profile},
      {"baseProfile", status.base_profile},
      {"stateDir", status.state_dir.string()},
  };
  return value.dump(2);
}

Supervisor::Supervisor(Config config, Env env, acp::Client::EventSink sink,
                       std::string client_version)
    : config_(std::move(config)),
      env_(std::move(env)),
      sink_(std::move(sink)),
      client_version_(std::move(client_version)),
      log_(config_.log_file) {}

Supervisor::~Supervisor() { teardown(); }

Result<void> Supervisor::ensure_state_dir() {
  std::error_code error;
  std::filesystem::create_directories(config_.state_dir, error);
  if (error) {
    return std::unexpected(Error{Layer::State, "cannot create the state directory", std::nullopt});
  }
  std::filesystem::permissions(config_.state_dir,
                               std::filesystem::perms::owner_all,
                               std::filesystem::perm_options::replace, error);
  std::filesystem::create_directories(config_.work_dir, error);
  if (error) {
    return std::unexpected(Error{Layer::State, "cannot create the work directory", std::nullopt});
  }
  return {};
}

Result<void> Supervisor::acquire_lock() {
  if (lock_fd_ >= 0) return {};
  const int fd = ::open(config_.lock_file.c_str(), O_RDWR | O_CREAT | O_CLOEXEC, 0600);
  if (fd < 0) {
    return std::unexpected(Error{Layer::State, "cannot open the lock file", std::nullopt});
  }
  if (::flock(fd, LOCK_EX | LOCK_NB) != 0) {
    std::string holder;
    std::ifstream pid_file(config_.pid_file);
    std::getline(pid_file, holder);
    ::close(fd);
    return std::unexpected(Error{Layer::State,
                                 "another supervisor holds the lock" +
                                     (holder.empty() ? std::string{} : " (pid " + holder + ")"),
                                 std::nullopt});
  }
  lock_fd_ = fd;
  return {};
}

void Supervisor::release_lock() {
  if (lock_fd_ >= 0) {
    ::flock(lock_fd_, LOCK_UN);
    ::close(lock_fd_);
    lock_fd_ = -1;
  }
}

Result<void> Supervisor::write_pid(pid_t pid) {
  return write_atomic(config_.pid_file, std::to_string(pid));
}

void Supervisor::remove_pid() {
  std::error_code ignored;
  std::filesystem::remove(config_.pid_file, ignored);
}

std::vector<std::pair<std::string, std::string>> Supervisor::child_env() const {
  std::vector<std::pair<std::string, std::string>> out;
  constexpr const char* kKeys[] = {
      "PATH", "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "TERM", "TMPDIR", "DSH_HOME",
      "DEEPSEEK_API_KEY", "BOOTH_STATE_DIR", "BOOTH_PROFILE", "BOOTH_BASE_PROFILE", "TYPESAFE_API_KEY", "TYPESAFE_BASE_URL", "OPENAI_API_KEY",
      "ANTHROPIC_API_KEY", "GOOGLE_API_KEY", "GEMINI_API_KEY", "QWEN_API_KEY",
  };
  for (const char* key : kKeys) {
    if (auto value = env_(key)) out.emplace_back(key, *value);
  }
  return out;
}

Result<void> Supervisor::up() {
  if (phase_ == DshPhase::Ready && client_) return {};

  auto dir = ensure_state_dir();
  if (!dir) return dir;
  auto profile = ensure_dsh_profile(config_);
  if (!profile) return profile;
  auto lock = acquire_lock();
  if (!lock) return lock;

  stderr_fd_ = ::open(config_.log_file.c_str(), O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
  if (stderr_fd_ < 0) {
    release_lock();
    return std::unexpected(Error{Layer::State, "cannot open the log file", std::nullopt});
  }

  SpawnSpec spec;
  spec.program = config_.dsh_bin;
  spec.args = {"--profile", config_.base_profile};
  spec.cwd = config_.work_dir;
  spec.env = child_env();
  spec.stderr_fd = stderr_fd_;

  auto child = spawn_child(spec);
  if (!child) {
    phase_ = DshPhase::Down;
    detail_ = to_string(child.error());
    if (stderr_fd_ >= 0) {
      ::close(stderr_fd_);
      stderr_fd_ = -1;
    }
    release_lock();
    return std::unexpected(child.error());
  }
  child_ = *child;
  client_ = std::make_unique<acp::Client>(child_->stdout_fd, child_->stdin_fd, sink_);
  phase_ = DshPhase::Starting;
  client_->start();

  auto initialized = client_->request(
      "initialize", acp::initialize_params(client_version_),
      std::chrono::seconds(config_.handshake_timeout_s));
  if (!initialized) {
    detail_ = to_string(initialized.error());
    log_.append(detail_, "supervisor");
    teardown();
    phase_ = DshPhase::Degraded;
    release_lock();
    return std::unexpected(initialized.error());
  }

  auto pid_written = write_pid(::getpid());
  if (!pid_written) {
    teardown();
    phase_ = DshPhase::Degraded;
    release_lock();
    return pid_written;
  }

  phase_ = DshPhase::Ready;
  detail_.clear();
  log_.append("ready pid " + std::to_string(child_->pid), "supervisor");
  return {};
}

void Supervisor::teardown() {
  if (paused_ && child_ && child_->pid > 0) {
    ::kill(child_->pid, SIGCONT);
    paused_ = false;
  }
  if (client_) client_->stop();
  if (child_) {
    if (child_->stdin_fd >= 0) {
      ::close(child_->stdin_fd);
      child_->stdin_fd = -1;
    }
    const auto exited = terminate_child(child_->pid, std::chrono::seconds(5));
    (void)exited;
    if (child_->stdout_fd >= 0) {
      ::close(child_->stdout_fd);
      child_->stdout_fd = -1;
    }
    child_.reset();
  }
  if (stderr_fd_ >= 0) {
    ::close(stderr_fd_);
    stderr_fd_ = -1;
  }
  client_.reset();
}

Result<void> Supervisor::down() {
  if (!child_) {
    phase_ = DshPhase::Down;
    remove_pid();
    release_lock();
    return {};
  }
  teardown();
  phase_ = DshPhase::Down;
  detail_.clear();
  remove_pid();
  release_lock();
  log_.append("stopped", "supervisor");
  return {};
}

Status Supervisor::status() const {
  Status status;
  status.phase = phase_;
  status.detail = detail_;
  status.pid = child_ ? child_->pid : -1;
  status.connected = phase_ == DshPhase::Ready && client_ != nullptr;
  status.profile = config_.profile;
  status.base_profile = config_.base_profile;
  status.state_dir = config_.state_dir;
  return status;
}

bool Supervisor::connected() const { return phase_ == DshPhase::Ready && client_ != nullptr; }

Result<void> Supervisor::pause() {
  if (!child_ || child_->pid <= 0) {
    return std::unexpected(Error{Layer::Process, "the kernel is not running", std::nullopt});
  }
  if (paused_) return {};
  if (::kill(child_->pid, SIGSTOP) != 0) {
    return std::unexpected(Error{Layer::Process, std::string("cannot pause the kernel: ") + std::strerror(errno),
                                 std::nullopt});
  }
  paused_ = true;
  return {};
}

Result<void> Supervisor::resume() {
  if (!child_ || child_->pid <= 0) {
    return std::unexpected(Error{Layer::Process, "the kernel is not running", std::nullopt});
  }
  if (!paused_) return {};
  if (::kill(child_->pid, SIGCONT) != 0) {
    return std::unexpected(Error{Layer::Process, std::string("cannot resume the kernel: ") + std::strerror(errno),
                                 std::nullopt});
  }
  paused_ = false;
  return {};
}

Result<acp::Json> Supervisor::request(std::string_view method, const acp::Json& params,
                                      std::chrono::milliseconds timeout) {
  if (!client_) {
    return std::unexpected(Error{Layer::Kernel, "the kernel is not running", std::nullopt});
  }
  return client_->request(method, params, timeout);
}

void Supervisor::notify(std::string_view method, const acp::Json& params) {
  if (client_) client_->notify(method, params);
}

Result<void> Supervisor::answer_permission(std::string_view request_id,
                                           const std::optional<std::string>& option_id) {
  if (!client_) {
    return std::unexpected(Error{Layer::Kernel, "the kernel is not running", std::nullopt});
  }
  return client_->answer_permission(request_id, option_id);
}

}  // namespace booth
