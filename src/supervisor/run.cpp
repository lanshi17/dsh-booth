#include "supervisor/run.hpp"

#include <chrono>
#include <string>

#include "subagent/acp/messages.hpp"

namespace booth {

Result<RunResult> run_task(Kernel& kernel, const RunRequest& request) {
  auto created = kernel.request("session/new", acp::new_session_params(request.cwd),
                                    std::chrono::seconds(20));
  if (!created) return std::unexpected(created.error());

  const std::string session = created->value("sessionId", std::string{});
  if (session.empty()) {
    return std::unexpected(
        Error{Layer::Protocol, "session/new returned no sessionId", std::nullopt});
  }

  RunResult result;
  result.session = session;

  if (!request.model.empty() && request.model != "auto") {
    auto selected = kernel.request(
        "session/set_config_option", acp::set_config_params(session, "model", request.model),
        std::chrono::seconds(10));
    if (!selected) return std::unexpected(selected.error());
  }

  auto prompted = kernel.request("session/prompt", acp::prompt_params(session, request.text, {}, {}),
                                     request.timeout);
  if (!prompted) return std::unexpected(prompted.error());

  result.stop_reason = prompted->value("stopReason", std::string{});
  return result;
}

}  // namespace booth
