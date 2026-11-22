#pragma once

#include <chrono>
#include <filesystem>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

#include "contract/error.hpp"
#include "contract/event.hpp"
#include "subagent/acp/client.hpp"
#include "subagent/kernel.hpp"
#include "supervisor/config.hpp"
#include "supervisor/logs.hpp"
#include "supervisor/process.hpp"

namespace booth {

struct Status {
  DshPhase phase{DshPhase::Down};
  std::string detail;
  pid_t pid{-1};
  bool connected{false};
  std::string profile;
  std::string base_profile;
  std::filesystem::path state_dir;
};



std::string status_json(const Status& status);

/// Reads the pid file and reports whether a supervisor is alive. No connection is made.
Status probe_status(const Config& config);

/// Owns one dsh process and its ACP connection. One supervisor per state dir.
class Supervisor : public Kernel {
 public:
  Supervisor(Config config, Env env, acp::Client::EventSink sink, std::string client_version);
  ~Supervisor();

  Supervisor(const Supervisor&) = delete;
  Supervisor& operator=(const Supervisor&) = delete;

  Result<void> up();
  Result<void> down();

  [[nodiscard]] Status status() const;
  [[nodiscard]] const Config& config() const { return config_; }
  [[nodiscard]] acp::Client* client() const { return client_.get(); }

  Result<acp::Json> request(std::string_view method, const acp::Json& params,
                            std::chrono::milliseconds timeout) override;
  Result<void> answer_permission(std::string_view request_id,
                                 const std::optional<std::string>& option_id) override;
  void notify(std::string_view method, const acp::Json& params) override;
  [[nodiscard]] bool connected() const override;

 private:
  Result<void> ensure_state_dir();
  Result<void> acquire_lock();
  void release_lock();
  Result<void> write_pid(pid_t pid);
  void remove_pid();
  std::vector<std::pair<std::string, std::string>> child_env() const;
  void teardown();

  Config config_;
  Env env_;
  acp::Client::EventSink sink_;
  std::string client_version_;
  Log log_;
  int lock_fd_{-1};
  int stderr_fd_{-1};
  std::optional<Child> child_;
  std::unique_ptr<acp::Client> client_;
  DshPhase phase_{DshPhase::Down};
  std::string detail_;
};

}  // namespace booth
