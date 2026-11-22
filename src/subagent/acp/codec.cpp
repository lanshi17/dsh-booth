#include "subagent/acp/codec.hpp"

namespace booth::acp {

Result<Json> decode(std::string_view line) {
  while (!line.empty() && (line.back() == '\n' || line.back() == '\r')) {
    line.remove_suffix(1);
  }
  if (line.empty()) {
    return std::unexpected(Error{Layer::Protocol, "empty frame", std::nullopt});
  }
  if (line.size() > kMaxLine) {
    return std::unexpected(Error{Layer::Protocol, "frame exceeds the limit", std::nullopt});
  }
  Json value = Json::parse(line.begin(), line.end(), nullptr, false);
  if (value.is_discarded()) {
    return std::unexpected(Error{Layer::Protocol, "frame is not JSON", std::nullopt});
  }
  if (!value.is_object()) {
    return std::unexpected(Error{Layer::Protocol, "frame is not a JSON object", std::nullopt});
  }
  return value;
}

std::string encode(const Json& message) {
  std::string out = message.dump();
  out.push_back('\n');
  return out;
}

}  // namespace booth::acp
