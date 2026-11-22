#pragma once

#include <chrono>
#include <optional>
#include <string>
#include <string_view>

#include "contract/error.hpp"
#include "subagent/acp/client.hpp"

namespace booth {

/// What the console needs from a kernel: requests, notifications and permission answers.
class Kernel {
 public:
  virtual ~Kernel() = default;

  virtual Result<acp::Json> request(std::string_view method, const acp::Json& params,
                                    std::chrono::milliseconds timeout) = 0;
  virtual void notify(std::string_view method, const acp::Json& params) = 0;
  virtual Result<void> answer_permission(std::string_view request_id,
                                         const std::optional<std::string>& option_id) = 0;
  [[nodiscard]] virtual bool connected() const = 0;
};

/// A kernel reached over the daemon's Unix socket.
class RemoteKernel final : public Kernel {
 public:
  RemoteKernel(int fd, acp::Client::EventSink sink);
  ~RemoteKernel() override;

  RemoteKernel(const RemoteKernel&) = delete;
  RemoteKernel& operator=(const RemoteKernel&) = delete;

  Result<acp::Json> request(std::string_view method, const acp::Json& params,
                            std::chrono::milliseconds timeout) override;
  void notify(std::string_view method, const acp::Json& params) override;
  Result<void> answer_permission(std::string_view request_id,
                                 const std::optional<std::string>& option_id) override;
  [[nodiscard]] bool connected() const override;

 private:
  struct FdGuard {
    FdGuard() = default;
    int fd{-1};
    ~FdGuard();
    FdGuard(const FdGuard&) = delete;
    FdGuard& operator=(const FdGuard&) = delete;
    FdGuard(FdGuard&&) = delete;
    FdGuard& operator=(FdGuard&&) = delete;
  };

  FdGuard fd_;
  acp::Client client_;
};

}  // namespace booth
