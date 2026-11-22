#include "subagent/kernel.hpp"

#include <unistd.h>

#include <utility>

namespace booth {

RemoteKernel::FdGuard::~FdGuard() {
  if (fd >= 0) ::close(fd);
}

RemoteKernel::RemoteKernel(int fd, acp::Client::EventSink sink) : client_(fd, fd, std::move(sink)) {
  fd_.fd = fd;
  client_.start();
}

RemoteKernel::~RemoteKernel() { client_.stop(); }

Result<acp::Json> RemoteKernel::request(std::string_view method, const acp::Json& params,
                                        std::chrono::milliseconds timeout) {
  return client_.request(method, params, timeout);
}

void RemoteKernel::notify(std::string_view method, const acp::Json& params) {
  client_.notify(method, params);
}

Result<void> RemoteKernel::answer_permission(std::string_view request_id,
                                             const std::optional<std::string>& option_id) {
  return client_.answer_permission(request_id, option_id);
}

bool RemoteKernel::connected() const { return fd_.fd >= 0; }

}  // namespace booth
