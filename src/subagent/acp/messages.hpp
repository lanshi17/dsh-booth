#pragma once

#include <optional>
#include <string>
#include <utility>
#include <vector>
#include <string_view>
#include <vector>

#include "contract/event.hpp"
#include "subagent/acp/codec.hpp"

namespace booth::acp {

inline constexpr int kProtocolVersion = 1;

Json initialize_params(std::string_view client_version);
Json new_session_params(std::string_view cwd);
struct ImageAttachment {
  std::string mime_type;
  std::string data;
  std::string name;
};

Json prompt_params(std::string_view session, std::string_view text,
                   const std::vector<std::pair<std::string, std::string>>& resources = {},
                   const std::vector<ImageAttachment>& images = {});
Json set_config_params(std::string_view session, std::string_view config_id,
                       std::string_view value);
Json cancel_params(std::string_view session);
Json permission_result(const std::optional<std::string>& option_id);

/// A JSON-RPC id as a stable map key (numbers and strings both work).
std::string id_key(const Json& id);

std::optional<PermissionRequest> permission_request(const Json& params, std::string_view request_id);

/// Decode one `session/update` notification into contract events.
std::vector<Event> session_updates(const Json& params);

/// Whether a tool call is the delegation tool (the run's start/end marker).
bool is_delegation(std::string_view name, std::string_view title);

}  // namespace booth::acp
